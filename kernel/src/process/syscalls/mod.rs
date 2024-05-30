use core::{ffi::CStr, mem};

use alloc::{borrow::Cow, string::String, vec::Vec};
use kernel_user_link::{
    clock::ClockType,
    file::{BlockingMode, DirEntry, FileMeta, OpenOptions, SeekFrom, SeekWhence},
    graphics::{BlitCommand, FrameBufferInfo, GraphicsCommand},
    process::{PriorityLevel, SpawnFileMapping},
    sys_arg,
    syscalls::{
        syscall_arg_to_u64, syscall_handler_wrapper, SyscallArgError, SyscallError, SyscallResult,
        NUM_SYSCALLS,
    },
    to_arg_err, verify_args, FD_STDERR,
};

use crate::{
    cpu::{self, idt::InterruptAllSavedState},
    devices::{self, clock},
    executable::elf::Elf,
    fs::{self, path::Path, FileSystemError},
    graphics,
    memory_management::memory_layout::{is_aligned, PAGE_4K},
    process::{scheduler, Process},
};

use super::scheduler::{
    exit_current_process, sleep_current_process, with_current_process, with_process,
};

type Syscall = fn(&mut InterruptAllSavedState) -> SyscallResult;

const SYSCALLS: [Syscall; NUM_SYSCALLS] = [
    sys_open,          // kernel_user_link::syscalls::SYS_OPEN
    sys_write,         // kernel_user_link::syscalls::SYS_WRITE
    sys_read,          // kernel_user_link::syscalls::SYS_READ
    sys_close,         // kernel_user_link::syscalls::SYS_CLOSE
    sys_blocking_mode, // kernel_user_link::syscalls::SYS_BLOCKING_MODE
    sys_exit,          // kernel_user_link::syscalls::SYS_EXIT
    sys_spawn,         // kernel_user_link::syscalls::SYS_SPAWN
    sys_inc_heap,      // kernel_user_link::syscalls::SYS_INC_HEAP
    sys_create_pipe,   // kernel_user_link::syscalls::SYS_CREATE_PIPE
    sys_wait_pid,      // kernel_user_link::syscalls::SYS_WAIT_PID
    sys_stat,          // kernel_user_link::syscalls::SYS_STAT
    sys_open_dir,      // kernel_user_link::syscalls::SYS_OPEN_DIR
    sys_read_dir,      // kernel_user_link::syscalls::SYS_READ_DIR
    sys_get_cwd,       // kernel_user_link::syscalls::SYS_GET_CWD
    sys_chdir,         // kernel_user_link::syscalls::SYS_CHDIR
    sys_set_file_meta, // kernel_user_link::syscalls::SYS_SET_FILE_META
    sys_get_file_meta, // kernel_user_link::syscalls::SYS_GET_FILE_META
    sys_sleep,         // kernel_user_link::syscalls::SYS_SLEEP
    sys_get_time,      // kernel_user_link::syscalls::SYS_GET_TIME
    sys_graphics,      // kernel_user_link::syscalls::SYS_GRAPHICS
    sys_seek,          // kernel_user_link::syscalls::SYS_SEEK
    sys_priority,      // kernel_user_link::syscalls::SYS_PRIORITY
];

impl From<FileSystemError> for SyscallError {
    fn from(e: FileSystemError) -> Self {
        match e {
            FileSystemError::InvalidPath => SyscallError::CouldNotOpenFile,
            FileSystemError::FileNotFound => SyscallError::FileNotFound,
            FileSystemError::ReadNotSupported => SyscallError::CouldNotReadFromFile,
            FileSystemError::WriteNotSupported | FileSystemError::CouldNotSetFileLength => SyscallError::CouldNotWriteToFile,
            FileSystemError::EndOfFile => SyscallError::EndOfFile,
            FileSystemError::IsNotDirectory => SyscallError::IsNotDirectory,
            FileSystemError::IsDirectory => SyscallError::IsDirectory,
            FileSystemError::AlreadyExists => SyscallError::AlreadyExists,
            FileSystemError::BufferNotLargeEnough(_) => SyscallError::BufferTooSmall,
            FileSystemError::OperationNotSupported => SyscallError::OperationNotSupported,
            FileSystemError::DiskReadError { .. }
            | FileSystemError::FatError(_)
            | FileSystemError::DeviceNotFound
            | FileSystemError::MustBeAbsolute   // should not happen from user mode
            | FileSystemError::PartitionTableNotFound => panic!("should not happen?"),
        }
    }
}

