use core::ffi::{c_char, CStr};

pub use kernel_user_link::process::SpawnFileMapping;
use kernel_user_link::{
    call_syscall,
    syscalls::{SyscallError, SYS_EXIT, SYS_SPAWN},
};

/// # Safety
/// No guarantees are made about the state of the system after this function returns.
pub unsafe fn exit(code: i32) -> ! {
    unsafe {
        call_syscall!(
            SYS_EXIT,
            code as u64, // code
        )
        .unwrap();
    }
    unreachable!("exit syscall should not return")
}

/// # Safety
/// path must be a valid C string.
/// argv must be a valid C string array. ending with a null pointer.
/// File mappings must be valid and present file mappings.
/// The fds used in the file mappings must never be used again by the caller, as the ownership is
/// transferred to the child process.
pub unsafe fn spawn(
    path: &CStr,
    argv: &[*const c_char],
    file_mappings: &[SpawnFileMapping],
) -> Result<u64, SyscallError> {
    unsafe {
        call_syscall!(
            SYS_SPAWN,
            path.as_ptr() as u64,          // path
            argv.as_ptr() as u64,          // argv
            file_mappings.as_ptr() as u64, // file_mappings
            file_mappings.len() as u64     // file_mappings_len
        )
    }
}
