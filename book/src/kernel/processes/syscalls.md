{{ #include ../../links.md }}

# Syscalls

> This is implemented in [`syscalls`][syscalls]

The numbers of the syscalls and some metadata like `SyscallResult` and `SyscallError` are defined in [kernel user link](../../extra/kernel_user_link.md) crate.

Syscalls specs is as follows:
- Syscalls can have up to `7` arguments, and are passed in `RCX`, `RDX`, `RSI`, `RDI`, `R8`, `R9`, `R10` in order.
- The syscall number is passed in `RAX`.
- The syscall return value is passed in `RAX`, the syscall may write to pointers passed in the other registers, but the result code will still be in `RAX`.
- The returned `RAX` value is an encoded value of `SyscallResult`. So, its **never** intended to be read as `u64` directly.
- The arguments are not modified by the syscall, but the syscall may read from them, or write to memory pointed by them.
- All pointers passed to the syscall are have to be valid, and point to user space memory only, the kernel will check that the memory is mapped
and write to it, but it doesn't guarantee the validity of the memory if it was modified by the kernel (i.e. if the memory was pointed to random part in the heap it could corrupt the heap for example).
- The syscall may block execution depend on the syscall itself, like `wait_pid` or a `read` to a blocking file with no data.

## Syscalls list

This shows the list of syscalls and their arguments and return values. Some notes on this:
- `a: *const/*mut T, b: usize` will be used in the kernel as a slice, for example `write(file_index: usize, buf: &[u8])`. But I didn't want to remove an argument
and confuse the reader, so I split them into two as being sent from userspace.
- types like `&Path`, `BlockingMode`, `SpawnFileMapping`, etc... are passed as `u64` as is everything, but the kernel checks validity and cast/parse
these pointers/values to the correct types.
- All the return types are `SyscallResult` and will report any error during execution, for simplicity, I didn't just repeat that in the table.
- When the return type is `()`, it means the kernel will return `SyscallResult::Ok(0)`, the userspace will check that its `0`.

| Name | Arguments | Return value | Description |
| ---- | --------- | ------------ | ----------- |
| `open` | `path: &Path, access_mode: u64, mode: u64` | `file_index: usize` | Opens a file |
| `write` | `file_index: usize, buf: *const u8, size: usize` | `bytes_written: usize` | Writes to a file |
| `read` | `file_index: usize, buf: *mut u8, size: usize` | `bytes_read: usize` | Reads from a file |
| `close` | `file_index: usize` | `()` | Closes a file |
| `blocking_mode` | `file_index: usize, blocking_mode: BlockingMode` | `()` | Sets the blocking mode of a file. This is **DEPRECATED**, and should be replaced with `set_file_meta` with [`FileMeta::BlockingMode`](https://docs.rs/emerald_kernel_user_link/0.2.1/emerald_kernel_user_link/file/enum.FileMeta.html) |
| `exit` | `exit_code: i32` | `!` | Exits the current process |
| `spawn` | `path: &Path, argv: *const *const u8, file_mappings: *const SpawnFileMapping, file_mappings_size: usize` | `pid: u64` | Spawns a new process |
| `inc_heap` | `increment: i64` | `old_heap_end: usize` | Increase/decrease the heap of the current process (similar `sbrk`) |
| `create_pipe` | `read_fd: *mut usize, write_fd: *mut usize` | `()` | Creates a pipe |
| `wait_pid` | `pid: u64, block: bool` | `exit_code: i32` | Waits for a process to exit |
| `stat` | `path: &Path, stat: *mut FileStat` | `()` | Gets the file stat of a file |
| `open_dir` | `path: &Path` | `dir_index: usize` | Opens a directory |
| `read_dir` | `dir_index: usize, buf: *mut DirEntry, len: usize` | `entries_read: usize` | Reads from a directory |
| `get_cwd` | `buf: *mut u8, len: usize` | `needed_bytes: usize` | Gets the current working directory, returns fferTooSmall` if the buffer is too small |
| `chdir` | `path: &Path` | `()` | Changes the current working directory |
| `set_file_meta` | `file_index: usize, meta_id: u64, meta_data: u64` | `()` | Sets the file meta |
| `get_file_meta` | `file_index: usize, meta_id: u64, meta_data: *mut u64` | `()` | Gets the file meta |
| `sleep` | `seconds: u64, nanos: u64` | `()` | Sleeps for a duration |
| `get_time` | `clock_type: ClockType, time: *mut ClockTime` | `()` | Gets the time based on the `clock_type`, see [Clocks](../clocks/index.md) |
| `graphics` | `command: GraphicsCommand, extra: *mut ()` | `()` | Graphics operations, see [Graphics:VGA](../graphics/vga.md#graphics-command) |
| `seek` | `file_index: usize, whence: SeekWhence, offset: i64` | `new_offset: u64` | Seeks a file |
