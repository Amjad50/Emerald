use super::ps2::Ps2;

use blinkcast::alloc::{Receiver as BlinkcastReceiver, Sender as BlinkcastSender};
use kernel_user_link::mouse::{MouseEvent, ScrollType};

#[allow(dead_code)]
pub mod scaling {
    pub const PER_SEC_10: u8 = 10;
    pub const PER_SEC_20: u8 = 20;
    pub const PER_SEC_40: u8 = 40;
    pub const PER_SEC_60: u8 = 60;
    pub const PER_SEC_80: u8 = 80;
    pub const PER_SEC_100: u8 = 100;
    pub const PER_SEC_200: u8 = 200;
}

#[allow(dead_code)]
pub mod resolution {
    pub const PIXEL_PER_MM_1: u8 = 0;
    pub const PIXEL_PER_MM_2: u8 = 1;
    pub const PIXEL_PER_MM_4: u8 = 2;
    pub const PIXEL_PER_MM_8: u8 = 3;
}

#[allow(dead_code)]
mod commands {
    pub const MOUSE_PREFIX: u8 = 0xD4;

    pub const MOUSE_RESET: u8 = 0xFF;
    pub const MOUSE_ENABLE_STREAMING: u8 = 0xF4;
    pub const MOUSE_DISABLE_STREAMING: u8 = 0xF5;
    pub const MOUSE_SET_DEFAULTS: u8 = 0xF6;
    pub const MOUSE_SET_SAMPLE_RATE: u8 = 0xF3;
    pub const MOUSE_GET_DEVICE_ID: u8 = 0xF2;
    pub const MOUSE_REQUEST_SINGLE_PACKET: u8 = 0xEB;
    pub const MOUSE_STATUS_REQUEST: u8 = 0xE9;
}

mod packet {
    pub const Y_OVERFLOW: u8 = 1 << 7;
    pub const X_OVERFLOW: u8 = 1 << 6;
    pub const Y_SIGN: u8 = 1 << 5;
    pub const X_SIGN: u8 = 1 << 4;
}

/// Number of events that can be buffered before being overwritten
const MOUSE_BUFFER_SIZE: usize = 1024;

// PS/2 mouse interrupt
pub const MOUSE_INT_NUM: u8 = 12;

pub type MouseReader = BlinkcastReceiver<MouseEvent>;

pub struct Mouse {
    ps2: Ps2,
    has_extra_byte: bool,
    sender: BlinkcastSender<MouseEvent>,
}

#[allow(dead_code)]
impl Mouse {
    pub fn new(ps2: Ps2) -> Mouse {
        let mut device = Mouse {
            ps2,
            has_extra_byte: false,
            sender: BlinkcastSender::new(MOUSE_BUFFER_SIZE),
        };

        // enable the mouse
        device.reset();
        let mut mouse_id = device.get_id();
        device.write_command(commands::MOUSE_SET_DEFAULTS, None);
        device.write_command(commands::MOUSE_ENABLE_STREAMING, None);

        // magic to transition to mouse id 3
        if mouse_id != 3 {
            device.write_command(commands::MOUSE_SET_SAMPLE_RATE, Some(scaling::PER_SEC_200));
            device.write_command(commands::MOUSE_SET_SAMPLE_RATE, Some(scaling::PER_SEC_100));
            device.write_command(commands::MOUSE_SET_SAMPLE_RATE, Some(scaling::PER_SEC_80));
            mouse_id = device.get_id();
        }

        // magic to transition to mouse id 4
        if mouse_id == 3 {
            device.write_command(commands::MOUSE_SET_SAMPLE_RATE, Some(scaling::PER_SEC_200));
            device.write_command(commands::MOUSE_SET_SAMPLE_RATE, Some(scaling::PER_SEC_200));
            device.write_command(commands::MOUSE_SET_SAMPLE_RATE, Some(scaling::PER_SEC_80));
            mouse_id = device.get_id();
        }

        if mouse_id == 3 || mouse_id == 4 {
            device.has_extra_byte = true;
        }

        device
    }

    pub fn new_receiver(&self) -> MouseReader {
        self.sender.new_receiver()
    }

    pub fn handle_mouse_data(&self) {
        let mut data = [0; 4];
        let read_len = if self.has_extra_byte { 4 } else { 3 };
        for d in data.iter_mut().take(read_len) {
            *d = self.ps2.read_data();
        }

        if data[0] & packet::X_OVERFLOW != 0 || data[0] & packet::Y_OVERFLOW != 0 {
            // overflow, ignore the data
            return;
        }

        // combine the data for the main buttons plus the 4th/5th if available
        let buttons = data[0] & 0b111 | ((data[3] >> 4) & 0b11) << 3;
        let mut x = data[1] as u16;
        let mut y = data[2] as u16;

        if data[0] & packet::X_SIGN != 0 {
            x |= 0xFF00;
        }
        if data[0] & packet::Y_SIGN != 0 {
            y |= 0xFF00;
        }

        let x = x as i16;
        let y = y as i16;

        let scroll_type = match data[3] & 0b1111 {
            1 => ScrollType::VerticalUp,
            0xF => ScrollType::VerticalDown,
            2 => ScrollType::HorizontalRight,
            0xE => ScrollType::HorizontalNegative,
            _ => ScrollType::None,
        };

        let event = MouseEvent {
            x,
            y,
            buttons,
            scroll_type,
        };

        self.sender.send(event);
    }

    fn reset(&self) {
        self.ps2.write_prefix(commands::MOUSE_PREFIX);
        self.ps2.write_command_data(commands::MOUSE_RESET).unwrap();
        loop {
            let data = self.ps2.wait_read_data();
            if data != 0xAA && data != 0xFA {
                panic!("failed to reset mouse: {:#X}", data);
            }
            if data == 0xAA {
                break;
            }
        }
    }

    fn write_command(&self, command: u8, extra: Option<u8>) {
        self.ps2.write_prefix(commands::MOUSE_PREFIX);
        self.ps2.write_command_data(command).unwrap();

        if let Some(extra) = extra {
            self.ps2.write_prefix(commands::MOUSE_PREFIX);
            if self.ps2.write_command_data(extra).is_none() {
                println!("[WARN] command {:#X} extra {:#X} failed", command, extra);
            }
        }
    }

    fn get_id(&self) -> u8 {
        self.write_command(commands::MOUSE_GET_DEVICE_ID, None);
        self.ps2.wait_read_data()
    }

    #[allow(dead_code)]
    fn get_status(&self) -> [u8; 3] {
        self.write_command(commands::MOUSE_STATUS_REQUEST, None);
        let mut status = [0; 3];
        self.ps2.wait_read_data_slice(&mut status);
        status
    }
}
