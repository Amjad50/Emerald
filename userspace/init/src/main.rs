#![feature(restricted_std)]
#![no_main]

use core::ffi::{c_char, CStr};

use kernel_user_link::{
    call_syscall,
    syscalls::{SYS_EXIT, SYS_SPAWN, SYS_WRITE},
    FD_STDOUT,
};

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

fn spawn(path: &CStr, argv: &[*const c_char]) -> u64 {
    unsafe {
        call_syscall!(
            SYS_SPAWN,
            path.as_ptr() as u64, // path
            argv.as_ptr() as u64, // argv
        )
        .unwrap()
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // we are in `init` now
    write_to_stdout("[init] Hello!\n\n".as_bytes());

    let shell_path = c"/shell";
    let shell_argv = [shell_path.as_ptr(), c"".as_ptr()];
    let shell_pid = spawn(shell_path, &shell_argv);

    write_to_stdout(format!("[init] spawned shell with pid {}\n", shell_pid).as_bytes());

    exit(111);
}
