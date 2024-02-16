{{ #include ../../links.md }}

# Virtual space

> This is implemented in [`virtual_space`][kernel_virtual_space]

Virtual space is a I'm using (not sure what other OSes call), that solves the issue of "I have a physical address of an object, but I don't have virtual space to map it to".
This is useful for reading structures that are in specific location in physical memory, such as `ACPI` tables, `PCI` configuration space, `memory mapped IO`, etc.

Its very simple, it will take memory from the `kernel extra` space, and map it to the physical address.

It can be used by [`VirtualSpace`][virtual_space_struct], which is similar to `Box`, i.e. its a wrapper for a pointer, and it will automatically unmap the memory when it goes out of scope.

```rust
let mut vs = unsafe { VirtualSpace::<u32>::new(0x1000).unwrap() };
*vs = 0x1234;
assert_eq!(*vs, 0x1234);
```
