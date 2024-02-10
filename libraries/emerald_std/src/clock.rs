pub use kernel_user_link::clock::{ClockTime, ClockType};
use kernel_user_link::{
    call_syscall,
    syscalls::{SyscallError, SYS_GET_TIME, SYS_SLEEP},
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

/// # Safety
/// There are no safety requirements for this function.
/// Its just that its a wrapper around a syscall.
pub unsafe fn get_time(time_type: ClockType) -> Result<ClockTime, SyscallError> {
    let mut time = ClockTime {
        seconds: 0,
        nanoseconds: 0,
    };
    unsafe {
        call_syscall!(
            SYS_GET_TIME,
            time_type as u64,                   // time_type
            &mut time as *mut ClockTime as u64, // time
        )
        .map(|e| assert!(e == 0))
        .map(|_| time)
    }
}
