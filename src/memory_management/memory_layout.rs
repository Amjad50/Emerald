use core::fmt;

extern "C" {
    static begin: usize;
    static end: usize;
    static rodata_end: usize;
}

// The virtual address of the kernel
// these are information variables, showing the memory mapping of the kernel
pub const KERNEL_BASE: usize = 0xFFFFFFFF80000000;
// memory extended start (1MB)
pub const EXTENDED_OFFSET: usize = 0x100000;
pub const KERNEL_LINK: usize = KERNEL_BASE + EXTENDED_OFFSET;
pub const KERNEL_MAPPED_SIZE: usize = 0x8000000;    // 128MB (from KERNEL_BASE)

pub const PAGE_4K: usize = 0x1000;
pub const PAGE_2M: usize = 0x200000;

pub fn kernel_end() -> usize {
    (unsafe { &end } as *const usize as usize)
}

pub fn kernel_size() -> usize {
    (unsafe { &end } as *const usize as usize) - (unsafe { &begin } as *const usize as usize)
}

pub fn kernel_rodata_end() -> usize {
    (unsafe { &rodata_end } as *const usize as usize)
}

pub fn align_up(addr: *mut u8, alignment: usize) -> *mut u8 {
    ((addr as usize + alignment - 1) & !(alignment - 1)) as *mut u8
}

pub fn align_down(addr: *mut u8, alignment: usize) -> *mut u8 {
    (addr as usize & !(alignment - 1)) as *mut u8
}

pub fn is_aligned(addr: *mut u8, alignment: usize) -> bool {
    (addr as usize & (alignment - 1)) == 0
}

#[inline(always)]
pub const fn virtual2physical(addr: usize) -> usize {
    addr - KERNEL_BASE
}

#[inline(always)]
pub const fn physical2virtual(addr: usize) -> usize {
    addr + KERNEL_BASE
}

pub struct MemSize(pub usize);

impl fmt::Display for MemSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // find the best unit
        let mut size = self.0;
        let mut unit = "B";
        if size >= 1024 {
            size /= 1024;
            unit = "KB";
        }
        if size >= 1024 {
            size /= 1024;
            unit = "MB";
        }
        if size >= 1024 {
            size /= 1024;
            unit = "GB";
        }
        if size >= 1024 {
            size /= 1024;
            unit = "TB";
        }
        size.fmt(f).and_then(|_| write!(f, "{unit}"))?;
        Ok(())
    }
}

impl fmt::Debug for MemSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
