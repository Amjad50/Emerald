# Kernel user link

This is a crate that contains common definitions for the kernel and the user space. And it is implemented in [`emerald_kernel_user_link`] crate.

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
- `clock` related structures and arguments:
    - `ClockType`: For specifying the type of the clock to get the time from.
        - `RealTime`: For getting the real time, which is based on the `unix time`.
        - `SystemTime`: For getting the time since the system booted.
    - `ClockTime`: Structure holding the time, `seconds` and `nanos`.
- `STDIN`, `STDOUT`, `STDERR` file descriptors numbers.

[`emerald_kernel_user_link`]: https://crates.io/crates/emerald_kernel_user_link
