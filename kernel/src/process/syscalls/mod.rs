use core::{ffi::CStr, mem};

use alloc::{string::String, vec::Vec};
use kernel_user_link::{
    process::SpawnFileMapping,
    sys_arg,
    syscalls::{
        syscall_arg_to_u64, syscall_handler_wrapper, SyscallArgError, SyscallError, SyscallResult,
        NUM_SYSCALLS,
    },
    to_arg_err, verify_args, FD_STDERR,
};

use crate::{
    cpu::idt::InterruptAllSavedState,
    devices,
    executable::elf::Elf,
    fs::{self, FileSystemError},
    memory_management::memory_layout::{is_aligned, PAGE_4K},
    process::{scheduler, Process},
};

use super::scheduler::{exit_current_process, with_current_process};

type Syscall = fn(&mut InterruptAllSavedState) -> SyscallResult;

const SYSCALLS: [Syscall; NUM_SYSCALLS] = [
    sys_open,        // kernel_user_link::syscalls::SYS_OPEN
    sys_write,       // kernel_user_link::syscalls::SYS_WRITE
    sys_read,        // kernel_user_link::syscalls::SYS_READ
    sys_exit,        // kernel_user_link::syscalls::SYS_EXIT
    sys_spawn,       // kernel_user_link::syscalls::SYS_SPAWN
    sys_inc_heap,    // kernel_user_link::syscalls::SYS_INC_HEAP
    sys_create_pipe, // kernel_user_link::syscalls::SYS_CREATE_PIPE
    sys_wait_pid,    // kernel_user_link::syscalls::SYS_WAIT_PID
];

impl From<FileSystemError> for SyscallError {
    fn from(e: FileSystemError) -> Self {
        match e {
            FileSystemError::InvalidPath => SyscallError::CouldNotOpenFile,
            FileSystemError::FileNotFound => SyscallError::FileNotFound,
            FileSystemError::ReadNotSupported => SyscallError::CouldNotReadFromFile,
            FileSystemError::WriteNotSupported => SyscallError::CouldNotWriteToFile,
            FileSystemError::EndOfFile => SyscallError::EndOfFile,
            FileSystemError::IsNotDirectory
            | FileSystemError::IsDirectory
            | FileSystemError::DeviceNotFound => todo!(),
            FileSystemError::DiskReadError { .. }
            | FileSystemError::InvalidOffset
            | FileSystemError::FatError(_)
            | FileSystemError::InvalidData
            | FileSystemError::PartitionTableNotFound => panic!("should not happen?"),
        }
    }
}

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

/// Allocates space fro the mapping and copies them
fn sys_arg_to_file_mappings_array(
    array_ptr: *const u8,
    array_size: usize,
) -> Result<Vec<SpawnFileMapping>, SyscallArgError> {
    if array_size == 0 {
        return Ok(Vec::new());
    }
    check_ptr(
        array_ptr,
        (array_size * mem::size_of::<SpawnFileMapping>()) as u64,
    )?;

    let array_ptr = array_ptr as *const SpawnFileMapping;

    let mut array: Vec<SpawnFileMapping> = Vec::new();
    for i in 0..array_size {
        let mapping = unsafe { array_ptr.add(i).read() };

        // before doing push check that we don't have duplicates
        if array
            .iter()
            .any(|m| m.src_fd == mapping.src_fd || m.dst_fd == mapping.dst_fd)
        {
            return Err(SyscallArgError::DuplicateFileMappings);
        }

        array.push(mapping);
    }

    Ok(array)
}

fn sys_open(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (path, _access_mode, flags, ..) = verify_args! {
        sys_arg!(0, all_state.rest => sys_arg_to_str(*const u8)),
        sys_arg!(1, all_state.rest => u64),
        sys_arg!(2, all_state.rest => u64),
    };
    let blocking_mode = kernel_user_link::file::parse_flags(flags)
        .ok_or(to_arg_err!(2, SyscallArgError::GeneralInvalid))?;
    // TODO: implement flags and access_mode, for now just open file for reading
    let file = fs::open_blocking(path, blocking_mode)?;
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
    let bytes_written = with_current_process(|process| -> Result<u64, SyscallError> {
        let file = process
            .get_file(file_index as _)
            .ok_or(SyscallError::InvalidFileIndex)?;

        file.write(buf).map_err(|e| e.into())
    })?;
    SyscallResult::Ok(bytes_written)
}