impl From<clock::ClockTime> for kernel_user_link::clock::ClockTime {
    fn from(time: clock::ClockTime) -> Self {
        assert!(time.nanoseconds < clock::NANOS_PER_SEC);
        Self {
            seconds: time.seconds,
            nanoseconds: time.nanoseconds as u32,
        }
    }
}

#[inline]
fn check_ptr(arg: *const u8, len: usize) -> Result<(), SyscallArgError> {
    if arg.is_null() {
        return Err(SyscallArgError::InvalidUserPointer);
    }
    if !with_current_process(|process| {
        process.is_user_address_mapped(arg as _)
        // very basic check, just check the last byte
        // TODO: check all mapped pages
            && process.is_user_address_mapped(arg as usize + len - 1 )
    }) {
        return Err(SyscallArgError::InvalidUserPointer);
    }
    Ok(())
}

#[inline]
fn ptr_as_mut<T>(ptr: *mut u8) -> Result<*mut T, SyscallArgError> {
    check_ptr(ptr as *const u8, mem::size_of::<T>())?;
    Ok(ptr as *mut T)
}

#[inline]
fn ptr_as_ref<T>(ptr: *const u8) -> Result<*const T, SyscallArgError> {
    check_ptr(ptr, mem::size_of::<T>())?;
    Ok(ptr as *const T)
}

// expects null terminated string
fn sys_arg_to_str<'a>(arg: *const u8) -> Result<&'a str, SyscallArgError> {
    check_ptr(arg, 1)?;

    let slice = unsafe { CStr::from_ptr(arg as _) };
    let string = CStr::to_str(slice).map_err(|_| SyscallArgError::NotValidUtf8)?;
    Ok(string)
}

fn sys_arg_to_path<'a>(arg: *const u8) -> Result<&'a Path, SyscallArgError> {
    sys_arg_to_str(arg).map(Path::new)
}

fn sys_arg_to_slice<'a, T: Sized>(buf: *const u8, len: usize) -> Result<&'a [T], SyscallArgError> {
    if len == 0 {
        return Ok(&[]);
    }

    check_ptr(buf, len * mem::size_of::<T>())?;

    let slice = unsafe { core::slice::from_raw_parts(buf as _, len) };
    Ok(slice)
}

fn sys_arg_to_mut_slice<'a, T: Sized>(
    buf: *mut u8,
    len: usize,
) -> Result<&'a mut [T], SyscallArgError> {
    if len == 0 {
        return Ok(&mut []);
    }

    check_ptr(buf, len * mem::size_of::<T>())?;

    let slice = unsafe { core::slice::from_raw_parts_mut(buf as _, len) };
    Ok(slice)
}

/// Allocates space for the strings and copies them
fn sys_arg_to_str_array(array_ptr: *const u8) -> Result<Vec<String>, SyscallArgError> {
    let array_ptr = ptr_as_ref::<*const u8>(array_ptr)?;

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
fn sys_arg_to_file_mappings_array<'a>(
    array_ptr: *const u8,
    array_size: usize,
) -> Result<&'a [SpawnFileMapping], SyscallArgError> {
    let mappings_array = sys_arg_to_slice::<SpawnFileMapping>(array_ptr, array_size)?;

    for i in 0..array_size {
        let mapping = mappings_array[i];

        // before doing push check that we don't have duplicates
        for other_mapping in mappings_array.iter().take(i) {
            if mapping.src_fd == other_mapping.src_fd || mapping.dst_fd == other_mapping.dst_fd {
                return Err(SyscallArgError::DuplicateFileMappings);
            }
        }
    }

    Ok(mappings_array)
}

