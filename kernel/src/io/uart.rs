use core::hint;

use crate::{cmdline, cpu};

#[repr(u32)]
#[derive(Clone, Copy)]
pub enum UartPort {
    COM1 = 0x3F8,
}

#[repr(u8)]
enum UartReg {
    Data = 0,
    InterruptEnable = 1,
    InterruptAndFifoControl = 2,
    LineControl = 3,
    ModemControl = 4,
    LineStatus = 5,
    #[allow(dead_code)]
    ModemStatus = 6,
    #[allow(dead_code)]
    Scratch = 7,
}

/// Enables the divisor latch access bit
const LINE_BAUD_LATCH: u8 = 1 << 7;

/// Controls the Data Terminal Ready Pin
const MODEM_CTL_DTR: u8 = 1 << 0;
/// Controls the Request To Send Pin
const MODEM_CTL_RTS: u8 = 1 << 1;
/// Controls the Out1 pin
const MODEM_CTL_OUT1: u8 = 1 << 2;
/// Controls the Out2 pin (used for interrupts)
const MODEM_CTL_OUT2: u8 = 1 << 3;
/// Controls the loopback mode
const MODEM_CTL_LOOPBACK: u8 = 1 << 4;

const IE_RX_READY: u8 = 1 << 0;
const IE_TX_READY: u8 = 1 << 1;

/// Got data
const LINE_RX_READY: u8 = 1 << 0;
/// Transmitter is empty
const LINE_TX_EMPTY: u8 = 1 << 5;

fn write_reg(port_addr: UartPort, reg: UartReg, val: u8) {
    unsafe { cpu::io_out(port_addr as u16 + reg as u16, val) }
}

fn read_reg(port_addr: UartPort, reg: UartReg) -> u8 {
    unsafe { cpu::io_in(port_addr as u16 + reg as u16) }
}

/// Will return `true` if the test pass, otherwise, the serial port is disabled
fn init_port(port_addr: UartPort) -> bool {
    // disable interrupts
    write_reg(port_addr, UartReg::InterruptEnable, 0);
    // disable FIFO
    write_reg(port_addr, UartReg::InterruptAndFifoControl, 0);

    // compute divisor
    let rate = if cmdline::cmdline().uart_baud == 0 {
        cmdline::cmdline().uart_baud
    } else {
        115200
    };
    let mut divisor = 115200 / rate;
    if divisor == 0 {
        divisor = 1;
    }
    if divisor >= 0x10000 {
        divisor = 0xFFFF;
    }

    // set baud rate
    // enable DLAB (change how Data and InterruptEnable)
    write_reg(port_addr, UartReg::LineControl, LINE_BAUD_LATCH);
    // set divisor
    // low byte
    write_reg(port_addr, UartReg::Data, divisor as u8);
    // high byte
    write_reg(port_addr, UartReg::InterruptEnable, (divisor >> 8) as u8);
    // disable DLAB
    // set 8 bits, no parity, one stop bit (8N1)
    write_reg(port_addr, UartReg::LineControl, 0x03);

    // enable FIFO, clear them, with 14-byte threshold
    // write_reg(port_addr, UartReg::InterruptAndFifoControl, 0x07);

    // enable receive and send interrupts
    write_reg(
        port_addr,
        UartReg::InterruptEnable,
        IE_RX_READY | IE_TX_READY,
    );

    // test the chip
    // set loopback mode
    write_reg(port_addr, UartReg::ModemControl, MODEM_CTL_LOOPBACK);
    write_reg(port_addr, UartReg::Data, 0xAA);
    // wait until we can read
    while read_reg(port_addr, UartReg::LineStatus) & LINE_RX_READY == 0 {
        hint::spin_loop();
    }
    // check if we got the same value (used later in the return)
    let val = read_reg(port_addr, UartReg::Data);

    // disable loopback mode go back to normal
    write_reg(
        port_addr,
        UartReg::ModemControl,
        MODEM_CTL_DTR | MODEM_CTL_RTS | MODEM_CTL_OUT1 | MODEM_CTL_OUT2,
    );

    // return true if the test pass, otherwise, we don't have serial port enabled
    val == 0xAA
}

#[derive(Clone)]
pub struct Uart {
    port_addr: UartPort,
    is_enabled: bool,
}

impl Uart {
    pub const fn new(port_addr: UartPort) -> Self {
        Self {
            port_addr,
            is_enabled: false,
        }
    }

    pub fn init(&mut self) {
        self.is_enabled = cmdline::cmdline().uart && init_port(self.port_addr);
    }

    /// SAFETY: `init` must be called before calling this function
    pub unsafe fn write_byte(&self, byte: u8) {
        if !self.is_enabled {
            return;
        }

        // wait until we can send
        while (read_reg(self.port_addr, UartReg::LineStatus) & LINE_TX_EMPTY) == 0 {
            hint::spin_loop();
        }
        // write the byte
        write_reg(self.port_addr, UartReg::Data, byte);
    }

    /// SAFETY: `init` must be called before calling this function
    pub unsafe fn try_read_byte(&self) -> Option<u8> {
        if !self.is_enabled {
            return None;
        }

        // wait until we can read
        if (read_reg(self.port_addr, UartReg::LineStatus) & LINE_RX_READY) == 0 {
            return None;
        }
        // read the byte
        Some(read_reg(self.port_addr, UartReg::Data))
    }

    #[allow(dead_code)]
    pub fn interrupt_num(&self) -> u8 {
        match self.port_addr {
            UartPort::COM1 => 4,
        }
    }
}
