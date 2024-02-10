{{ #include ../../links.md }}

# Advanced Programmable Interrupt Controller (APIC) and IO APIC

> This is implemented in [`apic`][kernel_cpu_apic].

The APIC is a part of the CPU, and it is used to manage interrupts and exceptions. It is used to manage the interrupts and exceptions that are triggered by hardware devices, and it is also used to manage the interrupts and exceptions that are triggered by the CPU itself.

## Initialization

The initialization of the `APIC` is done using [ACPI](../acpi/index.md) tables,
which contains data about the ACPI, such as:
- Number of CPUs.
- The APIC ID of each CPU.
- The address of the `IO APIC`.

The `APIC` is a memory-mapped address provided by the CPU, we can fetch it by reading the `MSR` register `0x1B` (which is the `APIC_BASE` register).

Then we can map the `APIC` and `IOAPIC` (also memory-mapped) addresses to the virtual address space using [virtual space](../memory/virtual_space.md).

## Interrupts

The `APIC` can be used to allocate and assign interrupts to hardware devices. We can use the functions:
- [`assign_io_irq`][kernel_cpu_apic_assign_io_irq] to assign an interrupt to a hardware device.
- [`assign_io_irq_custom`][kernel_cpu_apic_allocate_io_irq_custom] which is similar to the above but provide extra changes to the interrupt, such as:
    - `with_interrupt_polarity_low`: Set the interrupt polarity to low/high (boolean).
    - `with_trigger_mode_level`: Set the trigger mode to level/edge (boolean).
    - `with_mask`: Set the interrupt to be masked or not (boolean).

It will setup the interrupts with the correct `IO APIC` based on the argument `irq_num` provided.

Currently, we have these interrupts configured:
- `1`: Used by the [keyboard](../drivers/keyboard.md) driver.
- `14 & 15`: Used by the [IDE](../drivers/ide.md) driver.
- [HPET](../clocks/hpet.md) timer, with interrupt number specified dynamically based on its configuration,
but looks to be `2` on the VM.


We also have interrupts from the `APIC` itself, such as:
- **Timer interrupt**: this is used by the scheduler to switch between processes.
- **Error interrupt**: This is mapped, but haven't seen it triggered yet.
- **Spurious interrupt**: This is mapped, but haven't seen it triggered yet.
