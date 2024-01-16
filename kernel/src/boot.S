/* 
 * This is where the boot happens.
 * The multiboot2 loader will load the KERNEL at 0x100000 (extended memory)
 * 
 * Here, we are starting in 32-bit protected mode, with paging disabled.
 * We will setup long mode, and then jump to the kernel.
 *
 * We are setting up the virtual memory pages in the `.boot_page_tables` section.
 * This is inside the `.data` section with `PAGE_TABLE_ALLOC_PAGES` pages in size.
 */

MULTIBOOT2_CHECK_MAGIC = 0x36d76289

PHY_PAGE_SIZE_4K = 0x1000
PHY_PAGE_SIZE_2M = 0x200000
PAGE_TABLE_ALLOC_PAGES = 4  # only need 4

STACK_SIZE_PAGES = 128

# Page tables flags
PGE_PRESENT   = 1 << 0
PGE_WRITE     = 1 << 1
PGE_USER      = 1 << 2
PGE_PAGE_SIZE = 1 << 7

# GDT flags
GDT_WRITE     = 1 << 1
GDT_CODE      = 1 << 3
GDT_NOT_SYS   = 1 << 4
GDT_LONG_MODE = 1 << 5
GDT_PRESENT   = 1 << 7

# cpu registers flags
CR0_PE   = 1 << 0
CR0_PG   = 1 << 31
CR4_PAE  = 1 << 5
EFER_LME = 1 << 8

EFER_REG = 0xC0000080

CPUID_FEAT_CMD        = 0x1
CPUID_FEAT_EX_CMD     = 0x80000001
# Physical Address Extension available
CPUID_FEAT_EDX_PAE    = 1 << 6
# Intel 64/Long Mode available
CPUID_FEAT_EX_EDX_LM  = 1 << 29

# some helper macros that converts a virtual address to a physical address
# this should be used when loading any address while paging is disabled
.macro virtual_to_physical_mov reg:req,  addr:req
    mov \reg, offset \addr - 0xFFFFFFFF80000000
.endm

.macro virtual_to_physical_put type:req,  addr:req
    \type \addr - 0xFFFFFFFF80000000
.endm


# Start here
.code32
.section .multiboot2_header
mb_magic = 0xE85250D6   # multiboot2 magic number
mb_arch = 0x00000000    # x86
# header
mb2_header_start:
    .long mb_magic
    .long mb_arch
    .long mb2_header_end - mb2_header_start # header length
    .long -mb_magic-mb_arch-(mb2_header_end-mb2_header_start) # checksum
    # tags
    .align 8
    mb2_information_request_start:
        .short 1 # type (information request)
        .short 0 # flags
        .long mb2_information_request_end - mb2_information_request_start # size
    mb2_information_request_end:
    .align 8
    mb2_address_tag_start:
        .short 2 # type (address)
        .short 0 # flags
        .long mb2_address_tag_end - mb2_address_tag_start # size
        .long multiboot_load_addr
        .long multiboot_load_addr
        .long multiboot_load_end
        .long multiboot_bss_end
    mb2_address_tag_end:
    .align 8
    mb2_entry_address_tag_start:
        .short 3 # type (entry address)
        .short 0 # flags
        .long mb2_entry_address_tag_end - mb2_entry_address_tag_start # size
        .long multiboot_entry_addr
    mb2_entry_address_tag_end:
    .align 8
    mb2_module_align_tag_start:
        .short 6 # type (module align)
        .short 0 # flags
        .long mb2_module_align_tag_end - mb2_module_align_tag_start # size
    mb2_module_align_tag_end:
    .align 8
    mb2_end_tag_start:
        .short 0 # type (end)
        .short 0 # flags
        .long mb2_end_tag_end - mb2_end_tag_start # size
    mb2_end_tag_end:
mb2_header_end:

.section .text
.global entry
entry:
    cmp eax, MULTIBOOT2_CHECK_MAGIC
    jne not_multiboot2
    # check if we are running on a 64-bit CPU
    # save ebx, since its important containing the multiboot info
    # no stack yet, so lets use registers
    mov edi, ebx
    # check for PEA
    mov eax, CPUID_FEAT_CMD
    cpuid
    test edx, CPUID_FEAT_EDX_PAE
    jz not_64bit
    # check for long mode
    mov eax, CPUID_FEAT_EX_CMD
    cpuid
    test edx, CPUID_FEAT_EX_EDX_LM
    jz not_64bit

    # restore ebx
    mov ebx, edi

# load the kernel
# -- Setup paging --
# This will setup mapping the first 128MB of the ram to both of the following ranges
#   Will use 2MB pages for now, if we want to use 4KB pages, we need to setup 512 page tables
#   which will take around 2MB of space in the ram. So will do that later in the kernel if we need to.
# - [0x0000000000000000..0x0000000007FFFFFF] - 1:1 mapping with the physical pages
#     - This will be:
#         - PML4[0]
#         - PDPT[0]
#         - PDT[0..63] # 2MB each
# - [0xFFFFFFFF80000000..0xFFFFFFFF87FFFFFF] - the kernel ELF file virtual address space
#     - This will be:
#         - PML4[511]
#         - PDPT[510]
#         - PDT[0..63] # 2MB each (shared with the above)
#
# IMPORTANT: This is only for setup, we will change the page tables in the kernel

