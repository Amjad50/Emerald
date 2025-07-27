use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Size},
    pixelcolor::{Rgb888, RgbColor},
};
use emerald_std::graphics::{BlitCommand, FrameBufferInfo};

pub struct MovingAverage<const N: usize> {
    values: [f64; N],
    current_index: usize,
    filled: usize,
    sum: f64,
}

impl<const N: usize> MovingAverage<N> {
    pub fn new() -> Self {
        Self {
            values: [0.0; N],
            current_index: 0,
            filled: 0,
            sum: 0.0,
        }
    }

    pub fn add(&mut self, value: f64) {
        self.sum -= self.values[self.current_index];
        self.sum += value;
        self.values[self.current_index] = value;
        self.current_index = (self.current_index + 1) % self.values.len();
        if self.filled < self.values.len() {
            self.filled += 1;
        }
    }

    pub fn average(&self) -> f64 {
        self.sum / self.filled as f64
    }
}

impl<const N: usize> Default for MovingAverage<N> {
    fn default() -> Self {
        Self::new()
    }
}

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

pub struct Graphics {
    framebuffer: Box<[u8]>,
    framebuffer_info: FrameBufferInfo,
    last_changed_rect: Option<(usize, usize, usize, usize)>,
}

impl Graphics {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        emerald_std::graphics::take_ownership().unwrap();
        let info = emerald_std::graphics::get_framebuffer_info().unwrap();
        let memory = vec![0; info.memory_size()].into_boxed_slice();

