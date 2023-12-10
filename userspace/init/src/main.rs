#![no_std]
#![no_main]

use core::hint;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // we are in `init` now
    // create some delay
    loop {
        unsafe { core::arch::asm!("mov eax, 0; int 0xFE") };
        hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        hint::spin_loop();
    }
}
