# Boot

What happens during boot?

This OS doesn't include a [bootloader], the kernel is being loaded by default using [`grub`] using [`multiboot2`].

The [`kernel`] crate will be compiled as an `ELF` file implementing the [`multiboot2`] specification which can
be loaded by any bootloader that supports it.

For example, in grub we use the `multiboot2` command to load the kernel.

```txt
menuentry "Kernel" {
    insmod all_video
    multiboot2 /boot/kernel # the kernel ELF file
    boot
}
```

The kernel `ELF` file is in `elf64-x86-64` format, but we start in `x86` protected mode (32-bit) and then switch to `long mode` (64-bit) using assembly code in [`boot.S`].

## Initial boot, protected mode.

We start in assembly, since we are using rust, and we target `x64` for compilation, we have to add `32-bit` code manually using assembly.

The `boot.S` file is included in `main.rs` using `global_asm!` macro, and we use `.code32` directive to generate
`32-bit` code.

The initial boot here in the assembly performs the following:
- Setup initial page tables (required for `64-bit` mode).
- Setup basic GDT (Global Descriptor Table) (required for `64-bit` mode).
- Switch to `64-bit` long mode.
- Jump to `kernel_main` entry point in `main.rs`.

The kernel ELF file is loaded at `0x100000` physically, and `0xFFFFFFFF80100000` virtually.
The virtual address `0xFFFFFFFF80000000` will map to the physical address `0x0` initially.

> Note: you may notice in the code that we use `- 0xFFFFFFFF80000000` a lot,
such as `lgdt [gdtr64 - 0xFFFFFFFF80000000]`. And this is because we don't have virtual memory yet,
so we are operating with physical memory currently, and the linker will use `0xFFFFFFFF80000000` as
the base address. But since we know it maps to `0x0` physically, we can subtract to convert to physical addresses.


### Multiboot2 header
We specify some information to the bootloader that we want in the `multiboot2` header:

- **Address**: Here we specify load address information, which include:
    - `start` and `end` of the kernel image.
    - `bss_end` which is the end of the `.bss` section.
- **Entry point**: The entry point of the kernel, which is `entry` function in [`boot.S`],\
 i.e. where execution will start. This is executed in `32-bit` mode.
- **Module alignment**: Modules will be page aligned. (maybe not needed, as I thought it affected the kernel alignment, but looks like its for modules).

After that, execution will start at `entry`, where we check that the value in `EAX`
is equal to the special `multiboot2` magic value just to make sure we are correct.

### Switching to long mode
Then, we check that long mode is supported in the machine by making sure that `PAE` feature is supported in `CPUID:0000_0001` and `LM` feature is supported in `CPUID:8000_0001`.

If its not supported, we display an error and infinite loop.
```asm
# check for PEA
mov eax, 0x00000001
cpuid
test edx, CPUID_FEAT_EDX_PAE # (1 << 6)
jz not_64bit
# check for long mode
mov eax, 0x80000001
cpuid
test edx, CPUID_FEAT_EX_EDX_LM # (1 << 29)
jz not_64bit
```

If all is good, we setup some basic page tables as follows


#### Initial page tables

We map the first `128MB` of physical memory into two ranges.

- `[0x0000000000000000..0x0000000007FFFFFF]`, 1:1 mapping which give us easy access and required when we switch to `64-bit` mode.
- `[0xFFFFFFFF80000000..0xFFFFFFFF87FFFFFF]`, This is where the rust kernel is loaded in the ELF file, and all references addresses in this range.

The structure of the page table is as follows:
```txt
- [0x0000000000000000..0x0000000007FFFFFF] - 1:1 mapping with the physical pages
    - This will be:
        - PML4[0]
        - PDPT[0]
        - PDT[0..63] # 2MB each
- [0xFFFFFFFF80000000..0xFFFFFFFF87FFFFFF] - the kernel ELF file virtual address space
    - This will be:
        - PML4[511]
        - PDPT[510]
        - PDT[0..63] # 2MB each (shared with the above)
```

The location of where we store the initial page tables is at `boot_page_tables` which is a region of memory
in the `.boot_page_tables` section in the `.data` section that fits the size of `4` page tables,
each `4KB` in size.

The usage of `boot_page_tables` is as follows:
- `PML4                  (boot_page_tables[0])`
- `PML4[0]     -> PDPT-A (boot_page_tables[1])`
- `PML4[511]   -> PDPT-B (boot_page_tables[2])`
- `PDPT-A[0]   -> PDT    (boot_page_tables[3])`
- `PDPT-B[510] -> PDT    (boot_page_tables[3]) // same PDT as above`

And then `PDT[0..63]` is shared between the two and maps the first `128MB` of physical memory.

Then, `CR3` is set to the address of `PML4` and `CR4` is set to enable `PAE`.

#### GDT (Global Descritor Table)

We setup a very basic GDT that contain kernel `code` and `data` segments (even though, data probably is not needed).

```asm
.align 16
gdt64:
    .quad 0x0000000000000000    # null descriptor
    # Code segment (0x8)
    .long 0x00000000                                       # Limit & Base (low, bits 0-15)
    .byte 0                                                # Base (mid, bits 16-23)
    .byte GDT_CODE | GDT_NOT_SYS | GDT_PRESENT | GDT_WRITE # Access
    .byte GDT_LONG_MODE                                    # Flags & Limit (high, bits 16-19)
    .byte 0x00                                             # Base (high, bits 24-31)
    # Data segment (0x10)
    .long 0x00000000                                       # Limit & Base (low, bits 0-15)
    .byte 0                                                # Base (mid, bits 16-23)
    .byte GDT_NOT_SYS | GDT_PRESENT | GDT_WRITE            # Access
    .byte 0x00                                             # Flags & Limit (high, bits 16-19)
    .byte 0x00                                             # Base (high, bits 24-31)
```

I prefer it this way, but other just use a `quad` static value. This won't change at all so either is fine.

Then, we load the `GDT` using `lgdt` instruction.

```asm
lgdt [gdtr64 - 0xFFFFFFFF80000000]
```

#### Switch to long mode

We need to do the following:
- Setup page tables.
- Enable `PAE` in `CR4`.
- Enable `PG` (Paging) and `PE` (Protection Enable) in `CR0`.
- Enable `long mode` in `EFER` MSR.
    ```asm
    mov ecx, EFER_REG # (0xC0000080)
    rdmsr
    or  eax, EFER_LME # (1 << 8)
    wrmsr
    ```
- Setup `GDT`.
- Jump to `kernel_main` in `main.rs`.

But before we go to `kernel_main` directly, we jump to a location in `boot.S` (`kernel_main_low`).
Where we setup the data segment, the stack, also forward the multiboot2 information to `kernel_main`.

The stack used is `512` pages, i.e. `2MB` in size, and is located in the `.stack` section
which is inside the `.data` section.

The stack is a bit large, but we don't require that much stack most of the time,
its needed like this due to our recursive AML (ACPI) parser, which we should improve.

We also setup a **guard page** for the stack that is unmapped later using the more advanced virtual memory setup.


Then, we jump to `kernel_main` in `main.rs`, and its all rust from here.


[bootloader]: https://en.wikipedia.org/wiki/Bootloader
[`grub`]: https://en.wikipedia.org/wiki/GNU_GRUB
[`multiboot2`]: https://www.gnu.org/software/grub/manual/multiboot2/multiboot.html
[`kernel`]: https://github.com/Amjad50/OS/tree/master/kernel
[`boot.S`]: https://github.com/Amjad50/OS/blob/master/kernel/src/boot.S
