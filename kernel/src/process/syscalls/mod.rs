use core::ffi::CStr;

use alloc::{string::String, vec::Vec};
use common::{
    sys_arg,
    syscalls::{
        syscall_arg_to_u64, syscall_handler_wrapper, SyscallArgError, SyscallError, SyscallResult,
        NUM_SYSCALLS,
    },
    to_arg_err, verify_args, FD_STDERR, FD_STDIN, FD_STDOUT,
};

use crate::{
    cpu::idt::InterruptAllSavedState,
    executable::elf::Elf,
    fs,
    memory_management::memory_layout::{is_aligned, PAGE_4K},
    process::{scheduler, Process},
};

use super::scheduler::{exit_current_process, with_current_process};

type Syscall = fn(&mut InterruptAllSavedState) -> SyscallResult;

const SYSCALLS: [Syscall; NUM_SYSCALLS] = [
    sys_open,     // common::syscalls::SYS_OPEN
    sys_write,    // common::syscalls::SYS_WRITE
    sys_read,     // common::syscalls::SYS_READ
    sys_exit,     // common::syscalls::SYS_EXIT
    sys_spawn,    // common::syscalls::SYS_SPAWN
    sys_inc_heap, // common::syscalls::SYS_INC_HEAP
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

// expects null terminated string
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

/// Allocates space for the strings and copies them
fn sys_arg_to_str_array(array_ptr: *const u8) -> Result<Vec<String>, SyscallArgError> {
    check_ptr(array_ptr, 8)?;
    let array_ptr = array_ptr as *const *const u8;

    let mut array = Vec::new();
    let mut i = 0;
    loop {
        let ptr = unsafe { *array_ptr.offset(i) };
        if ptr.is_null() {
            break;
        }
        let str = sys_arg_to_str(ptr)?;
        if str.is_empty() {
            break;
        }
        array.push(String::from(str));
        i += 1;
    }

    Ok(array)
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

fn sys_exit(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (exit_code, ..) = verify_args! {
        sys_arg!(0, all_state.rest => u64),
    };

    // modify the all_state to go back to the kernel, the current all_state will be dropped
    exit_current_process(exit_code, all_state);
    SyscallResult::Ok(exit_code)
}

fn sys_spawn(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (path, argv, ..) = verify_args! {
        sys_arg!(0, all_state.rest => sys_arg_to_str(*const u8)),
        sys_arg!(1, all_state.rest => u64),   // array of pointers
    };
    let argv = argv as *const u8;

    let argv = sys_arg_to_str_array(argv).map_err(|err| to_arg_err!(1, err))?;
    let mut file = fs::open(path).map_err(|_| SyscallError::CouldNotOpenFile)?;
    let elf = Elf::load(&mut file).map_err(|_| SyscallError::CouldNotLoadElf)?;
    let current_pid = with_current_process(|process| process.id);
    let mut process = Process::allocate_process(current_pid, &elf, &mut file, argv)
        .map_err(|_| SyscallError::CouldNotAllocateProcess)?;

    // TODO: setup inheritance for process files

    // add the console manually for now,
    let console = fs::open("/devices/console").expect("Could not find `/devices/console`");
    process.attach_file_to_fd(FD_STDIN, console.clone());
    process.attach_file_to_fd(FD_STDOUT, console.clone());
    process.attach_file_to_fd(FD_STDERR, console);

    let new_pid = process.id();
    scheduler::push_process(process);

    SyscallResult::Ok(new_pid)
}

fn sys_inc_heap(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (increment, ..) = verify_args! {
        sys_arg!(0, all_state.rest => i64),
    };

    if !is_aligned(increment.unsigned_abs() as usize, PAGE_4K) {
        return Err(to_arg_err!(0, SyscallArgError::InvalidHeapIncrement));
    }

    let old_heap_end = with_current_process(|process| process.add_to_heap(increment as isize))
        .ok_or(SyscallError::HeapRangesExceeded)?;

    SyscallResult::Ok(old_heap_end as u64)
}

pub fn handle_syscall(all_state: &mut InterruptAllSavedState) {
    let syscall_number = all_state.rest.rax;

    // `syscall_handler_wrapper` will check the syscall number and return error if it exceed the
    // number of syscalls (NUM_SYSCALLS)
    all_state.rest.rax = syscall_handler_wrapper(syscall_number, || {
        let syscall_func = SYSCALLS[syscall_number as usize];
        syscall_func(all_state)
    });

    crate::scheduler::yield_current_if_any(all_state);
}
