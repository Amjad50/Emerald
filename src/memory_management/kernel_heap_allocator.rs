use core::alloc::GlobalAlloc;

use crate::{
    memory_management::{
        memory_layout::{virtual2physical, KERNEL_HEAP_SIZE},
        virtual_memory::{self, flags, VirtualMemoryMapEntry},
    },
    sync::spin::mutex::Mutex,
};

use super::{
    memory_layout::{align_up, KERNEL_HEAP_BASE, PAGE_4K},
    physical_page_allocator,
};

const KERNEL_HEAP_MAGIC: u32 = 0xF0B0CAFE;

#[global_allocator]
pub static ALLOCATOR: LockedKernelHeapAllocator = LockedKernelHeapAllocator::empty();

#[repr(C, align(16))]
struct AllocatedHeapBlockInfo {
    magic: u32,
    size: usize,
}

#[derive(Debug)]
struct HeapFreeBlock {
    prev: *mut HeapFreeBlock,
    next: *mut HeapFreeBlock,
    // including this header
    size: usize,
}

struct KernelHeapAllocator {
    heap_start: usize,
    mapped_pages: usize,
    free_list_addr: *mut HeapFreeBlock,
}

unsafe impl Send for KernelHeapAllocator {}

impl KernelHeapAllocator {
    const fn empty() -> Self {
        Self {
            heap_start: KERNEL_HEAP_BASE,
            mapped_pages: 0,
            free_list_addr: core::ptr::null_mut(),
        }
    }

    /// Prints all free blocks in forward and backward order
    ///
    /// This doesn't provide any `checking` functionality, it just prints the free blocks
    ///
    /// The purpose is to make it easy to spot missing blocks due to bugs.
    fn debug_free_blocks(&self) {
        let mut last: *mut HeapFreeBlock = core::ptr::null_mut();
        for block in self.iter_free_blocks() {
            let block_start = block as *mut _ as usize;
            println!(
                "Free block at {:p}..{:p}",
                block_start as *const u8,
                (block_start + block.size) as *const u8
            );
            last = block as _;
        }

        println!("--- Backwards ---");

        if !last.is_null() {
            // go back to the first block
            while !last.is_null() {
                let block_start = last as *mut _ as usize;
                println!(
                    "Free block at {:p}..{:p}",
                    block_start as *const u8,
                    (block_start + unsafe { (*last).size }) as *const u8
                );

                last = unsafe { (*last).prev };
            }
        }
    }

    fn get_free_block(&mut self, size: usize) -> *mut HeapFreeBlock {
        if self.mapped_pages == 0 {
            let size = align_up(size as _, PAGE_4K) as usize;
            self.allocate_more_pages(size / PAGE_4K);
            // call recursively
            return self.get_free_block(size);
        }
        // find best block
        let mut best_block: *mut HeapFreeBlock = core::ptr::null_mut();
        for block in self.iter_free_blocks() {
            if block.size >= size
                && (best_block.is_null() || block.size < unsafe { (*best_block).size })
            {
                best_block = block as _;
            }
        }

        if best_block.is_null() {
            // no block found, allocate more pages
            let size = align_up(size as _, PAGE_4K) as usize;
            self.allocate_more_pages(size / PAGE_4K);
            // call recursively
            return self.get_free_block(size);
        }

        best_block
    }

    fn iter_free_blocks(&self) -> impl Iterator<Item = &mut HeapFreeBlock> {
        let mut current_block = self.free_list_addr;
        core::iter::from_fn(move || {
            if current_block.is_null() {
                None
            } else {
                let block = current_block;
                current_block = unsafe { (*current_block).next };
                Some(unsafe { &mut *block })
            }
        })
    }

