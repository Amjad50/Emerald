use crate::cpu;

const PS2_STATUS_PORT: u16 = 0x64;
const PS2_DATA_PORT: u16 = 0x60;

#[allow(dead_code)]
pub mod status {
    pub const DATA_READY: u8 = 1 << 0;
    pub const INPUT_BUFFER_FULL: u8 = 1 << 1;
    pub const SYSTEM_FLAG: u8 = 1 << 2;
    pub const COMMAND_DATA: u8 = 1 << 3;
    pub const KEYBOARD_LOCKED: u8 = 1 << 4;
    pub const KEYBOARD_TIMEOUT_MOUSE_DATA: u8 = 1 << 5;
    pub const RECEIVE_TIMEOUT: u8 = 1 << 6;
    pub const PARITY_ERROR: u8 = 1 << 7;
}

#[derive(Debug, Clone, Copy)]
pub struct Ps2;

impl Ps2 {
    pub fn read_data(&self) -> u8 {
        unsafe { cpu::io_in(PS2_DATA_PORT) }
    }

    // returns (success, resend)
    fn wait_for_ack(&self) -> (bool, bool) {
        let mut timeout = 1000;
        while timeout > 0 {
            if self.read_status() & status::DATA_READY != 0 {
                let data = self.read_data();
                match data {
                    0xFA => return (true, false),
                    0xFE => return (false, true),
                    _ => {}
                }
                panic!("unexpected data from mouse: {:#X}, waiting for ack", data)
            }
            timeout -= 1;
        }
        (false, false)
    }

    pub fn read_status(&self) -> u8 {
        unsafe { cpu::io_in(PS2_STATUS_PORT) }
    }

    pub fn has_data(&self) -> bool {
        self.read_status() & status::DATA_READY != 0
    }

    pub fn write_prefix(&self, prefix: u8) {
        self.wait_for_command_clear();
        unsafe { cpu::io_out(PS2_STATUS_PORT, prefix) }
    }

    fn write_data(&self, data: u8) {
        unsafe { cpu::io_out(PS2_DATA_PORT, data) }
    }

    pub fn wait_read_data(&self) -> u8 {
        loop {
            if self.read_status() & status::DATA_READY != 0 {
                return self.read_data();
            }
        }
    }

    pub fn wait_read_data_slice(&self, data: &mut [u8]) {
        for byte in data.iter_mut() {
            *byte = self.wait_read_data();
        }
    }

    #[must_use]
    pub fn write_command_data(&self, data: u8) -> Option<()> {
        let mut attempts = 3;
        loop {
            self.wait_for_command_clear();
            self.write_data(data);
            let (ack, resend) = self.wait_for_ack();

            if resend {
                attempts -= 1;
                if attempts == 0 {
                    return None;
                }
                continue;
            }

            if ack {
                return Some(());
            }
        }
    }

    pub fn wait_for_command_clear(&self) {
        while self.read_status() & status::DATA_READY != 0 {
            // flush the buffer
            self.read_data();
        }
        while self.read_status() & status::INPUT_BUFFER_FULL != 0 {
            // wait for the buffer to be empty
            core::hint::spin_loop();
        }
    }
}
