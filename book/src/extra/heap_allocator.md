# Heap allocator

In the kernel, and also in userspace we need a heap. 

Heap simply is a memory region that we can allocate and deallocate memory from when
we need dynamically.

Anyway, we need heap implementation for the kernel and userspace.

This is achieved in [`increasing_heap_allocator`]. This is a crate implementing heap allocator based on "increasing" memory block.

What does that mean?

It means that this crate require a `Page` provider, i.e. an entity that can provide pages of memory that reside after each other,
and the rest of handling heap memory is done by the crate.

This example will make it clear, this is how its implemented in `userspace`:

```rust,no_run
use increasing_heap_allocator::{PageAllocatorProvider, HeapAllocator};

const PAGE_4K: usize = 4096;

struct PageAllocator {
    heap_start: usize,
    mapped_pages: usize,
}

impl PageAllocator {
    fn new() -> Self {
        Self {
            heap_start: unsafe { syscall::inc_dec_heap(0).unwrap() as usize },
            mapped_pages: 0,
        }
    }
}

impl PageAllocatorProvider<PAGE_4K> for PageAllocator {
    fn allocate_pages(&mut self, pages: usize) -> Option<*mut u8> {
        assert!(pages > 0);

        // calculate the last heap base, using the `heap_start` and `mapped_pages`
        // the next heap base will be `last_heap_base + (pages * PAGE_4K)`
        let last_heap_base = self.heap_start + self.mapped_pages * PAGE_4K;
        let new_addr = unsafe { syscall::inc_dec_heap((pages * PAGE_4K) as isize) };

        let Ok(new_addr) = new_addr else {
            return None;
        };
        // `inc_dec_heap` will return the top of the new block, so it must match where we think the heap ends
        assert!(new_addr as usize == last_heap_base);

        // update the mapped pages
        self.mapped_pages += pages;

        Some(new_addr)
    }
}

// just a simple usage
fn main() {
    let allocator = HeapAllocator::new(PageAllocator::new());

    let layout = !; // example of layout
    // allocate and deallocate
    let ptr = allocator.alloc(layout);
    allocator.dealloc(ptr, layout);
}
```

The only thing users of this crate need to implement is the `PageAllocatorProvider` trait, which is a simple trait that requires
a method to allocate pages.

`syscall::inc_dec_heap` is very similar to `sbrk` in Unix, it will increase or decrease the heap size by the given size.
An argument of `0` will return the current heap address without changing it, which we use in the beginning to get the initial heap address.

## Allocator implementation
The internal implementation of the allocator is as follows:

- The `HeapAllocator` keep of a free linked-list, these are free heap blocks. We exploit that the blocks are free and
    store some metadata in the block itself.
    ```rust
    struct HeapFreeBlock {
        prev: *mut HeapFreeBlock,
        next: *mut HeapFreeBlock,
        // including this header
        size: usize,
    }
    ```
    We have pointers to the previous and next free block, and the size of the block.
    As you might have guessed, `HeapAllocator` is a `!Sync` type, because we use raw pointers.

- Whenever we need to allocate a block, we check the free list, pick the smallest block that fits the requested size, and split it if necessary.
    If we can't find a block, we allocate new pages from `PageAllocatorProvider` and add the new block to the free list.
