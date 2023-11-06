# Kernel

This is a kernel written in rust from scratch.

The plan is to learn everything about the kernel and low level details, so I'm not using any libraries, even though
there are a lot of good libraries that do everything I'm doing (ex. handling GDT/IDT/etc...).
But maybe I'll add those just to make the code smaller and better to work with.


## Building and running
For building:
```sh
cargo build
```
For running:
```sh
cargo run
```
You need to have `qemu-system-x86_64` installed.

### Debugging
You can use `gdb` or `lldb` to debug this.

But I have included vscode configs to enable easily debugging with `CodeLLDB` extension.

## Information about the kernel
### Booting
Currently, this project compiles a multiboot ELF64 kernel that can be booted by qemu.

I'm not using any bootloader, or any disks for now, so the kernel is loaded directly into memory by qemu.

Qemu, and probably other bootloaders, will setup protected-mode (32bit) and then pass execution to the kernel.

The kernel then, finishes it setup, and then switches to long-mode (64bit), this is done in assembly in [`src/boot.S`](src/boot.S).
After setting up long-mode, we jump to rust code, and start executing the kernel.

when we jump to the kernel, we have mapped some basic parts of the kernel to virtual memory, a basic GDT with no IDT, and we have interrupts still disabled.
So in the kernel, we setup all of these.

## License
This project is licensed under the MIT license, see [LICENSE](LICENSE) for more information.
