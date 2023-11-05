use self::gdt::GlobalDescriptorTablePointer;

pub mod gdt;

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

pub unsafe fn set_cr3(cr3: u64) {
    core::arch::asm!("mov cr3, rax", in("rax") cr3, options(nomem, nostack, preserves_flags));
}

/// SAFETY: the data pointed to by `ldtr` must be static and never change
unsafe fn lgdt(ldtr: &GlobalDescriptorTablePointer) {
    core::arch::asm!("lgdt [rax]", in("rax") ldtr, options(nomem, nostack, preserves_flags));
}

unsafe fn ltr(tr: u16) {
    core::arch::asm!("ltr ax", in("ax") tr, options(nomem, nostack, preserves_flags));
}

pub unsafe fn set_cs(cs: u16) {
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
