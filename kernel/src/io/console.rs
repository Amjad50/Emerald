use core::{cell::RefCell, fmt::Write};

use crate::{
    collections::ring::RingBuffer,
    cpu::{
        self,
        idt::{BasicInterruptHandler, InterruptStackFrame64},
        interrupts::apic,
    },
    sync::spin::remutex::ReMutex,
};

use super::{
    keyboard::Keyboard,
    uart::{Uart, UartPort},
    video_memory::{VgaBuffer, DEFAULT_ATTRIB},
};

// SAFETY: the console is only used inside a lock or mutex
pub(super) static CONSOLE: ReMutex<RefCell<Console>> =
    ReMutex::new(RefCell::new(unsafe { Console::empty() }));

pub fn init() {
    CONSOLE.lock().borrow_mut().init();
}

pub fn setup_interrupts() {
    CONSOLE.lock().borrow_mut().setup_interrupts();
}

pub(super) struct Console {
    uart: Uart,
    keyboard: Keyboard,
    video_buffer: VgaBuffer,
    write_ring: RingBuffer<u8>,
}

impl Console {
    /// SAFETY: the console must be used inside a lock or mutex
    pub const unsafe fn empty() -> Self {
        Self {
            uart: Uart::new(UartPort::COM1),
            video_buffer: VgaBuffer::new(),
            keyboard: Keyboard::empty(),
            write_ring: RingBuffer::empty(),
        }
    }

    pub fn init(&mut self) {
        self.uart.init();
        self.video_buffer.init();
    }

    pub fn setup_interrupts(&mut self) {
        // assign keyboard interrupt to this CPU
        apic::assign_io_irq(
            keyboard_interrupt as BasicInterruptHandler,
            self.keyboard.interrupt_num(),
            cpu::cpu(),
        );
        apic::assign_io_irq(
            uart_interrupt as BasicInterruptHandler,
            self.uart.interrupt_num(),
            cpu::cpu(),
        );
    }

    fn keyboard_interrupt(&mut self) {
        if let Some(k) = self.keyboard.try_read_char() {
            if let Some(c) = k.virtual_char {
                self.feed_char_from_interrupt(c);
            }
        }
    }

    fn uart_interrupt(&mut self) {
        if let Some(c) = unsafe { self.uart.try_read_byte() } {
            self.feed_char_from_interrupt(c);
        }
    }

    fn feed_char_from_interrupt(&mut self, c: u8) {
        // convert carriage return to newline (from uart)
        let c = if c == b'\r' { b'\n' } else { c };
        // don't force pushing for now
        // TODO: implement a place where console buffer can be filled
        let _ = self.write_ring.try_push(c);
    }

    #[allow(dead_code)]
    pub fn read(&mut self, dst: &mut [u8]) -> usize {
        let mut i = 0;
        while let Some(c) = self.write_ring.pop() {
            dst[i] = c;
            i += 1;
        }
        i
    }

    /// SAFETY: the caller must assure that this is called from once place at a time
    ///         and should handle synchronization
    unsafe fn write_byte(&mut self, byte: u8) {
        self.video_buffer.write_byte(byte, DEFAULT_ATTRIB);
        self.uart.write_byte(byte);
    }

    pub fn write(&mut self, src: &[u8]) -> usize {
        for &c in src {
            unsafe {
                self.write_byte(c);
            }
        }
        src.len()
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write(s.as_bytes());
        Ok(())
    }
}

extern "x86-interrupt" fn keyboard_interrupt(_frame: InterruptStackFrame64) {
    let console = CONSOLE.lock();
    console.borrow_mut().keyboard_interrupt();

    apic::return_from_interrupt();
}

extern "x86-interrupt" fn uart_interrupt(_frame: InterruptStackFrame64) {
    let console = CONSOLE.lock();
    console.borrow_mut().uart_interrupt();
    apic::return_from_interrupt();
}
