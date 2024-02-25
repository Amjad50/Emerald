{{ #include ../../links.md }}

# Mouse

> This is implemented in [`mouse`][mouse]


The mouse driver is simple, and uses the legacy PS/2 interface at `0x60` and `0x64` ports, 
its implemented alongside the [keyboard](./keyboard.md) driver in the same file.

The driver provide events broadcasts to all listeners using [`blinkcast`]. These listeners
are mostly processes reading from the `/devices/mouse` file (see [mouse reader](#mouse-reader)).
```rust
pub enum ScrollType {
    None = 0,
    VerticalUp = 1,
    VerticalDown = 2,
    HorizontalRight = 3,
    HorizontalNegative = 4,
}

pub struct MouseEvent {
    pub x: i16,
    pub y: i16,
    pub scroll_type: ScrollType,
    pub buttons: u8,
}
```

The `buttons` field is a bitflags from [`buttons`], so use these constants to check a button is pressed.

Note, that this is the state of the mouse, so you must keep the old state to know if a button was pressed or released.

The buttons are:
- `LEFT`: `0b0000_0001`
- `RIGHT`: `0b0000_0010`
- `MIDDLE`: `0b0000_0100`
- `FORTH`: `0b0000_1000`
- `FIFTH`: `0b0001_0000`

## Mouse reader
The keyboard driver provide a way to get a [`blinkcast`] reader using [`get_mouse_reader`][get_mouse_reader], 
where the user can read mouse events without blocking anytime they want.

Userspace processes can read the mouse events through the virtual device at `/devices/mouse`.

A process can open a file descriptor to this device and read from it to get mouse events.

The file descriptor will hold a [`blinkcast`] reader to the mouse driver, then each process can read events without blocking.

The user can open the file and read the content, but since we are performing some encoding, its better to use the library [`emerald_runtime`] which provide easy way to read the events.

Example:
    
```rust,no_run,no_compile
use emerald_runtime::mouse::Mouse;

let mut mouse = Mouse::new();

if let Some(event) = mouse.get_event() {
    println!("Event: {:?}", event);
}
// or
for event in mouse.iter_events() {
    println!("Event: {:?}", event);
}
```

[`blinkcast`]: https://crates.io/crates/blinkcast
[`buttons`]: https://docs.rs/emerald_kernel_user_link/0.2.6/emerald_kernel_user_link/mouse/buttons/index.html
[`emerald_runtime`]: https://crates.io/crates/emerald_runtime
