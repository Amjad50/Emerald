use crate::process::ProcessContext;

use self::{
    gdt::{GlobalDescriptorTablePointer, SegmentSelector},
    idt::InterruptDescriptorTablePointer,
};

pub mod gdt;
pub mod idt;
pub mod interrupts;

const CPUID_FN_FEAT: u32 = 1;
const MAX_CPUS: usize = 8;

pub mod flags {
    pub const IF: u64 = 1 << 9;
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
        self.n_cli += 1;
    }

    pub fn pop_cli(&mut self) {
        let rflags = unsafe { rflags() };
        if rflags & flags::IF != 0 {
            panic!("interrupt shouldn't be set");
        }
        if self.n_cli == 0 {
            panic!("pop_cli called without push_cli");
        }

        self.n_cli -= 1;
        if self.n_cli == 0 && self.old_interrupt_enable {
            unsafe { set_interrupts() };
        }
    }
}

pub fn cpu() -> &'static mut Cpu {
    // TODO: use thread local to get the current cpu
    unsafe { &mut CPUS[0] }
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

#[allow(dead_code)]
pub unsafe fn set_interrupts() {
    core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
}

pub unsafe fn set_cr3(cr3: u64) {
    core::arch::asm!("mov cr3, rax", in("rax") cr3, options(nomem, nostack, preserves_flags));
}

pub unsafe fn get_cr3() -> u64 {
    let cr3: u64;
    core::arch::asm!("mov {0:r}, cr3", out(reg) cr3, options(readonly, nostack, preserves_flags));
    cr3
}

/// SAFETY: the data pointed to by `gdtr` must be static and never change
unsafe fn lgdt(gdtr: &GlobalDescriptorTablePointer) {
    // println!("lgdt: {:p}", gdtr);
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
        // this is not 0x1f, it is `1-forward`,
        // which gives the offset of the nearest `1:` label
        "lea {tmp}, [rip + 1f]",
        "push {tmp}",
        "retfq",
        "1:",
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

pub unsafe fn rdmsr(inp: u32) -> u64 {
    let (eax, edx): (u32, u32);
    core::arch::asm!("rdmsr", in("ecx") inp, out("eax") eax, out("edx") edx, options(readonly, nostack, preserves_flags));
    ((edx as u64) << 32) | (eax as u64)
}

pub unsafe fn wrmsr(inp: u32, val: u64) {
    let eax = val as u32;
    let edx = (val >> 32) as u32;
    core::arch::asm!("wrmsr", in("ecx") inp, in("eax") eax, in("edx") edx, options(readonly, nostack, preserves_flags));
}

pub unsafe fn halt() {
    core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
}

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
