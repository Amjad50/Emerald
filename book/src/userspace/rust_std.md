# Rust Standard Library

Currently, everything is built by [`Rust`], we don't have `libc` yet. And instead we have a custom **rust target** 
that we use to build our userspace programs.

This is implemented in my [fork](https://github.com/Amjad50/rust/tree/emerald_os) of `rust-lang/rust`.

We have the target `x86_64-unknown-emerald`. The changes needed are in [`rust/library/std/src/sys/emerald`] and [`rust/library/std/src/os/emerald`].

These changes are provided by another crate [emerald_std], where we have all the basic implementation of userspace functionalities including:
- `allocator`: See [Heap Allocator](../extra/heap_allocator.md) for more details.
- `files` and `io`: For file operations and input/output.
- `clock`: For clock and time operations.
- `graphics`: For graphics operations.
- `process`: For process management.

This crate, is very basic and only performs `syscalls` basically, nothing much else, then in `rust` we perform the
rest of the bindings.

Using `rust` like this gives us a lot of benefits, since we just need to implement basic `unsafe` functionalities,
and it will handle synchronization, memory management, etc., of course we need to make sure our implementation
is correct.


[`Rust`]: https://www.rust-lang.org/
[`rust/library/std/src/sys/emerald`]: https://github.com/Amjad50/rust/tree/emerald_os/library/std/src/sys/emerald
[`rust/library/std/src/os/emerald`]: https://github.com/Amjad50/rust/tree/emerald_os/library/std/src/os/emerald
[emerald_std]: https://crates.io/crates/emerald_std