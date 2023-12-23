#![no_std]
#![no_main]

use core::ffi::CStr;

use kernel_user_link::{
    call_syscall,
    syscalls::{SYS_EXIT, SYS_OPEN, SYS_READ, SYS_WRITE},
    FD_STDOUT,
};

fn write_to_stdout(s: &CStr) {
    unsafe {
        call_syscall!(
            SYS_WRITE,
            FD_STDOUT,                 // fd
            s.as_ptr() as u64,         // buf
            s.to_bytes().len() as u64  // size
        )
        .unwrap();
    }
}

fn open_file(path: &CStr) -> u64 {
    unsafe {
        call_syscall!(
            SYS_OPEN,
            path.as_ptr() as u64, // path
            0,                    // flags
            0                     // mode
        )
        .unwrap()
    }
}

fn read_file(fd: u64, buf: &mut [u8]) -> u64 {
    unsafe {
        call_syscall!(
            SYS_READ,
            fd,                      // fd
            buf.as_mut_ptr() as u64, // buf
            buf.len() as u64         // size
        )
        .unwrap()
    }
}

fn exit(code: u64) -> ! {
    unsafe {
        call_syscall!(
            SYS_EXIT,
            code, // code
        )
        .unwrap();
    }
    unreachable!("exit syscall should not return")
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // we are in `init` now
    // create some delay
    write_to_stdout(c"[shell] Hello!\n\n");

    // open `/message.txt` and print the result
    let fd = open_file(c"/message.txt");
    write_to_stdout(c"[shell] content of `/message.txt`:\n");
    let mut buf = [0u8; 1024];
    let read = read_file(fd, &mut buf);
    buf[read as usize] = 0; // null terminate
    write_to_stdout(CStr::from_bytes_until_nul(&buf[..read as usize + 1]).unwrap());

    exit(222);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    write_to_stdout(c"[shell] panicked!\n");
    exit(0xFF);
}
