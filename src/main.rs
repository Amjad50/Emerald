#![no_std]
#![no_main]

// The virtual address of the kernel
pub static KERNEL_BASE: u64 = 0xFFFFFFFF80000000;
pub static TEXT_OFFSET: u64 = 0x100000;
pub static KERNEL_LINK: u64 = KERNEL_BASE + TEXT_OFFSET;

core::arch::global_asm!("
    .section .multiboot_header
    .set mb_magic, 0x1BADB002
    .set mb_flags, 0x10000
    .set mb_checksum, -mb_magic-mb_flags

    .long mb_magic
    .long mb_flags
    .long mb_checksum
    .long multiboot_load_addr
    .long multiboot_load_addr
    .long multiboot_load_end
    .long multiboot_bss_end
    .long multiboot_entry_addr

    # a helper macro that converts a virtual address to a physical address
    # this should be used when loading any address while paging is disabled
    .macro virtual_to_physical reg:req,  addr:req
        mov \\reg, offset \\addr - 0xFFFFFFFF80000000
    .endm

    .section .text
    .code32
    .global entry
    entry:
        cmp eax, 0x2BADB002
        jne error_halt

    # load the kernel
    tmp_loop:
        pause
        jmp tmp_loop

    error_halt:
        mov edi, 0xb8000
        virtual_to_physical esi, message
    print:
        mov al, [esi]
        cmp al, 0
        je halt_loop
        mov [edi], al
        mov byte ptr [edi+1], 12    # red on black
        add edi, 2
        inc esi
        jmp print

    halt_loop:
        pause
        jmp halt_loop

    .section .rodata
    message:
        .ascii  \"[ERROR] Not a valid multiboot result!!!\0\"
");

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
