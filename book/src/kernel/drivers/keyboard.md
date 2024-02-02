{{ #include ../../links.md }}

# Keyboard

> This is implemented in [`keyboard`][keyboard]

The keyboard driver is simple, and uses the legacy PS/2 interface at `0x60` and `0x64` ports.

Currently we support the `US` layout only, but we have the return type from the device as 
```rust
pub struct Key {
    pub virtual_char: Option<u8>,
    pub key_type: KeyType,
}
```

Where `KeyType` is an enum containing all keys, even not mapping to a character, like arrows, function keys, etc.
Which we can use later to move mapping and other complex structures into the userspace.

The keyboard provide [`get_next_char`][keyboard_get_next_char] which will keep up to last `8` characters in buffer.
This is low level function, and will probably be read a lot from usermode, and `8` will probably be a lot of characters to keep in buffer, but lets see when the time comes.

This is currently used by the [console](../virtual_devices/console.md) to read characters.
