use colored::Colorize;
use emerald_runtime::{
    keyboard::{KeyType, Keyboard},
    mouse::Mouse,
};

fn main() {
    println!("Mouse debugging tool. Press {} to exit:", "ESC".blue());

    // stay in a loop, read the keyboard input and mouse input
    // only debug the mouse input
    // exit on ESC key
    let mut keyboard = Keyboard::new();
    let mut mouse = Mouse::new();

    loop {
        for key in keyboard.iter_keys() {
            if key.key_type == KeyType::Escape {
                return;
            }
        }

        for mouse_event in mouse.iter_events() {
            println!("{:?}", mouse_event);
        }
    }
}
