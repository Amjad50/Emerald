use alloc::ffi;

use common::{
    sys_arg,
    syscalls::{
        syscall_arg_to_u64, syscall_handler_wrapper, SyscallArgError, SyscallResult, NUM_SYSCALLS,
    },
    verify_args,
};

use crate::cpu::idt::InterruptAllSavedState;

type Syscall = fn(&mut InterruptAllSavedState) -> SyscallResult;

const SYSCALLS: [Syscall; NUM_SYSCALLS] = [
    sys_open, // common::syscalls::SYS_OPEN
];

fn convert_sys_arg_to_string(arg: *const u8) -> Result<&'static str, SyscallArgError> {
    // Err(SyscallArgError::NotValidUtf8)
    Ok("we are in `init` now {arg:p}")
}

fn sys_open(_all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (arg1, arg2, ..) = verify_args! {
        sys_arg!(0, _all_state.rest => convert_sys_arg_to_string(*const u8)),
        sys_arg!(1, _all_state.rest => u64)
    };
    println!("{arg1} {arg2}");

    // println!("sys_open");
    SyscallResult::Ok(arg2 + 1)
}

pub fn handle_syscall(all_state: &mut InterruptAllSavedState) {
    let syscall_number = all_state.rest.rax;

    // `syscall_handler_wrapper` will check the syscall number and return error if it exceed the
    // number of syscalls (NUM_SYSCALLS)
    all_state.rest.rax = syscall_handler_wrapper(syscall_number, || {
        let syscall_func = SYSCALLS[syscall_number as usize];
        syscall_func(all_state)
    });
}
