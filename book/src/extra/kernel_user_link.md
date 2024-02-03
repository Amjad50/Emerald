# Kernel user link

This is a crate that contains common definitions for the kernel and the user space. And it is implemented in [`amjad_os_kernel_user_link`] crate.

It contains common definitions such as:
- `syscall` numbers.
- `SyscallResult`, and `SyscallError` types.
- some `process` related arguments:
    - `SpawnFileMapping`: For mapping files from current process to new process.
- `file` related structures and arguments:
    - `DirEntry`: For directory entries.
    - `FileStat`: For file stats, such as size, type, etc.
    - `BlockingMode`: For blocking modes of the operations on files.
    - `FileType`: For file types.
    - `FileMeta`: For assigning and getting metadata of files.
- `STDIN`, `STDOUT`, `STDERR` file descriptors numbers.

[`amjad_os_kernel_user_link`]: https://crates.io/crates/amjad_os_kernel_user_link
