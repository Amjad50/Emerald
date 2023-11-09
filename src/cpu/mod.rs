use self::{gdt::GlobalDescriptorTablePointer, idt::InterruptDescriptorTablePointer};

pub mod gdt;
pub mod idt;
pub mod interrupts;

const CPUID_FN_FEAT: u32 = 1;
const MAX_CPUS: usize = 8;

static mut CPUS: [Cpu; MAX_CPUS] = [Cpu::empty(); MAX_CPUS];

// TODO: add thread/cpu local to hold a pointer to the current cpu

#[derive(Debug, Clone, Copy)]
struct Cpu {
    // index of myself inside `CPUS`
    id: usize,
    apic_id: usize,
}

impl Cpu {
    const fn empty() -> Self {
        Self { id: 0, apic_id: 0 }
    }
}

// TODO: implement cpu_id
pub fn cpu_id() -> u32 {
    0
}

pub unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!("out dx, al", in("al") val, in("dx") port, options(nomem, nostack, preserves_flags));
}

pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack, preserves_flags));
    val
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

/// SAFETY: the data pointed to by `ldtr` must be static and never change
unsafe fn lgdt(ldtr: &GlobalDescriptorTablePointer) {
    core::arch::asm!("lgdt [rax]", in("rax") ldtr, options(nomem, nostack, preserves_flags));
}

unsafe fn lidt(ldtr: &InterruptDescriptorTablePointer) {
    core::arch::asm!("lidt [rax]", in("rax") ldtr, options(nomem, nostack, preserves_flags));
}

unsafe fn ltr(tr: u16) {
    core::arch::asm!("ltr ax", in("ax") tr, options(nomem, nostack, preserves_flags));
}

unsafe fn set_cs(cs: u16) {
    core::arch::asm!(
        "push {:r}",
        // this is not 0x1f, it is `1-forward`,
        // which gives the offset of the nearest `1:` label
        "lea {tmp}, [rip + 1f]",
        "push {tmp}",
        "retfq",
        "1:",
        in(reg) cs as u64, tmp=lateout(reg) _, options(preserves_flags));
}

fn get_cs() -> u16 {
    let cs: u16;
    unsafe {
        core::arch::asm!("mov {0:r}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
    }
    cs
}

pub unsafe fn rdmsr(inp: u32) -> u64 {
    let (eax, edx): (u32, u32);
    core::arch::asm!("rdmsr", in("ecx") inp, out("eax") eax, out("edx") edx, options(nomem, nostack, preserves_flags));
    ((edx as u64) << 32) | (eax as u64)
}

pub unsafe fn wrmsr(inp: u32, val: u64) {
    let eax = val as u32;
    let edx = (val >> 32) as u32;
    core::arch::asm!("wrmsr", in("ecx") inp, in("eax") eax, in("edx") edx, options(nomem, nostack, preserves_flags));
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
