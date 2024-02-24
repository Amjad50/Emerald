pub const MOUSE_PATH: &str = "/devices/mouse";

pub mod buttons {
    pub const LEFT: u8 = 1 << 0;
    pub const RIGHT: u8 = 1 << 1;
    pub const MIDDLE: u8 = 1 << 2;
    pub const FOURTH: u8 = 1 << 3;
    pub const FIFTH: u8 = 1 << 4;
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ScrollType {
    None = 0,
    VerticalUp,
    VerticalDown,
    HorizontalRight,
    HorizontalNegative,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    pub x: i16,
    pub y: i16,
    pub scroll_type: ScrollType,
    pub buttons: u8,
}

impl MouseEvent {
    pub const BYTES_SIZE: usize = 5;

    /// # Safety
    /// The `bytes` must be a valid representation of a `MouseEvent`
    /// that has been created by `as_bytes`
    pub unsafe fn from_bytes(bytes: [u8; Self::BYTES_SIZE]) -> Self {
        let x = i16::from_le_bytes([bytes[0], bytes[1]]);
        let y = i16::from_le_bytes([bytes[2], bytes[3]]);
        let buttons = bytes[4] & 0b11111;
        let scroll_type = match bytes[4] >> 5 {
            0 => ScrollType::None,
            1 => ScrollType::VerticalUp,
            2 => ScrollType::VerticalDown,
            3 => ScrollType::HorizontalRight,
            4 => ScrollType::HorizontalNegative,
            _ => panic!("invalid scroll type"),
        };

        Self {
            x,
            y,
            buttons,
            scroll_type,
        }
    }

    pub fn as_bytes(&self) -> [u8; Self::BYTES_SIZE] {
        let mut bytes = [0; Self::BYTES_SIZE];

        // bytes[0..4] = x, y
        let x_bytes = self.x.to_le_bytes();
        let y_bytes = self.y.to_le_bytes();
        bytes[0..2].copy_from_slice(&x_bytes);
        bytes[2..4].copy_from_slice(&y_bytes);

        // merge the buttons and scroll type
        // low 5 bits are the buttons
        // high 3 bits are the scroll type
        let scroll_type = self.scroll_type as u8;
        bytes[4] = self.buttons & 0b11111 | (scroll_type << 5);

        bytes
    }
}
