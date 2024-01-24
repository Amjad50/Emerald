use core::fmt;

use alloc::sync::Arc;

use crate::{
    collections::ring::RingBuffer,
    cpu::{
        self,
        idt::{BasicInterruptHandler, InterruptStackFrame64},
        interrupts::apic,
    },
    sync::{once::OnceLock, spin::mutex::Mutex},
};

static KEYBOARD: OnceLock<Arc<Mutex<Keyboard>>> = OnceLock::new();

pub fn init_keyboard() {
    let keyboard = Keyboard::empty();

    let device = Arc::new(Mutex::new(keyboard));
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

pub fn get_keyboard() -> Arc<Mutex<Keyboard>> {
    KEYBOARD.get().clone()
}

// PS/2 keyboard interrupt
const KEYBOARD_INT_NUM: u8 = 1;

const KEYBOARD_STATUS_PORT: u16 = 0x64;
const KEYBOARD_DATA_PORT: u16 = 0x60;

/// The index is `KeyType`
const US_KEYTYPE_KEYMAP: [u8; 127] = [
    0, 27, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=', b'\x08', b'\t',
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n', 0, b'a', b's',
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', b'\'', b'`', 0, b'\\', b'z', b'x', b'c', b'v',
    b'b', b'n', b'm', b',', b'.', b'/', 0, b'*', 0, b' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    b'7', b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1', b'2', b'3', b'0', b'.', 0, 0, 0, 0, 0, 0,
    0, b'\n', 0, 0, 0, 0, 0, 0, 0, 0, b'/', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
];

/// The index is `KeyType`
const US_KEYTYPE_KEYMAP_SHIFTED: [u8; 127] = [
    0, 27, b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'_', b'+', b'\x08', b'\t',
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P', b'{', b'}', b'\n', 0, b'A', b'S',
    b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':', b'"', b'~', 0, b'|', b'Z', b'X', b'C', b'V',
    b'B', b'N', b'M', b'<', b'>', b'?', 0, b'*', 0, b' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, b'-', 0, b'5', 0, b'+', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyType {
    // normal keys (mapped 1:1 with set 1 scan codes)
    _None1,
    Escape,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Num0,
    Minus,
    Equals,
    Backspace,
    Tab,
    Q,
    W,
    E,
    R,
    T,
    Y,
    U,
    I,
    O,
    P,
    LeftBracket,
    RightBracket,
    Enter,
    LeftCtrl,
    A,
    S,
    D,
    F,
    G,
    H,
    J,
    K,
    L,
    Semicolon,
    SingleQuote,
    Backtick,
    LeftShift,
    Backslash,
    Z,
    X,
    C,
    V,
    B,
    N,
    M,
    Comma,
    Dot,
    Slash,
    RightShift,
    KeypadAsterisk,
    LeftAlt,
    Space,
    CapsLock,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    NumLock,
    ScrollLock,
    Keypad7,
    Keypad8,
    Keypad9,
    KeypadMinus,
    Keypad4,
    Keypad5,
    Keypad6,
    KeypadPlus,
    Keypad1,
    Keypad2,
    Keypad3,
    Keypad0,
    KeypadDot,
    _None2,
    _None3,
    _None4,
    F11,
    F12,

    // extended keys
    MultimediaPreviousTrack,
    MultimediaNextTrack,
    KeypadEnter,
    RightCtrl,
    MultimediaMute,
    Calculator,
    MultimediaPlayPause,
    MultimediaStop,
    VolumeDown,
    VolumeUp,
    WWWHome,
    KeypadSlash,
    RightAlt,
    Home,
    UpArrow,
    PageUp,
    LeftArrow,
    RightArrow,
    End,
    DownArrow,
    PageDown,
    Insert,
    Delete,
    LeftGUI,
    RightGUI,
    Application,
    Power,
    Sleep,
    Wake,
    WWWSearch,
    WWWFavorites,
    WWWRefresh,
    WWWStop,
    WWWForward,
    WWWBack,
    MyComputer,
    Email,
    MultimediaSelect,
}

impl KeyType {
    pub fn virtual_key(&self, shifted: bool) -> Option<u8> {
        let index = *self as usize;
        let mappings = if shifted {
            &US_KEYTYPE_KEYMAP_SHIFTED
        } else {
            &US_KEYTYPE_KEYMAP
        };

        assert!(index < mappings.len());
        let value = mappings[index];

        if value == 0 {
            None
        } else {
            Some(value)
        }
    }
}

impl TryFrom<u8> for KeyType {
    type Error = ();

    // 0x80 means extended key
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value & 0x80 == 0 {
            if value <= Self::F12 as u8 {
                // not extended.
                // we know that we are mapping the not extended keys directly
                // so we can just cast it
                let k = unsafe { core::mem::transmute(value) };
                if k == Self::_None1 || k == Self::_None2 || k == Self::_None3 || k == Self::_None4
                {
                    return Err(());
                }

                Ok(k)
            } else {
                // not a valid key
                Err(())
            }
        } else {
            // first, strip the extension
            let key = value & !0x80;

            // use match normally
            match key {
                0x10 => Ok(Self::MultimediaPreviousTrack),
                0x19 => Ok(Self::MultimediaNextTrack),
                0x1C => Ok(Self::KeypadEnter),
                0x1D => Ok(Self::RightCtrl),
                0x20 => Ok(Self::MultimediaMute),
                0x21 => Ok(Self::Calculator),
                0x22 => Ok(Self::MultimediaPlayPause),
                0x24 => Ok(Self::MultimediaStop),
                0x2E => Ok(Self::VolumeDown),
                0x30 => Ok(Self::VolumeUp),
                0x32 => Ok(Self::WWWHome),
                0x35 => Ok(Self::KeypadSlash),
                0x38 => Ok(Self::RightAlt),
                0x47 => Ok(Self::Home),
                0x48 => Ok(Self::UpArrow),
                0x49 => Ok(Self::PageUp),
                0x4B => Ok(Self::LeftArrow),
                0x4D => Ok(Self::RightArrow),
                0x4F => Ok(Self::End),
                0x50 => Ok(Self::DownArrow),
                0x51 => Ok(Self::PageDown),
                0x52 => Ok(Self::Insert),
                0x53 => Ok(Self::Delete),
                0x5B => Ok(Self::LeftGUI),
                0x5C => Ok(Self::RightGUI),
                0x5D => Ok(Self::Application),
                0x5E => Ok(Self::Power),
                0x5F => Ok(Self::Sleep),
                0x63 => Ok(Self::Wake),
                0x65 => Ok(Self::WWWSearch),
                0x66 => Ok(Self::WWWFavorites),
                0x67 => Ok(Self::WWWRefresh),
                0x68 => Ok(Self::WWWStop),
                0x69 => Ok(Self::WWWForward),
                0x6A => Ok(Self::WWWBack),
                0x6B => Ok(Self::MyComputer),
                0x6C => Ok(Self::Email),
                0x6D => Ok(Self::MultimediaSelect),
                _ => Err(()),
            }
        }
    }
}

