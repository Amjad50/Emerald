use core::mem;

use crate::{is_aligned, HeapStats, PageAllocatorProvider};

use super::align_up;

const HEAP_MAGIC: u32 = 0xF0B0CAFE;

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

pub struct HeapAllocator<const PAGE_SIZE: usize, T: PageAllocatorProvider<PAGE_SIZE>> {
    heap_start: usize,
    total_heap_size: usize,
    free_list_addr: *mut HeapFreeBlock,
    free_size: usize,
    used_size: usize,
    page_allocator: T,
}

unsafe impl<const PAGE_SIZE: usize, T: PageAllocatorProvider<PAGE_SIZE>> Send
    for HeapAllocator<PAGE_SIZE, T>
{
}

impl<const PAGE_SIZE: usize, T> HeapAllocator<PAGE_SIZE, T>
where
    T: PageAllocatorProvider<PAGE_SIZE>,
{
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

    fn get_free_block(&mut self, size: usize) -> *mut HeapFreeBlock {
        if self.total_heap_size == 0 {
            let size = align_up(size, PAGE_SIZE);
            self.allocate_more_pages(size / PAGE_SIZE);
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
            let size = align_up(size, PAGE_SIZE);
            self.allocate_more_pages(size / PAGE_SIZE);
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
        assert!(pages > 0);

        let new_heap_start = if self.total_heap_size == 0 {
            // first allocation
            self.heap_start = self.page_allocator.allocate_pages(pages).unwrap() as usize;
            self.heap_start
        } else {
            // allocate more pages
            self.page_allocator.allocate_pages(pages).unwrap() as usize
        };

        self.total_heap_size += pages * PAGE_SIZE;

        // add to the free list (fast path)
        if self.free_list_addr.is_null() {
            // no free list for now, add this as the very first free entry
            let free_block = new_heap_start as *mut HeapFreeBlock;

            unsafe {
                (*free_block).prev = core::ptr::null_mut();
                (*free_block).next = core::ptr::null_mut();
                (*free_block).size = pages * PAGE_SIZE;
            }

            self.free_list_addr = free_block;
        } else {
            unsafe {
                self.free_block(new_heap_start as _, pages * PAGE_SIZE);
            }
        }
        self.free_size += pages * PAGE_SIZE;
    }

    unsafe fn free_block(&mut self, freeing_block: usize, size: usize) {
        assert!(freeing_block <= self.heap_start + self.total_heap_size);
        assert!(freeing_block + size <= self.heap_start + self.total_heap_size);

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
                "Free block at {freeing_block_start:x}..{freeing_block_end:x} is in the middle of another block at {block_addr:x}..{block_end:x}",
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

        if !prev_block.is_null() && !next_block.is_null() {
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

// public interface
impl<const PAGE_SIZE: usize, T> HeapAllocator<PAGE_SIZE, T>
where
    T: PageAllocatorProvider<PAGE_SIZE>,
{
    pub fn new(page_allocator: T) -> Self {
        Self {
            heap_start: 0,
            free_list_addr: core::ptr::null_mut(),
            total_heap_size: 0,
            free_size: 0,
            used_size: 0,
            page_allocator,
        }
    }

    pub fn stats(&self) -> HeapStats {
        HeapStats {
            allocated: self.used_size,
            free_size: self.free_size,
            heap_size: self.total_heap_size,
        }
    }

    pub fn debug_free_blocks(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.iter_free_blocks()
            .map(|block| (block as *mut _ as usize, block.size))
    }

    /// # Safety
    /// Check [`core::alloc::GlobalAlloc::alloc`] for more info
    pub unsafe fn alloc(&mut self, layout: core::alloc::Layout) -> *mut u8 {
        // info header
        let block_info_layout = core::alloc::Layout::new::<AllocatedHeapBlockInfo>();

        // use minimum alignment AllocatedHeapBlockInfo
        // whole_layout here is the layout of the requested block + the info header
        // whole_block_offset is the offset of the block after the info header
        let (whole_layout, block_offset_from_header) = block_info_layout
            .extend(layout.align_to(block_info_layout.align()).unwrap())
            .unwrap();
        // at least align to AllocatedHeapBlockInfo (see above)
        // `allocation_size` is the size of the block we are going to allocate as a whole
        // this block include the info header and the requested block and maybe some padding
        let mut allocation_size = whole_layout.pad_to_align().size();

        let free_block = self.get_free_block(allocation_size);

        if free_block.is_null() {
            return core::ptr::null_mut();
        }

        // work on the pointer and add the info of the block before it, and handle alignment
        // so, we can use it to deallocate later
        let base = free_block as usize;
        // this should never fail, we are allocating of `block_info_layout.align()` alignment always
        assert!(is_aligned(base, block_info_layout.align()));
        let possible_next_offset = align_up(base, layout.align()) - base;
        let allocated_block_offset = if possible_next_offset < KERNEL_HEAP_BLOCK_INFO_SIZE {
            // if we can't fit the info header, we need to add to the offset
            possible_next_offset + KERNEL_HEAP_BLOCK_INFO_SIZE.max(layout.align())
        } else {
            possible_next_offset
        };
        assert!(allocated_block_offset >= KERNEL_HEAP_BLOCK_INFO_SIZE);
        if allocated_block_offset > block_offset_from_header {
            // we can exceed the calculated block sizes from the layout above, if that happens
            // we must increase the allocation size to account for that
            // this can happen when the alignment of the requested block is more than the info block
            //
            // example:
            //   requested layout: size=512, align=64
            //   info layout: size=32, align=16
            //   the above calculation `block_offset_from_header` will be 64
            //   the allocator, i.e. `free_block` will always be aligned to 16 (the info block)
            //   then, if the `possible_next_offset` happens to be 16, i.e. we are 48 bytes into a 64 bytes block
            //
            //       [ 16 bytes ][ 16 bytes ][ 16 bytes ][ 16 bytes ]
            //       ^ <64 byte alignment>               ^ free_block
            //
            //   since 16 is less than 32, we need to add more offset, but `layout.size()` is 64. So we are going to
            //   add 80 (64 + 16) as the `allocated_block_offset`, but that already exceed `64`.
            //   the `allocation_size` before this fix would have been 512+64=576,
            //   but the actual size we need 512+80=592. That's why we need this fix.
            //   (as you might have expected, these numbers are from an actual bug I found and debugged -_-)
            allocation_size += allocated_block_offset - block_offset_from_header;
        }
        let allocated_ptr = (free_block as *mut u8).add(allocated_block_offset);
        let allocated_block_info =
            allocated_ptr.sub(KERNEL_HEAP_BLOCK_INFO_SIZE) as *mut AllocatedHeapBlockInfo;

        let free_block_size = (*free_block).size;
        // for now, we hope we get enough size
        // FIXME: get a new block if this is not enough
        assert!(free_block_size >= allocation_size);
        let free_block_end = free_block as usize + allocation_size;
        let new_free_block = free_block_end as *mut HeapFreeBlock;

        // we have to make sure that the block after us has enough space to write the metadata,
        // and we won't corrupt the block that comes after (if there is any)
        let required_safe_size = allocation_size + mem::size_of::<HeapFreeBlock>();

        // store the actual size of the block
        // if we needed to extend (since the next free block is to small)
        // this will include the whole size and not just the size that
        // we were asked to allocate
        let mut this_allocation_size = allocation_size;

        // do we have empty space left?
        if free_block_size > required_safe_size {
            // update the previous block to point to this new subblock instead
            (*new_free_block).prev = (*free_block).prev;
            (*new_free_block).next = (*free_block).next;
            (*new_free_block).size = free_block_size - allocation_size;

            // update the next block to point to this new subblock instead
            if !(*new_free_block).next.is_null() {
                (*(*new_free_block).next).prev = new_free_block;
            }

            // update the previous block to point to this new subblock instead
            if !(*new_free_block).prev.is_null() {
                (*(*new_free_block).prev).next = new_free_block;
            } else {
                // this is the first block
                self.free_list_addr = new_free_block;
            }
        } else {
            // exact size
            this_allocation_size = free_block_size;

            // update the previous block to point to the next block instead
            if !(*free_block).prev.is_null() {
                (*(*free_block).prev).next = (*free_block).next;
            } else {
                // this is the first block
                self.free_list_addr = (*free_block).next;
            }
            if !(*free_block).next.is_null() {
                (*(*free_block).next).prev = (*free_block).prev;
            }
        }
        self.free_size -= this_allocation_size;
        self.used_size += this_allocation_size;

        // TODO: add flag to control when to enable this runtime checking
        if self.check_issues() {
            panic!("Found issues in `alloc`");
        }

        // make sure we are aligned
        assert!(is_aligned(allocated_ptr as _, layout.align()),
            "base_block={allocated_block_info:p}, offset={allocated_block_offset}, ptr={allocated_ptr:?}, layout={layout:?}, should_be_addr={:x}",
            align_up(allocated_block_info as usize, layout.align()));

        // write the info header
        (*allocated_block_info).magic = HEAP_MAGIC;
        (*allocated_block_info).size = this_allocation_size;
        (*allocated_block_info).pre_padding = allocated_block_offset;

        allocated_ptr
    }

    /// # Safety
    /// Check [`core::alloc::GlobalAlloc::dealloc`] for more info
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: core::alloc::Layout) {
        assert!(!ptr.is_null());

        // info header
        let base_layout = core::alloc::Layout::new::<AllocatedHeapBlockInfo>();

        let (whole_layout, _) = base_layout.extend(layout.align_to(16).unwrap()).unwrap();
        let size_to_free_from_layout = whole_layout.pad_to_align().size();

        let allocated_block_info =
            ptr.sub(KERNEL_HEAP_BLOCK_INFO_SIZE) as *mut AllocatedHeapBlockInfo;

        assert_eq!((*allocated_block_info).magic, HEAP_MAGIC);
        // This could be more than the layout size, because
        // we might increase the size of the block a bit to not leave
        // free blocks that are too small (see `alloc``)
        assert!((*allocated_block_info).size >= size_to_free_from_layout);
        assert!((*allocated_block_info).pre_padding >= KERNEL_HEAP_BLOCK_INFO_SIZE);
        let this_allocation_size = (*allocated_block_info).size;

        let freeing_block = ptr.sub((*allocated_block_info).pre_padding) as usize;

        self.free_block(freeing_block, this_allocation_size);
        self.used_size -= this_allocation_size;
        self.free_size += this_allocation_size;

        // TODO: add flag to control when to enable this runtime checking
        if self.check_issues() {
            panic!("Found issues in `dealloc`");
        }
    }
}
