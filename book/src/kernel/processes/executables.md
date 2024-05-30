{{ #include ../../links.md }}

# Executables

> This is implemented in [`executable`][executable]

Currently, we only have support for [ELF] and will probably stay like this for a while, I don't plan to support other formats soon.


## ELF

The [ELF] file is the executable format used by most of the Unix-like systems, and it is the format we will be using for our executables.

> This is implemented in [`elf`][elf]

We load elf on process creation, see [process creation](../processes/index.md#process-creation) for more information.

For now, we support very basic loading, no dynamic linking, shared libraries, or relocation.
Just loading segments.

[ELF]: https://en.wikipedia.org/wiki/Executable_and_Linkable_Format
