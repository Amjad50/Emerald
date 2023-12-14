/* 
 * This is where the boot happens.
 * The multiboot loader will load the KERNEL at 0x100000 (extended memory)
 * 
 * Here, we are starting in 32-bit protected mode, with paging disabled.
 * We will setup long mode, and then jump to the kernel.
 *
 * We have 2 regions of the ram that are important to us:
 * - [0x00000000..0x0007FFFF] - the first 512KB of ram, which is usable memory.
 *   the memory mapping we got from the BIOS gives us [0x00000000..0x0009fbff] which is 127KB more
 *   but for starters, lets use the lower 512KB until we read the memory map from the BIOS in the kernel.
 * - [0x00100000..0x????????] - this is the extended memory, and it spans to cover the whole ram 
 *   (I think, I didn't try with more than 512MB of ram)
 *
 * We are going to setup the pages tables in the first 512KB of ram.
 */

MULTIBOOT_FLAG_ALIGN_4K = 1 << 0
MULTIBOOT_FLAG_MEM_INFO = 1 << 1
MULTIBOOT_FLAG_VIDEO_INFO = 1 << 2
MULTIBOOT_FLAG_CUSTOM_LOAD_INFO = 1 << 16

MULTIBOOT_VIDEO_MODE_GRAPHICS = 0
MULTIBOOT_VIDEO_MODE_TEXT = 1

PHY_PAGE_SIZE_4K = 0x1000
PHY_PAGE_SIZE_2M = 0x200000
PAGE_TABLE_ALLOC_PAGES = 4  # only need 4

# Page tables flags
PGE_PRESENT   = 1 << 0
PGE_WRITE     = 1 << 1
PGE_USER      = 1 << 2
PGE_PAGE_SIZE = 1 << 7

# GDT flags
GDT_PRESENT   = 1 << 7
GDT_NOT_SYS   = 1 << 4
GDT_CODE      = 1 << 3
GDT_LONG_MODE = 1 << 5

# cpu registers flags
CR0_PG   = 1 << 31
CR4_PAE  = 1 << 5
EFER_LME = 1 << 8

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
.section .multiboot_header
mb_magic = 0x1BADB002
mb_flags = MULTIBOOT_FLAG_VIDEO_INFO | MULTIBOOT_FLAG_CUSTOM_LOAD_INFO | MULTIBOOT_FLAG_MEM_INFO | MULTIBOOT_FLAG_ALIGN_4K
mb_checksum = -mb_magic-mb_flags

# header
.long mb_magic
.long mb_flags
.long mb_checksum
# load address
.long multiboot_load_addr
.long multiboot_load_addr
.long multiboot_load_end
.long multiboot_bss_end
.long multiboot_entry_addr
# video info
.long MULTIBOOT_VIDEO_MODE_TEXT # mode type
.long 1024                      # width
.long 768                       # height
.long 32                        # depth

.section .text
.global entry
entry:
    cmp eax, 0x2BADB002
    jne error_halt

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

# Complete setting up the page tables
    virtual_to_physical_mov eax, boot_page_tables
    mov cr3, eax

# enable PAE
    mov eax, cr4
    or eax, CR4_PAE
    mov cr4, eax
# enable long mode
    mov ecx, 0xC0000080
    rdmsr
    or eax, EFER_LME
    wrmsr
# enable paging
    mov eax, cr0
    or eax, CR0_PG
    mov cr0, eax

# setup gdt and jump
    lgdt [gdtr64 - 0xFFFFFFFF80000000]
    ljmp 0x08, offset kernel_main_low - 0xFFFFFFFF80000000

.align 16
gdtr64:
    .word gdtr64_end - gdt64 - 1
    virtual_to_physical_put .quad, gdt64

.align 16
gdt64:
    .quad 0x0000000000000000    # null descriptor
    # Code segment (0x8)
    .long 0x00000000                               # Limit & Base (low, bits 0-15)
    .byte 0                                        # Base (mid, bits 16-23)
    .byte GDT_CODE | GDT_NOT_SYS | GDT_PRESENT     # Access
    .byte GDT_LONG_MODE                            # Flags & Limit (high, bits 16-19)
    .byte 0x00                                     # Base (high, bits 24-31)
    # Data segment (0x10)
    .long 0x00000000                               # Limit & Base (low, bits 0-15)
    .byte 0                                        # Base (mid, bits 16-23)
    .byte GDT_NOT_SYS | GDT_PRESENT                # Access
    .byte 0x00                                     # Flags & Limit (high, bits 16-19)
    .byte 0x00                                     # Base (high, bits 24-31)
gdtr64_end:


# Errors (still in 32bit)
error_halt:
    mov edi, 0xb8000
    virtual_to_physical_mov esi, message
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
    .ascii  "[ERROR] Not a valid multiboot result!!!\0"


# From here, its 64-bit code
# kernel_main_low (This is a trunk for kernel_main that is close to the `entry` so we can use a relative jump)
.align 16
.code64
.section .text
kernel_main_low:
    # clear interrupts (just in case)
    cli
    # reset the segments
    xor rax, rax
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    # setup the stack (grows downwards to 0x00000000, which gets us 64KB of stack)
    mov rax, 0xFFFFFFFF80010000
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
