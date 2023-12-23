use core::fmt;

extern "C" {
    static begin: usize;
    static end: usize;
    static text_end: usize;
    static rodata_end: usize;
    static data_end: usize;
    static stack_guard_page: usize;
}

// it starts at 0x10000, which is where the kernel is loaded, and grows down
pub const KERNEL_STACK_END: usize = 0xFFFF_FFFF_8001_0000;
// The virtual address of the kernel
// these are information variables, showing the memory mapping of the kernel
pub const KERNEL_BASE: usize = 0xFFFF_FFFF_8000_0000;
// memory extended start (1MB)
pub const EXTENDED_OFFSET: usize = 0x10_0000;
pub const KERNEL_LINK: usize = KERNEL_BASE + EXTENDED_OFFSET;
// 128MB (from KERNEL_BASE), and this indicates the address of the end of the kernel
// every memory used in the kernel, allocated or no, comes from the kernel memory
pub const KERNEL_MAPPED_SIZE: usize = 0x800_0000;
pub const KERNEL_END: usize = KERNEL_BASE + KERNEL_MAPPED_SIZE;

// The heap of the kernel
// this is mapped from the physical memory of the kernel
// so we are using the physical pages from the kernel space.
pub const KERNEL_HEAP_BASE: usize = KERNEL_END;
pub const KERNEL_HEAP_SIZE: usize = 0x100_0000; // 16MB

// The size of the stack for interrupt handlers
pub const INTR_STACK_SIZE: usize = PAGE_4K * 8;
pub const INTR_STACK_EMPTY_SIZE: usize = PAGE_4K;
pub const INTR_STACK_ENTRY_SIZE: usize = INTR_STACK_SIZE + INTR_STACK_EMPTY_SIZE;
pub const INTR_STACK_BASE: usize = KERNEL_HEAP_BASE + KERNEL_HEAP_SIZE;
pub const INTR_STACK_COUNT: usize = 7;
// we are going to setup a spacing at the end of the stack, so that we can detect stack overflows
pub const INTR_STACK_TOTAL_SIZE: usize = INTR_STACK_ENTRY_SIZE * INTR_STACK_COUNT;

// extra space that we can make virtual memory to when we don't care where we want to map it
// this is only in kernel space, as userspace programs should be mapped into the rest of the memory range
// that is below `KERNEL_BASE`
pub const KERNEL_EXTRA_MEMORY_BASE: usize = INTR_STACK_BASE + INTR_STACK_TOTAL_SIZE;
// to avoid overflow stuff, we don't use the last page
pub const KERNEL_LAST_POSSIBLE_ADDR: usize = 0xFFFF_FFFF_FFFF_F000;
pub const KERNEL_EXTRA_MEMORY_SIZE: usize = KERNEL_LAST_POSSIBLE_ADDR - KERNEL_EXTRA_MEMORY_BASE;

pub const PAGE_4K: usize = 0x1000;
pub const PAGE_2M: usize = 0x20_0000;

pub fn kernel_elf_end() -> usize {
    (unsafe { &end } as *const usize as usize)
}

#[allow(dead_code)]
pub fn kernel_elf_size() -> usize {
    (unsafe { &end } as *const usize as usize) - (unsafe { &begin } as *const usize as usize)
}

pub fn kernel_text_end() -> usize {
    (unsafe { &text_end } as *const usize as usize)
}

pub fn kernel_elf_rodata_end() -> usize {
    (unsafe { &rodata_end } as *const usize as usize)
}

pub fn kernel_elf_data_end() -> usize {
    (unsafe { &data_end } as *const usize as usize)
}

pub fn stack_guard_page_ptr() -> usize {
    (unsafe { &stack_guard_page } as *const usize as usize)
}

pub const fn align_up(addr: usize, alignment: usize) -> usize {
    (addr + alignment - 1) & !(alignment - 1)
}

pub fn align_down(addr: usize, alignment: usize) -> usize {
    addr & !(alignment - 1)
}

pub fn is_aligned(addr: usize, alignment: usize) -> bool {
    (addr & (alignment - 1)) == 0
}

pub fn align_range(addr: usize, size: usize, alignment: usize) -> (usize, usize, usize) {
    let addr_end: usize = addr + size;
    let start_aligned = align_down(addr, alignment);
    let end_aligned = align_up(addr_end, alignment);
    let size = end_aligned - start_aligned;
    assert!(size > 0);
    assert!(is_aligned(size, alignment));
    let offset = addr - start_aligned;

    (start_aligned, size, offset)
}

#[inline(always)]
pub const fn virtual2physical(addr: usize) -> usize {
    addr - KERNEL_BASE
}

#[inline(always)]
pub const fn physical2virtual(addr: usize) -> usize {
    addr + KERNEL_BASE
}

