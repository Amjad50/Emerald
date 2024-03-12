# Userspace

This section will cover the userspace part of the operating system. It will cover the programs, libraries and other userspace related topics.

## Building custom userspace programs

Since we are using [`Rust STD`](./rust_std.md), you can build (hopefully)
a lot of rust projects that don't have any dependencies on `libc`, `linux` or `windows` specific libraries.

### Getting the toolchain

First, you need to get the toolchain, and you can do that by either:
- [Using the prebuilt toolchain](#using-the-prebuilt-toolchain)
- [Building the toolchain](#building-the-toolchain)


#### Using the prebuilt toolchain
We distribute a prebuilt toolchain in:
- [toolchain.zip](https://nightly.link/Amjad50/Emerald/workflows/ci/master/toolchain.zip)
Where you can install with
```sh
sh tools/install_toolchain_and_link.sh <path_to_toolchain.zip>
```
This will install the toolchain into `extern/toolchain` and link it to `rustup` as `emerald`.

Then, when using our `cargo make` to build our programs, you need to provide the environment `USE_INSTALLED_TOOLCHAIN=true`.
```
USE_INSTALLED_TOOLCHAIN=true cargo make filesystem
```

#### Building the toolchain
You can build our toolchain with the command
```sh
cargo make toolchain
```
in [the root of the project](https://github.com/Amjad50/Emerald).

Which will install a toolchain in `./extern/toolchain`, which you can then link to your `rustup` toolchain with `rustup toolchain link emerald ./extern/toolchain`.

### Building the userspace programs

Then, you can build your project with the toolchain
```sh
cargo +emerald build --target x86_64-unknown-emerald
```

> [Please open an issue](https://github.com/Amjad50/Emerald/issues) if you have any problems building your project,
or if you want to add a new feature to the toolchain. 