    /// Allocates more pages and add them to the free list
    fn allocate_more_pages(&mut self, pages: usize) {
        eprintln!("Allocating {} pages", pages);
        assert!(pages > 0);

        let last_heap_base = self.heap_start + self.mapped_pages * PAGE_4K;
        let mut current_heap_base = last_heap_base;

        // do not exceed the heap size
        assert!((self.mapped_pages + pages) * PAGE_4K <= KERNEL_HEAP_SIZE);

        for _ in 0..pages {
            let page = unsafe { physical_page_allocator::alloc_zeroed() };

            let page_phy = virtual2physical(page as _);

            let mapping = VirtualMemoryMapEntry {
                virtual_address: current_heap_base as u64,
                start_physical_address: page_phy as u64,
                end_physical_address: page_phy as u64 + PAGE_4K as u64,
                flags: flags::PTE_WRITABLE,
            };
            virtual_memory::map(&mapping);
            current_heap_base += PAGE_4K;
        }

        self.mapped_pages += pages;

        // add to the free list (fast path)
        if self.free_list_addr.is_null() {
            // no free list for now, add this as the very first free entry
            let free_block = last_heap_base as *mut HeapFreeBlock;

            unsafe {
                (*free_block).prev = core::ptr::null_mut();
                (*free_block).next = core::ptr::null_mut();
                (*free_block).size = pages * PAGE_4K;
            }

            self.free_list_addr = free_block;
        } else {
            unsafe {
                self.free_block(last_heap_base as _, pages * PAGE_4K);
            }
        }
    }

    unsafe fn free_block(&mut self, freeing_block: usize, size: usize) {
        assert!(freeing_block <= self.heap_start + self.mapped_pages * PAGE_4K);
        assert!(freeing_block + size <= self.heap_start + self.mapped_pages * PAGE_4K);

        let freeing_block = freeing_block as *mut HeapFreeBlock;
        let freeing_block_start = freeing_block as usize;
        let freeing_block_end = freeing_block_start + size;

        // find blocks that are either before or after this block
        let mut prev_block: *mut HeapFreeBlock = core::ptr::null_mut();
        let mut next_block: *mut HeapFreeBlock = core::ptr::null_mut();
        for block in self.iter_free_blocks() {
            let block_addr = block as *mut _ as usize;
            let block_end = block_addr + block.size;

            if block_addr == freeing_block_start {
                // our block should not be in the free list
                panic!("double free");
            }

            // assert that we are not in the middle of a block
            assert!(
                (freeing_block_end <= block_addr) || (freeing_block_start >= block_end),
                "Free block at {:x}..{:x} is in the middle of another block at {:x}..{:x}",
                freeing_block_start,
                freeing_block_end,
                block_addr,
                block_end
            );

            if block_end == freeing_block_start {
                // this block is before the freeing block
                prev_block = block as _;
            } else if freeing_block_end == block_addr {
                // this block is after the freeing block
                next_block = block as _;
            }
        }

        eprintln!(
            "prev_block: {:x}, next_block: {:x}",
            prev_block as usize, next_block as usize
        );

        if !prev_block.is_null() && !next_block.is_null() {
            eprintln!("b: prev_block: [{:x}]={:X?}", prev_block as usize, unsafe {
                &mut *prev_block
            });
            eprintln!("b: next_block: [{:x}]={:X?}", next_block as usize, unsafe {
                &mut *next_block
            });
            let new_block = prev_block;
            // both are not null, so we are in the middle
            // merge the blocks
            (*new_block).size += size + (*next_block).size;

            // update the previous block to point to this new subblock instead
            if !(*next_block).next.is_null() {
                (*(*next_block).next).prev = new_block;
            }

            if !(*next_block).prev.is_null() {
                (*(*next_block).prev).next = new_block;
            } else {
                // this is the first block
                self.free_list_addr = new_block;
            }

            (*new_block).next = (*next_block).next;

            eprintln!("a: prev_block: [{:x}]={:X?}", prev_block as usize, unsafe {
                &mut *prev_block
            });
            eprintln!("a: next_block: [{:x}]={:X?}", next_block as usize, unsafe {
                &mut *next_block
            });
            eprintln!(
                "a: free_list_addr: [{:x}]={:X?}",
                self.free_list_addr as usize,
                unsafe { &mut *self.free_list_addr }
            );
        } else if !prev_block.is_null() {
            // no blocks after this
            // merge the blocks easily, we only need to change the size
            (*prev_block).size += size;
        } else if !next_block.is_null() {
            let new_block = freeing_block;

            // replace next with a new size
            (*new_block).size = (*next_block).size + size;
            (*new_block).prev = (*next_block).prev;
            (*new_block).next = (*next_block).next;

            // update references
            // update the next block to point to this new subblock instead
            if !(*next_block).next.is_null() {
                (*(*next_block).next).prev = new_block;
            }
            // update the previous block to point to this new subblock instead
            if !(*next_block).prev.is_null() {
                (*(*next_block).prev).next = new_block;
            } else {
                // this is the first block
                self.free_list_addr = new_block;
            }

            eprintln!("a: new-block: [{:x}]={:X?}", new_block as usize, unsafe {
                &mut *new_block
            });
        } else {
            // no blocks around this
            // add this to the free list
            (*freeing_block).prev = core::ptr::null_mut();
            (*freeing_block).next = self.free_list_addr;
            (*freeing_block).size = size;

            // update the next block to point to this new subblock instead
            if !(*freeing_block).next.is_null() {
                (*(*freeing_block).next).prev = freeing_block;
            }

            self.free_list_addr = freeing_block;
        }
    }
}

