use core::fmt::Write;

use crate::sync::spin;

use super::{
    uart::{Uart, UartPort},
    video_memory::{VgaBuffer, DEFAULT_ATTRIB},
};

// late init
pub(super) static mut CONSOLE: Console = Console::empty();

pub fn init() {
    unsafe {
        CONSOLE.init();
    }
}

pub(super) struct Console {
    lock: spin::Lock,
    uart: Uart,
    video_buffer: VgaBuffer,
}

impl Console {
    const fn empty() -> Self {
        Self {
            lock: spin::Lock::new("Console"),
            uart: Uart::new(UartPort::COM1),
            video_buffer: VgaBuffer::new(),
        }
    }

    fn init(&mut self) {
        self.uart.init();
        self.video_buffer.init();
    }

    #[allow(dead_code)]
    fn read(&mut self, _dst: &mut [u8]) -> usize {
        todo!()
    }
    /// SAFETY: the caller must assure that this is called from once place at a time
    ///         and should handle synchronization
    unsafe fn write_byte(&mut self, byte: u8) {
        self.video_buffer.write_byte(byte, DEFAULT_ATTRIB);
        unsafe { self.uart.write_byte(byte) };
    }

    fn write(&mut self, src: &[u8]) -> usize {
        self.lock.lock();
        let mut i = 0;
        for &c in src {
            i += 1;
            unsafe {
                self.write_byte(c);
            }
        }
        self.lock.unlock();
        i
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write(s.as_bytes());
        Ok(())
    }
}
