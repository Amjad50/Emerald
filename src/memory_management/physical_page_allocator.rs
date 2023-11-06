use super::memory_layout::{align_down, align_up, is_aligned, PAGE_4K};
use crate::sync::spin::mutex::Mutex;

struct FreePage {
    next: *mut FreePage,
}

static mut ALLOCATOR: Mutex<PhysicalPageAllocator> = Mutex::new(PhysicalPageAllocator::empty());

pub fn init(start: *mut u8, end: *mut u8) {
    unsafe {
        ALLOCATOR.lock().init(start, end);
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
    free_list_head: *mut FreePage,
    start: *mut u8,
    end: *mut u8,
    free_count: usize,
    used_count: usize,
}

impl PhysicalPageAllocator {
    const fn empty() -> Self {
        Self {
            free_list_head: core::ptr::null_mut(),
            start: core::ptr::null_mut(),
            end: core::ptr::null_mut(),
            free_count: 0,
            used_count: 0,
        }
    }

    fn init(&mut self, start: *mut u8, end: *mut u8) {
        let start = align_up(start, PAGE_4K);
        let end = align_down(end, PAGE_4K);
        assert!(start < end);

        self.start = start;
        self.end = end;

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
        if self.free_list_head.is_null() {
            panic!("out of memory");
        }

        let page = self.free_list_head;
        self.free_list_head = (*page).next;

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

        (*page).next = self.free_list_head;
        self.free_list_head = page;
        self.free_count += 1;
    }
}
