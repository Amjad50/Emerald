use core::sync::atomic::{AtomicU8, Ordering};

use blinkcast::alloc::{Receiver as BlinkcastReceiver, Sender as BlinkcastSender};
use kernel_user_link::keyboard::{modifier, Key, KeyType};

use super::ps2::Ps2;

/// Number of key events that can be buffered before being overwritten
/// We are expecting interested readers to be fast, so we don't need a very large buffer
const KEYBOARD_BUFFER_SIZE: usize = 256;

const KEY_PRESSED: u8 = 1 << 7;

// PS/2 keyboard interrupt
pub const KEYBOARD_INT_NUM: u8 = 1;

pub type KeyboardReader = BlinkcastReceiver<Key>;

pub struct Keyboard {
    active_modifiers: AtomicU8,
    active_toggles: AtomicU8,
    ps2: Ps2,

    sender: BlinkcastSender<Key>,
}

impl Keyboard {
    pub fn new(ps2: Ps2) -> Keyboard {
        let sender = BlinkcastSender::new(KEYBOARD_BUFFER_SIZE);
        Keyboard {
            active_modifiers: AtomicU8::new(0),
            active_toggles: AtomicU8::new(0),
            ps2,
            sender,
        }
    }

    pub fn new_receiver(&self) -> KeyboardReader {
        self.sender.new_receiver()
    }

    fn modifiers(&self) -> u8 {
        // remove the saved toggles (this is used for safe-keeping which toggle are we still pressing)
        let modifiers_only = self.active_modifiers.load(Ordering::Relaxed)
            & !(modifier::CAPS_LOCK | modifier::NUM_LOCK | modifier::SCROLL_LOCK);

        modifiers_only | self.active_toggles.load(Ordering::Relaxed)
    }

    pub fn handle_keyboard_data(&self) {
        if !self.ps2.has_data() {
            return;
        }

        let data = self.ps2.read_data();

        if data == 0xE0 {
            // this is an extended key
            let data = self.ps2.read_data();
            let pressed = data & KEY_PRESSED == 0;
            let Some(key) = key_type_from_device(data | 0x80) else {
                // not a valid key
                return;
            };

            self.sender.send(Key {
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
        let Some(key_type) = key_type_from_device(data) else {
            // not a valid key
            return;
        };

        self.sender.send(Key {
            pressed,
            modifiers: self.modifiers(),
            key_type,
        })
    }
}

// 0x80 means extended key
fn key_type_from_device(value: u8) -> Option<KeyType> {
    if value & 0x80 == 0 {
        if value <= KeyType::F12 as u8 {
            // not extended.
            // we know that we are mapping the not extended keys directly
            // so we can just cast it
            let k = unsafe { core::mem::transmute::<u8, KeyType>(value) };
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
