use core::ffi::CStr;

use common::{
    sys_arg,
    syscalls::{
        syscall_arg_to_u64, syscall_handler_wrapper, SyscallArgError, SyscallResult, NUM_SYSCALLS,
    },
    verify_args,
};

use crate::cpu::idt::InterruptAllSavedState;

use super::scheduler::with_current_process;

type Syscall = fn(&mut InterruptAllSavedState) -> SyscallResult;

const SYSCALLS: [Syscall; NUM_SYSCALLS] = [
    sys_open, // common::syscalls::SYS_OPEN
];

fn convert_sys_arg_to_string<'a>(arg: *const u8) -> Result<&'a str, SyscallArgError> {
    if arg.is_null() {
        println!("arg is null");
        return Err(SyscallArgError::InvalidUserPointer);
    }
    if !with_current_process(|process| process.is_user_address_mapped(arg as _)) {
        println!("arg is not mapped");
        return Err(SyscallArgError::InvalidUserPointer);
    }

    let slice = unsafe { CStr::from_ptr(arg as _) };
    let string = CStr::to_str(slice).map_err(|_| SyscallArgError::NotValidUtf8)?;
    Ok(string)
}

fn sys_open(_all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (path, access_mode, flags, ..) = verify_args! {
        sys_arg!(0, _all_state.rest => convert_sys_arg_to_string(*const u8)),
        sys_arg!(1, _all_state.rest => u64),
        sys_arg!(2, _all_state.rest => u64),
    };
    println!("sys_open: {path} {access_mode} {flags}");

    SyscallResult::Ok(0)
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
