{{ #include ../../links.md }}

# Memory layout

> The memory layout constants and code is defined in [`memory_layout`][kernel_memory_layout]

This is the structure of the memory for any process running in the system.

## Layout
```txt
0000_0000_0000_0000 .. FFFF_FF7F_FFFF_FFFF - User   (15.99~ EB)
FFFF_FF80_0000_0000 .. FFFF_FFFF_7FFF_FFFF - Process specific kernel (510 GB)
FFFF_FFFF_8000_0000 .. FFFF_FFFF_FFFF_FFFF - Kernel (2 GB)
```

### Kernel layout
```txt
FFFF_FFFF_8000_0000..FFFF_FFFF_8010_0000          nothing
FFFF_FFFF_8010_0000..FFFF_FFFF_8XXX_XXXX          kernel elf (text, rodata, data, bss)
FFFF_FFFF_8XXX_XXXX..FFFF_FFFF_8800_0000          (kernel end) physical allocator low (until 128MB mark pre-mapped in `boot`)
FFFF_FFFF_8800_0000..FFFF_FFFF_8900_0000          kernel heap (16MB)
FFFF_FFFF_8900_0000..FFFF_FFFF_8903_F000          interrupt stacks
    FFFF_FFFF_8900_0000..FFFF_FFFF_8900_1000      interrupt stack 0 guard page (4KB) *not mapped by purpose*
    FFFF_FFFF_8900_1000..FFFF_FFFF_8900_9000      interrupt stack 0 (8 * 4KB = 32KB)
    ... *repeat for 6 more stacks*
FFFF_FFFF_8903_F000..FFFF_FFFF_FFFF_F000          Kernel extra (virtual space, free virtual space to use)
```

The kernel is loaded by the bootloader at physical address `0x10_0000`, and then it will
perform virtual mapping for physical `0x0` into `0xFFFF_FFFF_8000_0000` for `128MB`
i.e. until the end of the initial `physical page allocator`. See more details in the [boot] chapter.

Look at [virtual space] for more info on it.


#### Virtual space

Virtual space is a I'm using (not sure what other OSes call), that solves the issue of "I have a physical address of an object, but I don't have virtual space to map it to".
This is useful for reading structures that are in specific location in physical memory, such as `ACPI` tables, `PCI` configuration space, `memory mapped IO`, etc.

Its very simple, it will take memory from the `kernel extra` space, and map it to the physical address.

### Process specific kernel layout
```txt
FFFF_FF80_0000_0000..FFFF_FF80_0000_1000          process kernel stack guard page (4KB) *not mapped by purpose*
FFFF_FF80_0000_1000..FFFF_FF80_0000_9000          process kernel stack (8 * 4KB = 32KB)
```

This is a space specific to each process, but reside in kernel space.

The idea is to have structures that are specific to processes here, that others won't have access and thus reduce the need to setup a lock around them.

We use it currently for `kernel stack`, which is where the kernel will store the stack when an interrupt happens while we are in user space.

It solves the issue where having a single kernel stack for all processes can't work, as if two processes gets interrupted while the first one is still in the kernel, the second one will overwrite the first one's stack.

> As you might have expect, the previous paragraph was a source of a crazy bug that introduced this `kernel stack`. Fixed in [0dc04f8]

### User layout
Not much to talk about here, as this will depend on the process itself and where to load the ELF file, currently we load at the address specified in the ELF file.
This is of course not safe, as we don't do extra checks for that value.

But anyway, the other parts of the userspace are as follows:
```txt
FFFF_FF7F_FFFF_E000 .. XXXX_XXXX_XXXX_XXXX - Stack. From the top and grows down
XXXX_XXXX_XXXX_XXXX .. YYYY_YYYY_YYYY_YYYY - Heap. From the end of the ELF and grows up
YYYY_YYYY_YYYY_YYYY .. ZZZZ_ZZZZ_ZZZZ_ZZZZ - ELF file
```

A lot of symbols XD. But in general, the stack is at the top of the user space, and the elf file is at the bottom,
and the heap is in the middle starts after the elf file.


[boot]: ../boot.md
[virtual space]: ./virtual_space.md
[0dc04f8]: https://github.com/Amjad50/Emerald/commit/0dc04f8