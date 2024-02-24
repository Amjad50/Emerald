{{ #include ../../links.md }}

# Keyboard

> This is implemented in [`keyboard_device`][keyboard_device]

Don't be confused with [keyboard driver](../drivers/keyboard.md), in there, we implement the hardware driver.

This module is a client of that driver and provide user space access to the keyboard through the virtual device at `/devices/keyboard`.

A process can open a file descriptor to this device and read from it to get keyboard events.

The file descriptor will hold a [`blinkcast`] reader to the keyboard driver, then each process can read events without blocking.

The user can open the file and read the content, but since we are performing some encoding, its better to use the library [`emerald_keyboard`] which provide easy way to read the events.

Example:
    
```rust,no_run,no_compile
use emerald_keyboard::Keyboard;

let mut keyboard = Keyboard::new();

if let Some(key) = keyboard.get_key_event() {
    println!("Key: {:?}", key);
}
// or
for key in keyboard.iter_keys() {
    println!("Key: {:?}", key);
}
```

[`blinkcast`]: https://crates.io/crates/blinkcast
[`emerald_keyboard`]: https://crates.io/crates/emerald_keyboard


