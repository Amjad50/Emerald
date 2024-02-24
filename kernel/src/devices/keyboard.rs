//! Reading keyboard events from [`KeyboardReader`]

use core::fmt;

use alloc::sync::Arc;

use crate::{
    io::keyboard::{self, Key, KeyboardReader},
    sync::spin::mutex::Mutex,
};

use super::Device;

/// This is just a helper definition, and should not be used directly in any file
#[derive(Debug)]
pub struct KeyboardDeviceCreator;

impl Device for KeyboardDeviceCreator {
    fn name(&self) -> &str {
        "keyboard"
    }

    fn clone_device(&self) -> Result<(), crate::fs::FileSystemError> {
        Err(crate::fs::FileSystemError::OperationNotSupported)
    }

    fn try_create(&self) -> Option<Result<Arc<dyn Device>, crate::fs::FileSystemError>> {
        Some(Ok(Arc::new(KeyboardDevice {
            reader: Mutex::new(keyboard::get_keyboard_reader()),
        })))
    }
}

pub struct KeyboardDevice {
    reader: Mutex<KeyboardReader>,
}

impl fmt::Debug for KeyboardDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeyboardDevice")
    }
}

impl Device for KeyboardDevice {
    fn name(&self) -> &str {
        "keyboard_instance"
    }

    fn read(&self, _offset: u64, buf: &mut [u8]) -> Result<u64, crate::fs::FileSystemError> {
        if buf.len() < Key::BYTES_SIZE {
            return Err(crate::fs::FileSystemError::BufferNotLargeEnough(
                Key::BYTES_SIZE,
            ));
        }

        let mut reader = self.reader.lock();

        let mut i = 0;

        while i + Key::BYTES_SIZE <= buf.len() {
            if let Some(key) = reader.recv() {
                let key_bytes = key.as_bytes();
                buf[i..i + Key::BYTES_SIZE].copy_from_slice(&key_bytes);
                i += Key::BYTES_SIZE;
            } else {
                break;
            }
        }

        Ok(i as u64)
    }
}
