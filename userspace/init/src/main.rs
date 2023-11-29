#![no_std]
#![no_main]

use core::hint;

#[no_mangle]
pub extern "C" fn _start() {}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        hint::spin_loop();
    }
}
