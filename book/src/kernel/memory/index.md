# Memory

Explaining the memory management in the kernel.

We have several tools to manage memory in the kernel, each with its own purpose and goal.

First, let's look at the [memory layout] of the kernel, to know where everything is located.

Then, we can look at the [physical allocator] which provide us with the raw memory from the hardware,
which we can use later with the [virtual mapper] to map it into virtual memory in order to access it.

The heap in the kernel is implemented with [heap allocator], and the `PageAllocator` will allocate pages using the [virtual mapper].

Lastly we have the [virtual space] which is a very useful tool to get a virtual address for components that are not
part of the kernel directly but we know where they are located in physical space. This includes, `Memory mapped IO`, `ACPI` structures, `Multiboot` structures, etc.


[physical allocator]: physical_allocator.md
[virtual mapper]: virtual_mapper.md
[virtual space]: virtual_space.md
[memory layout]: memory_layout.md
[heap allocator]: ../../extra/heap_allocator.md
