{{ #include ../../links.md }}

# Virtual Mapper

> This is implemented in [`virtual_memory_mapper`][virtual_memory_mapper]

This is where we map physical memory to virtual memory, and where virtual memory pages are managed after [boot].

The main features is to map and unmap physical memory to virtual memory, and to allocate and free virtual memory pages.

The `map` function takes 1 argument `VirtualMemoryMapEntry` which is a struct contains information about the mapping:
```rust
pub struct VirtualMemoryMapEntry {
    pub virtual_address: usize,
    pub physical_address: Option<u64>,
    pub size: usize,
    pub flags: u64,
}
```
- `virtual_address` is the virtual address to map to, and must be known of course
- `physical_address` is the physical address to map to, if its `None`, then we will allocate new memory from the [physical allocator]
- `size` is the size of the mapping, must be `4K` aligned
- `flags` is the flags of the mapping, such as `Writable`, `UserAccessible`. For now, these are just constants mapping directly to
  `x86` page table flags.

The `unmap` function takes the same struct, but `physical_address` must be `None`, and `virtual_address` must be known, it also takes
`is_allocated` which is a boolean to indicate if the memory was allocated by the virtual memory mapper, if it was, it will be freed.

This API can be improved, but currently we don't keep track of the mappings, so we rely on the caller to do that for us :D.

Beside that, we got other functionalities used by processes. Like:
- `switch_to_this`: to switch to the `self` `VirtualMemoryMapper`, which each process has its own `VirtualMemoryMapper`.
- `get_current_vm`: to get the current `VirtualMemoryMapper` of the current process.
- `clone_current_vm_as_user`: Clones the kernel mappings of the current vm, and mark it as `user` vm,
  so it doesn't allow `kernel` mappings anymore.

[boot](../boot.md)
[physical allocator](./physical_allocator.md)