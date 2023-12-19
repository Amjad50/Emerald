# Kernel

This is a kernel written in rust from scratch.

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
For building the ISO image:
```sh
cargo make kernel_iso
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

## Information about the kernel
### Booting
Currently, this project compiles a multiboot2 ELF64 kernel that can be booted by several bootloaders,
I'm using GRUB using a bootloader like GRUB.

GRUB and probably other bootloaders, will setup protected-mode (32bit) and then pass execution to the kernel starting in [`src/boot.S`].
> Note here, since we have moved to multiboot2 in #2, we can directly start in 64bit with EFI, but right now
> since we already have [`src/boot.S`] running in 32bit in boot, let's keep on that, so that we can support both BIOS and EFI.

In the start of the kernel, we only do basic setup to switch to long-mode (64bit), this is done in assembly in [`src/boot.S`].
After setting up long-mode, we jump to rust code, and start executing the `kernel_main` function.

when we jump to the `kernel_main`, we have mapped some basic parts of the kernel to virtual memory, a basic GDT with no IDT, and we have interrupts still disabled.
So we setup all of those and the rest of the OS then.

## License
This project is licensed under the MIT license, see [LICENSE](LICENSE) for more information.

- [`src/boot.S`]: src/boot.S
