use crate::cpu::idt::InterruptAllSavedState;

type Syscall = fn(&mut InterruptAllSavedState) -> u64;

#[repr(u8)]
#[allow(dead_code)]
enum SyscallArgError {
    Valid = 0,
    Invalid = 1,
}

fn syscall_error_invalid_number() -> u64 {
    -1i64 as u64
}

/// Creates an error u64 value for syscall, each byte controls the error for that
/// argument, if the byte is 0, then the argument is valid, otherwise it is
/// invalid. and it represent the error code
/// the error value contains up to 7 arguments errors, the most significant bit
/// indicate a negative number.
///
/// This function will always set the msb to 1
#[allow(dead_code)]
fn create_syscall_error(
    arg1: SyscallArgError,
    arg2: SyscallArgError,
    arg3: SyscallArgError,
    arg4: SyscallArgError,
    arg5: SyscallArgError,
    arg6: SyscallArgError,
    arg7: SyscallArgError,
) -> u64 {
    let mut error = 0u64;
    error |= arg1 as u64;
    error |= (arg2 as u64) << 8;
    error |= (arg3 as u64) << 16;
    error |= (arg4 as u64) << 24;
    error |= (arg5 as u64) << 32;
    error |= (arg6 as u64) << 40;
    error |= (arg7 as u64) << 48;
    error |= 1 << 63;
    error
}

const SYSCALLS: [Syscall; 1] = [sys_open];

fn sys_open(_all_state: &mut InterruptAllSavedState) -> u64 {
    // TODO: implement
    // let path = unsafe { &*(all_state.rdi as *const str) };
    // let flags = all_state.rsi;
    // let mode = all_state.rdx;

    // let fd = crate::fs::open(path, flags, mode);

    // all_state.rax = fd as u64;
    println!("sys_open");
    0
}

pub fn handle_syscall(all_state: &mut InterruptAllSavedState) {
    let syscall_number = all_state.rest.rax;
    if syscall_number >= SYSCALLS.len() as u64 {
        all_state.rest.rax = syscall_error_invalid_number();
        return;
    }
    let syscall_func = SYSCALLS[syscall_number as usize];
    all_state.rest.rax = syscall_func(all_state);
}
