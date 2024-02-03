# Rust Standard Library

Currently, everything is built by [`Rust`], we don't have `libc` yet. And instead we have a custom **rust target** 
that we use to build our userspace programs.

This is implemented in my [fork](https://github.com/Amjad50/rust/tree/amjad50_os_new_target) of `rust-lang/rust`.

We have the target `x86_64-unknown-amjad_os`. The changes needed are in [`rust/library/std/src/sys/amjad_os`] and [`rust/library/std/src/os/amjad_os`].

These changes are provided by another crate [amjad_os_user_std], where we have all the basic implementation of userspace functionalities including:
- `allocator`: See [Heap Allocator](../extra/heap_allocator.md) for more details.
- `files` and `io`: For file operations and input/output.
- `process`: For process management.

This crate, is very basic and only performs `syscalls` basically, nothing much else, then in `rust` we perform the
rest of the bindings.

Using `rust` like this gives us a lot of benefits, since we just need to implement basic `unsafe` functionalities,
and it will handle synchronization, memory management, etc., of course we need to make sure our implementation
is correct.


[`Rust`]: https://www.rust-lang.org/
[`rust/library/std/src/sys/amjad_os`]: https://github.com/Amjad50/rust/tree/amjad50_os_new_target/library/std/src/sys/amjad_os
[`rust/library/std/src/os/amjad_os`]: https://github.com/Amjad50/rust/tree/amjad50_os_new_target/library/std/src/os/amjad_os
[amjad_os_user_std]: https://crates.io/crates/amjad_os_user_std