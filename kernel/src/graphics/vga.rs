use core::sync::atomic::{AtomicI64, Ordering};

use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{self, OriginDimensions},
    pixelcolor::Rgb888,
};

use crate::{
    memory_management::virtual_space::VirtualSpace,
    multiboot2::{self, FramebufferColorInfo},
    sync::{
        once::OnceLock,
        spin::mutex::{Mutex, MutexGuard},
    },
};

use super::Pixel;

static VGA_DISPLAY_CONTROLLER: OnceLock<VgaDisplayController> = OnceLock::new();

pub fn init(framebuffer: Option<multiboot2::Framebuffer>) {
    if VGA_DISPLAY_CONTROLLER.try_get().is_some() {
        panic!("VGA display controller already initialized");
    }

    match framebuffer {
        Some(framebuffer) => match framebuffer.color_info {
            FramebufferColorInfo::Indexed { .. } => {}
            FramebufferColorInfo::Rgb { .. } => {
                // only initialize if the framebuffer is RGB
                VGA_DISPLAY_CONTROLLER.get_or_init(|| VgaDisplayController::new(framebuffer));
            }
            FramebufferColorInfo::EgaText => {}
        },
        None => panic!("No framebuffer provided"),
    }
}

pub fn controller() -> Option<&'static VgaDisplayController> {
    VGA_DISPLAY_CONTROLLER.try_get()
}

pub struct VgaDisplayController {
    display: Mutex<VgaDisplay>,
    framebuffer_info: FrameBufferInfo,
    owner_process: AtomicI64,
}

#[allow(dead_code)]
impl VgaDisplayController {
    pub fn new(framebuffer: multiboot2::Framebuffer) -> Self {
        let display = VgaDisplay::new(framebuffer);

        Self {
            framebuffer_info: display.fb_info,
            display: Mutex::new(display),
            owner_process: AtomicI64::new(-1),
        }
    }

    pub fn lock_process(&self, pid: u64) -> Option<MutexGuard<VgaDisplay>> {
        if self.owner_process.load(Ordering::Relaxed) == pid as i64 {
            Some(self.display.lock())
        } else {
            None
        }
    }

    pub fn lock_kernel(&self) -> Option<MutexGuard<VgaDisplay>> {
        if self.owner_process.load(Ordering::Relaxed) == -1 {
            Some(self.display.lock())
        } else {
            None
        }
    }

