use core::alloc::{GlobalAlloc, Layout};

use increasing_heap_allocator::{HeapAllocator, HeapStats, PageAllocatorProvider};
use kernel_user_link::{
    call_syscall,
    syscalls::{SyscallError, SYS_INC_HEAP},
};

use crate::sync::{once::OnceLock, spin::mutex::Mutex};

pub extern crate alloc;

const PAGE_4K: usize = 0x1000;

unsafe fn inc_dec_heap(increment: isize) -> Result<*mut u8, SyscallError> {
    unsafe {
        call_syscall!(
            SYS_INC_HEAP,
            increment as u64, // increment
        )
        .map(|addr| addr as *mut u8)
    }
}

#[global_allocator]
pub static ALLOCATOR: LockedKernelHeapAllocator = LockedKernelHeapAllocator::empty();

struct PageAllocator {
    heap_start: usize,
    mapped_pages: usize,
}

impl PageAllocator {
    fn new() -> Self {
        Self {
            heap_start: unsafe { inc_dec_heap(0).unwrap() as usize },
            mapped_pages: 0,
        }
    }
}

impl PageAllocatorProvider<PAGE_4K> for PageAllocator {
    fn allocate_pages(&mut self, pages: usize) -> Option<*mut u8> {
        // eprintln!("Allocating {} pages", pages);
        assert!(pages > 0);

        let last_heap_base = self.heap_start + self.mapped_pages * PAGE_4K;
        let new_addr = unsafe { inc_dec_heap((pages * PAGE_4K) as isize) };

        let Ok(new_addr) = new_addr else {
            return None;
        };
        assert!(new_addr as usize == last_heap_base);

        self.mapped_pages += pages;

        Some(new_addr)
    }

    fn deallocate_pages(&mut self, _pages: usize) -> bool {
        todo!()
    }
}

pub struct LockedKernelHeapAllocator {
    inner: OnceLock<Mutex<HeapAllocator<PAGE_4K, PageAllocator>>>,
}

impl LockedKernelHeapAllocator {
    const fn empty() -> Self {
        Self {
            inner: OnceLock::new(),
        }
    }

    fn init_mutex() -> Mutex<HeapAllocator<PAGE_4K, PageAllocator>> {
        Mutex::new(HeapAllocator::new(PageAllocator::new()))
    }

    #[allow(dead_code)]
    pub fn stats(&self) -> HeapStats {
        let inner = self.inner.get_or_init(Self::init_mutex).lock();
        inner.stats()
    }
}

unsafe impl GlobalAlloc for LockedKernelHeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.inner
            .get_or_init(Self::init_mutex)
            .lock()
            .alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.inner
            .get_or_init(Self::init_mutex)
            .lock()
            .dealloc(ptr, layout)
    }
}
