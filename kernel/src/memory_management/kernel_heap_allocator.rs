use core::alloc::{GlobalAlloc, Layout};

use increasing_heap_allocator::{HeapAllocator, HeapStats, PageAllocatorProvider};

use crate::{
    memory_management::{
        memory_layout::KERNEL_HEAP_SIZE,
        virtual_memory_mapper::{self, flags, VirtualMemoryMapEntry},
    },
    sync::{once::OnceLock, spin::mutex::Mutex},
};

use super::memory_layout::{KERNEL_HEAP_BASE, PAGE_4K};

#[global_allocator]
pub static ALLOCATOR: LockedKernelHeapAllocator = LockedKernelHeapAllocator::empty();

struct PageAllocator {
    heap_start: usize,
    mapped_pages: usize,
}

impl PageAllocator {
    fn new() -> Self {
        Self {
            heap_start: KERNEL_HEAP_BASE,
            mapped_pages: 0,
        }
    }
}

impl PageAllocatorProvider<PAGE_4K> for PageAllocator {
    fn allocate_pages(&mut self, pages: usize) -> Option<*mut u8> {
        eprintln!("Allocating {} pages", pages);
        assert!(pages > 0);

        let last_heap_base = self.heap_start + self.mapped_pages * PAGE_4K;
        let current_heap_base = last_heap_base;

        // do not exceed the heap size
        if (self.mapped_pages + pages) * PAGE_4K > KERNEL_HEAP_SIZE {
            return None;
        }

        virtual_memory_mapper::map_kernel(&VirtualMemoryMapEntry {
            virtual_address: current_heap_base as u64,
            physical_address: None,
            size: (PAGE_4K * pages) as u64,
            flags: flags::PTE_WRITABLE,
        });

        self.mapped_pages += pages;

        Some(current_heap_base as *mut u8)
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