- Whenever we add a free block to the list, i.e. by `dealloc` or when getting new pages, we will try to merge with free blocks around it
    to avoid fragmentation. The implementation of this is at [`free_block`](https://docs.rs/increasing_heap_allocator/0.1.3/src/increasing_heap_allocator/allocator.rs.html#178) And explained later at [`dealloc` algorithm](#dealloc-algorithm).
- The allocated block will contain metadata residing before it in memory, this is used to know the size of the block and
any padding it has, which the `dealloc` function can use later to correctly deallocate it without leaving any memory behind.
The structure is:
    ```rust
    struct AllocatedHeapBlockInfo {
        magic: u32,
        size: usize,
        pre_padding: usize,
    }
    ```
    The `magic` is a constant value `0xF0B0CAFE` that we use to check if some bug happened, it could probably be removed
    when we have a stable implementation. But it has helped me a lot in debugging.
    `size` is the size of the block, and `pre_padding` is the padding before the block, which is used to align the block to the correct
    alignment, see [`alloc` algorithm](#alloc-algorithm) for more details.


### `alloc` algorithm

First, we need to know how much memory we need for this allocation, we get the argument `Layout` which contain `size`, but we need to
make sure of:
- Including the `AllocatedHeapBlockInfo` metadata.
- Making sure the block is aligned to the correct alignment, [`Layout::align`].

First, we get a rough estimation of the size by doing:
```rust,no_run
let block_info_layout = Layout::new::<AllocatedHeapBlockInfo>();
// whole_layout here is the layout of the the info header + requested block
// `whole_block_offset` is the offset of the block after the info header
let (whole_layout, block_offset_from_header) = block_info_layout
    .extend(layout.align_to(block_info_layout.align()).unwrap())
    .unwrap();

let allocation_size = whole_layout.pad_to_align().size();
```

This will give us the raw size of the block along with any padding we may need to align it.

The alignment of this `whole_layout` is equal to the alignment of the largest of the two.
Example:
```rust
# use std::alloc::Layout;
#[repr(align(32))]
struct A(i32);

#[repr(align(128))]
struct C(i32);

let layout_a = Layout::new::<A>();
let layout_c = Layout::new::<C>();

let (whole_layout, _) = layout_a.extend(layout_c).unwrap(); // whole_layout.align() == 128
let (whole_layout, _) = layout_c.extend(layout_a).unwrap(); // whole_layout.align() == 128
```

Which means we can totally just use `allocation_size`, and be done with it.
But the issue is that we don't know the address of the `free_block` we will get, i.e. in order
for this to work, we must make sure the address of the block we will return be aligned by `whole_layout.align()`.

> And the whole allignment of the returned free_block caused a bug before, which was fixed in [471eff0].

We know that `free_block` will always be aligned to `AllocatedHeapBlockInfo`, since all allocations are aligned to it.

But if the `block` we require, needs higher alignment we will probably need to increase the size of `allocation_size` to account
for the extra padding.

The algorithm is as follows:
- Get a free block from the free list that fits the `allocation_size`.
- Get the `allocated_block_offset` which is an offset from the `free_block` pointer to the start of the block.
    - The new pointer from this `offset` is aligned to the `block`, that the user will get.
    - This is computed as such:
        ```rust,no_run
        // `layout` is the input `block` layout
        // `align_up` is a function that aligns the `base` to the `align`
        let possible_next_offset = align_up(base, layout.align()) - base;
        let allocated_block_offset = if possible_next_offset < mem::size_of::<AllocatedHeapBlockInfo>() {
            // if we can't fit the info header, we need to add to the offset
            possible_next_offset + mem::size_of::<AllocatedHeapBlockInfo>().max(layout.align())
        } else {
            possible_next_offset
        };
        ```
- Then, the `ptr` result will be `free_block + allocated_block_offset`.
  And the metadata will be stored in `ptr - mem::size_of::<AllocatedHeapBlockInfo>()`. i.e. directly before the `ptr`.

  It may seem that this is `unsafe` since `ptr` may not be aligned to `AllocatedHeapBlockInfo`, and thus
  we shouldn't just `ptr - mem::size_of::<AllocatedHeapBlockInfo>()`. But if `layout` has lower alignment than `AllocatedHeapBlockInfo`,
  the previous `if` statement will be `true` and we will get `possible_next_offset + mem::size_of::<AllocatedHeapBlockInfo>().max(layout.align())`.git log

  Maybe this is not the best way, or at least it should be documented better.
- We need to check that `allocated_block_offset` doesn't exceed `block_offset_from_header`, if it does, then `allocation_size` is not enough anymore. As it relies on the previous `block_offset_from_header`, and we are using `allocated_block_offset`, which is higher.
  
  To fix this, we just increase `allocation_size`.
  Currently we don't check that `free_block` is enough for the new `allocation_size`, but we should. (Check [TODO])

- As a last step, we need to shrink/split the `free_block`. The idea is to see if the free block is larger than the `allocation_size`, then we split it into two blocks, one for the allocation and the other for the remaining free block.
  The remaining free block will be kept in the free list.

  But here we must be careful, we need to make sure that the remaining free block is large enough to hold the `HeapFreeBlock` metadata, i.e. `mem::size_of::<HeapFreeBlock>()`. Otherwise, when we update the metadata of the new split, we will overwrite the next block metadata.
  > This was a bug before, and was fixed in [cf53cf9].

  The split is done as such:
    ```rust,no_run
    let required_safe_size = allocation_size + mem::size_of::<HeapFreeBlock>();
    // do we have empty space left?
    if free_block_size > required_safe_size {
        // split
        let new_free_block = free_block + allocation_size;
        // linked-list logic....
    } else {
        // no space left, just remove the block from the free list
        // we need to note the size we took
        allocation_size = free_block_size;
        // linked-list logic....
    }
    ```

  The `add_free_block` will add the new free block to the free list, and try to merge it with the free blocks around it.

- The `pre_padding` is the `allocated_block_offset` value, the `size` is the `allocation_size` at the end after all the considerations.

### `dealloc` algorithm

This is a lot simpler than `alloc`, as the data is already provided by `AllocatedHeapBlockInfo` metadata.

The algorithm is as follows:
- We get the pointer to the `AllocatedHeapBlockInfo` metadata, and we make sure the `magic` is correct.
- We get the `size` and `pre_padding` from the metadata, the freeing block address will be `ptr - pre_padding`, and the size will be `size`.
- We call `free_block` here, which is the same function used when getting new pages.

#### `free_block` algorithm

This is a function that will take a pointer to a block and its size, and add it to the free list.

In the beginning we look at all the free blocks (maybe slow?) and get the following information:
- previous block if present (this is the block that ends at `ptr - 1`)
- next block if present (this is the block that starts at `ptr + size`)
- "**closest previous block**", this is an optimization, where we find the closest block that ends before `ptr`, the new block (this) will be put after this in the linked list if its not merged with anything before it.
  As we need to keep the list sorted. 
  > And as you might expect this was discovered after a bug in [da23ee9] :'(.

The merge logic is very simple as follows:
- If we have previous **AND** next block, we merge with both. The `next` block will be removed from the list as well.
- If we have previous block, we merge with it, very easy `prev.size += size`.
- If we have next block, we merge with it, the `next` block will be removed and this will replace it in the list.
- If we have none, we just add the block to the list, after the **closest previous block**.
    - If we don't have a **closest previous block**, then we are the first block in the list, and we set it as the `head` of the list.


## TODO
- [ ] Add special implementation for `realloc`, which would be smarter without the need to `alloc` and `dealloc`.
- [ ] I think there is a better way to implement `alloc`, currently, we waste a lot of space. Imagine this
      `layout` is `align=512` and `AllocatedHeapBlockInfo` is `align=8,size=32`. The `whole_layout` will be `align=512` and the allocated size will be `1024` even though we clearly don't need that.
      Also we can improve how we handle unaligned `free_block`.
- [ ] Handle cases where we can't increase `allocation_size` as the `free_block` isn't enough



[`increasing_heap_allocator`]: https://crates.io/crates/increasing_heap_allocator
[`Layout::align`]: https://doc.rust-lang.org/std/alloc/struct.Layout.html#method.align
[471eff0]: https://github.com/Amjad50/OS/commit/471eff0
[cf53cf9]: https://github.com/Amjad50/OS/commit/cf53cf9
[da23ee9]: https://github.com/Amjad50/OS/commit/da23ee9
[TODO]: #TODO