{{ #include ../../links.md }}

# Virtual space

> This is implemented in [`virtual_space`][kernel_virtual_space]

Virtual space is a I'm using (not sure what other OSes call), that solves the issue of "I have a physical address of an object, but I don't have virtual space to map it to".
This is useful for reading structures that are in specific location in physical memory, such as `ACPI` tables, `PCI` configuration space, `memory mapped IO`, etc.

Its very simple, it will take memory from the `kernel extra` space, and map it to the physical address.

It can be used by [`allocate_and_map_virtual_space`][allocate_and_map_virtual_space] and [`deallocate_virtual_space`][deallocate_virtual_space], which allow mapping and unmapping of virtual space.

Currently its very basic and will return a number pointer, which should be `unsafe` XD, but I'm planning
to make it a safer API.
