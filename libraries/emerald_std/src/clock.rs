use kernel_user_link::{
    call_syscall,
    syscalls::{SyscallError, SYS_SLEEP},
};

/// # Safety
/// This function assumes that `seconds` and `nanoseconds` are valid, nano seconds should be less than 1_000_000_000.
pub unsafe fn sleep(seconds: u64, nanoseconds: u64) -> Result<(), SyscallError> {
    unsafe {
        call_syscall!(
            SYS_SLEEP,
            seconds,     // seconds
            nanoseconds, // nanoseconds
        )
        .map(|e| assert!(e == 0))
    }
}
