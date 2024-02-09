use core::fmt;

use crate::cpu;

pub const CURRENT_CENTURY: u16 = 2000 / 100;

pub const RTC_ADDRESS: u16 = 0x70;
pub const RTC_DATA: u16 = 0x71;

pub const RTC_SECONDS: u8 = 0x00;
pub const RTC_MINUTES: u8 = 0x02;
pub const RTC_HOURS: u8 = 0x04;
pub const RTC_DAY_OF_MONTH: u8 = 0x07;
pub const RTC_MONTH: u8 = 0x08;
pub const RTC_YEAR: u8 = 0x09;

pub const RTC_STATUS_A: u8 = 0x0A;
pub const RTC_STATUS_B: u8 = 0x0B;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RtcTime {
    seconds: u8,
    minutes: u8,
    hours: u8,
    day_of_month: u8,
    month: u8,
    year: u16,
}

pub struct Rtc {
    century_reg: Option<u8>,
}

impl fmt::Display for RtcTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{year:04}-{month:02}-{day_of_month:02} {hours:02}:{minutes:02}:{seconds:02}",
            year = self.year,
            month = self.month,
            day_of_month = self.day_of_month,
            hours = self.hours,
            minutes = self.minutes,
            seconds = self.seconds,
        )
    }
}

impl Rtc {
    pub const fn new(century_reg: Option<u8>) -> Self {
        let century_reg = if let Some(century_reg) = century_reg {
            if century_reg == 0 {
                None
            } else {
                Some(century_reg)
            }
        } else {
            None
        };
        Self { century_reg }
    }

    fn read_register(&self, reg: u8) -> u8 {
        unsafe {
            cpu::io_out(RTC_ADDRESS, reg);
            cpu::io_in(RTC_DATA)
        }
    }

    fn is_updating(&self) -> bool {
        self.read_register(RTC_STATUS_A) & 0x80 != 0
    }

    fn is_bcd(&self) -> bool {
        self.read_register(RTC_STATUS_B) & 0x04 == 0
    }

    fn get_time_sync(&self) -> (RtcTime, u8) {
        // keep getting until we get a consistent time
        let mut t = RtcTime::default();
        let mut century = 0;

        loop {
            while self.is_updating() {}
            let mut century_new = century;
            let t_new = RtcTime {
                seconds: self.read_register(RTC_SECONDS),
                minutes: self.read_register(RTC_MINUTES),
                hours: self.read_register(RTC_HOURS),
                day_of_month: self.read_register(RTC_DAY_OF_MONTH),
                month: self.read_register(RTC_MONTH),
                year: self.read_register(RTC_YEAR) as u16,
            };

            if let Some(century_reg) = self.century_reg {
                century_new = self.read_register(century_reg);
            }

            // if we get a consistent time, break
            if t_new == t && century_new == century {
                break;
            }
            t = t_new;
            century = century_new;
        }
        (t, century)
    }

    pub fn get_time(&self) -> RtcTime {
        let (mut t, mut century) = self.get_time_sync();

        if self.is_bcd() {
            t.seconds = (t.seconds & 0x0F) + ((t.seconds / 16) * 10);
            t.minutes = (t.minutes & 0x0F) + ((t.minutes / 16) * 10);
            t.hours = ((t.hours & 0x0F) + (((t.hours & 0x70) / 16) * 10)) | (t.hours & 0x80);
            t.day_of_month = (t.day_of_month & 0x0F) + ((t.day_of_month / 16) * 10);
            t.month = (t.month & 0x0F) + ((t.month / 16) * 10);
            t.year = (t.year & 0x0F) + ((t.year / 16) * 10);
            if self.century_reg.is_some() {
                century = (century & 0x0F) + ((century / 16) * 10);
            }
        }
        if t.hours & 0x80 != 0 {
            t.hours = ((t.hours & 0x7F) + 12) % 24;
        }
        let century = if self.century_reg.is_some() {
            century as u16
        } else {
            CURRENT_CENTURY
        };
        t.year += century * 100;

        t
    }
}
