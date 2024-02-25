use std::{fs::File, io::Read, thread::sleep};

use kernel_user_link::mouse::MOUSE_PATH;
pub use kernel_user_link::mouse::{buttons, MouseEvent, ScrollType};

pub struct Mouse {
    file: File,
}

impl Mouse {
    pub fn new() -> Self {
        let file = File::open(MOUSE_PATH).unwrap();
        Mouse { file }
    }

    pub fn get_event(&mut self) -> Option<MouseEvent> {
        let mut buf = [0; MouseEvent::BYTES_SIZE];
        if self.file.read(&mut buf).unwrap() == MouseEvent::BYTES_SIZE {
            // Safety: we are using the same size as the MouseEvent struct
            // and this is provided by the kernel, so it must
            // be valid
            Some(unsafe { MouseEvent::from_bytes(buf) })
        } else {
            None
        }
    }

    pub fn iter_events(&mut self) -> impl Iterator<Item = MouseEvent> + '_ {
        std::iter::from_fn(move || self.get_event())
    }

    // TODO: this is a temporary solution, we should use a better way to wait for input
    pub fn wait_for_events(&mut self) -> MouseEvent {
        loop {
            if let Some(key) = self.get_event() {
                return key;
            }
            sleep(std::time::Duration::from_millis(10));
        }
    }
}
