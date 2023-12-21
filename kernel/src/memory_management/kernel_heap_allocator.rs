use core::{alloc::GlobalAlloc, mem};

use crate::{
    memory_management::{
        memory_layout::{is_aligned, KERNEL_HEAP_SIZE},
        virtual_memory_mapper::{self, flags, VirtualMemoryMapEntry},
    },
    sync::spin::mutex::Mutex,
};

use super::memory_layout::{align_up, KERNEL_HEAP_BASE, PAGE_4K};

const KERNEL_HEAP_MAGIC: u32 = 0xF0B0CAFE;

#[global_allocator]
pub static ALLOCATOR: LockedKernelHeapAllocator = LockedKernelHeapAllocator::empty();

#[repr(C, align(16))]
struct AllocatedHeapBlockInfo {
    magic: u32,
    size: usize,
    pre_padding: usize,
}

const KERNEL_HEAP_BLOCK_INFO_SIZE: usize = mem::size_of::<AllocatedHeapBlockInfo>();

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
    free_size: usize,
    used_size: usize,
}

unsafe impl Send for KernelHeapAllocator {}

impl KernelHeapAllocator {
    const fn empty() -> Self {
        Self {
            heap_start: KERNEL_HEAP_BASE,
            mapped_pages: 0,
            free_list_addr: core::ptr::null_mut(),
            free_size: 0,
            used_size: 0,
        }
    }

    fn is_free_blocks_in_cycle(&self) -> bool {
        // use floyd algorithm to detect if we are in cycle
        let mut slow = self.free_list_addr;
        let mut fast = self.free_list_addr;

        // advance fast first
        if fast.is_null() {
            return false;
        } else {
            fast = unsafe { (*fast).next };
        }

        while fast != slow {
            if fast.is_null() {
                return false;
            } else {
                fast = unsafe { (*fast).next };
            }
            if fast.is_null() {
                return false;
            } else {
                fast = unsafe { (*fast).next };
            }

            if slow.is_null() {
                return false;
            } else {
                slow = unsafe { (*slow).next };
            }
        }

        true
    }

    fn check_free_blocks(&self) -> bool {
        let mut forward_count = 0;
        let mut last: *mut HeapFreeBlock = core::ptr::null_mut();
        for block in self.iter_free_blocks() {
            forward_count += 1;
            last = block as _;
        }

        let mut backward_count = 0;
        if !last.is_null() {
            // go back to the first block
            while !last.is_null() {
                backward_count += 1;
                last = unsafe { (*last).prev };
            }
        }

        forward_count != backward_count
    }

    fn check_issues(&self) -> bool {
        self.is_free_blocks_in_cycle() || self.check_free_blocks()
    }

