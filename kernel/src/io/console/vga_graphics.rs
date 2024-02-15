use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point},
    mono_font::{ascii::FONT_9X15, MonoTextStyle},
    pixelcolor::{Rgb888, RgbColor},
    text::{renderer::TextRenderer, Baseline},
    Pixel,
};

use crate::{memory_management::virtual_space, multiboot2};

use super::VideoConsole;

const TEXT_STYLE: MonoTextStyle<Rgb888> = MonoTextStyle::new(&FONT_9X15, Rgb888::WHITE);

pub(super) struct VgaGraphics {
    pitch: usize,
    height: usize,
    width: usize,
    field_pos: (u8, u8, u8),
    mask: (u8, u8, u8),
    byte_per_pixel: u8,
    memory: &'static mut [u8],

    pos: Point,
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
        let memory_addr =
            virtual_space::allocate_and_map_virtual_space(physical_addr, memory_size as usize)
                as *mut u8;
        let memory = unsafe { core::slice::from_raw_parts_mut(memory_addr, memory_size as usize) };

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
        let line_chunk_size = self.pitch * TEXT_STYLE.line_height() as usize;
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
            self.pos.y += TEXT_STYLE.line_height() as i32;
        }
        if self.pos.y + TEXT_STYLE.line_height() as i32 >= self.height as i32 {
            // scroll up
            self.scroll(TEXT_STYLE.line_height() as usize);

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
            self.pos = Point::new(0, self.pos.y + TEXT_STYLE.line_height() as i32);
        } else {
            let mut dst = [0; 4];
            let str = (c as char).encode_utf8(&mut dst);

            self.pos = TEXT_STYLE
                .draw_string(str, self.pos, Baseline::Bottom, self)
                .unwrap();
        }
        self.fix_after_advance();
    }

    fn set_attrib(&mut self, _attrib: u8) {
        // TODO: implement
    }

    fn get_attrib(&self) -> u8 {
        // TODO: implement
        0
    }
}
