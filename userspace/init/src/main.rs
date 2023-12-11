#![no_std]
#![no_main]
#![feature(c_str_literals)]

use core::hint;

use common::{call_syscall, syscalls::SYS_OPEN};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // we are in `init` now
    // create some delay
    let access_mode = 0;
    let flags = 0;
    unsafe {
        // TODO: properly implement
        // must be C string
        call_syscall!(SYS_OPEN, c"filename".as_ptr() as u64, access_mode, flags).unwrap()
    };
    loop {
        hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        hint::spin_loop();
    }
}
