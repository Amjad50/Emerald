//! Reading keyboard events from [`KeyboardReader`]

use alloc::sync::Arc;
use core::{
    fmt,
    sync::atomic::{AtomicU8, Ordering},
};

use crate::{
    cpu::{
        self,
        idt::{BasicInterruptHandler, InterruptStackFrame64},
        interrupts::apic,
    },
    sync::{once::OnceLock, spin::mutex::Mutex},
};

use blinkcast::alloc::{Receiver as BlinkcastReceiver, Sender as BlinkcastSender};
pub use kernel_user_link::keyboard::{modifier, Key, KeyType};

use super::Device;

static KEYBOARD: OnceLock<Arc<Keyboard>> = OnceLock::new();

/// Number of key events that can be buffered before being overwritten
/// We are expecting interested readers to be fast, so we don't need a very large buffer
const KEYBOARD_BUFFER_SIZE: usize = 256;

pub fn init_keyboard() {
    let keyboard = Keyboard::empty();

    let device = Arc::new(keyboard);
    KEYBOARD
        .set(device)
        .unwrap_or_else(|_| panic!("keyboard already initialized"));

    // assign after we have assigned the keyboard
    apic::assign_io_irq(
        keyboard_interrupt_handler as BasicInterruptHandler,
        KEYBOARD_INT_NUM,
        cpu::cpu(),
    )
}

pub fn get_keyboard_reader() -> KeyboardReader {
    KEYBOARD.get().get_reader()
}

// PS/2 keyboard interrupt
const KEYBOARD_INT_NUM: u8 = 1;

const KEYBOARD_STATUS_PORT: u16 = 0x64;
const KEYBOARD_DATA_PORT: u16 = 0x60;

// 0x80 means extended key
fn key_type_from_device(value: u8) -> Option<KeyType> {
    if value & 0x80 == 0 {
        if value <= KeyType::F12 as u8 {
            // not extended.
            // we know that we are mapping the not extended keys directly
            // so we can just cast it
            let k = unsafe { core::mem::transmute(value) };
            if k == KeyType::_None1
                || k == KeyType::_None2
                || k == KeyType::_None3
                || k == KeyType::_None4
            {
                return None;
            }

            Some(k)
        } else {
            // not a valid key
            None
        }
    } else {
        // first, strip the extension
        let key = value & !0x80;

        // use match normally
        match key {
            0x10 => Some(KeyType::MultimediaPreviousTrack),
            0x19 => Some(KeyType::MultimediaNextTrack),
            0x1C => Some(KeyType::KeypadEnter),
            0x1D => Some(KeyType::RightCtrl),
            0x20 => Some(KeyType::MultimediaMute),
            0x21 => Some(KeyType::Calculator),
            0x22 => Some(KeyType::MultimediaPlayPause),
            0x24 => Some(KeyType::MultimediaStop),
            0x2E => Some(KeyType::VolumeDown),
            0x30 => Some(KeyType::VolumeUp),
            0x32 => Some(KeyType::WWWHome),
            0x35 => Some(KeyType::KeypadSlash),
            0x38 => Some(KeyType::RightAlt),
            0x47 => Some(KeyType::Home),
            0x48 => Some(KeyType::UpArrow),
            0x49 => Some(KeyType::PageUp),
            0x4B => Some(KeyType::LeftArrow),
            0x4D => Some(KeyType::RightArrow),
            0x4F => Some(KeyType::End),
            0x50 => Some(KeyType::DownArrow),
            0x51 => Some(KeyType::PageDown),
            0x52 => Some(KeyType::Insert),
            0x53 => Some(KeyType::Delete),
            0x5B => Some(KeyType::LeftGUI),
            0x5C => Some(KeyType::RightGUI),
            0x5D => Some(KeyType::Application),
            0x5E => Some(KeyType::Power),
            0x5F => Some(KeyType::Sleep),
            0x63 => Some(KeyType::Wake),
            0x65 => Some(KeyType::WWWSearch),
            0x66 => Some(KeyType::WWWFavorites),
            0x67 => Some(KeyType::WWWRefresh),
            0x68 => Some(KeyType::WWWStop),
            0x69 => Some(KeyType::WWWForward),
            0x6A => Some(KeyType::WWWBack),
            0x6B => Some(KeyType::MyComputer),
            0x6C => Some(KeyType::Email),
            0x6D => Some(KeyType::MultimediaSelect),
            _ => None,
        }
    }
}

#[allow(dead_code)]

