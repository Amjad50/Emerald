use crate::{
    memory_layout::{align_up, PAGE_4K},
    sync::spin::mutex::Mutex,
};

// use 4MB for the page allocator
pub const PAGE_ALLOCATOR_SIZE: usize = 4 * 1024 * 1024;

struct FreePage {
    next: *mut FreePage,
}

pub struct PhysicalPageAllocator {
    free_list_head: *mut FreePage,
    start: *mut u8,
    end: *mut u8,
}

// late init
static mut ALLOCATOR: Mutex<PhysicalPageAllocator> =
    Mutex::new(PhysicalPageAllocator::empty());

pub fn init(start: *mut u8, pages: usize) {
    unsafe {
        ALLOCATOR.lock().init(start, pages);
    }
}

#[allow(dead_code)]
pub fn alloc() -> *mut u8 {
    unsafe { ALLOCATOR.lock().alloc() }
}

#[allow(dead_code)]
pub fn free(page: *mut u8) {
    unsafe { ALLOCATOR.lock().free(page) };
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
            self.free(page);
            page = unsafe { page.add(PAGE_4K) };
        }
    }

    fn alloc(&mut self) -> *mut u8 {
        if self.free_list_head.is_null() {
            panic!("out of memory");
        }

        let page = self.free_list_head;
        unsafe {
            self.free_list_head = (*page).next;
        }

        let page = page as *mut u8;
        // fill with random data to catch dangling pointer bugs
        unsafe { page.write_bytes(1, PAGE_4K) };

        page
    }

    fn free(&mut self, page: *mut u8) {
        // fill with random data to catch dangling pointer bugs
        unsafe {
            page.write_bytes(2, PAGE_4K);
        }

        let page = page as *mut FreePage;

        if page.is_null()
            || (page as usize) % PAGE_4K != 0
            || page > unsafe { page.add(1) }
            || page >= self.end as _
            || page < self.start as _
        {
            panic!("freeing invalid page: {:p}", page);
        }

        unsafe {
            (*page).next = self.free_list_head;
            self.free_list_head = page;
        }
    }
}