/// Get the absolute path, if the `path` is relative, it will use the current process working directory to get the absolute path.
/// If the `path` is absolute, it will return it as is.
fn path_to_proc_absolute_path(path: &Path) -> Cow<'_, Path> {
    let absolute_path = if path.is_absolute() {
        Cow::Borrowed(path)
    } else {
        let current_dir =
            with_current_process(|process| process.get_current_dir().path().to_path_buf());
        Cow::Owned(current_dir.join(path))
    };
    assert!(absolute_path.is_absolute());

    absolute_path
}

fn sys_open(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (path, open_options, flags, ..) = verify_args! {
        sys_arg!(0, all_state.rest => sys_arg_to_path(*const u8)),
        sys_arg!(1, all_state.rest => u64),
        sys_arg!(2, all_state.rest => u64),
    };

    let open_options = OpenOptions::from_u64(open_options)
        .ok_or(to_arg_err!(1, SyscallArgError::GeneralInvalid))?;

    let blocking_mode = kernel_user_link::file::parse_flags(flags)
        .ok_or(to_arg_err!(2, SyscallArgError::GeneralInvalid))?;

    let absolute_path = path_to_proc_absolute_path(path);
    let file = fs::File::open_blocking(absolute_path, blocking_mode, open_options)?;
    let file_index = with_current_process(|process| process.push_fs_node(file));

    SyscallResult::Ok(file_index as u64)
}

fn sys_write(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (file_index, buf, size, ..) = verify_args! {
        sys_arg!(0, all_state.rest => usize),
        sys_arg!(1, all_state.rest => *const u8),
        sys_arg!(2, all_state.rest => usize),
    };
    let buf = sys_arg_to_slice(buf, size).map_err(|err| to_arg_err!(0, err))?;
    let bytes_written = with_current_process(|process| -> Result<u64, SyscallError> {
        let file = process
            .get_fs_node(file_index)
            .ok_or(SyscallError::InvalidFileIndex)?;

        file.as_file_mut()?.write(buf).map_err(|e| e.into())
    })?;
    SyscallResult::Ok(bytes_written)
}

fn sys_read(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (file_index, buf, size, ..) = verify_args! {
        sys_arg!(0, all_state.rest => usize),
        sys_arg!(1, all_state.rest => *mut u8),
        sys_arg!(2, all_state.rest => usize),
    };
    let buf = sys_arg_to_mut_slice(buf, size).map_err(|err| to_arg_err!(0, err))?;

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
            .get_fs_node(file_index)
            .ok_or(SyscallError::InvalidFileIndex)?
            .as_file_mut()?;
        if file.is_blocking() {
            // take file now
            let file = process
                .take_fs_node(file_index)
                .ok_or(SyscallError::InvalidFileIndex)?;
            Ok((0, Some(file)))
        } else {
            let bytes_read = file.read(buf)?;
            Ok::<_, SyscallError>((bytes_read, None))
        }
    })?;

    let bytes_read = if let Some(mut file) = file {
        let bytes_read = file.as_file_mut()?.read(buf)?;
        // put file back
        with_current_process(|process| process.put_fs_node(file_index, file));
        bytes_read
    } else {
        bytes_read
    };
    SyscallResult::Ok(bytes_read)
}

fn sys_close(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (file_index, ..) = verify_args! {
        sys_arg!(0, all_state.rest => usize),
    };

    with_current_process(|process| {
        process
            .take_fs_node(file_index)
            .ok_or(SyscallError::InvalidFileIndex)?;
        Ok::<_, SyscallError>(())
    })?;

    SyscallResult::Ok(0)
}

fn sys_blocking_mode(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (file_index, blocking_mode, ..) = verify_args! {
        sys_arg!(0, all_state.rest => usize),
        sys_arg!(1, all_state.rest => u64),
    };

    let blocking_mode = BlockingMode::try_from(blocking_mode)
        .map_err(|_| to_arg_err!(1, SyscallArgError::GeneralInvalid))?;

    with_current_process(|process| {
        let file = process
            .get_fs_node(file_index)
            .ok_or(SyscallError::InvalidFileIndex)?;
        file.as_file_mut()?.set_blocking(blocking_mode);
        Ok::<_, SyscallError>(())
    })?;

    SyscallResult::Ok(0)
}

