#![no_std]
#![no_main]

use core::hint;

#[no_mangle]
pub extern "C" fn _start() {
    // we are in `init` now
    // create some delay
    for _ in 0..10_000_000 {
        hint::spin_loop();
    }
    // return to kernel
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        hint::spin_loop();
    }
}
