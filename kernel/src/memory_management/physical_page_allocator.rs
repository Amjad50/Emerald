use super::memory_layout::{align_down, align_up, is_aligned, PAGE_4K};
use crate::{
    memory_management::memory_layout::{
        kernel_elf_end, physical2virtual, virtual2physical, EXTENDED_OFFSET, KERNEL_END,
        KERNEL_LINK,
    },
    multiboot2::{MemoryMapType, MultiBoot2Info},
    sync::spin::mutex::Mutex,
};

struct FreePage {
    next: *mut FreePage,
}

static mut ALLOCATOR: Mutex<PhysicalPageAllocator> = Mutex::new(PhysicalPageAllocator::empty());

pub fn init(multiboot_info: &MultiBoot2Info) {
    unsafe {
        ALLOCATOR.lock().init(multiboot_info);
    }
}

/// SAFETY: this must be called after `init`
///
/// Allocates a 4K page of memory, the returned address is guaranteed to be aligned to 4K, and is mapped into virtual space
/// Please use `virtual2physical` to get the physical address
pub unsafe fn alloc() -> *mut u8 {
    ALLOCATOR.lock().alloc()
}

/// SAFETY: this must be called after `init`
///
/// Allocates a 4K page of memory, the returned address is guaranteed to be aligned to 4K, and is mapped into virtual space
/// Please use `virtual2physical` to get the physical address
pub unsafe fn alloc_zeroed() -> *mut u8 {
    let page = alloc();
    page.write_bytes(0, PAGE_4K);
    page
}

/// SAFETY: this must be called after `init`
///
/// panics if:
/// - `page` is not a valid page
/// - `page` is already free
/// - `page` is not in the range of the allocator
/// - `page` is not aligned to 4K
pub unsafe fn free(page: *mut u8) {
    ALLOCATOR.lock().free(page);
}

pub fn stats() -> (usize, usize) {
    let allocator = unsafe { ALLOCATOR.lock() };
    (allocator.free_count, allocator.used_count)
}

struct PhysicalPageAllocator {
    low_mem_free_list_head: *mut FreePage,
    #[allow(dead_code)]
    // TODO: handle more memory
    high_mem_start: *mut u8,
    start: *mut u8,
    end: *mut u8,
    free_count: usize,
    used_count: usize,
}

impl PhysicalPageAllocator {
    const fn empty() -> Self {
        Self {
            low_mem_free_list_head: core::ptr::null_mut(),
            high_mem_start: core::ptr::null_mut(),
            start: core::ptr::null_mut(),
            end: core::ptr::null_mut(),
            free_count: 0,
            used_count: 0,
        }
    }

    fn init(&mut self, multiboot_info: &MultiBoot2Info) {
        const PHYSICAL_KERNEL_START: u64 = virtual2physical(KERNEL_LINK);
        // get the end of the kernel, align, and add 5 PAGES of alignment as well
        // because the multiboot info might be stored there by grub
        let mut physical_kernel_end = virtual2physical(align_up(kernel_elf_end(), PAGE_4K));
        let multiboot_end = align_up(
            virtual2physical(multiboot_info.end_address() as usize) as usize,
            PAGE_4K,
        ) as u64;
        println!("multiboot end: {multiboot_end:x}",);
        println!(
            "physical_kernel_start: {:p}",
            PHYSICAL_KERNEL_START as *mut u8
        );
        println!("physical_kernel_end: {:p}", physical_kernel_end as *mut u8);

        // if the multiboot info is after the kernel, make sure we are not allocating it
        if multiboot_end > physical_kernel_end {
            // if its after the kernel by a lot, then panic, so we can handle it, we don't want
            // to make this more complex if we don't need to
            assert!(
                multiboot_end - physical_kernel_end < PAGE_4K as u64 * 5,
                "Multiboot is after the kernel by a lot",
            );
            physical_kernel_end = multiboot_end;
        }

        for memory in multiboot_info.memory_maps().unwrap() {
            // skip all the memory before the kernel, it could be used by the bootloader
            // its generally not a lot, just 1 MB, so its fine to skip it
            if (memory.base_addr + memory.length) < EXTENDED_OFFSET as u64 {
                continue;
            }
            if memory.mem_type != MemoryMapType::Available {
                continue;
            }
            // if this is the range where the kernel is mapped, skip the pages the kernel use
            // and start after that
            let start_physical;
            let end_physical;
            if memory.base_addr <= PHYSICAL_KERNEL_START
                && (memory.base_addr + memory.length) >= physical_kernel_end
            {
                start_physical = physical_kernel_end;
                end_physical =
                    align_down(memory.base_addr as usize + memory.length as usize, PAGE_4K) as u64;
                self.start = physical2virtual(physical_kernel_end) as _;
            } else {
                assert!(memory.base_addr >= physical_kernel_end);

                start_physical = align_up(memory.base_addr as usize, PAGE_4K) as u64;
                end_physical =
                    align_down(memory.base_addr as usize + memory.length as usize, PAGE_4K) as u64;
            }
            let mut high_mem_start = core::ptr::null_mut();
            let end_virtual = if end_physical >= virtual2physical(KERNEL_END) as _ {
                high_mem_start = KERNEL_END as *mut u8;
                KERNEL_END as *mut u8
            } else {
                physical2virtual(end_physical) as _
            };
            let start_virtual = physical2virtual(start_physical) as _;

            if start_virtual < end_virtual {
                self.end = end_virtual;

                self.init_range(start_virtual, end_virtual);
                if !high_mem_start.is_null() {
                    self.high_mem_start = high_mem_start;
                    break;
                }
            }
        }
    }

    fn init_range(&mut self, start: *mut u8, end: *mut u8) {
        println!("init physical pages: [{:p}, {:p})", start, end);
        let start = align_up(start as usize, PAGE_4K) as _;
        let end = align_down(end as usize, PAGE_4K) as _;
        assert!(start < end);
        let mut page = start;
        while page < end {
            unsafe { self.free(page) };
            page = unsafe { page.add(PAGE_4K) };
        }
    }

    /// SAFETY: this must be called after `init`
    ///
    /// Allocates a 4K page of memory
    unsafe fn alloc(&mut self) -> *mut u8 {
        if self.low_mem_free_list_head.is_null() {
            panic!("out of memory");
        }

        let page = self.low_mem_free_list_head;
        self.low_mem_free_list_head = (*page).next;

        let page = page as *mut u8;
        // fill with random data to catch dangling pointer bugs
        page.write_bytes(1, PAGE_4K);
        self.used_count += 1;
        page
    }

    /// SAFETY: this must be called after `init`
    ///
    /// panics if:
    /// - `page` is not a valid page
    /// - `page` is already free
    /// - `page` is not in the range of the allocator
    /// - `page` is not aligned to 4K
    unsafe fn free(&mut self, page: *mut u8) {
        // fill with random data to catch dangling pointer bugs
        page.write_bytes(2, PAGE_4K);

        let page = page as *mut FreePage;

        if page.is_null()
            || !is_aligned(page as _, PAGE_4K)
            || page > unsafe { page.add(1) }
            || page >= self.end as _
            || page < self.start as _
        {
            panic!("freeing invalid page: {:p}", page);
        }
        // TODO: for now make sure we are not freeing the high memory for now
        assert!(self.high_mem_start.is_null() || page < self.high_mem_start as _);

        (*page).next = self.low_mem_free_list_head;
        self.low_mem_free_list_head = page;
        self.free_count += 1;
    }
}
