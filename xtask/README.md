# xtask

> From https://github.com/matklad/cargo-xtask
>
> cargo-xtask is way to add free-form automation to a Rust project, a-la make, npm run or bespoke bash scripts.
> 
> The two distinguishing features of xtask are:
> 
> It doesn't require any other binaries besides cargo and rustc, it fully bootstraps from them
> Unlike bash, it can more easily be cross platform, as it doesn't use the shell.


We use this to manage our build tools to make it easier to build the kernel, the toolchain and userspace programs.

Use `cargo xtask help` to see what's available.
