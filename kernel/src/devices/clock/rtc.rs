use core::fmt;

use crate::{cpu, testing};

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

pub const SECONDS_PER_MINUTE: u64 = 60;
pub const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
pub const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;
pub const DAYS_PER_MONTH_ARRAY: [u64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

/// This is used to offset the calculated seconds for all days from unix time
const UNIX_EPOCH_IN_SECONDS: u64 = 62135596800;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RtcTime {
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
    pub day_of_month: u8,
    pub month: u8,
    pub year: u16,
}

impl RtcTime {
    pub fn seconds_since_unix_epoch(&self) -> Option<u64> {
        // unix starts at 1970-01-01 00:00:00
        if self.year < 1970 {
            return None;
        }

        let is_year_leap =
            self.month > 2 && self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0);

        let last_year = (self.year - 1) as u64;
        let days_in_last_years =
            (last_year * 365) + (last_year / 4) - (last_year / 100) + (last_year / 400);
        let this_year_days = DAYS_PER_MONTH_ARRAY[self.month as usize - 1]
            + self.day_of_month as u64
            - !is_year_leap as u64;

        let total_days = days_in_last_years + this_year_days;

        let timestamp_since_unix = total_days * SECONDS_PER_DAY
            + self.hours as u64 * SECONDS_PER_HOUR
            + self.minutes as u64 * SECONDS_PER_MINUTE
            + self.seconds as u64;

        Some(timestamp_since_unix - UNIX_EPOCH_IN_SECONDS)
    }
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

#[macro_rules_attribute::apply(testing::test)]
fn test_seconds_since_unix_epoch() {
    // to silence clippy type warning
    type RtcTestData = (u16, u8, u8, u8, u8, u8);
    const TESTS: [(RtcTestData, u64); 5] = [
        ((2024, 1, 1, 12, 3, 45), 1704110625),
        ((1987, 11, 28, 0, 0, 0), 565056000),
        ((5135, 3, 4, 9, 33, 45), 99883100025),
        ((2811, 3, 4, 9, 33, 45), 26544792825),
        ((2404, 2, 29, 9, 33, 45), 13700828025),
    ];

    for ((year, month, day, hours, minutes, seconds), expected) in TESTS {
        let t = RtcTime {
            seconds,
            minutes,
            hours,
            day_of_month: day,
            month,
            year,
        };
        assert_eq!(t.seconds_since_unix_epoch(), Some(expected));
    }
}
