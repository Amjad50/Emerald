{{ #include ../../links.md }}

# Interrupts and exceptions

> This is implemented in [`interrupts`][kernel_cpu_interrupts].

This is where interrupts are managed. We have 2 types of interrupts.

- **Exceptions**: These are errors that occur in the CPU, like division by zero, invalid opcode, etc, and these are
defined in specific places in the [IDT].
- **Interrupts**: These are events that are triggered by devices, like the keyboard, timer, etc,
these do not have specific placement in the [IDT], and are instead handled mostly in the `user_interrupts` range
of the [IDT], which is from `32` to `255`.

The last `16` entries of the `user interrupts` are used specially by the kernel as such:

- `SPECIAL_SCHEDULER_INTERRUPT=0xFF`: Used by scheduler to switch between contexts.
- `SPECIAL_SYSCALL_INTERRUPT=0xFE`: Used by the syscall instruction to switch to kernel mode, this is defined in [kernel user link](../../extra/kernel_user_link.md).

Generally we use functions like [`allocate_user_interrupt`][kernel_cpu_interrupts_allocate_int], which will allocate
an interrupt entry, and puts our function there (see [Interrupts Handlers](#interrupts-handlers) later for types of interrupts), then it will give us where it was allocated, so we can use it later, for example with [APIC](./apic.md).

The [`InterruptDescriptorTableEntry`][kernel_InterruptDescriptorTableEntry] gives us some extra functionalities that
are not provided by default. Such as:
- `set_stack_index` which sets the stack index to use when handling the interrupt.
- `set_privilege_level` which sets the privilege level of the interrupt.
- `set_disable_interrupts` which sets if the interrupts flag should disabled when handling this interrupt.
- `override_code_segment` which sets the code segment to use when handling the interrupt.

## Interrupts handlers

There are 2 types of interrupts handlers based on what arguments they take:
- The minimal, which takes `InterruptStackFrame64`, i.e. the data provided by the CPU automatically.
    ```rust
    pub struct InterruptStackFrame64 {
        pub rip: u64,
        pub cs: u8,
        pub rflags: u64,
        pub rsp: u64,
        pub ss: u8,
    }
    ```
- The full, which takes all registers (except `fxsave`), this is used when we expect to switch between processes or need
those extra registers.
    ```rust
    pub struct InterruptAllSavedState {
        pub rest: RestSavedRegisters,   // contain all the rest of the registers
        pub number: u64,
        pub error: u64,
        pub frame: InterruptStackFrame64,
    }
    ```

Generally, [APIC](./apic.md) is the only user of [`allocate_user_interrupt`][kernel_cpu_interrupts_allocate_int], as it
uses it to allocate interrupts for hardware devices. Look at the [APIC](./apic.md) for more details on what interrupts
we have now.

[IDT]: https://wiki.osdev.org/Interrupt_Descriptor_Table
