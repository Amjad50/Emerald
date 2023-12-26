use core::ffi::{c_char, CStr};

use kernel_user_link::{
    call_syscall,
    syscalls::{SyscallError, SYS_EXIT, SYS_SPAWN},
};

/// # Safety
/// No guarantees are made about the state of the system after this function returns.
pub unsafe fn exit(code: u64) -> ! {
    unsafe {
        call_syscall!(
            SYS_EXIT,
            code, // code
        )
        .unwrap();
    }
    unreachable!("exit syscall should not return")
}

/// # Safety
/// path must be a valid C string.
/// argv must be a valid C string array. ending with a null pointer.
pub unsafe fn spawn(path: &CStr, argv: &[*const c_char]) -> Result<u64, SyscallError> {
    unsafe {
        call_syscall!(
            SYS_SPAWN,
            path.as_ptr() as u64, // path
            argv.as_ptr() as u64, // argv
        )
    }
}