    fn debug_free_blocks(&self) {
        println!();
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
            let size = align_up(size, PAGE_4K);
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
            let size = align_up(size, PAGE_4K);
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
        let current_heap_base = last_heap_base;

        // do not exceed the heap size
        assert!((self.mapped_pages + pages) * PAGE_4K <= KERNEL_HEAP_SIZE);

        virtual_memory_mapper::map_kernel(&VirtualMemoryMapEntry {
            virtual_address: current_heap_base as u64,
            physical_address: None,
            size: (PAGE_4K * pages) as u64,
            flags: flags::PTE_WRITABLE,
        });

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
        self.free_size += pages * PAGE_4K;
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
        let mut closest_prev_block: *mut HeapFreeBlock = core::ptr::null_mut();
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

            if block_addr < freeing_block_start {
                // this block is before the freeing block
                if closest_prev_block.is_null() || block_addr > (closest_prev_block as usize) {
                    closest_prev_block = block as _;
                }
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
            // add this to the free list in the correct order
            if closest_prev_block.is_null() {
                // this is the first block
                (*freeing_block).prev = core::ptr::null_mut();
                (*freeing_block).next = self.free_list_addr;
                (*freeing_block).size = size;

                // update the next block to point to this new subblock instead
                if !(*freeing_block).next.is_null() {
                    (*(*freeing_block).next).prev = freeing_block;
                }

                self.free_list_addr = freeing_block;
            } else {
                // put this after the closest previous block
                let closest_next_block = (*closest_prev_block).next;
                (*freeing_block).prev = closest_prev_block;
                (*freeing_block).next = closest_next_block;
                (*freeing_block).size = size;

                (*closest_prev_block).next = freeing_block;
                if !closest_next_block.is_null() {
                    (*closest_next_block).prev = freeing_block;
                }
            }
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

    #[allow(dead_code)]
    pub fn stats(&self) -> (usize, usize) {
        let inner = self.inner.lock();
        (inner.free_size, inner.used_size)
    }
}

unsafe impl GlobalAlloc for LockedKernelHeapAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut inner = self.inner.lock();

        // info header
        let block_info_layout = core::alloc::Layout::new::<AllocatedHeapBlockInfo>();

        let (whole_layout, whole_block_offset) = block_info_layout
            .extend(layout.align_to(16).unwrap())
            .unwrap();
        // at least align to 16 bytes
        let size_to_allocate = whole_layout.pad_to_align().size();

        eprintln!("Allocating {} bytes", size_to_allocate);

        let free_block = inner.get_free_block(size_to_allocate);
        eprintln!("Got free block at {:x}", free_block as usize);
        if free_block.is_null() {
            return core::ptr::null_mut();
        }

        let free_block_size = (*free_block).size;
        let free_block_end = free_block as usize + size_to_allocate;
        let new_free_block = free_block_end as *mut HeapFreeBlock;

        // we have to make sure that the block after us has enough space to write the metadat
        // and we won't corrupt the block that comes after (if there is anys)
        let whole_size = size_to_allocate + mem::size_of::<HeapFreeBlock>();

        // store the actual size of the block
        // if we needed to extend (since the next free block is to small)
        // this will include the whole size and not just the size that
        // we were asked to allocate
        let mut this_allocation_size = size_to_allocate;

        // do we have empty space left?
        if free_block_size > whole_size {
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
            this_allocation_size = free_block_size;

            // update the previous block to point to the next block instead
            if !(*free_block).prev.is_null() {
                (*(*free_block).prev).next = (*free_block).next;
            } else {
                // this is the first block
                inner.free_list_addr = (*free_block).next;
            }
            if !(*free_block).next.is_null() {
                (*(*free_block).next).prev = (*free_block).prev;
            }
        }
        // drop inner early
        inner.free_size -= this_allocation_size;
        inner.used_size += this_allocation_size;

        // TODO: add flag to control when to enable this runtime checking
        if inner.check_issues() {
            println!("alloc: {:x}..{:x}", free_block as usize, free_block_end);
            println!("alloc: Issue detected");
            inner.debug_free_blocks();
            panic!(); // mostly won't reach here since debug_free_blocks will not finish
        }

        drop(inner);

        // work on the pointer and add the info of the block before it
        // so we can use it to deallocate later
        let base = free_block as usize;
        let possible_next_offset = align_up(base, layout.align()) - base;
        let allocated_block_offset = if possible_next_offset < KERNEL_HEAP_BLOCK_INFO_SIZE {
            possible_next_offset + KERNEL_HEAP_BLOCK_INFO_SIZE
        } else {
            possible_next_offset
        };
        assert!(allocated_block_offset >= KERNEL_HEAP_BLOCK_INFO_SIZE);
        assert!(allocated_block_offset <= whole_block_offset);
        let allocated_ptr = (free_block as *mut u8).add(allocated_block_offset);
        let allocated_block_info =
            allocated_ptr.sub(KERNEL_HEAP_BLOCK_INFO_SIZE) as *mut AllocatedHeapBlockInfo;
        // make sure we are aligned
        assert!(
            is_aligned(allocated_ptr as _, layout.align(),),
            "base_block={allocated_block_info:p}, offset={allocated_block_offset}, ptr={allocated_ptr:?}, layout={layout:?}, should_be_addr={:x}",
            align_up(allocated_block_info as usize, layout.align())
        );

        // write the info header
        (*allocated_block_info).magic = KERNEL_HEAP_MAGIC;
        (*allocated_block_info).size = this_allocation_size;
        (*allocated_block_info).pre_padding = allocated_block_offset;

        allocated_ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        assert!(!ptr.is_null());

        // info header
        let base_layout = core::alloc::Layout::new::<AllocatedHeapBlockInfo>();

        let (whole_layout, _) = base_layout.extend(layout.align_to(16).unwrap()).unwrap();
        let size_to_free_from_layout = whole_layout.pad_to_align().size();

        let allocated_block_info =
            ptr.sub(KERNEL_HEAP_BLOCK_INFO_SIZE) as *mut AllocatedHeapBlockInfo;

        eprintln!(
            "deallocating {:p}, size={}",
            allocated_block_info, size_to_free_from_layout
        );
        assert_eq!((*allocated_block_info).magic, KERNEL_HEAP_MAGIC);
        // This could be more than the layout size, because
        // we might increase the size of the block a bit to not leave
        // free blocks that are too small (see `alloc``)
        assert!((*allocated_block_info).size >= size_to_free_from_layout);
        assert!((*allocated_block_info).pre_padding >= KERNEL_HEAP_BLOCK_INFO_SIZE);
        let this_allocation_size = (*allocated_block_info).size;

        let freeing_block = ptr.sub((*allocated_block_info).pre_padding) as usize;

        let mut inner = self.inner.lock();
        inner.free_block(freeing_block, this_allocation_size);
        inner.used_size -= this_allocation_size;
        inner.free_size += this_allocation_size;

        // TODO: add flag to control when to enable this runtime checking
        if inner.check_issues() {
            println!(
                "dealloc: {:x}..{:x}",
                freeing_block,
                freeing_block + this_allocation_size
            );
            println!("dealloc: Issue detected");
            inner.debug_free_blocks();
            panic!(); // mostly won't reach here since debug_free_blocks will not finish
        }
    }
}
