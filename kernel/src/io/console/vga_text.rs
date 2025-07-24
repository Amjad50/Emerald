//! A temporary tool to allow for easy printing to the screen.
//! We are using the VGA text mode buffer to print to the screen.

use crate::{memory_management::virtual_space::VirtualSpace, multiboot2};

use super::{VideoConsole, VideoConsoleAttribute};

/// White on black text
const DEFAULT_ATTRIB: u8 = 0x0f;

pub(super) struct VgaText {
    pos: (usize, usize),
    attrib: u8,
    pitch: usize,
    height: usize,
    width: usize,
    memory: VirtualSpace<[u8]>,
}

impl VgaText {
    pub fn new(framebuffer: multiboot2::Framebuffer) -> Self {
        assert!(matches!(
            framebuffer.color_info,
            multiboot2::FramebufferColorInfo::EgaText
        ));
        let physical_addr = framebuffer.addr;
        let memory_size = framebuffer.pitch * framebuffer.height;
        let memory =
            unsafe { VirtualSpace::new_slice(physical_addr, memory_size as usize).unwrap() };

        Self {
            pos: (0, 0),
            attrib: DEFAULT_ATTRIB,
            pitch: framebuffer.pitch as usize,
            height: framebuffer.height as usize,
            width: framebuffer.width as usize,
            memory,
        }
    }

    fn get_arr_pos(&self, pos: (usize, usize)) -> usize {
        pos.0 * 2 + pos.1 * self.pitch
    }

    fn fix_after_advance(&mut self) {
        if self.pos.0 >= self.width {
            self.pos.0 = 0;
            self.pos.1 += 1;
        }
        if self.pos.1 >= self.height {
            // scroll up
            for i in 0..self.height - 1 {
                let copy_to = self.get_arr_pos((0, i));
                let copy_from = self.get_arr_pos((0, i + 1));
                let size = self.pitch;
                let (before, after) = self.memory.split_at_mut(copy_from);

                before[copy_to..copy_to + size].copy_from_slice(&after[..size]);
            }

            self.pos.1 -= 1;
            self.clear_line(self.pos.1);
            // just to make sure we are not out of bounds by more than 1 line
            self.fix_after_advance();
        }
    }

    #[allow(dead_code)]
    fn clear(&mut self) {
        for i in 0..self.height {
            self.clear_line(i);
        }
        self.pos = (0, 0);
    }

    fn clear_line(&mut self, line: usize) {
        for i in 0..self.width {
            let pos = self.get_arr_pos((i, line));

            self.memory[pos] = b' ';
            self.memory[pos + 1] = 0x0;
        }
    }
}

impl VideoConsole for VgaText {
    fn init(&mut self) {
        self.clear();
    }

    fn set_attrib(&mut self, attrib: VideoConsoleAttribute) {
        let to_vga_color = |color: u8| {
            let mappings = &[
                0,  // black
                4,  // red
                2,  // green
                6,  // brown
                1,  // blue
                5,  // magenta
                3,  // cyan
                7,  // light gray
                8,  // dark gray
                12, // light red
                10, // light green
                14, // yellow
                9,  // light blue
                13, // light magenta
                11, // light cyan
                15, // white
            ];
            mappings[color as usize]
        };

        let mut fg_index = attrib.foreground as u8;
        if attrib.faint && fg_index >= 8 {
            fg_index -= 8;
        }
        if attrib.bold && fg_index < 8 {
            fg_index += 8;
        }

        let fg = to_vga_color(fg_index);
        let bg = to_vga_color(attrib.background as u8);
        self.attrib = (bg << 4) | fg;
    }

    fn write_byte(&mut self, c: u8) {
        if c == b'\n' {
            self.pos.0 = 0;
            self.pos.1 += 1;
            self.fix_after_advance();
            return;
        }
        let i = self.get_arr_pos(self.pos);
        self.memory[i] = c;
        self.memory[i + 1] = self.attrib;
        self.pos.0 += 1;
        self.fix_after_advance();
    }

    fn backspace(&mut self) {
        if self.pos.0 == 0 {
            if self.pos.1 == 0 {
                return;
            }
            self.pos.0 = self.width - 1;
            self.pos.1 -= 1;
        } else {
            self.pos.0 -= 1;
        }
        let i = self.get_arr_pos(self.pos);
        self.memory[i] = b' ';
        self.memory[i + 1] = self.attrib;
    }
}
