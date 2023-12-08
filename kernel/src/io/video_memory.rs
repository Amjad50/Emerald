//! A temporary tool to allow for easy printing to the screen.
//! We are using the VGA text mode buffer to print to the screen.
//! Which is in the memory address 0xb8000.

use crate::memory_management::memory_layout::physical2virtual;

const VGA_BUFFER_ADDR: *mut u8 = physical2virtual(0xb8000) as *mut u8;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

/// White on black text
pub(super) const DEFAULT_ATTRIB: u8 = 0x0f;

fn get_index(pos: (usize, usize)) -> isize {
    (pos.0 + pos.1 * VGA_WIDTH) as isize
}

pub(super) struct VgaBuffer {
    pos: (usize, usize),
}

impl VgaBuffer {
    pub const fn new() -> Self {
        Self { pos: (0, 0) }
    }

    pub fn init(&mut self) {
        self.clear();
    }

    fn fix_after_advance(&mut self) {
        if self.pos.0 >= VGA_WIDTH {
            self.pos.0 = 0;
            self.pos.1 += 1;
        }
        if self.pos.1 >= VGA_HEIGHT {
            // scroll up
            let start_arr_index = get_index((0, 1)) * 2;
            let end_arr_end = get_index((VGA_WIDTH, VGA_HEIGHT)) * 2;
            unsafe {
                core::ptr::copy(
                    VGA_BUFFER_ADDR.offset(start_arr_index),
                    VGA_BUFFER_ADDR,
                    (end_arr_end - start_arr_index) as usize,
                );
            }
            self.pos.1 -= 1;
            self.clear_line(self.pos.1);
            // just to make sure we are not out of bounds by more than 1 line
            self.fix_after_advance();
        }
    }

    pub fn write_byte(&mut self, c: u8, attrib: u8) {
        if c == b'\n' {
            self.pos.0 = 0;
            self.pos.1 += 1;
            self.fix_after_advance();
            return;
        }
        let i = get_index(self.pos);
        unsafe {
            *VGA_BUFFER_ADDR.offset(i * 2) = c;
            *VGA_BUFFER_ADDR.offset(i * 2 + 1) = attrib;
        }
        self.pos.0 += 1;
        self.fix_after_advance();
    }

    fn clear(&mut self) {
        for i in 0..VGA_HEIGHT {
            self.clear_line(i);
        }
        self.pos = (0, 0);
    }

    fn clear_line(&mut self, line: usize) {
        for i in 0..VGA_WIDTH {
            let pos = get_index((i, line));
            unsafe {
                *VGA_BUFFER_ADDR.offset(pos * 2) = b' ';
                *VGA_BUFFER_ADDR.offset(pos * 2 + 1) = 0x0;
            }
        }
    }
}