pub struct LockedKernelHeapAllocator {
    inner: Mutex<KernelHeapAllocator>,
}

impl LockedKernelHeapAllocator {
    const fn empty() -> Self {
        Self {
            inner: Mutex::new(KernelHeapAllocator::empty()),
        }
    }

    #[allow(dead_code)]
    pub fn debug_free_blocks(&self) {
        self.inner.lock().debug_free_blocks();
    }
}

unsafe impl GlobalAlloc for LockedKernelHeapAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut inner = self.inner.lock();

        // info header
        let base_layout = core::alloc::Layout::new::<AllocatedHeapBlockInfo>();

        let (whole_layout, allocated_block_offset) =
            base_layout.extend(layout.align_to(16).unwrap()).unwrap();
        // at least align to 16 bytes
        let size_to_allocate = whole_layout.pad_to_align().size();

        eprintln!("Allocating {} bytes", size_to_allocate);

        let free_block = inner.get_free_block(size_to_allocate);
        eprintln!("Got free block at {:x}", free_block as usize);
        if free_block.is_null() {
            return core::ptr::null_mut();
        } else {
            let free_block_size = (*free_block).size;
            let free_block_end = free_block as usize + size_to_allocate;
            let new_free_block = free_block_end as *mut HeapFreeBlock;

            // do we have empty space left?
            if free_block_size > size_to_allocate {
                // update the previous block to point to this new subblock instead
                (*new_free_block).prev = (*free_block).prev;
                (*new_free_block).next = (*free_block).next;
                (*new_free_block).size = free_block_size - size_to_allocate;

                eprintln!("a: prev_block: [{:x}]", (*new_free_block).prev as usize,);
                eprintln!("a: next_block: [{:x}]", (*new_free_block).next as usize);

                // update the next block to point to this new subblock instead
                if !(*new_free_block).next.is_null() {
                    (*(*new_free_block).next).prev = new_free_block;
                }

                // update the previous block to point to this new subblock instead
                if !(*new_free_block).prev.is_null() {
                    (*(*new_free_block).prev).next = new_free_block;
                } else {
                    // this is the first block
                    inner.free_list_addr = new_free_block;
                }
            } else {
                // exact size

                // update the previous block to point to the next block instead
                if !(*free_block).prev.is_null() {
                    (*(*free_block).prev).next = (*free_block).next;
                } else {
                    // this is the first block
                    inner.free_list_addr = (*free_block).next;
                }
            }
        }

        // write the info header
        let allocated_block_info = free_block as *mut AllocatedHeapBlockInfo;
        (*allocated_block_info).magic = KERNEL_HEAP_MAGIC;
        (*allocated_block_info).size = size_to_allocate;

        drop(inner);

        unsafe { (allocated_block_info as *mut u8).add(allocated_block_offset) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        assert!(!ptr.is_null());

        // info header
        let base_layout = core::alloc::Layout::new::<AllocatedHeapBlockInfo>();

        let (whole_layout, allocated_block_offset) =
            base_layout.extend(layout.align_to(16).unwrap()).unwrap();
        let size_to_free_from_layout = whole_layout.pad_to_align().size();

        let allocated_block_info = ptr.sub(allocated_block_offset) as *mut AllocatedHeapBlockInfo;

        eprintln!("deallocating {:p}", allocated_block_info);
        assert_eq!((*allocated_block_info).magic, KERNEL_HEAP_MAGIC);
        assert_eq!((*allocated_block_info).size, size_to_free_from_layout);

        let mut inner = self.inner.lock();

        let freeing_block = allocated_block_info as *mut HeapFreeBlock;
        inner.free_block(freeing_block as _, size_to_free_from_layout);
    }
}
