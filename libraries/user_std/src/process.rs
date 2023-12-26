use kernel_user_link::{call_syscall, syscalls::SYS_EXIT};

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