#[allow(dead_code)]
pub fn display_kernel_map() {
    println!("Kernel map:");
    let nothing = KERNEL_BASE..KERNEL_LINK;
    let kernel_elf_end = align_up(kernel_elf_end(), PAGE_4K);
    let kernel_elf = KERNEL_LINK..kernel_elf_end;
    let kernel_elf_text = KERNEL_LINK..kernel_text_end();
    let kernel_elf_rodata = kernel_text_end()..kernel_elf_rodata_end();
    let kernel_elf_data = kernel_elf_rodata_end()..kernel_elf_data_end();
    let kernel_elf_bss = kernel_elf_data_end()..kernel_elf_end;
    let kernel_physical_allocator_low = kernel_elf_end..KERNEL_END;
    let kernel_heap = KERNEL_HEAP_BASE..KERNEL_HEAP_BASE + KERNEL_HEAP_SIZE;
    let interrupt_stack = INTR_STACK_BASE..INTR_STACK_BASE + INTR_STACK_TOTAL_SIZE;
    let kernel_extra_memory =
        KERNEL_EXTRA_MEMORY_BASE..KERNEL_EXTRA_MEMORY_BASE + KERNEL_EXTRA_MEMORY_SIZE;

    println!(
        "  range={:016x}..{:016x}, len={:4}  nothing",
        nothing.start,
        nothing.end,
        MemSize(nothing.len() as u64)
    );
    println!(
        "  range={:016x}..{:016x}, len={:4}  kernel elf",
        kernel_elf.start,
        kernel_elf.end,
        MemSize(kernel_elf.len() as u64)
    );
    // inner map for the elf
    println!(
        "    range={:016x}..{:016x}, len={:4}  kernel elf text",
        kernel_elf_text.start,
        kernel_elf_text.end,
        MemSize(kernel_elf_text.len() as u64)
    );
    println!(
        "    range={:016x}..{:016x}, len={:4}  kernel elf rodata",
        kernel_elf_rodata.start,
        kernel_elf_rodata.end,
        MemSize(kernel_elf_rodata.len() as u64)
    );
    println!(
        "    range={:016x}..{:016x}, len={:4}  kernel elf data",
        kernel_elf_data.start,
        kernel_elf_data.end,
        MemSize(kernel_elf_data.len() as u64)
    );
    println!(
        "    range={:016x}..{:016x}, len={:4}  kernel elf bss",
        kernel_elf_bss.start,
        kernel_elf_bss.end,
        MemSize(kernel_elf_bss.len() as u64)
    );
    println!(
        "  range={:016x}..{:016x}, len={:4}  kernel physical allocator low",
        kernel_physical_allocator_low.start,
        kernel_physical_allocator_low.end,
        MemSize(kernel_physical_allocator_low.len() as u64)
    );
    println!(
        "  range={:016x}..{:016x}, len={:4}  kernel heap",
        kernel_heap.start,
        kernel_heap.end,
        MemSize(kernel_heap.len() as u64)
    );
    println!(
        "  range={:016x}..{:016x}, len={:4}  interrupt stack",
        interrupt_stack.start,
        interrupt_stack.end,
        MemSize(interrupt_stack.len() as u64)
    );
    println!(
        "  range={:016x}..{:016x}, len={:4}  kernel extra (virtual space)",
        kernel_extra_memory.start,
        kernel_extra_memory.end,
        MemSize(kernel_extra_memory.len() as u64)
    );

    // number of bytes approx used from physical memory
    println!(
        "whole kernel physical size (startup/low): {}",
        MemSize((KERNEL_END - KERNEL_BASE) as u64)
    );
    // total addressable virtual kernel memory
    println!(
        "whole kernel size: {}",
        MemSize(u64::MAX - KERNEL_BASE as u64 + 1)
    );
}

#[repr(transparent)]
pub struct MemSize(pub u64);

impl fmt::Display for MemSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // find the best unit
        let mut size = self.0;
        let mut remaining = 0;
        let mut unit = "B";
        if size >= 1024 {
            remaining = size % 1024;
            size /= 1024;
            unit = "KB";
        }
        if size >= 1024 {
            remaining = size % 1024;
            size /= 1024;
            unit = "MB";
        }
        if size >= 1024 {
            remaining = size % 1024;
            size /= 1024;
            unit = "GB";
        }
        if size >= 1024 {
            remaining = size % 1024;
            size /= 1024;
            unit = "TB";
        }
        if size >= 1024 {
            remaining = size % 1024;
            size /= 1024;
            unit = "PB";
        }

        size.fmt(f).and_then(|_| {
            let remaining = remaining * 100 / 1024;
            write!(f, ".{remaining:02}")?;
            write!(f, "{unit}")
        })
    }
}

impl fmt::Debug for MemSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
