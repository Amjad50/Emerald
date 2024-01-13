use core::ffi::CStr;

pub use kernel_user_link::file::BlockingMode;
pub use kernel_user_link::file::DirEntry;
pub use kernel_user_link::file::DirFilename;
pub use kernel_user_link::file::FileStat;
pub use kernel_user_link::file::FileType;
pub use kernel_user_link::file::MAX_FILENAME_LEN;
pub use kernel_user_link::FD_STDERR;
pub use kernel_user_link::FD_STDIN;
pub use kernel_user_link::FD_STDOUT;

use kernel_user_link::call_syscall;
use kernel_user_link::syscalls::SyscallError;
use kernel_user_link::syscalls::SYS_BLOCKING_MODE;
use kernel_user_link::syscalls::SYS_CHDIR;
use kernel_user_link::syscalls::SYS_CLOSE;
use kernel_user_link::syscalls::SYS_CREATE_PIPE;
use kernel_user_link::syscalls::SYS_GET_CWD;
use kernel_user_link::syscalls::SYS_OPEN;
use kernel_user_link::syscalls::SYS_OPEN_DIR;
use kernel_user_link::syscalls::SYS_READ;
use kernel_user_link::syscalls::SYS_READ_DIR;
use kernel_user_link::syscalls::SYS_STAT;
use kernel_user_link::syscalls::SYS_WRITE;

/// # Safety
/// This function assumes that `fd` is a valid file descriptor.
/// And that `buf` is a valid buffer.
pub unsafe fn syscall_read(fd: usize, buf: &mut [u8]) -> Result<u64, SyscallError> {
    unsafe {
        call_syscall!(
            SYS_READ,
            fd,                      // fd
            buf.as_mut_ptr() as u64, // buf
            buf.len() as u64         // size
        )
    }
}

/// # Safety
/// This function assumes that `fd` is a valid file descriptor.
/// And that `buf` is a valid buffer.
pub unsafe fn syscall_write(fd: usize, buf: &[u8]) -> Result<u64, SyscallError> {
    unsafe {
        call_syscall!(
            SYS_WRITE,
            fd,                  // fd
            buf.as_ptr() as u64, // buf
            buf.len() as u64     // size
        )
    }
}

/// # Safety
/// This function assumes that `path` is a valid C string.
/// And that `access_mode` and `flags` are valid.
pub unsafe fn syscall_open(
    path: &CStr,
    access_mode: usize,
    flags: usize,
) -> Result<usize, SyscallError> {
    unsafe {
        call_syscall!(
            SYS_OPEN,
            path.as_ptr() as u64, // path
            access_mode as u64,   // access_mode
            flags as u64          // flags
        )
        .map(|fd| fd as usize)
    }
}

/// # Safety
/// This function assumes that `fd` is a valid file descriptor.
pub unsafe fn syscall_close(fd: usize) -> Result<(), SyscallError> {
    unsafe {
        call_syscall!(
            SYS_CLOSE,
            fd,                  // fd
        )
        .map(|e| assert!(e == 0))
    }
}

/// # Safety
/// This function creates a pipe and return the descriptors.
/// Callers must ensure to use the descriptors correctly.
pub unsafe fn syscall_create_pipe() -> Result<(usize, usize), SyscallError> {
    let mut in_fd: u64 = 0;
    let mut out_fd: u64 = 0;
    unsafe {
        call_syscall!(
            SYS_CREATE_PIPE,
            &mut in_fd as *mut u64 as u64,  // in_fd
            &mut out_fd as *mut u64 as u64  // out_fd
        )?
    };

    Ok((in_fd as usize, out_fd as usize))
}

/// # Safety
/// This function assumes that `fd` is a valid file descriptor.
pub unsafe fn syscall_blocking_mode(
    fd: usize,
    blocking_mode: BlockingMode,
) -> Result<(), SyscallError> {
    let mode = blocking_mode.to_u64();
    unsafe {
        call_syscall!(
            SYS_BLOCKING_MODE,
            fd,   // fd
            mode  // mode
        )
        .map(|e| assert!(e == 0))
    }
}

/// # Safety
/// This function assumes that `path` is a valid C string.
/// Also assume `stat` is a valid pointer to a valid `FileStat` struct.
pub unsafe fn syscall_stat(path: &CStr, stat: &mut FileStat) -> Result<(), SyscallError> {
    let stat_ptr = stat as *mut FileStat as u64;
    unsafe {
        call_syscall!(
            SYS_STAT,
            path.as_ptr() as u64, // path
            stat_ptr              // stat_ptr
        )
        .map(|e| assert!(e == 0))
    }
}

/// # Safety
/// This function assumes that `path` is a valid C string.
pub unsafe fn syscall_open_dir(path: &CStr) -> Result<usize, SyscallError> {
    unsafe {
        call_syscall!(
            SYS_OPEN_DIR,
            path.as_ptr() as u64, // path
        )
        .map(|fd| fd as usize)
    }
}

/// # Safety
/// This function assumes that `fd` is a valid file descriptor.
/// Also assume `entry` is a valid pointer to a valid `DirEntry` struct.
pub unsafe fn syscall_read_dir(fd: usize, entries: &mut [DirEntry]) -> Result<usize, SyscallError> {
    let entries_ptr = entries.as_mut_ptr() as u64;
    unsafe {
        call_syscall!(
            SYS_READ_DIR,
            fd,                   // fd
            entries_ptr,          // entries_ptr
            entries.len() as u64  // len
        )
        .map(|written| written as usize)
    }
}

/// # Safety
/// This function assumes that `path` is a valid C string.
pub unsafe fn syscall_chdir(path: &CStr) -> Result<(), SyscallError> {
    unsafe {
        call_syscall!(
            SYS_CHDIR,
            path.as_ptr() as u64, // path
        )
        .map(|e| assert!(e == 0))
    }
}

/// # Safety
/// This function assumes that `path` is a valid buffer.
/// The result will be a string written in the buffer, NULL won't be written, but the written length will be returned
pub unsafe fn syscall_get_cwd(path: &mut [u8]) -> Result<usize, SyscallError> {
    unsafe {
        call_syscall!(
            SYS_GET_CWD,
            path.as_mut_ptr() as u64, // path buffer
            path.len() as u64         // len
        )
        .map(|written| written as usize)
    }
}
