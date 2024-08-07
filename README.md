<p align="center">
  <a href="https://github.com/Amjad50/Emerald"><img alt="emerald OS logo" src="book/src/assets/logo.svg" width="40%"></a>
  <p align="center">Emerald <em>OS</em></p>
</p>

[![OS Build](https://github.com/Amjad50/Emerald/actions/workflows/ci.yml/badge.svg)](https://github.com/Amjad50/Emerald/actions/workflows/ci.yml)
[![Documentation](https://github.com/Amjad50/Emerald/actions/workflows/docs.yml/badge.svg)](https://amjad.alsharafi.dev/Emerald)

**Emerald** is an OS written in [Rust] from scratch.

The plan is to learn everything about the kernel and low level details, so I'm implementing as much as
possible without using any libraries.
But maybe I'll add those just to make the code smaller and better to work with.

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

The whole building and packaging is done by [xtask](./xtask/)


The ISO file can be used to run on other VMs/hardware(not tested)

For building the ISO image, you can use `make` but you need to have other dependencies installed to build and run the ISO:
```
xorriso mtools grub-pc-bin qemu-system-x86
```
Build kernel iso:
```sh
cargo xtask build-iso
```
### Building userspace programs
This builds userspace programs into [`filesystem`](filesystem) directory (used by qemu):

The userspace programs are built using a custom `rust` toolchain (See more info [here](https://amjad.alsharafi.dev/Emerald/userspace/rust_std.html))

Anyway, there are 2 options to build our userspace programs and in general any other program.

#### Using the prebuilt toolchain
We distribute a prebuilt toolchain in:
- [toolchain.zip](https://nightly.link/Amjad50/Emerald/workflows/ci/master/toolchain.zip)
Where you can install with
```sh
bash tools/install_toolchain_and_link.sh <path_to_toolchain.zip>
```
This will install the toolchain into `extern/toolchain` and link it to `rustup` as `emerald`.

Then, `xtask` will use the installed toolchain to build userspace programs, if its not installed
it will give an error.
```
cargo xtask userspace build
```

#### Building the toolchain
We don't build the toolchain automatically, i.e. if you don't have the toolchain you can build the toolchain yourself from source if you don't want to installed prebuilt.


```sh
cargo xtask toolchain
```
If you want to build and install from source into [`extern/toolchain`](extern/toolchain) directory
> You can then use `rustup toolchain link ...` to link to this folder
```sh
cargo xtask toolchain --install
```

### Building and running

To build and run kernel and userspace programs:
```sh
cargo xtask run
```
You need to have `qemu-system-x86_64` installed.

### Debugging
You can use `gdb` or `lldb` to debug this.

But I have included vscode configs to enable easily debugging with `CodeLLDB` extension.

And to boot QEMU in debug mode you can use (it will wait for debugger on port `:1234`)
```sh
cargo xtask run --gdb
```

## Documentation

The main documentation is in the [`book`](book) directory, you can build it using `mdbook`:
```sh
mdbook build book
```

Its also served in https://amjad.alsharafi.dev/Emerald/

## Kernel
### Booting
Currently, this project compiles a multiboot2 ELF64 kernel that can be booted by several bootloaders,
I'm using GRUB using a bootloader like GRUB.

GRUB and probably other bootloaders, will setup protected-mode (32bit) and then pass execution to the kernel starting in [`kernel/src/boot.S`].
> Note here, since we have moved to multiboot2 in [#2], we can directly start in 64bit with EFI, but right now
> since we already have [`kernel/src/boot.S`] running in 32bit in boot, let's keep on that, so that we can support both BIOS and EFI easily from
> the same entry point.

In the start of the kernel, we only do basic setup to switch to long-mode (64bit), this is done in assembly in [`kernel/src/boot.S`].
After setting up long-mode, we jump to rust code, and start executing the `kernel_main` function.

when we jump to the `kernel_main`, we have mapped some basic parts of the kernel to virtual memory, a basic GDT with no IDT, and we have interrupts still disabled.
So we setup all of those and the rest of the OS then.

## Userland

Currently, the main focus for running userspace applications is by having `std` in rust, as all userspace applications
are build in rust, this is the primary support. Thus, we don't have `libc` for now. We have [`emerald_std`](libraries/emerald_std/)
which is the main dependency for that `std` uses.

We have our own target `x86_64-unknown-emerald` which is a custom target for our OS, added to custom fork
of `rustc` in here: [`rust`].

## Demo to userspace programs

Here is a demo of the shell as is here. Another more complex application I can run is [lprs-fork](https://github.com/Amjad50/lprs) (not included in the demo below, but there is a demo in the README of that project).

![demo](./assets/demo.gif)

## License
This project is licensed under the MIT license, see [LICENSE](LICENSE) for more information.

[Rust]: https://www.rust-lang.org/
[#2]: https://github.com/Amjad50/Emerald/pull/2
[`kernel/src/boot.S`]: kernel/src/boot.S
[`rust`]: https://github.com/Amjad50/rust/tree/emerald_os
