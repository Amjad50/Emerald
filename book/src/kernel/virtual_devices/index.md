# Virtual devices

These are "devices" that we interact with from other parts of the kernel, but may not be actually available in hardware.
They provide a sort of abstraction layer above other drivers/devices and maybe other components.

One of the examples, is the [console], which is a virtual device that we use to print and read characters from the screen, it will use the [keyboard] and [uart] drivers to do so.

[console]: ./console.md