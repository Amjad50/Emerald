use core::ptr::NonNull;

use super::memory_layout::{align_down, align_up, is_aligned, PAGE_4K};
use crate::{
    memory_management::memory_layout::{
        kernel_elf_end, physical2virtual, virtual2physical, EXTENDED_OFFSET, KERNEL_END,
        KERNEL_LINK,
    },
    multiboot2::{MemoryMapType, MultiBoot2Info},
    sync::{once::OnceLock, spin::mutex::Mutex},
    testing,
};

struct FreePage {
    next: Option<NonNull<FreePage>>,
}

static ALLOCATOR: OnceLock<Mutex<PhysicalPageAllocator>> = OnceLock::new();

pub fn init(multiboot_info: &MultiBoot2Info) {
    if ALLOCATOR.try_get().is_some() {
        panic!("PhysicalPageAllocator already initialized");
    }

    ALLOCATOR.get_or_init(|| Mutex::new(PhysicalPageAllocator::new(multiboot_info)));
}

/// SAFETY: this must be called after `init`
///
/// Allocates a 4K page of memory, the returned address is guaranteed to be aligned to 4K, and is mapped into virtual space
/// Please use `virtual2physical` to get the physical address
pub unsafe fn alloc() -> *mut u8 {
    ALLOCATOR.get().lock().alloc()
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

/// SAFETY:
/// this must be called after `init`
/// this must never be called with same page twice, the allocator doesn't check itself
///
/// panics if:
/// - `page` is not a valid page
/// - `page` is not in the range of the allocator
/// - `page` is not aligned to 4K
pub unsafe fn free(page: *mut u8) {
    let r = { ALLOCATOR.get().lock().free(page) };
    r.unwrap_or_else(|| panic!("Page {page:p} not valid"))
}

pub fn stats() -> (usize, usize) {
    let allocator = ALLOCATOR.get().lock();
    (allocator.free_count, allocator.used_count)
}

struct PhysicalPageAllocator {
    low_mem_free_list_head: Option<NonNull<FreePage>>,
    #[allow(dead_code)]
    // TODO: handle more memory
    high_mem_start: usize,
    start: usize,
    end: usize,
    free_count: usize,
    used_count: usize,
}

unsafe impl Send for PhysicalPageAllocator {}

impl PhysicalPageAllocator {
    fn new(multiboot_info: &MultiBoot2Info) -> Self {
        const PHYSICAL_KERNEL_START: u64 = virtual2physical(KERNEL_LINK);
        // get the end of the kernel, align, and add 5 PAGES of alignment as well
        // because the multiboot info might be stored there by grub
        let mut physical_kernel_end = virtual2physical(align_up(kernel_elf_end(), PAGE_4K));
        let multiboot_end = align_up(
            virtual2physical(multiboot_info.end_address() as usize),
            PAGE_4K,
        );
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

        let mut s = Self {
            low_mem_free_list_head: None,
            high_mem_start: 0,
            start: 0,
            end: 0,
            free_count: 0,
            used_count: 0,
        };

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
                end_physical = align_down(memory.base_addr + memory.length, PAGE_4K);
                s.start = physical2virtual(physical_kernel_end);
            } else {
                assert!(memory.base_addr >= physical_kernel_end);

                start_physical = align_up(memory.base_addr, PAGE_4K);
                end_physical = align_down(memory.base_addr + memory.length, PAGE_4K);
            }
            let mut high_mem_start = 0;
            let end_virtual = if end_physical >= virtual2physical(KERNEL_END) {
                high_mem_start = KERNEL_END;
                KERNEL_END
            } else {
                physical2virtual(end_physical)
            };
            let start_virtual = physical2virtual(start_physical);

            if start_virtual < end_virtual {
                s.end = end_virtual;

                s.init_range(start_virtual as _, end_virtual as _);
                if high_mem_start != 0 {
                    s.high_mem_start = high_mem_start;
                    break;
                }
            }
        }
        s
    }

    fn init_range(&mut self, start: *mut u8, end: *mut u8) {
        println!("init physical pages: [{:p}, {:p})", start, end);
        let start = align_up(start as usize, PAGE_4K) as _;
        let end = align_down(end as usize, PAGE_4K) as _;
        assert!(start < end);
        let mut page = start;
        while page < end {
            unsafe { self.free(page).expect("valid page") };
            page = unsafe { page.add(PAGE_4K) };
        }
    }

    /// SAFETY: this must be called after `init`
    ///
    /// Allocates a 4K page of memory
    unsafe fn alloc(&mut self) -> *mut u8 {
        let Some(low_mem_free_list_head) = self.low_mem_free_list_head else {
            panic!("out of memory");
        };

        let page = low_mem_free_list_head;
        self.low_mem_free_list_head = page.as_ref().next;

        let page = page.as_ptr() as *mut u8;
        // fill with random data to catch dangling pointer bugs
        page.write_bytes(1, PAGE_4K);
        self.used_count += 1;
        page
    }

    /// SAFETY:
    /// this must be called after `init`
    /// this must never be called with same page twice, the allocator doesn't check itself
    ///
    /// fails if:
    /// - `page` is null
    /// - `page` is not in the range of the allocator
    /// - `page` is not aligned to 4K
    /// with `None`, otherwise, `Some(())`
    #[must_use]
    unsafe fn free(&mut self, page: *mut u8) -> Option<()> {
        let page = page.cast::<FreePage>();

        if page.is_null()
            || !is_aligned(page as usize, PAGE_4K)
            || page >= self.end as _
            || page < self.start as _
        {
            return None;
        }

        // fill with random data to catch dangling pointer bugs
        page.cast::<u8>().write_bytes(2, PAGE_4K);
        // TODO: for now make sure we are not freeing the high memory for now
        assert!(self.high_mem_start == 0 || page < self.high_mem_start as _);
        let mut page = NonNull::new_unchecked(page);

        page.as_mut().next = self.low_mem_free_list_head;
        self.low_mem_free_list_head = Some(page);
        self.free_count += 1;
        Some(())
    }
}