# PML4 (edi=boot_page_tables[0])
# PML4[0] ->   PDPT-A (esi=boot_page_tables[1])
    virtual_to_physical_mov edi, boot_page_tables
    lea esi, [edi + PHY_PAGE_SIZE_4K]
    or esi, PGE_PRESENT | PGE_WRITE
    mov eax, esi
    mov [edi], eax
# PML4[511] -> PDPT-B (esi=boot_page_tables[2])
    lea esi, [edi + PHY_PAGE_SIZE_4K * 2]
    or esi, PGE_PRESENT | PGE_WRITE
    mov eax, esi
    mov [edi + 8 * 511], eax

# PDPT-A (edi=boot_page_tables[1])
# PDPT-A[0] -> PDT (esi=boot_page_tables[3])
    virtual_to_physical_mov eax, boot_page_tables
    lea edi, [eax + PHY_PAGE_SIZE_4K]
    lea esi, [eax + PHY_PAGE_SIZE_4K * 3]
    or esi, PGE_PRESENT | PGE_WRITE
    mov eax, esi
    mov [edi], eax
# PDPT-B (edi=boot_page_tables[2])
# PDPT-B[510] -> PDT (esi=boot_page_tables[3])
    virtual_to_physical_mov eax, boot_page_tables
    lea edi, [eax + PHY_PAGE_SIZE_4K * 2]
    or esi, PGE_PRESENT | PGE_WRITE
    mov eax, esi
    mov [edi + 8 * 510], eax
# PDT (edi=boot_page_tables[3])
# PDT[0..63] -> 2MB pages (0x0000000..0x7FFFFFF)
    virtual_to_physical_mov eax, boot_page_tables
    lea edi, [eax + PHY_PAGE_SIZE_4K * 3]
    mov eax, 0x0000000 | PGE_PRESENT | PGE_WRITE | PGE_PAGE_SIZE
    mov ecx, 64
fill_pdt_loop:
    mov [edi], eax
    add eax, PHY_PAGE_SIZE_2M
    add edi, 8
    loop fill_pdt_loop

# Setup empty IDT
    lidt [idtr - 0xFFFFFFFF80000000]

# enable PAE
    mov eax, cr4
    or eax, CR4_PAE
    mov cr4, eax
# Complete setting up the page tables
    virtual_to_physical_mov eax, boot_page_tables
    mov cr3, eax
# enable long mode
    mov ecx, EFER_REG
    rdmsr
    or eax, EFER_LME
    wrmsr
# enable paging, and protection (should be enabled already)
    mov eax, cr0
    or eax, CR0_PG | CR0_PE
    mov cr0, eax

# setup gdt and jump
    lgdt [gdtr64 - 0xFFFFFFFF80000000]
    jmp 0x08, offset kernel_main_low - 0xFFFFFFFF80000000

.align 16
idtr:
    .word 0
    .long 0

.align 16
gdtr64:
    .word gdtr64_end - gdt64 - 1
    virtual_to_physical_put .long, gdt64

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
gdtr64_end:


not_multiboot2:
    virtual_to_physical_mov esi, message_not_valid_multiboot
    jmp error_halt
not_64bit:
    virtual_to_physical_mov esi, message_not_64bit
    jmp error_halt
# Errors (still in 32bit)
error_halt:
    mov edi, 0xb8000
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
message_not_valid_multiboot:
    .ascii  "[ERROR] Not a valid multiboot result!!!\0"
message_not_64bit:
    .ascii  "[ERROR] Not a 64-bit CPU!!!\0"

# From here, its 64-bit code
# kernel_main_low (This is a trunk for kernel_main that is close to the `entry` so we can use a relative jump)
.align 16
.code64
.section .text
kernel_main_low:
    # clear interrupts (just in case)
    cli
    # set the data segment
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    # setup the stack (grows downwards to stack_guard_page)
    mov rax, offset stack_end - 8
    mov rsp, rax

    # (first argument) rdi = multiboot info (we haven't toched ebx, so it should have the same value since `entry`)
    mov rdi, rbx
    # convert to virtual address
    add rdi, 0xFFFFFFFF80000000

    mov rax, offset kernel_main
    jmp rax

# place where we have a temporary page tables
.section .boot_page_tables
.align PHY_PAGE_SIZE_4K
boot_page_tables:
    .space PHY_PAGE_SIZE_4K * PAGE_TABLE_ALLOC_PAGES, 0

.section .stack
.align PHY_PAGE_SIZE_4K
.global stack_guard_page
stack_guard_page:
    .space PHY_PAGE_SIZE_4K, 0
.align PHY_PAGE_SIZE_4K
stack:
    .space PHY_PAGE_SIZE_4K * STACK_SIZE_PAGES, 0
stack_end: