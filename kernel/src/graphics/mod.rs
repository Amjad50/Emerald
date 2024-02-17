use embedded_graphics::pixelcolor::RgbColor;

pub mod vga;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl<T: RgbColor> From<T> for Pixel {
    fn from(color: T) -> Self {
        Self {
            r: color.r(),
            g: color.g(),
            b: color.b(),
        }
    }
}
