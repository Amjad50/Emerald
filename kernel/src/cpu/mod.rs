use core::pin::Pin;

use crate::{
    cpu::gdt::GlobalDescriptorManager,
    memory_management::virtual_memory_mapper::ProcessKernelStack, process::ProcessContext,
};

use self::{
    gdt::{GlobalDescriptorTablePointer, SegmentSelector},
    idt::InterruptDescriptorTablePointer,
};

pub mod gdt;
pub mod idt;
pub mod interrupts;

const MAX_CPUS: usize = 8;

pub mod flags {
    pub const IF: u64 = 1 << 9;
}

#[allow(dead_code)]
pub mod msr {
    pub const APIC_BASE: u32 = 0x1b;
    pub const EFER: u32 = 0xc0000080;

    pub unsafe fn read(reg: u32) -> u64 {
        let (eax, edx): (u32, u32);
        core::arch::asm!("rdmsr", in("ecx") reg, out("eax") eax, out("edx") edx, options(readonly, nostack, preserves_flags));
        ((edx as u64) << 32) | (eax as u64)
    }

    pub unsafe fn write(reg: u32, val: u64) {
        let eax = val as u32;
        let edx = (val >> 32) as u32;
        core::arch::asm!("wrmsr", in("ecx") reg, in("eax") eax, in("edx") edx, options(readonly, nostack, preserves_flags));
    }
}

#[allow(dead_code)]
pub mod cpuid {
    pub const FN_FEAT: u32 = 1;

    pub const FEAT_EDX_TSC: u32 = 1 << 4;
    pub const FEAT_EDX_APIC: u32 = 1 << 9;

    #[macro_export]
    macro_rules! cpuid {
        ($rax:expr) => {
            ::core::arch::x86_64::__cpuid_count($rax, 0)
        };
        ($rax:expr, $rcx:expr) => {
            ::core::arch::x86_64::__cpuid_count($rax, $rcx)
        };
    }
    #[allow(unused_imports)]
    pub use cpuid;
}

static mut CPUS: [Cpu; MAX_CPUS] = [Cpu::empty(); MAX_CPUS];

#[derive(Debug, Clone, Copy)]
pub struct Cpu {
    // index of myself inside `CPUS`
    pub id: usize,
    apic_id: u8,
    old_interrupt_enable: bool,
    // number of times we have called `cli`
    n_cli: usize,

    // saved context, when switching from kernel to user and vice versa
    // if there is a value here, it indicates that we are running a processing now
    // either about to run a process, or in the middle
    //
    // i.e., if this is `None`, then we are in the kernel and free to take a process
    pub context: Option<ProcessContext>,
    // the process id of the current process
    pub process_id: u64,
    pub scheduling: bool,

    /// GDT
    gdt: GlobalDescriptorManager,
}

impl Cpu {
    const fn empty() -> Self {
        Self {
            id: 0,
            apic_id: 0,
            old_interrupt_enable: false,
            n_cli: 0,
            context: None,
            process_id: 0,
            scheduling: false,
            gdt: GlobalDescriptorManager::empty(),
        }
    }

    fn init(&mut self, id: usize, apic_id: u8) {
        self.id = id;
        self.apic_id = apic_id;
    }

    pub fn push_cli(&mut self) {
        if self.n_cli == 0 {
            let rflags = unsafe { rflags() };
            let old_interrupt_flag = rflags & flags::IF != 0;
            unsafe { clear_interrupts() };
            self.old_interrupt_enable = old_interrupt_flag;
        }
        // re-read the flags
        let rflags = unsafe { rflags() };
        assert!(self.n_cli < usize::MAX);
        assert_eq!(rflags & flags::IF, 0);
        self.n_cli += 1;
    }

    pub fn pop_cli(&mut self) {
        let rflags = unsafe { rflags() };
        assert!(self.n_cli > 0);
        assert_eq!(rflags & flags::IF, 0);

        self.n_cli -= 1;
        if self.n_cli == 0 && self.old_interrupt_enable {
            unsafe { set_interrupts() };
        }
    }

    pub fn interrupts_disabled(&self) -> bool {
        unsafe { rflags() & flags::IF == 0 }
    }

    pub fn n_cli(&self) -> usize {
        self.n_cli
    }

    fn gdt(self: Pin<&'static mut Self>) -> Pin<&'static mut GlobalDescriptorManager> {
        // SAFETY: we are guaranteed that `self` is static and never changes
        unsafe { self.map_unchecked_mut(|s| &mut s.gdt) }
    }

    pub fn init_kernel_gdt(self: Pin<&'static mut Self>) {
        // initialize the GDT
        self.gdt().init_segments();
    }

    pub fn load_process_kernel_stack(&mut self, stack: &ProcessKernelStack) {
        self.gdt.load_process_kernel_stack(stack);
    }
}

pub fn cpu() -> Pin<&'static mut Cpu> {
    // TODO: use thread local to get the current cpu
    Pin::static_mut(unsafe { &mut CPUS[0] })
}

pub unsafe fn rflags() -> u64 {
    let rflags: u64;
    core::arch::asm!("pushfq; pop {0:r}", out(reg) rflags, options(nostack, preserves_flags));
    rflags
}

unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("al") val, in("dx") port, options(readonly, nostack, preserves_flags));
}

unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port, options(readonly, nostack, preserves_flags));
    val
}

unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    core::arch::asm!("in ax, dx", out("ax") val, in("dx") port, options(readonly, nostack, preserves_flags));
    val
}

unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("ax") val, in("dx") port, options(readonly, nostack, preserves_flags));
}

unsafe fn outd(port: u16, val: u32) {
    core::arch::asm!("out dx, eax", in("eax") val, in("dx") port, options(readonly, nostack, preserves_flags));
}