fn sys_exit(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (exit_code, ..) = verify_args! {
        sys_arg!(0, all_state.rest => i32),
    };

    // modify the all_state to go back to the kernel, the current all_state will be dropped
    exit_current_process(exit_code, all_state);
    SyscallResult::Ok(exit_code as u64)
}

fn sys_spawn(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (path, argv, file_mappings, file_mappings_size, ..) = verify_args! {
        sys_arg!(0, all_state.rest => sys_arg_to_path(*const u8)),
        sys_arg!(1, all_state.rest => *const u8),   // array of pointers
        sys_arg!(2, all_state.rest => *const u8),   // array of mappings or null
        sys_arg!(3, all_state.rest => usize),       // size of the array
    };
    let argv = sys_arg_to_str_array(argv).map_err(|err| to_arg_err!(1, err))?;
    let file_mappings = sys_arg_to_file_mappings_array(file_mappings, file_mappings_size)
        .map_err(|err| to_arg_err!(2, err))?;

    // don't go into lock if no need to
    if !file_mappings.is_empty() {
        // a bit unoptimal, but check all files first before taking them and doing any action
        with_current_process(|process| {
            for mapping in file_mappings {
                process
                    .get_fs_node(mapping.src_fd)
                    .ok_or(SyscallError::InvalidFileIndex)?;
            }
            Ok::<_, SyscallError>(())
        })?;
    }

    let absolute_path = path_to_proc_absolute_path(path);

    let mut file = fs::File::open(absolute_path)?;
    let elf = Elf::load(&mut file).map_err(|_| SyscallError::CouldNotLoadElf)?;
    let (current_pid, current_dir) =
        with_current_process(|process| (process.id, process.get_current_dir().clone()));
    let mut new_process =
        Process::allocate_process(current_pid, &elf, &mut file, argv, current_dir)
            .map_err(|_| SyscallError::CouldNotAllocateProcess)?;

    let mut std_needed = [true; 3];
    with_current_process(|process| {
        // take the files if any
        for mapping in file_mappings.iter() {
            let file = process
                .take_fs_node(mapping.src_fd)
                .ok_or(SyscallError::InvalidFileIndex)?;
            new_process.attach_fs_node_to_fd(mapping.dst_fd, file);
            if mapping.dst_fd <= FD_STDERR {
                std_needed[mapping.dst_fd] = false;
            }
        }

        // inherit files STD files if not set
        for (i, _) in std_needed.iter().enumerate().filter(|(_, &b)| b) {
            let file = process
                .get_fs_node(i)
                .ok_or(SyscallError::InvalidFileIndex)?;
            let inherited_file = file.as_file()?.clone_inherit();
            new_process.attach_fs_node_to_fd(i, inherited_file);
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

    if !is_aligned(increment.unsigned_abs(), PAGE_4K) {
        return Err(to_arg_err!(0, SyscallArgError::InvalidHeapIncrement));
    }

    let old_heap_end = with_current_process(|process| process.add_to_heap(increment as isize))
        .ok_or(SyscallError::HeapRangesExceeded)?;

    SyscallResult::Ok(old_heap_end as u64)
}

fn sys_create_pipe(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (read_fd_ptr, write_fd_ptr, ..) = verify_args! {
        sys_arg!(0, all_state.rest => *mut usize),
        sys_arg!(1, all_state.rest => *mut usize),
    };
    let read_fd_ptr = ptr_as_mut(read_fd_ptr as *mut u8).map_err(|err| to_arg_err!(0, err))?;
    let write_fd_ptr = ptr_as_mut(write_fd_ptr as *mut u8).map_err(|err| to_arg_err!(1, err))?;

    let (read_file, write_file) = devices::pipe::create_pipe_pair();
    let (read_fd, write_fd) = with_current_process(|process| {
        (
            process.push_fs_node(read_file),
            process.push_fs_node(write_file),
        )
    });

    unsafe {
        *read_fd_ptr = read_fd;
        *write_fd_ptr = write_fd;
    }

    SyscallResult::Ok(0)
}

fn sys_wait_pid(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (pid, block, ..) = verify_args! {
        sys_arg!(0, all_state.rest => u64),
        sys_arg!(1, all_state.rest => u64),
    };
    let block = block != 0;

    // see if this is a child process
    let process_exit = with_current_process(|process| process.get_child_exit(pid));
    if let Some(exit_code) = process_exit {
        return SyscallResult::Ok(exit_code as u64);
    }

    if !block {
        if scheduler::is_process_running(pid) {
            return Err(SyscallError::ProcessStillRunning);
        }
        return Err(SyscallError::PidNotFound);
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

fn sys_stat(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (path, stat_ptr, ..) = verify_args! {
        sys_arg!(0, all_state.rest => sys_arg_to_path(*const u8)),
        sys_arg!(1, all_state.rest => *mut u8),
    };
    let stat_ptr = ptr_as_mut(stat_ptr).map_err(|err| to_arg_err!(1, err))?;

    let absolute_path = path_to_proc_absolute_path(path);
    let (_, inode) = fs::open_inode(absolute_path)?;

    unsafe {
        *stat_ptr = inode.as_file_stat();
    }

    SyscallResult::Ok(0)
}

fn sys_open_dir(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (path, ..) = verify_args! {
        sys_arg!(0, all_state.rest => sys_arg_to_path(*const u8)),
    };

    let absolute_path = path_to_proc_absolute_path(path);
    let dir = fs::Directory::open(absolute_path)?;
    let dir_index = with_current_process(|process| process.push_fs_node(dir));

    SyscallResult::Ok(dir_index as u64)
}

fn sys_read_dir(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (dir_index, buf, len, ..) = verify_args! {
        sys_arg!(0, all_state.rest => usize),
        sys_arg!(1, all_state.rest => *mut u8),
        sys_arg!(2, all_state.rest => usize),
    };
    let buf = sys_arg_to_mut_slice::<DirEntry>(buf, len).map_err(|err| to_arg_err!(1, err))?;

    let entries_read = with_current_process(|process| -> Result<usize, SyscallError> {
        let file = process
            .get_fs_node(dir_index)
            .ok_or(SyscallError::InvalidFileIndex)?;
        file.as_dir_mut()?.read(buf).map_err(|e| e.into())
    })?;

    SyscallResult::Ok(entries_read as u64)
}

fn sys_get_cwd(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (buf, len, ..) = verify_args! {
        sys_arg!(0, all_state.rest => *mut u8),
        sys_arg!(1, all_state.rest => usize),
    };
    let buf = sys_arg_to_mut_slice::<u8>(buf, len).map_err(|err| to_arg_err!(0, err))?;

    let needed_bytes = with_current_process(|process| -> Result<usize, SyscallError> {
        let cwd = process.get_current_dir().path();
        let needed_bytes = cwd.as_str().as_bytes().len();
        if needed_bytes > len {
            return Err(SyscallError::BufferTooSmall);
        }
        buf[..needed_bytes].copy_from_slice(cwd.as_str().as_bytes());
        Ok(needed_bytes)
    })?;

    SyscallResult::Ok(needed_bytes as u64)
}

fn sys_chdir(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (path, ..) = verify_args! {
        sys_arg!(0, all_state.rest => sys_arg_to_path(*const u8)),
    };

    let absolute_path = path_to_proc_absolute_path(path);
    let dir = fs::Directory::open(absolute_path)?;
    with_current_process(|process| process.set_current_dir(dir));

    SyscallResult::Ok(0)
}

fn sys_set_file_meta(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (file_index, meta_id, meta_data, ..) = verify_args! {
        sys_arg!(0, all_state.rest => usize),
        sys_arg!(1, all_state.rest => u64),
        sys_arg!(2, all_state.rest => u64),
    };

    let meta_op = FileMeta::try_from((meta_id, meta_data))
        .ok()
        .ok_or(to_arg_err!(1, SyscallArgError::GeneralInvalid))?;

    let op_on_file = |op: &dyn Fn(&mut fs::File)| {
        with_current_process(|process| {
            let file = process
                .get_fs_node(file_index)
                .ok_or(SyscallError::InvalidFileIndex)?;
            op(file.as_file_mut()?);
            Ok::<_, SyscallError>(())
        })
    };

    match meta_op {
        FileMeta::BlockingMode(blocking_mode) => {
            op_on_file(&|file| file.set_blocking(blocking_mode))?;
        }
        FileMeta::IsTerminal(is_terminal) => {
            op_on_file(&|file| file.set_terminal(is_terminal))?;
        }
        _ => {
            return Err(to_arg_err!(1, SyscallArgError::GeneralInvalid));
        }
    }

    SyscallResult::Ok(0)
}

fn sys_get_file_meta(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (file_index, meta_id, meta_data_ptr, ..) = verify_args! {
        sys_arg!(0, all_state.rest => usize),
        sys_arg!(1, all_state.rest => u64),
        sys_arg!(2, all_state.rest => *mut u64),
    };
    let meta_data_ptr = ptr_as_mut(meta_data_ptr as *mut u8).map_err(|err| to_arg_err!(2, err))?;

    let meta_op = FileMeta::try_from((meta_id, 0))
        .ok()
        .ok_or(to_arg_err!(1, SyscallArgError::GeneralInvalid))?;

    let data = with_current_process(|process| {
        let file = process
            .get_fs_node(file_index)
            .ok_or(SyscallError::InvalidFileIndex)?;

        let meta_data = match meta_op {
            FileMeta::BlockingMode(..) => file.as_file()?.blocking_mode().to_u64(),
            FileMeta::IsTerminal(..) => file.as_file()?.is_terminal() as u64,
            _ => {
                return Err(to_arg_err!(1, SyscallArgError::GeneralInvalid));
            }
        };

        Ok::<_, SyscallError>(meta_data)
    })?;

    // Safety: we checked that the pointer is valid
    unsafe { *meta_data_ptr = data }

    SyscallResult::Ok(0)
}

fn sys_sleep(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (seconds, nanoseconds, ..) = verify_args! {
        sys_arg!(0, all_state.rest => u64),
        sys_arg!(1, all_state.rest => u64),
    };

    if nanoseconds >= clock::NANOS_PER_SEC {
        return Err(to_arg_err!(1, SyscallArgError::InvalidNanoseconds));
    }

    let time = clock::ClockTime {
        seconds,
        nanoseconds,
    };

    // put the result manually, as we will go back to the kernel after the call below
    all_state.rest.rax = 0;

    // modify the all_state to go back to the kernel, the current all_state will be dropped
    sleep_current_process(time, all_state);

    // the result will be saved in kernel's all_state, so we should write the result we want before calling
    // `sleep_current_process`
    SyscallResult::Ok(0)
}

fn sys_get_time(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (time_type, time_ptr, ..) = verify_args! {
        sys_arg!(0, all_state.rest => u64),
        sys_arg!(1, all_state.rest => *mut u8),
    };
    let time_ptr: *mut kernel_user_link::clock::ClockTime =
        ptr_as_mut(time_ptr).map_err(|err| to_arg_err!(1, err))?;

    let time_type = ClockType::try_from(time_type)
        .map_err(|_| to_arg_err!(0, SyscallArgError::GeneralInvalid))?;

    let time = match time_type {
        ClockType::RealTime => clock::clocks().time_since_unix_epoch().into(),
        ClockType::SystemTime => clock::clocks().time_since_startup().into(),
    };
    // Safety: we checked that the pointer is valid
    unsafe {
        *time_ptr = time;
    }

    SyscallResult::Ok(0)
}

fn sys_graphics(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (command_id, extra, ..) = verify_args! {
        sys_arg!(0, all_state.rest => u64),
        sys_arg!(1, all_state.rest => *mut u8)
    };

    let command = GraphicsCommand::from_u64(command_id)
        .ok_or(to_arg_err!(0, SyscallArgError::GeneralInvalid))?;

    let pid = cpu::cpu().process_id;

    match command {
        GraphicsCommand::TakeOwnership => {
            if !graphics::vga::controller()
                .ok_or(SyscallError::GraphicsNotAvailable)?
                .take_ownership(pid)
            {
                return Err(SyscallError::GraphicsAlreadyTaken);
            }
        }
        GraphicsCommand::ReleaseOwnership => {
            if !graphics::vga::controller()
                .ok_or(SyscallError::GraphicsNotAvailable)?
                .release(pid)
            {
                return Err(SyscallError::GraphicsNotOwned);
            }
        }
        GraphicsCommand::GetFrameBufferInfo => {
            let info = *graphics::vga::controller()
                .ok_or(SyscallError::GraphicsNotAvailable)?
                .framebuffer_info();
            let info_ptr =
                ptr_as_mut::<FrameBufferInfo>(extra).map_err(|err| to_arg_err!(1, err))?;
            // Safety: we checked that the pointer is valid
            unsafe {
                *info_ptr = info;
            }
        }
        GraphicsCommand::Blit => {
            let blit = ptr_as_ref::<BlitCommand>(extra).map_err(|err| to_arg_err!(1, err))?;
            // Safety: we checked that the pointer is valid
            let blit = unsafe { *blit };

            let buffer_len = blit.src_framebuffer_info.memory_size();
            let buffer = sys_arg_to_slice(blit.memory, buffer_len)
                .map_err(|_| SyscallError::InvalidGraphicsBuffer)?;

            graphics::vga::controller()
                .ok_or(SyscallError::GraphicsNotAvailable)?
                .lock_process(pid)
                .ok_or(SyscallError::GraphicsNotOwned)?
                .blit(
                    buffer,
                    &blit.src_framebuffer_info,
                    blit.src,
                    blit.dst,
                    blit.size.0,
                    blit.size.1,
                );
        }
        c => panic!("invalid graphics command {c:?}"),
    }

    SyscallResult::Ok(0)
}

fn sys_seek(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (file_index, whence, offset, ..) = verify_args! {
        sys_arg!(0, all_state.rest => usize),
        sys_arg!(1, all_state.rest => u64),
        sys_arg!(2, all_state.rest => i64),
    };
    let seek = SeekFrom {
        whence: whence
            .try_into()
            .map_err(|_| to_arg_err!(1, SyscallArgError::GeneralInvalid))?,
        offset,
    };

    let new_position = with_current_process(|process| {
        let file = process
            .get_fs_node(file_index)
            .ok_or(SyscallError::InvalidFileIndex)?;
        let file = file.as_file_mut()?;

        let size = file.size();

        let new_location: u64 = match seek.whence {
            SeekWhence::Start => seek
                .offset
                .try_into()
                .map_err(|_| SyscallError::InvalidOffset)?,
            SeekWhence::Current => {
                let current: i64 = file.current_position().try_into().expect(
                    "current position should be positive and would be less than i64 bit in size",
                );
                current
                    .checked_add(seek.offset)
                    .ok_or(SyscallError::InvalidOffset)?
                    .try_into()
                    .map_err(|_| SyscallError::InvalidOffset)?
            }
            SeekWhence::End => {
                let end: i64 = size
                    .try_into()
                    .expect("size should be positive and would be less than i64 bit in size");
                end.checked_add(seek.offset)
                    .ok_or(SyscallError::InvalidOffset)?
                    .try_into()
                    .map_err(|_| SyscallError::InvalidOffset)?
            }
        };

        file.seek(new_location)?;

        Ok::<_, SyscallError>(new_location)
    })?;

    SyscallResult::Ok(new_position)
}

/// Set and Get process priority
/// TODO: implement security levels, as now we can change the priority of any process
fn sys_priority(all_state: &mut InterruptAllSavedState) -> SyscallResult {
    let (pid, priority_level, ..) = verify_args! {
        sys_arg!(0, all_state.rest => u64),
        sys_arg!(1, all_state.rest => u64),
    };

    // if its `None`, just get the value
    let priority_level = if priority_level == 0 {
        None
    } else {
        Some(
            PriorityLevel::from_u64(priority_level)
                .ok_or(to_arg_err!(1, SyscallArgError::GeneralInvalid))?,
        )
    };

    let current_priority = with_process(pid, |process| {
        if let Some(priority_level) = priority_level {
            process.set_priority(priority_level);
        }

        Ok::<_, SyscallError>(process.get_priority())
    })?;

    SyscallResult::Ok(current_priority.to_u64())
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
