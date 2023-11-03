
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