#[allow(dead_code)]
mod modifier {
    pub const SHIFT: u8 = 1 << 0;
    pub const CTRL: u8 = 1 << 1;
    pub const ALT: u8 = 1 << 2;

    pub const CAPS_LOCK: u8 = 1 << 3;
    pub const NUM_LOCK: u8 = 1 << 4;
    pub const SCROLL_LOCK: u8 = 1 << 5;
    pub const EXTENDED: u8 = 1 << 6;
}

#[allow(dead_code)]
const fn is_modifier(key: u8) -> bool {
    key == modifier::SHIFT || key == modifier::CTRL || key == modifier::ALT
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

#[derive(Debug, Clone, Copy)]
pub struct Key {
    pub virtual_char: Option<u8>,
    pub key_type: KeyType,
}

// A mini keyboard driver/mapper
pub struct Keyboard {
    active_modifiers: u8,
    active_toggles: u8,
    // use a small buffer, there is no need to have long buffer,
    // its bad, since user input may be saved for later
    input_ring: RingBuffer<Key, 8>,
}

impl fmt::Debug for Keyboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Keyboard").finish()
    }
}

impl Keyboard {
    pub fn empty() -> Self {
        Self {
            active_modifiers: 0,
            active_toggles: 0,
            input_ring: RingBuffer::empty(),
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
        let modifiers_only = self.active_modifiers
            & !(modifier::CAPS_LOCK | modifier::NUM_LOCK | modifier::SCROLL_LOCK);

        modifiers_only | self.active_toggles
    }

    fn try_read_char(&mut self) -> Option<Key> {
        if self.read_status() & status::DATA_READY == 0 {
            return None;
        }

        let data = self.read_data();

        if data == 0xE0 {
            // this is an extended key
            let data = self.read_data();
            let key = (data | 0x80).try_into().ok()?;

            return Some(Key {
                virtual_char: None,
                key_type: key,
            });
        }

        let pressed = data & KEY_PRESSED == 0;
        let data = data & !KEY_PRESSED; // strip the pressed bit
        if let Some(modifier_key) = get_modifier(data) {
            if pressed {
                self.active_modifiers |= modifier_key;
            } else {
                self.active_modifiers &= !modifier_key;
            }
        } else if let Some(toggle_key) = get_toggle(data) {
            // keep a copy in the modifier so that we only toggle on a press
            let should_toggle = pressed && self.active_modifiers & toggle_key == 0;
            if should_toggle {
                self.active_toggles ^= toggle_key;
            }

            // add to the modifier
            if pressed {
                self.active_modifiers |= toggle_key;
            } else {
                self.active_modifiers &= !toggle_key;
            }
        } else {
            // this is a normal key
            if pressed {
                let key_type: KeyType = data.try_into().ok()?;
                let virtual_char = key_type.virtual_key(self.modifiers() & modifier::SHIFT != 0);

                return Some(Key {
                    virtual_char,
                    key_type,
                });
            }
        }
        None
    }

    pub fn get_next_char(&mut self) -> Option<Key> {
        self.input_ring.pop().or_else(|| self.try_read_char())
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame64) {
    let mut keyboard = KEYBOARD.get().lock();
    // fill in the buffer, and replace if filled
    if let Some(key) = keyboard.try_read_char() {
        keyboard.input_ring.push_replace(key);
    }

    apic::return_from_interrupt();
}
