//! This is a demo of using the graphics API to draw a bouncing circle and text on the screen.

use std::thread::sleep;

use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Dimensions, OriginDimensions, Point},
    mono_font::{ascii::FONT_9X15, MonoTextStyle},
    pixelcolor::{Rgb888, RgbColor},
    primitives::{Circle, Primitive, PrimitiveStyle},
    text::{Baseline, Text},
    transform::Transform,
    Drawable,
};
use emerald_std::graphics::{BlitCommand, FrameBufferInfo};

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

struct Graphics {
    framebuffer: Box<[u8]>,
    framebuffer_info: FrameBufferInfo,
}

impl Graphics {
    pub fn new() -> Self {
        emerald_std::graphics::take_ownership().unwrap();
        let info = emerald_std::graphics::get_framebuffer_info().unwrap();
        let memory = vec![0; info.memory_size()].into_boxed_slice();

        Self {
            framebuffer: memory,
            framebuffer_info: info,
        }
    }

    fn write_pixel(&mut self, pos: (usize, usize), color: Pixel) -> Option<()> {
        let pixel_mem = self
            .framebuffer_info
            .pixel_mem_mut(&mut self.framebuffer, pos)?;
        let r = color.r & self.framebuffer_info.mask.0;
        let g = color.g & self.framebuffer_info.mask.1;
        let b = color.b & self.framebuffer_info.mask.2;
        pixel_mem[self.framebuffer_info.field_pos.0 as usize] = r;
        pixel_mem[self.framebuffer_info.field_pos.1 as usize] = g;
        pixel_mem[self.framebuffer_info.field_pos.2 as usize] = b;

        Some(())
    }

    pub fn clear_rect(
        &mut self,
        dest_x: usize,
        dest_y: usize,
        width: usize,
        height: usize,
        color: Pixel,
    ) -> Option<()> {
        if dest_x + width > self.framebuffer_info.width
            || dest_y + height > self.framebuffer_info.height
        {
            return None;
        }

        if height == 0 || width == 0 {
            return Some(());
        }

        let line_chunk_size = width * self.framebuffer_info.byte_per_pixel as usize;
        let first_line_start = self.framebuffer_info.get_arr_pos((dest_x, dest_y)).unwrap();
        let first_line_end = first_line_start + line_chunk_size;

        // fill the first line
        for i in 0..width {
            self.write_pixel((dest_x + i, dest_y), color);
        }

        if height == 1 {
            return Some(());
        }

        // take from the end of the first line, i.e. `before` will have the first line
        // and `after` will have the rest of the memory
        let second_line_start = self.framebuffer_info.get_arr_pos((0, dest_y + 1)).unwrap();
        let (before, after) = self.framebuffer.split_at_mut(second_line_start);
        let first_line = &before[first_line_start..first_line_end];

        for y in 1..height {
            let dest_i = self.framebuffer_info.get_arr_pos((dest_x, y - 1)).unwrap();
            let dest_line = &mut after[dest_i..dest_i + line_chunk_size];
            dest_line.copy_from_slice(first_line);
        }
        Some(())
    }

    pub fn present(&self) {
        emerald_std::graphics::blit(&BlitCommand {
            memory: &self.framebuffer,
            src_framebuffer_info: self.framebuffer_info,
            src: (0, 0),
            dst: (0, 0),
            size: (self.framebuffer_info.width, self.framebuffer_info.height),
        })
        .unwrap();
    }
}

impl Drop for Graphics {
    fn drop(&mut self) {
        emerald_std::graphics::release_ownership().unwrap();
    }
}

impl DrawTarget for Graphics {
    type Color = Rgb888;

    type Error = ();

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::prelude::Pixel<Self::Color>>,
    {
        for pixel in pixels {
            let pos = pixel.0;
            let color = pixel.1.into();
            self.write_pixel((pos.x as usize, pos.y as usize), color)
                .ok_or(())?;
        }
        Ok(())
    }

    fn fill_solid(
        &mut self,
        area: &embedded_graphics::primitives::Rectangle,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
        if area.top_left.x < 0
            || area.top_left.y < 0
            || area.bottom_right().unwrap().x >= self.framebuffer_info.width as i32
            || area.bottom_right().unwrap().y >= self.framebuffer_info.height as i32
        {
            return Err(());
        }

        let (x, y) = (area.top_left.x as usize, area.top_left.y as usize);
        let (width, height) = (area.size.width as usize, area.size.height as usize);
        self.clear_rect(x, y, width, height, color.into());

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.clear_rect(
            0,
            0,
            self.framebuffer_info.width,
            self.framebuffer_info.height,
            color.into(),
        );
        Ok(())
    }
}

impl OriginDimensions for Graphics {
    fn size(&self) -> embedded_graphics::geometry::Size {
        embedded_graphics::geometry::Size::new(
            self.framebuffer_info.width as u32,
            self.framebuffer_info.height as u32,
        )
    }
}

fn main() {
    let mut graphics = Graphics::new();

    let mut circle =
        Circle::new(Point::new(64, 64), 64).into_styled(PrimitiveStyle::with_fill(Rgb888::RED));
    let mut v = Point::new(10, 10);

    // Create a new character style
    let style = MonoTextStyle::new(&FONT_9X15, Rgb888::WHITE);
    let mut fps_text = "FPS: 0".to_string();

    loop {
        let time = std::time::SystemTime::now();

        graphics.clear(Rgb888::BLACK).unwrap();

        // update
        {
            // move the circle
            circle.translate_mut(v);

            // bounce the circle
            if circle.bounding_box().top_left.x < 0
                || circle.bounding_box().bottom_right().unwrap().x >= graphics.size().width as i32
            {
                v.x = -v.x;
            }
            if circle.bounding_box().top_left.y < 0
                || circle.bounding_box().bottom_right().unwrap().y >= graphics.size().height as i32
            {
                v.y = -v.y;
            }
        }
        // render
        {
            circle.draw(&mut graphics).ok();
            let text = Text::with_baseline(&fps_text, Point::new(0, 0), style, Baseline::Top);
            text.draw(&mut graphics).ok();
        }
        graphics.present();
        let remaining =
            std::time::Duration::from_millis(1000 / 60).checked_sub(time.elapsed().unwrap());
        if let Some(remaining) = remaining {
            sleep(remaining);
        }
        let fps = 1.0 / time.elapsed().unwrap().as_secs_f64();
        fps_text = format!("FPS: {:.2}", fps);
    }
}
