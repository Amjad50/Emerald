use core::fmt;

extern "C" {
    static begin: usize;
    static end: usize;
    static rodata_end: usize;
}

pub const ONE_MB: usize = 1024 * 1024;

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

// The BIOS has some information that is located after the end of the middle chunk of the ram (~3GB)
// the issue is that its not at the same place and depend on the ram
// so we will map it to the same virtual space
//
// The bios already has some data before `EXTENDED_OFFSET`, but that is fine, since we can map it to kernel space easily.
pub const EXTENDED_BIOS_BASE_VIRTUAL: usize = 0xFFFF_FFFF_C000_0000;
pub static mut EXTENDED_BIOS_BASE_PHYSICAL: usize = 0;
pub static mut EXTENDED_BIOS_SIZE: usize = 0;

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

pub fn align_up(addr: usize, alignment: usize) -> usize {
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

#[allow(dead_code)]
pub fn physical2virtual_bios(addr: usize) -> usize {
    let base_physical = unsafe { EXTENDED_BIOS_BASE_PHYSICAL };
    if addr > base_physical {
        addr - base_physical + EXTENDED_BIOS_BASE_VIRTUAL
    } else {
        addr + KERNEL_BASE
    }
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
