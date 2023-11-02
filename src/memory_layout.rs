extern "C" {
    static begin: u64;
    static end: u64;
}

// The virtual address of the kernel
// these are information variables, showing the memory mapping of the kernel
pub static KERNEL_BASE: u64 = 0xFFFFFFFF80000000;
// memory extended start (1MB)
pub static EXTENDED_OFFSET: u64 = 0x100000;
pub static KERNEL_LINK: u64 = KERNEL_BASE + EXTENDED_OFFSET;

pub fn kernel_end() -> u64 {
    (unsafe { &end } as *const u64 as u64)
}

pub fn kernel_size() -> u64 {
    (unsafe { &end } as *const u64 as u64) - (unsafe { &begin } as *const u64 as u64)
}
