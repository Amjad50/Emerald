use crate::{
    memory_layout::{align_up, PAGE_4K},
    sync::spin,
};

// use 4MB for the page allocator
pub const PAGE_ALLOCATOR_SIZE: usize = 4 * 1024 * 1024;

struct FreePage {
    next: *mut FreePage,
}

pub struct PhysicalPageAllocator {
    lock: spin::Lock,
    free_list_head: *mut FreePage,
    start: *mut u8,
    end: *mut u8,
}

// late init
static mut ALLOCATOR: PhysicalPageAllocator = PhysicalPageAllocator::empty();

pub fn init(start: *mut u8, pages: usize) {
    unsafe {
        ALLOCATOR = PhysicalPageAllocator::new(start, pages);
    }
}

pub fn alloc() -> *mut u8 {
    unsafe { ALLOCATOR.alloc() }
}

pub fn free(page: *mut u8) {
    unsafe { ALLOCATOR.free(page) };
}

impl PhysicalPageAllocator {
    const fn empty() -> Self {
        Self {
            lock: spin::Lock::new("EmptyPhysicalPageAllocator"),
            free_list_head: core::ptr::null_mut(),
            start: core::ptr::null_mut(),
            end: core::ptr::null_mut(),
        }
    }

    fn new(start: *mut u8, pages: usize) -> Self {
        let end = unsafe { start.add(pages * PAGE_4K) };
        let mut s = Self {
            lock: spin::Lock::new("PhysicalPageAllocator"),
            free_list_head: core::ptr::null_mut(),
            start,
            end,
        };

        let mut page = align_up(start, PAGE_4K);
        for _ in 0..pages {
            s.free(page);
            page = unsafe { page.add(PAGE_4K) };
        }

        s
    }

    fn alloc(&mut self) -> *mut u8 {
        self.lock.lock();

        if self.free_list_head.is_null() {
            panic!("out of memory");
        }

        let page = self.free_list_head;
        unsafe {
            self.free_list_head = (*page).next;
        }

        self.lock.unlock();

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

        self.lock.lock();

        unsafe {
            (*page).next = self.free_list_head;
            self.free_list_head = page;
        }

        self.lock.unlock();
    }
}
