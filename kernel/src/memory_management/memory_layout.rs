use core::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

extern "C" {
    static begin: usize;
    static end: usize;
    static rodata_end: usize;
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
pub const INTR_STACK_SIZE: usize = PAGE_4K * 4;
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
pub const KERNEL_EXTRA_MEMORY_SIZE: usize = DEVICE_BASE_VIRTUAL - KERNEL_EXTRA_MEMORY_BASE;
static KERNEL_EXTRA_MEMORY_USED_PAGES: AtomicUsize = AtomicUsize::new(0);

// Where to map IO memory, and memory mapped devices in virtual space
// the reason this is hear, is that these registers are at the bottom of the physica memory
// and converting those addresses to `virtual(kernel)` addresses, will result in an overflow
// so this is a special range for them
//
// When looking at the CPU docs, looks like the important addresses such as the APIC, are located
// at the end, around 0xFxxxxxxx place, but we use `0xDxxxxxxx` to cover more if we need to.
// Since anyway we are not using this memory
pub const DEVICE_BASE_VIRTUAL: usize = 0xFFFF_FFFF_D000_0000;
pub const DEVICE_BASE_PHYSICAL: usize = 0x0000_0000_D000_0000;
// not inclusive, we want to map until 0xFFFF_FFFF
pub const DEVICE_PHYSICAL_END: usize = 0x0000_0001_0000_0000;

pub const PAGE_4K: usize = 0x1000;
pub const PAGE_2M: usize = 0x20_0000;

pub fn kernel_elf_end() -> usize {
    (unsafe { &end } as *const usize as usize)
}

#[allow(dead_code)]
pub fn kernel_elf_size() -> usize {
    (unsafe { &end } as *const usize as usize) - (unsafe { &begin } as *const usize as usize)
}

pub fn kernel_elf_rodata_end() -> usize {
    (unsafe { &rodata_end } as *const usize as usize)
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

#[inline(always)]
pub const fn virtual2physical(addr: usize) -> usize {
    addr - KERNEL_BASE
}

#[inline(always)]
pub const fn physical2virtual(addr: usize) -> usize {
    addr + KERNEL_BASE
}

#[inline(always)]
#[allow(dead_code)]
pub const fn physical2virtual_io(addr: usize) -> usize {
    addr - DEVICE_BASE_PHYSICAL + DEVICE_BASE_VIRTUAL
}

// Gets the virtual address of free virtual memory that anyone can use
// These pages are not returned at all
// this just provide, a kind of dynamic nature in mapping physical
// memory that we don't really care where they are mapped virtually
pub unsafe fn allocate_from_extra_kernel_pages(pages: usize) -> *mut u8 {
    let mut used_pages = KERNEL_EXTRA_MEMORY_USED_PAGES.load(Ordering::Relaxed);
    loop {
        let new_used_pages = used_pages + pages;
        assert!(
            new_used_pages <= KERNEL_EXTRA_MEMORY_SIZE / PAGE_4K,
            "out of extra kernel virtual memory"
        );
        match KERNEL_EXTRA_MEMORY_USED_PAGES.compare_exchange_weak(
            used_pages,
            new_used_pages,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => {
                return (KERNEL_EXTRA_MEMORY_BASE + used_pages * PAGE_4K) as _;
            }
            Err(x) => used_pages = x,
        }
    }
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
