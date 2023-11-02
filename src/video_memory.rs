//! A temporary tool to allow for easy printing to the screen.
//! We are using the VGA text mode buffer to print to the screen.
//! Which is in the memory address 0xb8000.

use core::fmt::{self, Write};

// implement print! and println! macros
#[macro_export]
macro_rules! print {
    ($vga_buffer:ident, $($arg:tt)*) => {
        $crate::video_memory::_print(&mut $vga_buffer, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    ($vga_buffer:ident) => ($crate::print!($vga_buffer, "\n"));
    ($vga_buffer:ident, $($arg:tt)*) => ($crate::print!($vga_buffer, "{}\n", format_args!($($arg)*)));
}

pub struct MemSize(pub u64);

impl fmt::Display for MemSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // find the best unit
        let mut size = self.0;
        let mut unit = "B";
        if size >= 1024 {
            size /= 1024;
            unit = "KB";
        }
        if size >= 1024 {
            size /= 1024;
            unit = "MB";
        }
        if size >= 1024 {
            size /= 1024;
            unit = "GB";
        }
        if size >= 1024 {
            size /= 1024;
            unit = "TB";
        }
        size.fmt(f).and_then(|_| write!(f, "{unit}"))?;
        Ok(())
    }
}

impl fmt::Debug for MemSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

pub(crate) fn _print(vga_buffer: &mut VgaBuffer, args: core::fmt::Arguments) {
    vga_buffer.write_fmt(args).unwrap();
}

const VGA_BUFFER_ADDR: *mut u8 = 0xb8000 as *mut u8;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

/// White on black text
const DEFAULT_ATTRIB: u8 = 0x0f;

fn get_index(pos: (usize, usize)) -> isize {
    (pos.0 + pos.1 * VGA_WIDTH) as isize
}

pub struct VgaBuffer {
    pos: (usize, usize),
}

impl VgaBuffer {
    pub fn new() -> Self {
        let mut s = Self { pos: (0, 0) };
        s.clear();
        s
    }

    fn fix_after_advance(&mut self) {
        if self.pos.0 >= VGA_WIDTH {
            self.pos.0 = 0;
            self.pos.1 += 1;
        }
        if self.pos.1 >= VGA_HEIGHT {
            // scroll up
            for i in 1..VGA_HEIGHT {
                for j in 0..VGA_WIDTH {
                    let pos_from = get_index((j, i));
                    let pos_to = get_index((j, i - 1));
                    unsafe {
                        *VGA_BUFFER_ADDR.offset(pos_to * 2) = *VGA_BUFFER_ADDR.offset(pos_from * 2);
                        *VGA_BUFFER_ADDR.offset(pos_to * 2 + 1) =
                            *VGA_BUFFER_ADDR.offset(pos_from * 2 + 1);
                    }
                }
            }
            self.pos.1 -= 1;
            self.clear_line(self.pos.1);
            // just to make sure we are not out of bounds by more than 1 line
            self.fix_after_advance();
        }
    }

    pub fn out_str(&mut self, s: &str, attrib: u8) {
        for c in s.chars() {
            self.out_char(c, attrib);
        }
    }

    pub fn out_char(&mut self, c: char, attrib: u8) {
        if c == '\n' {
            self.pos.0 = 0;
            self.pos.1 += 1;
            self.fix_after_advance();
            return;
        }
        let i = get_index(self.pos);
        unsafe {
            *VGA_BUFFER_ADDR.offset(i * 2) = c as u8;
            *VGA_BUFFER_ADDR.offset(i * 2 + 1) = attrib;
        }
        self.pos.0 += 1;
        self.fix_after_advance();
    }

    pub fn clear(&mut self) {
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

impl Write for VgaBuffer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.out_str(s, DEFAULT_ATTRIB);
        Ok(())
    }
}