unsafe fn ind(port: u16) -> u32 {
    let val: u32;
    core::arch::asm!("in eax, dx", out("eax") val, in("dx") port, options(readonly, nostack, preserves_flags));
    val
}

pub trait IoPortInt {
    fn io_out(port: u16, val: Self);
    fn io_in(port: u16) -> Self;
}

impl IoPortInt for u8 {
    fn io_out(port: u16, val: Self) {
        unsafe { outb(port, val) }
    }
    fn io_in(port: u16) -> Self {
        unsafe { inb(port) }
    }
}

impl IoPortInt for u16 {
    fn io_out(port: u16, val: Self) {
        unsafe { outw(port, val) }
    }
    fn io_in(port: u16) -> Self {
        unsafe { inw(port) }
    }
}

impl IoPortInt for u32 {
    fn io_out(port: u16, val: Self) {
        unsafe { outd(port, val) }
    }
    fn io_in(port: u16) -> Self {
        unsafe { ind(port) }
    }
}

pub unsafe fn io_out<T: IoPortInt>(port: u16, val: T) {
    T::io_out(port, val);
}

pub unsafe fn io_in<T: IoPortInt>(port: u16) -> T {
    T::io_in(port)
}

pub unsafe fn clear_interrupts() {
    core::arch::asm!("cli", options(nomem, nostack, preserves_flags));
}

pub unsafe fn set_interrupts() {
    core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
}

#[allow(dead_code)]
pub unsafe fn get_cr0() -> u64 {
    let cr0: u64;
    core::arch::asm!("mov {0:r}, cr0", out(reg) cr0, options(readonly, nostack, preserves_flags));
    cr0
}

#[allow(dead_code)]
pub unsafe fn set_cr0(cr0: u64) {
    core::arch::asm!("mov cr0, rax", in("rax") cr0, options(nomem, nostack, preserves_flags));
}

pub unsafe fn set_cr3(cr3: u64) {
    core::arch::asm!("mov cr3, rax", in("rax") cr3, options(nomem, nostack, preserves_flags));
}

pub unsafe fn get_cr3() -> u64 {
    let cr3: u64;
    core::arch::asm!("mov {0:r}, cr3", out(reg) cr3, options(readonly, nostack, preserves_flags));
    cr3
}

#[allow(dead_code)]
pub unsafe fn get_cr4() -> u64 {
    let cr4: u64;
    core::arch::asm!("mov {0:r}, cr4", out(reg) cr4, options(readonly, nostack, preserves_flags));
    cr4
}

#[allow(dead_code)]
pub unsafe fn set_cr4(cr4: u64) {
    core::arch::asm!("mov cr4, rax", in("rax") cr4, options(nomem, nostack, preserves_flags));
}

/// SAFETY: the data pointed to by `gdtr` must be static and never change
unsafe fn lgdt(gdtr: &GlobalDescriptorTablePointer) {
    core::arch::asm!("lgdt [rax]", in("rax") gdtr, options(readonly, nostack, preserves_flags));
}

/// SAFETY: the data pointed to by `ldtr` must be static and never change
unsafe fn lidt(ldtr: &InterruptDescriptorTablePointer) {
    core::arch::asm!("lidt [rax]", in("rax") ldtr, options(readonly, nostack, preserves_flags));
}

unsafe fn ltr(tr: SegmentSelector) {
    core::arch::asm!("ltr ax", in("ax") tr.0, options(nomem, nostack, preserves_flags));
}

unsafe fn set_cs(cs: SegmentSelector) {
    core::arch::asm!(
        "push {0:r}",
        // this is not 0x1f, it is `2-forward`,
        // which gives the offset of the nearest `2:` label
        "lea {tmp}, [rip + 2f]",
        "push {tmp}",
        "retfq",
        "2:",
        in(reg) cs.0, tmp=lateout(reg) _, options(preserves_flags));
}

unsafe fn set_data_segments(ds: SegmentSelector) {
    core::arch::asm!(
        "mov ds, {0:r}",
        "mov es, {0:r}",
        "mov ss, {0:r}",
        "mov fs, {0:r}",
        "mov gs, {0:r}",
        in(reg) ds.0, options(preserves_flags));
}

fn get_cs() -> u16 {
    let cs: u16;
    unsafe {
        core::arch::asm!("mov {0:r}, cs", out(reg) cs, options(readonly, nostack, preserves_flags));
    }
    cs
}

pub unsafe fn halt() {
    core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
}

pub unsafe fn invalidate_tlp(virtual_address: u64) {
    core::arch::asm!("invlpg [{0}]", in(reg) virtual_address);
}

#[allow(dead_code)]
pub unsafe fn read_tsc() -> u64 {
    let (low, high): (u32, u32);
    core::arch::asm!("rdtsc", out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags));
    ((high as u64) << 32) | (low as u64)
}

#[macro_export]
macro_rules! rip {
    () => {
        {
            let rip: u64;
            unsafe {
                core::arch::asm!("lea {0:r}, [rip]", out(reg) rip, options(nomem, nostack, preserves_flags));
            }
            rip
        }
    };
}
#[allow(unused_imports)]
pub use rip;

#[macro_export]
macro_rules! rbp {
    () => {
        {
            let rbp: u64;
            unsafe {
                core::arch::asm!("mov {0:r}, rbp", out(reg) rbp, options(nomem, nostack, preserves_flags));
            }
            rbp
        }
    };
}
#[allow(unused_imports)]
pub use rbp;

#[macro_export]
macro_rules! rsp {
    () => {
        {
            let rsp: u64;
            unsafe {
                core::arch::asm!("mov {0:r}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags));
            }
            rsp
        }
    };
}
#[allow(unused_imports)]
pub use rsp;
