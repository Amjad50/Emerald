mod keyboard;
mod mouse;
mod ps2;

use alloc::sync::Arc;
use core::fmt;

use crate::{
    cpu::{
        self,
        idt::{BasicInterruptHandler, InterruptStackFrame64},
        interrupts::apic,
    },
    sync::{once::OnceLock, spin::mutex::Mutex},
};

pub use kernel_user_link::{keyboard::Key, mouse::MouseEvent};

use self::{
    keyboard::{Keyboard, KEYBOARD_INT_NUM},
    mouse::{Mouse, MOUSE_INT_NUM},
    ps2::status,
};

pub use self::{keyboard::KeyboardReader, mouse::MouseReader};

use super::Device;

static KEYBOARD_MOUSE: OnceLock<KeyboardMouse> = OnceLock::new();

pub fn init_device() {
    let device = KeyboardMouse::new();

    KEYBOARD_MOUSE
        .set(device)
        .unwrap_or_else(|_| panic!("keyboard-mouse already initialized"));

    // assign after we have assigned the device
    apic::assign_io_irq(
        ps2_interrupt_handler as BasicInterruptHandler,
        KEYBOARD_INT_NUM,
        cpu::cpu(),
    );
    apic::assign_io_irq(
        ps2_interrupt_handler as BasicInterruptHandler,
        MOUSE_INT_NUM,
        cpu::cpu(),
    );
}

pub fn poll_events() {
    KEYBOARD_MOUSE.try_get().map(KeyboardMouse::read_all_events);
}

pub fn get_keyboard_reader() -> KeyboardReader {
    KEYBOARD_MOUSE.get().get_keyboard_reader()
}

pub fn get_mouse_reader() -> MouseReader {
    KEYBOARD_MOUSE.get().get_mouse_reader()
}

pub fn reset_system() -> ! {
    KEYBOARD_MOUSE.get().ps2.reset_system();
}

// A mini keyboard driver/mapper
pub struct KeyboardMouse {
    keyboard: Keyboard,
    mouse: Mouse,
    ps2: ps2::Ps2,
}

impl fmt::Debug for KeyboardMouse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Keyboard").finish()
    }
}

impl KeyboardMouse {
    pub fn new() -> KeyboardMouse {
        let ps2 = ps2::Ps2;
        let keyboard = Keyboard::new(ps2);
        let mouse = Mouse::new(ps2);

        KeyboardMouse {
            keyboard,
            mouse,
            ps2,
        }
    }

    fn fetch_next_data(&self) -> bool {
        let status = self.ps2.read_status();

        if status & status::DATA_READY == 0 {
            return false;
        }

        if status & status::PARITY_ERROR != 0 {
            // ignore the data
            self.ps2.read_data();
            return false;
        }

        if status & status::KEYBOARD_TIMEOUT_MOUSE_DATA != 0 {
            self.mouse.handle_mouse_data();
        } else {
            self.keyboard.handle_keyboard_data();
        }

        true
    }

    fn read_all_events(&self) {
        while self.fetch_next_data() {
            // keep fetching
        }
    }

    pub fn get_keyboard_reader(&self) -> KeyboardReader {
        self.keyboard.new_receiver()
    }

    #[allow(dead_code)]
    pub fn get_mouse_reader(&self) -> MouseReader {
        self.mouse.new_receiver()
    }
}

extern "x86-interrupt" fn ps2_interrupt_handler(_stack_frame: InterruptStackFrame64) {
    poll_events();
    apic::return_from_interrupt();
}

// Virtual devices
// -------------------

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
            reader: Mutex::new(get_keyboard_reader()),
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

#[derive(Debug)]
pub struct MouseDeviceCreator;

impl Device for MouseDeviceCreator {
    fn name(&self) -> &str {
        "mouse"
    }

    fn clone_device(&self) -> Result<(), crate::fs::FileSystemError> {
        Err(crate::fs::FileSystemError::OperationNotSupported)
    }

    fn try_create(&self) -> Option<Result<Arc<dyn Device>, crate::fs::FileSystemError>> {
        Some(Ok(Arc::new(MouseDevice {
            reader: Mutex::new(get_mouse_reader()),
        })))
    }
}

pub struct MouseDevice {
    reader: Mutex<MouseReader>,
}

impl fmt::Debug for MouseDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MouseDevice")
    }
}

impl Device for MouseDevice {
    fn name(&self) -> &str {
        "mouse_instance"
    }

    fn read(&self, _offset: u64, buf: &mut [u8]) -> Result<u64, crate::fs::FileSystemError> {
        if buf.len() < MouseEvent::BYTES_SIZE {
            return Err(crate::fs::FileSystemError::BufferNotLargeEnough(
                MouseEvent::BYTES_SIZE,
            ));
        }

        let mut reader = self.reader.lock();

        let mut i = 0;

        while i + MouseEvent::BYTES_SIZE <= buf.len() {
            if let Some(mouse_event) = reader.recv() {
                let event_bytes = mouse_event.as_bytes();
                buf[i..i + MouseEvent::BYTES_SIZE].copy_from_slice(&event_bytes);
                i += MouseEvent::BYTES_SIZE;
            } else {
                break;
            }
        }

        Ok(i as u64)
    }
}
