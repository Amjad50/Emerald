extern "C" {
    static begin: usize;
    static end: usize;
}

// The virtual address of the kernel
// these are information variables, showing the memory mapping of the kernel
pub const KERNEL_BASE: usize = 0xFFFFFFFF80000000;
// memory extended start (1MB)
pub const EXTENDED_OFFSET: usize = 0x100000;
pub const KERNEL_LINK: usize = KERNEL_BASE + EXTENDED_OFFSET;

pub const PAGE_4K: usize = 0x1000;
pub const PAGE_2M: usize = 0x200000;

pub fn kernel_end() -> usize {
    (unsafe { &end } as *const usize as usize)
}

pub fn kernel_size() -> usize {
    (unsafe { &end } as *const usize as usize) - (unsafe { &begin } as *const usize as usize)
}

pub const fn align_up(value: usize, alignment: usize) -> usize {
    (value + (alignment - 1)) & !(alignment - 1)
}

pub const fn align_down(value: usize, alignment: usize) -> usize {
    value & !(alignment - 1)
}
