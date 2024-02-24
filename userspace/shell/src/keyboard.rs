use colored::Colorize;
use emerald_keyboard::{KeyType, Keyboard};

fn main() {
    println!("Keyboard debugging tool. Press {} to exit:", "ESC".blue());

    // stay in a loop, read the keyboard input and print it
    // exit on ESC key
    let mut keyboard = Keyboard::new();
    loop {
        if let Some(key) = keyboard.get_key_event() {
            let press = if key.pressed { "+".green() } else { "-".red() };
            println!("[{press}] {}", format!("{:?}", key.key_type).blue());
            if key.key_type == KeyType::Escape {
                break;
            }
        }
    }
}
