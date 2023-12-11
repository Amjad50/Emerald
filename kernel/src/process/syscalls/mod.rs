use common::syscalls::{syscall_handler_wrapper, SyscallResult, NUM_SYSCALLS};

use crate::cpu::idt::InterruptAllSavedState;

type Syscall = fn(&mut InterruptAllSavedState) -> SyscallResult;

const SYSCALLS: [Syscall; NUM_SYSCALLS] = [
    sys_open, // common::syscalls::SYS_OPEN
];

fn sys_open(_all_state: &mut InterruptAllSavedState) -> SyscallResult {
    // println!("sys_open");
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
