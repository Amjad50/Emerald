use std::{fs::File, io::Read, thread::sleep};

use kernel_user_link::keyboard::KEYBOARD_PATH;
pub use kernel_user_link::keyboard::{modifier, Key, KeyType};

pub struct Keyboard {
    file: File,
}

impl Keyboard {
    pub fn new() -> Self {
        let file = File::open(KEYBOARD_PATH).unwrap();
        Keyboard { file }
    }

    pub fn get_key_event(&mut self) -> Option<Key> {
        let mut buf = [0; Key::BYTES_SIZE];
        if self.file.read(&mut buf).unwrap() == Key::BYTES_SIZE {
            // Safety: we are using the same size as the Key struct
            // and this is provided by the kernel, so it must
            // be valid
            Some(unsafe { Key::from_bytes(buf) })
        } else {
            None
        }
    }

    pub fn iter_keys(&mut self) -> impl Iterator<Item = Key> + '_ {
        std::iter::from_fn(move || self.get_key_event())
    }

    // TODO: this is a temporary solution, we should use a better way to wait for input
    pub fn wait_for_input(&mut self) -> Key {
        loop {
            if let Some(key) = self.get_key_event() {
                return key;
            }
            // core::hint::spin_loop();
            sleep(std::time::Duration::from_millis(10));
        }
    }
}
