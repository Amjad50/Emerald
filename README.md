<p align="center">
  <a href="https://github.com/Amjad50/Emerald"><img alt="emerald OS logo" src="book/src/assets/logo.svg" width="40%"></a>
  <p align="center">Emerald <em>OS</em></p>
</p>

**Emerald** is an OS written in [Rust] from scratch.

The plan is to learn everything about the kernel and low level details, so I'm not using any libraries, even though
there are a lot of good libraries that do everything I'm doing (ex. handling GDT/IDT/etc...).
But maybe I'll add those just to make the code smaller and better to work with.


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
Build userspace programs into [`filesystem`](filesystem) directory (used by qemu):
> Note: this will build the `rust` toolchain if it hasn't been built before (might take some time)
```sh
cargo make filesystem
```
(optional) Install the toolchain into [`extern/toolchain`](extern/toolchain) directory:
> You can then use `rustup toolchain link ...` to link to this folder
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
which is the main dependancy for that `std` uses.

We have our own target `x86_64-unknown-emerald` which is a custom target for our OS, added to custom fork
of `rustc` in here: [`rust`].

## Demo to userspace programs

Here is a demo of a program I can run on my OS, [see the repo here](https://github.com/Amjad50/lprs)
![gif demo](https://github.com/Amjad50/lprs/blob/master/demo.gif)

## License
This project is licensed under the MIT license, see [LICENSE](LICENSE) for more information.

[Rust]: https://www.rust-lang.org/
[#2]: https://github.com/Amjad50/Emerald/pull/2
[`kernel/src/boot.S`]: kernel/src/boot.S
[`rust`]: https://github.com/Amjad50/rust/tree/emerald_os
