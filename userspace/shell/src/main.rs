#![feature(restricted_std)]
#![no_main]

use core::ffi::CStr;

use kernel_user_link::{
    call_syscall,
    syscalls::{SYS_EXIT, SYS_OPEN, SYS_READ, SYS_WRITE},
    FD_STDOUT,
};
use std::string::String;

fn write_to_stdout(s: &[u8]) {
    unsafe {
        call_syscall!(
            SYS_WRITE,
            FD_STDOUT,         // fd
            s.as_ptr() as u64, // buf
            s.len() as u64     // size
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
    write_to_stdout("[shell] Hello!\n\n".as_bytes());

    // open `/message.txt` and print the result
    let fd = open_file(c"/message.txt");
    write_to_stdout("[shell] content of `/message.txt`:\n".as_bytes());
    let mut buf = [0u8; 1024];
    let read = read_file(fd, &mut buf);
    let data = String::from_utf8_lossy(&buf[..read as usize]);

    write_to_stdout(data.as_bytes());
    exit(222);
}
