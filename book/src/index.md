<p align="center">
  <a href="https://github.com/Amjad50/Emerald"><img alt="emerald OS logo" src="./assets/logo.svg" width="50%"></a>
  <p align="center">Emerald <em>OS</em></p>
</p>

# Emerald
This is an operating system that I'm building for fun and learning in [Rust].

Please check it out, and if you have any questions, feel free to ask, open an issue, fix a bug, etc...

## Running

If you don't want to build the project, you can download the latest artifacts from:
- [kernel.zip](https://nightly.link/Amjad50/Emerald/workflows/ci/master/kernel.zip)
- [filesystem_programs.zip](https://nightly.link/Amjad50/Emerald/workflows/ci/master/filesystem_programs.zip)

You get `ISO` file containing the kernel and compressed `filesystem` directory containing the userspace programs.

The current command is what we use normally to run the OS, but can be run by any VM with some setup.
```sh
qemu-system-x86_64 -cdrom <kernel.iso> -serial mon:stdio -m 512 -boot d -drive format=raw,file=fat:rw:<filesystem>
```

where `<kernel.iso>` is the path to the ISO file, and `<filesystem>` is the path to the filesystem directory decompressed.

> Some extra info:
> - `-serial mon:stdio` is used to redirect the serial output to the terminal.
> - `-m 512` is the amount of memory to allocate for the VM, `512MB`.
> - `-boot d` is to boot from the CD-ROM we just loaded.
> - `-drive format=raw,file=fat:rw:<filesystem>` is to pass the filesystem directory to the kernel as a disk.
>
> Here we use a feature of QEMU, `virtual fat`, where it will treat the directory as a FAT filesystem, and being passed
> to the kernel as a disk.

## Building
We are using [`cargo-make`](https://github.com/sagiegurari/cargo-make) utility to build everything and run it with qemu.
```sh
cargo install cargo-make
```

The ISO file can be used to run on other VMs/hardware(not tested)

For building the ISO image, you can use `make` but you need to have other dependencies installed to build and run the ISO:
```
xorriso mtools grub-pc-bin qemu-system-x86
```
Build kernel iso:
```sh
cargo make kernel_iso
```
Build userspace programs into [`filesystem`](https://github.com/Amjad50/Emerald/tree/master/filesystem) directory (used by `qemu`):
> Note: this will build the `rust` toolchain if it hasn't been built before (might take some time)
```sh
cargo make filesystem
```
(optional) Install the toolchain into [`extern/toolchain`](https://github.com/Amjad50/Emerald/tree/master/extern/toolchain) directory:
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