const fn get_modifier(key: u8) -> Option<u8> {
    match key {
        0x2A => Some(modifier::SHIFT),
        0x36 => Some(modifier::SHIFT),
        0x1D => Some(modifier::CTRL),
        0x38 => Some(modifier::ALT),
        _ => None,
    }
}

const fn get_toggle(key: u8) -> Option<u8> {
    match key {
        0x3A => Some(modifier::CAPS_LOCK),
        0x45 => Some(modifier::NUM_LOCK),
        0x46 => Some(modifier::SCROLL_LOCK),
        _ => None,
    }
}

const KEY_PRESSED: u8 = 1 << 7;

#[allow(dead_code)]
mod status {
    pub const DATA_READY: u8 = 1 << 0;
    pub const INPUT_BUFFER_FULL: u8 = 1 << 1;
    pub const SYSTEM_FLAG: u8 = 1 << 2;
    pub const COMMAND_DATA: u8 = 1 << 3;
    pub const KEYBOARD_LOCKED: u8 = 1 << 4;
    pub const KEYBOARD_TIMEOUT_MOUSE_DATA: u8 = 1 << 5;
    pub const RECEIVE_TIMEOUT: u8 = 1 << 6;
    pub const PARITY_ERROR: u8 = 1 << 7;
}

pub type KeyboardReader = BlinkcastReceiver<Key>;

// A mini keyboard driver/mapper
pub struct Keyboard {
    active_modifiers: AtomicU8,
    active_toggles: AtomicU8,
    input_ring: BlinkcastSender<Key>,
}

impl fmt::Debug for Keyboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Keyboard").finish()
    }
}

impl Keyboard {
    pub fn empty() -> Self {
        Self {
            active_modifiers: AtomicU8::new(0),
            active_toggles: AtomicU8::new(0),
            input_ring: BlinkcastSender::new(KEYBOARD_BUFFER_SIZE),
        }
    }

    fn read_status(&self) -> u8 {
        unsafe { cpu::io_in(KEYBOARD_STATUS_PORT) }
    }

    fn read_data(&self) -> u8 {
        unsafe { cpu::io_in(KEYBOARD_DATA_PORT) }
    }

    pub fn modifiers(&self) -> u8 {
        // remove the saved toggles (this is used for safe-keeping which toggle are we still pressing)
        let modifiers_only = self.active_modifiers.load(Ordering::Relaxed)
            & !(modifier::CAPS_LOCK | modifier::NUM_LOCK | modifier::SCROLL_LOCK);

        modifiers_only | self.active_toggles.load(Ordering::Relaxed)
    }

    fn try_read_char(&self) -> Option<Key> {
        if self.read_status() & status::DATA_READY == 0 {
            return None;
        }

        let data = self.read_data();

        if data == 0xE0 {
            // this is an extended key
            let data = self.read_data();
            let pressed = data & KEY_PRESSED == 0;
            let key = key_type_from_device(data | 0x80)?;

            return Some(Key {
                pressed,
                modifiers: self.modifiers(),
                key_type: key,
            });
        }

        let pressed = data & KEY_PRESSED == 0;
        let data = data & !KEY_PRESSED; // strip the pressed bit
        if let Some(modifier_key) = get_modifier(data) {
            if pressed {
                self.active_modifiers
                    .fetch_or(modifier_key, Ordering::Relaxed);
            } else {
                self.active_modifiers
                    .fetch_and(!modifier_key, Ordering::Relaxed);
            }
        } else if let Some(toggle_key) = get_toggle(data) {
            // keep a copy in the modifier so that we only toggle on a press
            let should_toggle =
                pressed && self.active_modifiers.load(Ordering::Relaxed) & toggle_key == 0;
            if should_toggle {
                self.active_toggles.fetch_xor(toggle_key, Ordering::Relaxed);
            }

            // add to the modifier
            if pressed {
                self.active_modifiers
                    .fetch_or(toggle_key, Ordering::Relaxed);
            } else {
                self.active_modifiers
                    .fetch_and(!toggle_key, Ordering::Relaxed);
            }
        }
        // this is a normal key
        let key_type = key_type_from_device(data)?;

        Some(Key {
            pressed,
            modifiers: self.modifiers(),
            key_type,
        })
    }

    pub fn get_reader(&self) -> KeyboardReader {
        self.input_ring.new_receiver()
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame64) {
    let keyboard = KEYBOARD.get();
    // fill in the buffer, and replace if filled
    while let Some(key) = keyboard.try_read_char() {
        keyboard.input_ring.send(key);
    }

    apic::return_from_interrupt();
}

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
