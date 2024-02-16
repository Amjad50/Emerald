use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point},
    mono_font::{
        ascii::{FONT_9X15, FONT_9X15_BOLD},
        MonoTextStyle,
    },
    pixelcolor::{Rgb888, RgbColor},
    text::{
        renderer::{CharacterStyle, TextRenderer},
        Baseline,
    },
    Pixel,
};

use crate::{memory_management::virtual_space::VirtualSpace, multiboot2};

use super::{VideoConsole, VideoConsoleAttribute};

pub(super) struct VgaGraphics {
    pitch: usize,
    height: usize,
    width: usize,
    field_pos: (u8, u8, u8),
    mask: (u8, u8, u8),
    byte_per_pixel: u8,
    memory: VirtualSpace<[u8]>,

    pos: Point,
    text_style: MonoTextStyle<'static, Rgb888>,
}

impl VgaGraphics {
    pub fn new(framebuffer: multiboot2::Framebuffer) -> Self {
        let multiboot2::FramebufferColorInfo::Rgb {
            red_field_position,
            red_mask_size,
            green_field_position,
            green_mask_size,
            blue_field_position,
            blue_mask_size,
        } = framebuffer.color_info
        else {
            panic!("Only RGB framebuffer is supported");
        };

        let physical_addr = framebuffer.addr;
        let memory_size = framebuffer.pitch * framebuffer.height;
        let memory =
            unsafe { VirtualSpace::new_slice(physical_addr, memory_size as usize).unwrap() };

        let red_mask = (1 << red_mask_size) - 1;
        let green_mask = (1 << green_mask_size) - 1;
        let blue_mask = (1 << blue_mask_size) - 1;
        Self {
            pitch: framebuffer.pitch as usize,
            height: framebuffer.height as usize,
            width: framebuffer.width as usize,
            field_pos: (
                red_field_position as u8 / 8,
                green_field_position as u8 / 8,
                blue_field_position as u8 / 8,
            ),
            mask: (red_mask as u8, green_mask as u8, blue_mask as u8),
            byte_per_pixel: (framebuffer.bpp + 7) / 8,
            memory,
            pos: Point::new(0, 0),
            text_style: MonoTextStyle::new(&FONT_9X15, Rgb888::WHITE),
        }
    }

    fn get_arr_pos(&self, pos: (usize, usize)) -> usize {
        pos.0 * self.byte_per_pixel as usize + pos.1 * self.pitch
    }

    pub fn put_pixel(&mut self, x: usize, y: usize, color: Rgb888) {
        let i = self.get_arr_pos((x, y));
        let pixel_mem = &mut self.memory[i..i + self.byte_per_pixel as usize];

        let r = color.r() & self.mask.0;
        let g = color.g() & self.mask.1;
        let b = color.b() & self.mask.2;
        pixel_mem[self.field_pos.0 as usize] = r;
        pixel_mem[self.field_pos.1 as usize] = g;
        pixel_mem[self.field_pos.2 as usize] = b;
    }

    pub fn clear(&mut self) {
        self.memory.fill(0);
    }

    pub fn clear_current_text_line(&mut self) {
        let start = self.get_arr_pos((0, self.pos.y as usize));
        let line_chunk_size = self.pitch * self.text_style.line_height() as usize;
        self.memory[start..start + line_chunk_size].fill(0);
    }

    fn scroll(&mut self, lines: usize) {
        let chunk_size = self.pitch * lines;

        let mut i = 0;
        while i < self.height - lines * 2 {
            // copy chunks of height (lines) each time
            let copy_to_start = self.get_arr_pos((0, i));
            let copy_from_start = self.get_arr_pos((0, i + lines));
            let (before, after) = self.memory.split_at_mut(copy_from_start);

            before[copy_to_start..copy_to_start + chunk_size].copy_from_slice(&after[..chunk_size]);
            i += lines;
        }
        self.pos.y -= lines as i32;
    }

    fn fix_after_advance(&mut self) {
        if self.pos.x >= self.width as i32 {
            self.pos.x = 0;
            self.pos.y += self.text_style.line_height() as i32;
        }
        if self.pos.y + self.text_style.line_height() as i32 >= self.height as i32 {
            // scroll up
            self.scroll(self.text_style.line_height() as usize);

            // clear line
            self.clear_current_text_line();
            // just to make sure we are not out of bounds by more than 1 line
            self.fix_after_advance();
        }
    }
}

impl DrawTarget for VgaGraphics {
    type Color = Rgb888;

    type Error = ();

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::prelude::Pixel<Self::Color>>,
    {
        for Pixel(pos, color) in pixels {
            self.put_pixel(pos.x as usize, pos.y as usize, color);
        }
        Ok(())
    }
}

impl OriginDimensions for VgaGraphics {
    fn size(&self) -> embedded_graphics::geometry::Size {
        embedded_graphics::geometry::Size::new(self.width as u32, self.height as u32)
    }
}

impl VideoConsole for VgaGraphics {
    fn init(&mut self) {
        self.clear();
    }

    fn write_byte(&mut self, c: u8) {
        if c == b'\n' {
            self.pos = Point::new(0, self.pos.y + self.text_style.line_height() as i32);
        } else {
            let mut dst = [0; 4];
            let str = (c as char).encode_utf8(&mut dst);

            let style = self.text_style;

            self.pos = style
                .draw_string(str, self.pos, Baseline::Bottom, self)
                .unwrap();
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
