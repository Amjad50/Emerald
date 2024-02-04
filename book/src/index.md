<p align="center">
  <a href="https://github.com/Amjad50/OS"><img alt="emerald OS logo" src="./assets/logo.svg" width="50%"></a>
  <p align="center">Emerald <em>OS</em></p>
</p>

# Emerald
This is an operating system that I'm building for fun and learning in [Rust].

Please check it out, and if you have any questions, feel free to ask, open an issue, fix a bug, etc...

## Building and running
We are using [`cargo-make`](https://github.com/sagiegurari/cargo-make) utility to build a grub rescue iso, and run it with qemu.

The ISO file can be used to run on other VMs/hardware(not tested)

For building just the kernel ELF file:
```sh
cargo build
```
For building the ISO image, you can use `make` but you need to have other dependencies installed to build and run the ISO:
```
xorriso mtools grub-pc-bin qemu-system-x86
```
Build kernel iso:
```sh
cargo make kernel_iso
```
Build userspace programs into [`filesystem`](https://github.com/Amjad50/OS/tree/master/filesystem) directory (used by `qemu`):
> Note: this will build the `rust` toolchain if it hasn't been built before (might take some time)
```sh
cargo make filesystem
```
(optional) Install the toolchain into [`extern/toolchain`](https://github.com/Amjad50/OS/tree/master/extern/toolchain) directory:
> You can then use `rustup toolchain link ...` to link to this folder (See more in [userspace](./userspace/index.md))
```sh
cargo make toolchain
```
For running:
```sh
cargo make run_iso
```
You need to have `qemu-system-x86_64` installed.

### Debugging
You can use `gdb` or `lldb` to debug this.

But I have included vscode configs to enable easily debugging with `CodeLLDB` extension.

And to boot QEMU in debug mode you can use (it will wait for debugger on port `:1234`)
```sh
cargo make run_iso_gdb
```

[Rust]: https://www.rust-lang.org/