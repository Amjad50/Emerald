# xtask

`xtask` is the tool that helps us build, test and run the kernel and userspace programs.
Its a custom wrapper around `cargo` and various other tools to make developing `Emerald` easier.

```txt
Usage: xtask [--release] [--profile] <command> [<args>]

XTask - a task runner

Options:
  --release         build in release mode
  --profile         use profile mode (and run qmp socket when running qemu)
  --help, help      display usage information

Commands:
  run               Run the kernel
  test              Test the kernel
  build-iso         Build the kernel ISO
  kernel            Run rust commands on the kernel
  userspace         Run rust commands on the userspace programs
  toolchain         Build the toolchain distribution
  profiler          Profile the kernel using QMP
```
