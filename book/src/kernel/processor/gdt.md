{{ #include ../../links.md }}

# Global Descriptor Table (GDT) and others

> This is implemented in [`gdt`][kernel_gdt] and [`idt`][kernel_idt].

The Global Descriptor Table ([GDT]) is a data structure used by the x86 architecture to define the characteristics of the various memory
and privileges of the segments used by the CPU.

The Interrupt Descriptor Table ([IDT]) is a data structure used by the x86 architecture to define the characteristics of the various interrupts and exceptions.

## Interrupt Descriptor Table (IDT)

I'll start with this just to get it out of the way.

The setup for [IDT] is very simple, we just have a static memory of "default" empty handlers,
and we use the `lidt` instruction to load the IDT.

Later, when we add an interrupt, we just modify the [IDT] entry with the new handler, and it will be used from now on.

For more information about specific usage of interrupts, see [Interrupts and exceptions](./interrupts.md) and [APIC](./apic.md).


## Global Descriptor Table (GDT)

This kernel is `x86_64`, and segments are not used as much as they were in the past, so we have a very basic implementation of the GDT.

Currently, we have 4 segments excluding the `NULL` segment:
- `KERNEL_CODE`: This is the code segment for the kernel.
    - flags: `flags::PRESENT | flags::CODE | flags::USER | flags::dpl(KERNEL_RING)`
- `USER_CODE`: This is the code segment for the userspace.
    - flags: `flags::PRESENT | flags::CODE | flags::USER | flags::dpl(USER_RING)`
- `KERNEL_DATA`: This is the data segment for the kernel.
    - flags: `flags::PRESENT | flags::USER | flags::WRITE | flags::dpl(KERNEL_RING)`
- `USER_DATA`: This is the data segment for the userspace.
    - flags: `flags::PRESENT | flags::USER | flags::WRITE | flags::dpl(USER_RING)`

The code segments will have the `LONG` flag set. Technically, we don't also need the `KERNEL_DATA` segment, but It's included to be
more consistent.

> I won't go into details of the flags, you can check the documentation of [GDT] or the CPU manual.

> Also an interesting node, `flags::WRITE` seem to be required, at least with `qemu` it would crash when switching to data segment where
its not available, even though, the AMD64 manual says that the CPU ignores those bits in 64-bit mode.

From above:
- `KERNEL_RING` is `0`
- `USER_RING` is `3`

As part of the `GDT` setup, we also setup the `TSS` (Task State Segment), which is used by the CPU to switch between tasks generally.
But since we don't use hardware tasks, we at least need to set it up to configure interrupts stacks.

## Task State Segment (TSS)

The [TSS] is a structure that is used by the CPU to switch between tasks, and it also contains the `IST` (Interrupt Stack Table) which is used to provide a separate stack for interrupts, also provide the stack for when to go from user to kernel modes.

For us, we setup `7` stacks, usable by any interrupt. Look at [memory layout](../memory/memory_layout.md) for where those are located.
The interrupts manager can then choose the
stack to use for each interrupt with `set_stack_index` (see [Interrupts and exceptions](./interrupts.md#interrupts-and-exceptions)).
A value of `None` means to use the default stack.

The default stack will be the current stack if the privilege level is the same as the current privilege level, 
otherwise it will change to the stack specified in the [TSS] based on the target privilege level.

Currently, we only have 1 stack for `KERNEL_RING`, which is at `Process kernel stack` in the [memory layout](../memory/memory_layout.md).
I.e. this is a stack specific to each process, as this will only be used when transitioning from user to kernel mode, and inside user mode, we will always be inside a process.


[IDT]: https://wiki.osdev.org/Interrupt_Descriptor_Table
[GDT]: https://wiki.osdev.org/Global_Descriptor_Table
[TSS]: https://wiki.osdev.org/Task_State_Segment
