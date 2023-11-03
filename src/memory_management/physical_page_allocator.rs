use super::memory_layout::{align_up, PAGE_4K};
use crate::sync::spin::mutex::Mutex;

// use 4MB for the page allocator
pub const PAGE_ALLOCATOR_SIZE: usize = 4 * 1024 * 1024;

struct FreePage {
    next: *mut FreePage,
}

static mut ALLOCATOR: Mutex<PhysicalPageAllocator> = Mutex::new(PhysicalPageAllocator::empty());

pub fn init(start: *mut u8, pages: usize) {
    unsafe {
        ALLOCATOR.lock().init(start, pages);
    }
}

#[allow(dead_code)]
pub unsafe fn alloc() -> *mut u8 {
    ALLOCATOR.lock().alloc()
}

#[allow(dead_code)]
pub unsafe fn free(page: *mut u8) {
    ALLOCATOR.lock().free(page);
}

struct PhysicalPageAllocator {
    free_list_head: *mut FreePage,
    start: *mut u8,
    end: *mut u8,
}

impl PhysicalPageAllocator {
    const fn empty() -> Self {
        Self {
            free_list_head: core::ptr::null_mut(),
            start: core::ptr::null_mut(),
            end: core::ptr::null_mut(),
        }
    }

    fn init(&mut self, start: *mut u8, pages: usize) {
        let end = unsafe { start.add(pages * PAGE_4K) };
        self.start = start;
        self.end = end;

        let mut page = align_up(start, PAGE_4K);
        for _ in 0..pages {
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
            || (page as usize) % PAGE_4K != 0
            || page > unsafe { page.add(1) }
            || page >= self.end as _
            || page < self.start as _
        {
            panic!("freeing invalid page: {:p}", page);
        }

        (*page).next = self.free_list_head;
        self.free_list_head = page;
    }
}
