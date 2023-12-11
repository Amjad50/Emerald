use core::ffi::CStr;

use common::{
    sys_arg,
    syscalls::{
        syscall_arg_to_u64, syscall_handler_wrapper, SyscallArgError, SyscallError, SyscallResult,
        NUM_SYSCALLS,
    },
    to_arg_err, verify_args,
};

use crate::{cpu::idt::InterruptAllSavedState, fs};

use super::scheduler::with_current_process;

type Syscall = fn(&mut InterruptAllSavedState) -> SyscallResult;

const SYSCALLS: [Syscall; NUM_SYSCALLS] = [
    sys_open,  // common::syscalls::SYS_OPEN
    sys_write, // common::syscalls::SYS_WRITE
    sys_read,  // common::syscalls::SYS_READ
];

#[inline]
fn check_ptr(arg: *const u8, len: u64) -> Result<(), SyscallArgError> {
    if arg.is_null() {
        return Err(SyscallArgError::InvalidUserPointer);
    }
    if !with_current_process(|process| {
        process.is_user_address_mapped(arg as _)
        // very basic check, just check the last byte
        // TODO: check all mapped pages
            && process.is_user_address_mapped((arg as usize + len as usize - 1) as _)
    }) {
        return Err(SyscallArgError::InvalidUserPointer);
    }
    Ok(())
}

fn sys_arg_to_str<'a>(arg: *const u8) -> Result<&'a str, SyscallArgError> {
    check_ptr(arg, 1)?;

    let slice = unsafe { CStr::from_ptr(arg as _) };
    let string = CStr::to_str(slice).map_err(|_| SyscallArgError::NotValidUtf8)?;
    Ok(string)
}

fn sys_arg_to_byte_slice<'a>(buf: *const u8, size: u64) -> Result<&'a [u8], SyscallArgError> {
    check_ptr(buf, size)?;

    let slice = unsafe { core::slice::from_raw_parts(buf as _, size as _) };
    Ok(slice)
}

fn sys_arg_to_mut_byte_slice<'a>(buf: *mut u8, size: u64) -> Result<&'a mut [u8], SyscallArgError> {
    check_ptr(buf, size)?;

    let slice = unsafe { core::slice::from_raw_parts_mut(buf as _, size as _) };
    Ok(slice)
}

fn sys_open(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (path, _access_mode, _flags, ..) = verify_args! {
        sys_arg!(0, all_state.rest => sys_arg_to_str(*const u8)),
        sys_arg!(1, all_state.rest => u64),
        sys_arg!(2, all_state.rest => u64),
    };
    // TODO: implement flags and access_mode, for now just open file for reading
    let file = fs::open(path).map_err(|_| SyscallError::CouldNotOpenFile)?;
    let file_index = with_current_process(|process| process.push_file(file));

    SyscallResult::Ok(file_index as u64)
}

fn sys_write(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (file_index, buf, size, ..) = verify_args! {
        sys_arg!(0, all_state.rest => u64),
        sys_arg!(1, all_state.rest => *const u8),
        sys_arg!(2, all_state.rest => u64),
    };
    let buf = sys_arg_to_byte_slice(buf, size).map_err(|err| to_arg_err!(0, err))?;
    let bytes_written = with_current_process(|process| {
        let file = process
            .get_file(file_index as _)
            .ok_or(SyscallError::InvalidFileIndex)?;

        file.write(buf)
            .map_err(|_| SyscallError::CouldNotWriteToFile)
    })?;
    SyscallResult::Ok(bytes_written as u64)
}

fn sys_read(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (file_index, buf, size, ..) = verify_args! {
        sys_arg!(0, all_state.rest => u64),
        sys_arg!(1, all_state.rest => *mut u8),
        sys_arg!(2, all_state.rest => u64),
    };
    let buf = sys_arg_to_mut_byte_slice(buf, size).map_err(|err| to_arg_err!(0, err))?;
    let bytes_read = with_current_process(|process| {
        let file = process
            .get_file(file_index as _)
            .ok_or(SyscallError::InvalidFileIndex)?;

        file.read(buf)
            .map_err(|_| SyscallError::CouldNotReadFromFile)
    })?;
    SyscallResult::Ok(bytes_read as u64)
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
