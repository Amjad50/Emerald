{{ #include ../../links.md }}

# Console

> This is implemented in [`console`][console]

The console is a virtual device that we use to print and read characters from the screen, it will use the [keyboard] and [uart] drivers to do so.

This is called by `print!` and `println!` macros.

We have 2 consoles, for now, I don't like the design now, and would like to change it in the future.

## `EarlyConsole`
This is a console object that is statically initialized, can only write, and doesn't have access to the keyboard.

## `LateConsole`
This is the main console that is initialized later, and can read and write and has access to keyboard.

The main purpose of this is to add this to the `/devices` directory, and act as a kernel device, so we can use it from the userspace.

The design can be improved, the issue is that `LateConsole` is inside an `Arc<Mutex<>>`
(so it can be used as a device), `EarlyConsole` is `static`,
there is several differences, so there is a lot of code duplication, and I would like to improve it somehow.
