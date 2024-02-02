{{ #include ../../links.md }}

# UART

> This is implemented in [`uart`][uart]

A very basic UART driver, connects to `0x3F8` port (COM1). And can be read and written to.

It is used by the [console](../virtual_devices/console.md) to print and read characters.