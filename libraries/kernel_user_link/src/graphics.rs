#[repr(u64)]
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum GraphicsCommand {
    /// Take ownership of the graphics device
    /// No arguments
    TakeOwnership,
    /// Release ownership of the graphics device
    /// No arguments
    ReleaseOwnership,
    /// Get information about the framebuffer
    /// &mut FrameBufferInfo
    GetFrameBufferInfo,
    /// Blit a region from userspace memory into the graphics framebuffer
    /// (must have ownership of the graphics device)
    /// &BlitCommand
    Blit,
}

impl GraphicsCommand {
    pub fn from_u64(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::TakeOwnership),
            1 => Some(Self::ReleaseOwnership),
            2 => Some(Self::GetFrameBufferInfo),
            3 => Some(Self::Blit),
            _ => None,
        }
    }

    pub fn to_u64(self) -> u64 {
        self as u64
    }
}

#[repr(C)]
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
    /// The size of the memory buffer required to hold the framebuffer
    pub fn memory_size(&self) -> usize {
        self.pitch * self.height
    }

    /// Get the position in the memory buffer for a given pixel
    /// Returns None if the position is out of bounds
    pub fn get_arr_pos(&self, pos: (usize, usize)) -> Option<usize> {
        if pos.0 >= self.width || pos.1 >= self.height {
            return None;
        }
        Some(pos.0 * self.byte_per_pixel as usize + pos.1 * self.pitch)
    }

    /// Get the pixel slice at a given position (read-only)
    pub fn pixel_mem<'a>(&self, memory: &'a [u8], pos: (usize, usize)) -> Option<&'a [u8]> {
        let i = self.get_arr_pos(pos)?;
        Some(&memory[i..i + self.byte_per_pixel as usize])
    }

    /// Get the pixel slice at a given position
    pub fn pixel_mem_mut<'a>(
        &self,
        memory: &'a mut [u8],
        pos: (usize, usize),
    ) -> Option<&'a mut [u8]> {
        let i = self.get_arr_pos(pos)?;
        Some(&mut memory[i..i + self.byte_per_pixel as usize])
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BlitCommand {
    /// The memory buffer to blit from, this represent the whole framebuffer
    /// even if we are only blitting a part of it
    pub memory: *const u8,
    /// The framebuffer info of the source memory
    /// i.e. metadata about `memory`
    pub src_framebuffer_info: FrameBufferInfo,
    /// The position in the source framebuffer to start blitting from
    pub src: (usize, usize),
    /// The position in the destination framebuffer to start blitting to
    /// this destination is the kernel's framebuffer
    pub dst: (usize, usize),
    /// The size of the region to blit (width, height)
    pub size: (usize, usize),
}