testing::test! {
    fn test_general() {
        let page1 = unsafe { alloc() };
        let page2 = unsafe { alloc() };
        let page3 = unsafe { alloc() };

        // make sure its aligned
        assert_eq!(page1 as usize % PAGE_4K, 0);
        assert_eq!(page2 as usize % PAGE_4K, 0);
        assert_eq!(page3 as usize % PAGE_4K, 0);

        // make sure its after one another in reverse
        assert_eq!(page1 as usize, page2 as usize + PAGE_4K);
        assert_eq!(page2 as usize, page3 as usize + PAGE_4K);

        // make sure the content are 1
        assert!(unsafe { core::slice::from_raw_parts(page1, PAGE_4K) }
            .iter()
            .all(|&x| x == 1),);
        assert!(unsafe { core::slice::from_raw_parts(page2, PAGE_4K) }
            .iter()
            .all(|&x| x == 1),);
        assert!(unsafe { core::slice::from_raw_parts(page3, PAGE_4K) }
            .iter()
            .all(|&x| x == 1),);

        let zeros = unsafe { alloc_zeroed() };
        assert!(unsafe { core::slice::from_raw_parts(zeros, PAGE_4K) }
            .iter()
            .all(|&x| x == 0),);

        unsafe {
            free(page1);
            free(page2);
            free(page3);
            free(zeros);
        }
    }

    fn test_free_realloc() {
        let page = unsafe { alloc() };
        let addr = page as usize;

        unsafe { free(page) };

        let page2 = unsafe { alloc() };

        assert_eq!(page as usize, addr);

        unsafe { free(page2) };
    }

    #[should_panic]
    fn test_unaligned_free() {
        let page = unsafe { alloc() };

        let addr_inside_page = unsafe { page.add(1) };

        unsafe { free(addr_inside_page) };
    }
}
