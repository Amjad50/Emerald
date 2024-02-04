### Increasing Heap Allocator

This is a simple implementation of a heap allocator that can be used anywhere.

It only requires the ability to get more `pages` of memory, these pages must be after one another.

It can be implemented easily for example using `sbrk`, but also in a custom kernel.

See: https://github.com/Amjad50/Emerald