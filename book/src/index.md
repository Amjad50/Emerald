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

## Building

The whole building and packaging is done by [xtask](https://github.com/Amjad50/Emerald/tree/master/xtask/)


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

[Rust]: https://www.rust-lang.org/