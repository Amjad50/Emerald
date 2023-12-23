#![no_std]

pub use allocator::HeapAllocator;

mod allocator;

// helper functions
const fn align_up(addr: usize, alignment: usize) -> usize {
    (addr + alignment - 1) & !(alignment - 1)
}

const fn is_aligned(addr: usize, alignment: usize) -> bool {
    (addr & (alignment - 1)) == 0
}

pub struct HeapStats {
    pub allocated: usize,
    pub free_size: usize,
    pub heap_size: usize,
}

pub trait PageAllocatorProvider<const PAGE_SIZE: usize> {
    /// Return the start address of the new allocated heap
    fn allocate_pages(&mut self, pages: usize) -> Option<*mut u8>;
    /// Deallocate pages from the end of the heap
    /// Return true if the deallocation was successful
    fn deallocate_pages(&mut self, pages: usize) -> bool;
}
