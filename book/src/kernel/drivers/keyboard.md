{{ #include ../../links.md }}

# Keyboard

> This is implemented in [`keyboard`][keyboard]

The keyboard driver is simple, and uses the legacy PS/2 interface at `0x60` and `0x64` ports.

The driver provide events broadcasts to all listeners using [`blinkcast`]. These listeners
are mostly processes reading from the `/devices/keyboard` file (see [keyboard virtual device](../virtual_devices/keyboard.md)).
```rust
pub struct Key {
    pub pressed: bool,
    // the state of the modifiers at the time of the fetch
    pub modifiers: u8,
    pub key_type: KeyType,
}
```

Where `KeyType` is an enum containing all keys from a US mapping.

The keyboard user can then use this as the origin, and map it to any other key depending on the layout they want.

Currently, we use the `US` layout to get the character of a key using the function [`Key::virtual_key`] (used in the kernel and userspace).

The `modifiers` field is a bitflags from [`modifier`], so use these constants to check if a specific modifier is on.

There are 2 types of modifiers:
- Held modifiers: `SHIFT`, `CTRL`, `ALT`
- Toggled modifiers: `CAPSLOCK`, `NUMLOCK`, `SCROLLLOCK`

The keyboard driver provide a way to get a [`blinkcast`] reader using [`get_reader`][keyboard_get_reader], where the user can read
keyboard events without blocking anytime they want.

This is currently used by the [console](../virtual_devices/console.md) to read characters.

[`blinkcast`]: https://crates.io/crates/blinkcast
[`Key::virtual_key`]: https://docs.rs/emerald_kernel_user_link/0.2.5/emerald_kernel_user_link/keyboard/struct.Key.html#method.virtual_char
[`modifier`]: https://docs.rs/emerald_kernel_user_link/0.2.5/emerald_kernel_user_link/keyboard/modifier
