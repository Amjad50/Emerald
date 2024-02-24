pub const KEYBOARD_PATH: &str = "/devices/keyboard";

pub mod modifier {
    pub const SHIFT: u8 = 1 << 0;
    pub const CTRL: u8 = 1 << 1;
    pub const ALT: u8 = 1 << 2;

    pub const CAPS_LOCK: u8 = 1 << 3;
    pub const NUM_LOCK: u8 = 1 << 4;
    pub const SCROLL_LOCK: u8 = 1 << 5;
    pub const EXTENDED: u8 = 1 << 6;

    // a way to compress the `bytes` value
    pub const PRESSED: u8 = 1 << 7;
}

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

#[derive(Debug, Clone, Copy)]
pub struct Key {
    pub pressed: bool,
    pub modifiers: u8,
    pub key_type: KeyType,
}

impl Key {
    pub const BYTES_SIZE: usize = 2;

    /// # Safety
    /// The `bytes` must be a valid representation of a `Key`
    /// that has been created by `as_bytes`
    pub unsafe fn from_bytes(bytes: [u8; Self::BYTES_SIZE]) -> Self {
        let pressed = bytes[0] & modifier::PRESSED != 0;
        let modifiers = bytes[0] & !modifier::PRESSED;
        // Safety: we know that the `bytes` is a valid representation of `KeyType`
        //         responsability of the caller to ensure that
        let key_type = core::mem::transmute(bytes[1]);

        Self {
            pressed,
            modifiers,
            key_type,
        }
    }

    pub fn as_bytes(&self) -> [u8; 2] {
        let mut bytes = [0; 2];
        bytes[0] = (self.modifiers & !modifier::PRESSED)
            | if self.pressed { modifier::PRESSED } else { 0 };
        bytes[1] = self.key_type as u8;
        bytes
    }

    pub fn virtual_char(&self) -> Option<u8> {
        let shifted = self.modifiers & modifier::SHIFT != 0;
        self.key_type.virtual_key(shifted)
    }
}
