{{ #include ../../links.md }}

# Physical Allocator

> This is implemented in [`physical_page_allocator`][physical_page_allocator]

This provides physical memory allocation and deallocation.

Currently it is very basic, and can allocate in 4KB pages. And only allocates 1 page at a time.
This is due to our design.

## Current design
The design now is built using a linked list, each page point to the next free page. The reason we can't
get more pages is that when we create the list, we create it by **freeing** all the pages,
and **free** means that we add it to the list.

The issue is that we are adding them one by one, start to finish, and when we allocate, we take the last
one from the list, so its disconnected from the rest of the pages, it doesn't know if the next page is immediately
after it in memory or not.

This could be solved by having a more complex allocator, like what we have in the [heap allocator], but I want to
use another design that is fast, since that one is slow.

Another issue is that we only have `128MB` of memory to allocate from, and we can't allocate more than that.

This is not a design issue, but the `physical page allocator` initially relies on the memory we have during `boot`
where we map the first `128MB` of memory directly into the kernel space, see [boot] and [memory layout] for more details.

## Design issues to fix
- Can only allocate 1 page at a time
- Only has `128MB` of memory to allocate from


## Ideas
Linux uses a tree like structure, where each node in the tree is a page, and the children are the sub-pages, this allows easy allocation of powers of `4KB` pages, and also allows easy coalescing of pages.

I'm thinking of doing something like that.

For the `128MB` limit, I'm thinking of having another allocator, that won't have direct access to the memory, i.e. is not mapped, and we will store metadata about those pages in the heap, which we already have access to. i.e. it will be a "late" physical page allocator.

[heap allocator]: ../../extra/heap_allocator.md
[boot]: ../boot.md
[memory layout]: ./memory_layout.md