        Self {
            framebuffer: memory,
            framebuffer_info: info,
            last_changed_rect: Some((0, 0, info.width as usize, info.height as usize)),
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

        if let Some((x, y, w, h)) = self.last_changed_rect {
            let (min_x, min_y) = (dest_x.min(x), dest_y.min(y));
            let (max_x, max_y) = ((dest_x + width).max(x + w), (dest_y + height).max(y + h));
            self.last_changed_rect = Some((min_x, min_y, max_x - min_x, max_y - min_y));
        } else {
            self.last_changed_rect = Some((dest_x, dest_y, width, height));
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

    pub fn last_changed_rect(&self) -> Option<(usize, usize, usize, usize)> {
        self.last_changed_rect
    }

    pub fn clear_changed(&mut self) {
        self.last_changed_rect = None;
    }

    pub fn merge_clear_rect(&mut self, rect: Option<(usize, usize, usize, usize)>) {
        if let Some((x, y, w, h)) = self.last_changed_rect {
            if let Some((dest_x, dest_y, width, height)) = rect {
                let (min_x, min_y) = (dest_x.min(x), dest_y.min(y));
                let (max_x, max_y) = ((dest_x + width).max(x + w), (dest_y + height).max(y + h));
                self.last_changed_rect = Some((min_x, min_y, max_x - min_x, max_y - min_y));
            }
        } else {
            self.last_changed_rect = rect;
        }
    }

    pub fn present_changed(&mut self) {
        let Some((dest_x, dest_y, width, height)) = self.last_changed_rect else {
            return;
        };

        let changed_xy = (dest_x, dest_y);
        self.clear_changed();

        emerald_std::graphics::blit(&BlitCommand {
            memory: &self.framebuffer,
            src_framebuffer_info: self.framebuffer_info,
            src: changed_xy,
            dst: changed_xy,
            size: (width, height),
        })
        .unwrap();
    }

    // this is assumed to be rgb format
    pub fn draw_image(&mut self, img_bytes: &[u8], pos: (i32, i32), size: (usize, usize)) {
        assert_eq!(img_bytes.len(), size.0 * size.1 * 3);
        assert!(self.framebuffer_info.width as i32 >= pos.0 + size.0 as i32);
        assert!(self.framebuffer_info.height as i32 >= pos.1 + size.1 as i32);

        let pos = (pos.0 as usize, pos.1 as usize);

        for y in 0..size.1 {
            // copy one row at once
            let fb_start_i = pos.0 * self.framebuffer_info.byte_per_pixel as usize
                + (pos.1 + y) * self.framebuffer_info.pitch;
            let fb_end_i = fb_start_i + size.0 * self.framebuffer_info.byte_per_pixel as usize;
            let img_start_i = y * size.0 * 3;
            let img_end_i = img_start_i + size.0 * 3;

            // fastest
            if self.framebuffer_info.field_pos == (0, 1, 2)
                && self.framebuffer_info.byte_per_pixel == 3
                && self.framebuffer_info.mask != (0xFF, 0xFF, 0xFF)
            {
                self.framebuffer[fb_start_i..fb_end_i]
                    .copy_from_slice(&img_bytes[img_start_i..img_end_i]);
            } else {
                let r = self.framebuffer_info.field_pos.0 as usize;
                let g = self.framebuffer_info.field_pos.1 as usize;
                let b = self.framebuffer_info.field_pos.2 as usize;
                let mask_r = self.framebuffer_info.mask.0;
                let mask_g = self.framebuffer_info.mask.1;
                let mask_b = self.framebuffer_info.mask.2;

                self.framebuffer[fb_start_i..fb_end_i]
                    .chunks_mut(self.framebuffer_info.byte_per_pixel as usize)
                    .zip(img_bytes[img_start_i..img_end_i].chunks(3))
                    .for_each(|(fb_chunk, img_chunk)| {
                        fb_chunk[r] = img_chunk[0] & mask_r;
                        fb_chunk[g] = img_chunk[1] & mask_g;
                        fb_chunk[b] = img_chunk[2] & mask_b;
                    });
            }
        }

        if let Some((x, y, w, h)) = self.last_changed_rect {
            let (min_x, min_y) = (x.min(pos.0 as usize), y.min(pos.1 as usize));
            let (max_x, max_y) = (
                (x + w).max(pos.0 as usize + size.0),
                (y + h).max(pos.1 as usize + size.1),
            );
            self.last_changed_rect = Some((min_x, min_y, max_x - min_x, max_y - min_y));
        } else {
            self.last_changed_rect = Some((pos.0 as usize, pos.1 as usize, size.0, size.1));
        }
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
        let (mut min_x, mut min_y, mut max_x, mut max_y) =
            if let Some((x, y, w, h)) = self.last_changed_rect {
                let (min_x, min_y) = (x, y);
                let (max_x, max_y) = (x + w, y + h);

                (min_x, min_y, max_x, max_y)
            } else {
                (
                    self.framebuffer_info.width,
                    self.framebuffer_info.height,
                    0,
                    0,
                )
            };

        for pixel in pixels {
            let pos = pixel.0;

            if pos.x < 0
                || pos.y < 0
                || pos.x >= self.framebuffer_info.width as i32
                || pos.y >= self.framebuffer_info.height as i32
            {
                continue;
            }

            let pos_x = pos.x as usize;
            let pos_y = pos.y as usize;

            if pos_x < min_x {
                min_x = pos_x;
            }
            if pos_y < min_y {
                min_y = pos_y;
            }
            if pos_x > max_x {
                max_x = pos_x;
            }
            if pos_y > max_y {
                max_y = pos_y;
            }

            let color = pixel.1.into();
            self.write_pixel((pos.x as usize, pos.y as usize), color)
                .ok_or(())?;
        }

        self.last_changed_rect = Some((min_x, min_y, max_x - min_x, max_y - min_y));
        Ok(())
    }

    fn fill_solid(
        &mut self,
        area: &embedded_graphics::primitives::Rectangle,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
        if area.size.width == 0 || area.size.height == 0 {
            return Ok(());
        }
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
    fn size(&self) -> Size {
        Size::new(
            self.framebuffer_info.width as u32,
            self.framebuffer_info.height as u32,
        )
    }
}