    pub fn take_ownership(&self, pid: u64) -> bool {
        assert!(pid < i64::MAX as u64);
        self.owner_process
            .compare_exchange(
                -1,
                pid as i64,
                core::sync::atomic::Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    pub fn release(&self, pid: u64) -> bool {
        assert!(pid < i64::MAX as u64);
        self.owner_process
            .compare_exchange(
                pid as i64,
                -1,
                core::sync::atomic::Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    pub fn framebuffer_info(&self) -> &FrameBufferInfo {
        &self.framebuffer_info
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrameBufferInfo {
    pub pitch: usize,
    pub height: usize,
    pub width: usize,
    pub field_pos: (u8, u8, u8),
    pub mask: (u8, u8, u8),
    pub byte_per_pixel: u8,
}

impl FrameBufferInfo {
    fn get_arr_pos(&self, pos: (usize, usize)) -> usize {
        pos.0 * self.byte_per_pixel as usize + pos.1 * self.pitch
    }

    fn read_pixel(&self, memory: &[u8], pos: (usize, usize)) -> Pixel {
        let i = self.get_arr_pos(pos);
        let pixel_mem = &memory[i..i + self.byte_per_pixel as usize];
        Pixel {
            r: pixel_mem[self.field_pos.0 as usize],
            g: pixel_mem[self.field_pos.1 as usize],
            b: pixel_mem[self.field_pos.2 as usize],
        }
    }

    fn write_pixel(&self, memory: &mut [u8], pos: (usize, usize), color: Pixel) {
        let i = self.get_arr_pos(pos);
        let pixel_mem = &mut memory[i..i + self.byte_per_pixel as usize];

        let r = color.r & self.mask.0;
        let g = color.g & self.mask.1;
        let b = color.b & self.mask.2;
        pixel_mem[self.field_pos.0 as usize] = r;
        pixel_mem[self.field_pos.1 as usize] = g;
        pixel_mem[self.field_pos.2 as usize] = b;
    }
}

pub struct VgaDisplay {
    fb_info: FrameBufferInfo,
    memory: VirtualSpace<[u8]>,
}

#[allow(dead_code)]
impl VgaDisplay {
    fn new(framebuffer: multiboot2::Framebuffer) -> Self {
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
        assert!(
            framebuffer.bpp % 8 == 0,
            "Only byte aligned bpp is supported"
        );
        assert!(
            red_field_position % 8 == 0
                && green_field_position % 8 == 0
                && blue_field_position % 8 == 0,
            "Only byte aligned field position is supported"
        );

        let physical_addr = framebuffer.addr;
        let memory_size = framebuffer.pitch * framebuffer.height;
        let memory =
            unsafe { VirtualSpace::new_slice(physical_addr, memory_size as usize).unwrap() };

        let red_mask = (1 << red_mask_size) - 1;
        let green_mask = (1 << green_mask_size) - 1;
        let blue_mask = (1 << blue_mask_size) - 1;
        Self {
            fb_info: FrameBufferInfo {
                pitch: framebuffer.pitch as usize,
                height: framebuffer.height as usize,
                width: framebuffer.width as usize,
                field_pos: (
                    red_field_position / 8,
                    green_field_position / 8,
                    blue_field_position / 8,
                ),
                mask: (red_mask as u8, green_mask as u8, blue_mask as u8),
                byte_per_pixel: (framebuffer.bpp + 7) / 8,
            },
            memory,
        }
    }

    pub fn put_pixel(&mut self, x: usize, y: usize, color: Pixel) {
        self.fb_info.write_pixel(&mut self.memory, (x, y), color);
    }

    pub fn clear(&mut self) {
        self.memory.fill(0);
    }

    pub fn blit_inner_ranges(
        &mut self,
        src: (usize, usize),
        dest: (usize, usize),
        width: usize,
        height: usize,
    ) {
        let (src_x, src_y) = src;
        let (dest_x, dest_y) = dest;
        assert!(src_x + width <= self.fb_info.width);
        assert!(src_y + height <= self.fb_info.height);
        assert!(dest_x + width <= self.fb_info.width);
        assert!(dest_y + height <= self.fb_info.height);

        // assert no overlap
        assert!(
            (src_x + width <= dest_x
                || dest_x + width <= src_x
                || src_y + height <= dest_y
                || dest_y + height <= src_y)
        );

        let chunk_size = width * self.fb_info.byte_per_pixel as usize;

        // Some optimization to avoid checking the condition for each pixel
        // we create a closure that will be called for each line
        let is_src_after =
            src_y * self.fb_info.pitch + src_x > dest_y * self.fb_info.pitch + dest_x;
        let mut copy_handler_src_after;
        let mut copy_handler_src_before;
        let copy_handler: &mut dyn FnMut(usize, usize, &mut [u8]) = if is_src_after {
            copy_handler_src_after = |src_i: usize, dest_i: usize, memory: &mut [u8]| {
                let (before, after) = memory.split_at_mut(src_i);
                before[dest_i..dest_i + chunk_size].copy_from_slice(&after[..chunk_size]);
            };
            &mut copy_handler_src_after
        } else {
            copy_handler_src_before = |src_i: usize, dest_i: usize, memory: &mut [u8]| {
                let (before, after) = memory.split_at_mut(src_i);
                before[dest_i..dest_i + chunk_size].copy_from_slice(&after[..chunk_size]);
            };
            &mut copy_handler_src_before
        };

        for y in 0..height {
            let src_i = self.fb_info.get_arr_pos((src_x, src_y + y));
            let dest_i = self.fb_info.get_arr_pos((dest_x, dest_y + y));

            copy_handler(src_i, dest_i, &mut self.memory);
        }
    }

    pub fn blit(
        &mut self,
        src_buffer: &[u8],
        src_framebuffer_info: &FrameBufferInfo,
        src: (usize, usize),
        dest: (usize, usize),
        width: usize,
        height: usize,
    ) {
        if self.fb_info.byte_per_pixel == src_framebuffer_info.byte_per_pixel
            && self.fb_info.field_pos == src_framebuffer_info.field_pos
            && self.fb_info.mask == src_framebuffer_info.mask
        {
            // Safety: we know this can work because we have the same format
            unsafe { self.blit_fast(src_buffer, src_framebuffer_info, src, dest, width, height) }
        } else {
            self.blit_slow(src_buffer, src_framebuffer_info, src, dest, width, height)
        }
    }

    /// blit the src framebuffer to the current framebuffer
    /// `fast` here means that we assume the src and dest have the same format
    unsafe fn blit_fast(
        &mut self,
        src_buffer: &[u8],
        src_framebuffer_info: &FrameBufferInfo,
        src: (usize, usize),
        dest: (usize, usize),
        width: usize,
        height: usize,
    ) {
        let (src_x, src_y) = src;
        let (dest_x, dest_y) = dest;
        assert!(src_x + width <= src_framebuffer_info.width);
        assert!(src_y + height <= src_framebuffer_info.height);
        assert!(dest_x + width <= self.fb_info.width);
        assert!(dest_y + height <= self.fb_info.height);

        // assume same chunk size
        let chunk_size = width * self.fb_info.byte_per_pixel as usize;

        for y in 0..height {
            let src_i = src_framebuffer_info.get_arr_pos((src_x, src_y + y));
            let dest_i = self.fb_info.get_arr_pos((dest_x, dest_y + y));

            let src_line = &src_buffer[src_i..src_i + chunk_size];
            let dest_line = &mut self.memory[dest_i..dest_i + chunk_size];
            dest_line.copy_from_slice(src_line);
        }
    }

    fn blit_slow(
        &mut self,
        src_buffer: &[u8],
        src_framebuffer_info: &FrameBufferInfo,
        src: (usize, usize),
        dest: (usize, usize),
        width: usize,
        height: usize,
    ) {
        let (src_x, src_y) = src;
        let (dest_x, dest_y) = dest;

        assert!(dest_x + width <= self.fb_info.width);
        assert!(dest_y + height <= self.fb_info.height);
        assert!(src_x + width <= src_framebuffer_info.width);
        assert!(src_y + height <= src_framebuffer_info.height);

        for y in 0..height {
            for x in 0..width {
                let src_pixel = src_framebuffer_info.read_pixel(src_buffer, (src_x + x, src_y + y));
                self.fb_info
                    .write_pixel(&mut self.memory, (dest_x + x, dest_y + y), src_pixel);
            }
        }
    }

    pub fn clear_rect(
        &mut self,
        dest_x: usize,
        dest_y: usize,
        width: usize,
        height: usize,
        color: Pixel,
    ) {
        assert!(dest_x + width <= self.fb_info.width);
        assert!(dest_y + height <= self.fb_info.height);

        if height == 0 || width == 0 {
            return;
        }

        let line_chunk_size = width * self.fb_info.byte_per_pixel as usize;
        let first_line_start = self.fb_info.get_arr_pos((dest_x, dest_y));
        let first_line_end = first_line_start + line_chunk_size;
        let first_line = &mut self.memory[first_line_start..first_line_end];

        // fill the first line
        for i in 0..width {
            self.fb_info.write_pixel(first_line, (i, 0), color);
        }

        // take from the end of the first line, i.e. `before` will have the first line
        // and `after` will have the rest of the memory
        let second_line_start = self.fb_info.get_arr_pos((0, dest_y + 1));
        let (before, after) = self.memory.split_at_mut(second_line_start);
        let first_line = &before[first_line_start..first_line_end];

        for y in 1..height {
            let dest_i = self.fb_info.get_arr_pos((dest_x, y - 1));
            let dest_line = &mut after[dest_i..dest_i + line_chunk_size];
            dest_line.copy_from_slice(first_line);
        }
    }
}

impl DrawTarget for VgaDisplay {
    type Color = Rgb888;

    type Error = ();

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::prelude::Pixel<Self::Color>>,
    {
        for embedded_graphics::prelude::Pixel(pos, color) in pixels {
            self.put_pixel(pos.x as usize, pos.y as usize, color.into());
        }
        Ok(())
    }

    fn fill_solid(
        &mut self,
        area: &embedded_graphics::primitives::Rectangle,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
        let (x, y) = (area.top_left.x as usize, area.top_left.y as usize);
        let (width, height) = (area.size.width as usize, area.size.height as usize);
        self.clear_rect(x, y, width, height, color.into());

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.clear_rect(0, 0, self.fb_info.width, self.fb_info.height, color.into());
        Ok(())
    }
}

impl OriginDimensions for VgaDisplay {
    fn size(&self) -> geometry::Size {
        geometry::Size::new(self.fb_info.width as u32, self.fb_info.height as u32)
    }
}
