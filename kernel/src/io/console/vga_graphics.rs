use embedded_graphics::{
    geometry::Point,
    mono_font::{
        ascii::{FONT_9X15, FONT_9X15_BOLD},
        MonoTextStyle,
    },
    pixelcolor::{Rgb888, RgbColor},
    text::{
        renderer::{CharacterStyle, TextRenderer},
        Baseline,
    },
};

use crate::graphics::{self, vga, Pixel};

use super::{VideoConsole, VideoConsoleAttribute};

pub(super) struct VgaGraphics {
    pos: Point,
    text_style: MonoTextStyle<'static, Rgb888>,
    vga: &'static vga::VgaDisplayController,
}

impl VgaGraphics {
    pub fn new() -> Self {
        Self {
            pos: Point::new(0, 0),
            text_style: MonoTextStyle::new(&FONT_9X15, Rgb888::WHITE),
            vga: graphics::vga::controller().expect("We should have a VGA controller by now!"),
        }
    }

    pub fn clear_current_text_line(&mut self, vga: &mut vga::VgaDisplay) {
        let fb_info = self.vga.framebuffer_info();
        vga.clear_rect(
            0,
            self.pos.y as usize,
            fb_info.width,
            self.text_style.line_height() as usize,
            Pixel { r: 0, g: 0, b: 0 },
        );
    }

    fn scroll(&mut self, lines: usize, vga: &mut vga::VgaDisplay) {
        let fb_info = self.vga.framebuffer_info();
        let height = fb_info.height;
        let width = fb_info.width;
        let mut i = 0;
        while i < height - lines * 2 {
            vga.blit_inner_ranges((0, i + lines), (0, i), width, lines);

            i += lines;
        }
        self.pos.y -= lines as i32;
    }

    fn fix_after_advance(&mut self) {
        let fb_info = self.vga.framebuffer_info();
        let width = fb_info.width as i32;
        let height = fb_info.height as i32;
        if self.pos.x >= width {
            self.pos.x = 0;
            self.pos.y += self.text_style.line_height() as i32;
        }
        if self.pos.y + self.text_style.line_height() as i32 >= height {
            if let Some(mut vga) = self.vga.lock_kernel() {
                // scroll up
                self.scroll(self.text_style.line_height() as usize, &mut vga);

                // clear line
                self.clear_current_text_line(&mut vga);
            }

            // just to make sure we are not out of bounds by more than 1 line
            self.fix_after_advance();
        }
    }
}

impl VideoConsole for VgaGraphics {
    fn init(&mut self) {
        if let Some(mut vga) = self.vga.lock_kernel() {
            vga.clear();
        }
    }

    fn write_byte(&mut self, c: u8) {
        if c == b'\n' {
            self.pos = Point::new(0, self.pos.y + self.text_style.line_height() as i32);
        } else {
            let mut dst = [0; 4];
            let str = (c as char).encode_utf8(&mut dst);

            let style = self.text_style;

            if let Some(mut vga) = self.vga.lock_kernel() {
                self.pos = style
                    .draw_string(str, self.pos, Baseline::Bottom, &mut *vga)
                    .unwrap();
            } else {
                // nothing changed, so no need to check for scroll
                return;
            }
        }
        self.fix_after_advance();
    }

    fn set_attrib(&mut self, attrib: VideoConsoleAttribute) {
        // These colors are used in PowerShell 6 in Windows 10
        // except for black, changed to all zeros
        let to_color = |color: u8| match color {
            0 => Rgb888::new(0, 0, 0),
            1 => Rgb888::new(197, 15, 31),
            2 => Rgb888::new(19, 161, 14),
            3 => Rgb888::new(193, 156, 0),
            4 => Rgb888::new(0, 55, 218),
            5 => Rgb888::new(136, 23, 152),
            6 => Rgb888::new(58, 150, 221),
            7 => Rgb888::new(204, 204, 204),

            8 => Rgb888::new(118, 118, 118),
            9 => Rgb888::new(231, 72, 86),
            10 => Rgb888::new(22, 198, 12),
            11 => Rgb888::new(249, 241, 165),
            12 => Rgb888::new(59, 120, 255),
            13 => Rgb888::new(180, 0, 158),
            14 => Rgb888::new(97, 214, 214),
            _ => Rgb888::new(242, 242, 242),
        };

        self.text_style
            .set_background_color(Some(to_color(attrib.background as u8)));
        self.text_style
            .set_text_color(Some(to_color(attrib.foreground as u8)));

        if attrib.bold {
            self.text_style.font = &FONT_9X15_BOLD;
        } else {
            self.text_style.font = &FONT_9X15;
        }
    }
}
