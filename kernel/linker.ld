OUTPUT_FORMAT("elf64-x86-64", "elf64-x86-64", "elf64-x86-64")
ENTRY(entry)

load_addr = 0x100000;

SECTIONS {
    /* This is equal to KERNEL_LINK (virtual) */
    . = 0xFFFFFFFF80100000;

    PROVIDE(begin = .);

    .text : AT(load_addr) ALIGN(4K)
    {
        KEEP(*(.multiboot2_header))
        *(.text .text.*)
    } : kernel_rx

    PROVIDE(text_end = .);

    .rodata :
    {
        *(.rodata .rodata.*)
    } : kernel_ro

    /* Adjust the address for the data segment to the next page */
    . = ALIGN(4K);

    PROVIDE(rodata_end = .);

    .data :
    {
        KEEP(*(.boot_page_tables .boot_page_tables.*))
        KEEP(*(.stack .stack.*))
        *(.data .data.*)
    } : kernel_rw

    PROVIDE(data_end = .);

    .bss :
    {
        *(COMMON)
        *(.bss .bss.*)
    } : kernel_rw

    PROVIDE(end = .);

    /DISCARD/ :
    {
        *(.eh_frame .eh_frame*)
    }
}

multiboot_load_addr = load_addr;
multiboot_load_end = load_addr + (data_end - begin);
multiboot_bss_end = load_addr + (end - begin);
multiboot_entry_addr = load_addr + (entry - begin);

