# Userspace

This section will cover the userspace part of the operating system. It will cover the programs, libraries and other userspace related topics.

## Building custom userspace programs

Since we are using [`Rust STD`](./rust_std.md), you can build (hopefully)
a lot of rust projects that don't have any dependencies on `libc`, `linux` or `windows` specific libraries.

This can be done by:
- You can build our toolchain with the command: `cargo make toolchain` in [the root of the project](https://github.com/Amjad50/Emerald).
Which will install a toolchain in `./extern/toolchain`, which you can then link to your `rustup` toolchain with `rustup toolchain link emerald ./extern/toolchain`.
- Build the project with the toolchain
    ```txt
    cargo +emerald build --target x86_64-unknown-emerald
    ```

> [Please open an issue](https://github.com/Amjad50/Emerald/issues) if you have any problems building your project,
or if you want to add a new feature to the toolchain. 

