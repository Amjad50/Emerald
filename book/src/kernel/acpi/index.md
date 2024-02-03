{{ #include ../../links.md }}

# Advanced Configuration and Power Interface (ACPI)

> This is implemented in [`acpi`][kernel_acpi].

ACPI is a standard for operating systems to discover and configure computer hardware components, to perform power management, and to configure the system's power management features. It is a successor to Advanced Power Management (APM), and it is a part of the [UEFI] specification.

We have some support to some of the components of the ACPI standard.

ACPI comes in tables, and these tables include information about the system's hardware.

## ACPI Tables Support

We have parsing support for these tables now, having support does mean we use all the features inside them.
It just means that we can read the table, and that's it, some of them are used, but not all.

- [RSDP]: This is either provided by `multiboot2` during [boot](../boot.md),
  or we search for it in memory, and this just points to the `RSDT` or `XSDT`.
- [RSDT]: This is the Root System Description Table, and it contains pointers to other tables.
- [MADT/APIC]: This is the Multiple APIC Description Table, and it contains information about the APIC,see [APIC](./apic.md) for more details.
- [FACP]: This is the Fixed ACPI Description Table, and it contains information about the power management features of the system, also contain some info about the [RTC](../drivers/rtc.md).
- [HPET]: This is the High Precision Event Timer, and it contains information about the HPET, see [HPET](../drivers/hpet.md) for more details.
- [DSDT] (not used*): This is the Differentiated System Description Table, and it contains [AML](./aml.md) code, which is used to configure the system.
- [SSDT] (not used*): This is the Secondary System Description Table, and it also contains [AML](./aml.md) code, which is used to configure the system.
- [BGRT] (not used): This is the Boot Graphics Resource Table, and it contains information about the boot logo, and it is used by the [UEFI] firmware.
- [WAET] (not used): This is the Windows ACPI Emulated Devices Table, and it contains information about the emulated devices, and it is used by the [UEFI] firmware.
- [SRAT] (not used): This is the System Resource Affinity Table, and it contains information about the system's memory and processors locality.

> *: We don't use `DSDT` and `SSDT` data for now, but we do parse it as `AML` code, see [AML](./aml.md) for more details.

We use [virtual space](../memory/virtual_space.md) to map `APIC` tables, and then we copy
them to the heap, this will make it easier to use, and we can reclaim `ACPI` memory later.

[UEFI]: https://en.wikipedia.org/wiki/UEFI
[RSDP]: https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/05_ACPI_Software_Programming_Model/ACPI_Software_Programming_Model.html#root-system-description-pointer-rsdp-structure
[RSDT]: https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/05_ACPI_Software_Programming_Model/ACPI_Software_Programming_Model.html#root-system-description-table-rsdt
[MADT/APIC]: https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/05_ACPI_Software_Programming_Model/ACPI_Software_Programming_Model.html#multiple-apic-description-table-madt
[FACP]: https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/05_ACPI_Software_Programming_Model/ACPI_Software_Programming_Model.html#fixed-acpi-description-table-fadt
[HPET]: https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/05_ACPI_Software_Programming_Model/ACPI_Software_Programming_Model.html#high-precision-event-timer-hpet
[DSDT]: https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/05_ACPI_Software_Programming_Model/ACPI_Software_Programming_Model.html#differentiated-system-description-table-dsdt
[SSDT]: https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/05_ACPI_Software_Programming_Model/ACPI_Software_Programming_Model.html#secondary-system-description-table-ssdt
[BGRT]: https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/05_ACPI_Software_Programming_Model/ACPI_Software_Programming_Model.html#boot-graphics-resource-table-bgrt
[WAET]: https://uefi.org/acpi
[SRAT]: https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/05_ACPI_Software_Programming_Model/ACPI_Software_Programming_Model.html#system-resource-affinity-table-srat