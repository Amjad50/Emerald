use core::{cell::RefCell, fmt::Write};

use crate::sync::spin::remutex::ReMutex;

use super::{
    uart::{Uart, UartPort},
    video_memory::{VgaBuffer, DEFAULT_ATTRIB},
};

// SAFETY: the console is only used inside a lock or mutex
pub(super) static CONSOLE: ReMutex<RefCell<Console>> =
    ReMutex::new(RefCell::new(unsafe { Console::empty() }));

pub fn init() {
    CONSOLE.lock().borrow_mut().init();
}

pub(super) struct Console {
    uart: Uart,
    video_buffer: VgaBuffer,
}

impl Console {
    /// SAFETY: the console must be used inside a lock or mutex
    pub const unsafe fn empty() -> Self {
        Self {
            uart: Uart::new(UartPort::COM1),
            video_buffer: VgaBuffer::new(),
        }
    }

    pub fn init(&mut self) {
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
        let mut i = 0;
        for &c in src {
            i += 1;
            unsafe {
                self.write_byte(c);
            }
        }
        i
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write(s.as_bytes());
        Ok(())
    }
}
