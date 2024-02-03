{{ #include ../../links.md }}

# Processor

> This is implemented in [`cpu`][kernel_cpu].

Here we talk about processor related structures and functions.
Including:

- Interrupts and exceptions
- Global Descriptor Table (GDT)
- Advanced Programmable Interrupt Controller (APIC) and IO APIC.

Let's first talk about other processor stuff that are not the above.

## Saved CPU state

Each CPU (currently only 1) has a [structure that contain the state related to the CPU][kernel_cpu_struct].

Where it contains among others:
- the `id` and `apic_id` of the cpu, for identification.
- the `n_cli` and `old_interrupt_enable` which is used by the implementation of [locks][kernel_lock].
- the `context` which is a process context, used when switching between processes, also `process_id` for current process, and other scheduling related fields.

## CPU initialization

Currently, we don't perform any additional initialization after [boot](../boot.md), and its causing some issues.
As UEFI results in a different CPU state than BIOS, and we need to handle that, [there is an issue for that #34](https://github.com/Amjad50/OS/issues/34).
