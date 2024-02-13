# Memory

Explaining the memory management in the kernel.

We have several tools to manage memory in the kernel, each with its own purpose and goal.

First, let's look at the [memory layout] of the kernel, to know where everything is located.

Then, we can look at the [physical allocator] which provide us with the raw memory from the hardware,
which we can use later with the [virtual mapper] to map it into virtual memory in order to access it.

The heap in the kernel is implemented with [heap allocator], and the `PageAllocator` will allocate pages using the [virtual mapper].

Lastly we have the [virtual space] which is a very useful tool to get a virtual address for components that are not
part of the kernel directly but we know where they are located in physical space. This includes, `Memory mapped IO`, `ACPI` structures, `Multiboot` structures, etc.


## Memory pointer types
Another thing you might notice in the code is that we use `u64` sometimes and `usize` other times. Here is what they mean:

- `u64`: This is a 64-bit unsigned integer, and it will be `64` no matter the platform.
  It is used to represent physical addresses. Because in `x86` with `CR0.PG` bit enabled, the CPU can map
  `40-bit` physical addresses to `32-bit` virtual addresses. And thus it can be more than `32-bit`.
- `usize`: This is a pointer-sized unsigned integer, and it will be `32` or `64` depending on the platform.
  It is used to represent virtual addresses, and it is the same size as a pointer on the platform.

Something tangent. For filesystem operations we only use `u64`,
as hardware drives can have easily more than `4GB` of space. without needing for the CPU to be `64-bit`.


[physical allocator]: physical_allocator.md
[virtual mapper]: virtual_mapper.md
[virtual space]: virtual_space.md
[memory layout]: memory_layout.md
[heap allocator]: ../../extra/heap_allocator.md