fn sys_read(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (file_index, buf, size, ..) = verify_args! {
        sys_arg!(0, all_state.rest => u64),
        sys_arg!(1, all_state.rest => *mut u8),
        sys_arg!(2, all_state.rest => u64),
    };
    let buf = sys_arg_to_mut_byte_slice(buf, size).map_err(|err| to_arg_err!(0, err))?;

    // TODO: fix this hack
    //
    // So, that's this about?
    // We want to read files in blocking mode, and some of these, for example the `/console` file
    // relies on the keyboard interrupts, but while we are in `with_current_process` we don't get interrupts
    // because we are inside a lock.
    // So instead, we take the file out, read from it, and put it back
    // this is only done for files that are blocking, otherwise we just read from it directly.
    //
    // This is a big issue because when threads come in view later, since reading from another thread will report that
    // the file is not found which is not correct.
    //
    // A good solution would be to have waitable objects.
    let (bytes_read, file) = with_current_process(|process| {
        let file = process
            .get_file(file_index as _)
            .ok_or(SyscallError::InvalidFileIndex)?;
        if file.is_blocking() {
            // take file now
            let file = process
                .take_file(file_index as _)
                .ok_or(SyscallError::InvalidFileIndex)?;
            Ok((0, Some(file)))
        } else {
            let bytes_read = file.read(buf)?;
            Ok::<_, SyscallError>((bytes_read, None))
        }
    })?;

    let bytes_read = if let Some(mut file) = file {
        let bytes_read = file.read(buf)?;
        // put file back
        with_current_process(|process| process.put_file(file_index as _, file));
        bytes_read
    } else {
        bytes_read
    };
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
    let (path, argv, file_mappings, file_mappings_size, ..) = verify_args! {
        sys_arg!(0, all_state.rest => sys_arg_to_str(*const u8)),
        sys_arg!(1, all_state.rest => *const u8),   // array of pointers
        sys_arg!(2, all_state.rest => *const u8),   // array of mappings or null
        sys_arg!(3, all_state.rest => u64),       // size of the array
    };
    let argv = sys_arg_to_str_array(argv).map_err(|err| to_arg_err!(1, err))?;
    let file_mappings = sys_arg_to_file_mappings_array(file_mappings, file_mappings_size as usize)
        .map_err(|err| to_arg_err!(2, err))?;

    // don't go into lock if no need to
    if !file_mappings.is_empty() {
        // a bit unoptimal, but check all files first before taking them and doing any action
        with_current_process(|process| {
            for mapping in &file_mappings {
                process
                    .get_file(mapping.src_fd as _)
                    .ok_or(SyscallError::InvalidFileIndex)?;
            }
            Ok::<_, SyscallError>(())
        })?;
    }

    let mut file = fs::open(path).map_err(|_| SyscallError::CouldNotOpenFile)?;
    let elf = Elf::load(&mut file).map_err(|_| SyscallError::CouldNotLoadElf)?;
    let current_pid = with_current_process(|process| process.id);
    let mut new_process = Process::allocate_process(current_pid, &elf, &mut file, argv)
        .map_err(|_| SyscallError::CouldNotAllocateProcess)?;

    let mut std_needed = [true; 3];
    with_current_process(|process| {
        // take the files if any
        for mapping in file_mappings.iter() {
            let file = process
                .take_file(mapping.src_fd as _)
                .ok_or(SyscallError::InvalidFileIndex)?;
            new_process.attach_file_to_fd(mapping.dst_fd as _, file);
            if mapping.dst_fd as usize <= FD_STDERR {
                std_needed[mapping.dst_fd as usize] = false;
            }
        }

        // inherit files STD files if not set
        for (i, _) in std_needed.iter().enumerate().filter(|(_, &b)| b) {
            let file = process
                .get_file(i as _)
                .ok_or(SyscallError::InvalidFileIndex)?;
            let inherited_file = file.clone_inherit();
            new_process.attach_file_to_fd(i as _, inherited_file);
        }

        Ok::<_, SyscallError>(())
    })?;

    let new_pid = new_process.id();
    // make sure fds are setup correctly
    new_process.finish_stdio();
    scheduler::push_process(new_process);

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

fn sys_create_pipe(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (read_fd_ptr, write_fd_ptr, ..) = verify_args! {
        sys_arg!(0, all_state.rest => *mut u64),
        sys_arg!(1, all_state.rest => *mut u64),
    };
    check_ptr(read_fd_ptr as *const u8, 8).map_err(|err| to_arg_err!(0, err))?;
    check_ptr(write_fd_ptr as *const u8, 8).map_err(|err| to_arg_err!(1, err))?;

    let (read_file, write_file) = devices::pipe::create_pipe_pair();
    let (read_fd, write_fd) = with_current_process(|process| {
        (process.push_file(read_file), process.push_file(write_file))
    });

    unsafe {
        *read_fd_ptr = read_fd as u64;
        *write_fd_ptr = write_fd as u64;
    }

    SyscallResult::Ok(0)
}

fn sys_wait_pid(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (pid, ..) = verify_args! {
        sys_arg!(0, all_state.rest => u64),
    };

    // see if this is a child process
    let process_exit = with_current_process(|process| process.get_child_exit(pid));
    if let Some(exit_code) = process_exit {
        return SyscallResult::Ok(exit_code as u64);
    }

    // if not, wait for it
    // this stash the current process until the other process exits
    if !scheduler::wait_for_pid(all_state, pid) {
        return Err(SyscallError::PidNotFound);
    }
    // if we are waiting by the scheduler, this result is not important since it will be overwritten
    // when we get back
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

    crate::scheduler::yield_current_if_any(all_state);
}
