#![no_std]
#![no_main]

use core::hint;

use common::{call_syscall, syscalls::SYS_OPEN};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // we are in `init` now
    // create some delay
    loop {
        unsafe { call_syscall!(SYS_OPEN) }; // TODO: properly implement

        hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        hint::spin_loop();
    }
}
