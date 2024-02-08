mod hpet;
mod rtc;

use crate::acpi::tables::{self, BiosTables, Facp};

use self::rtc::Rtc;

const NANOS_PER_SEC: u64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockTime {
    /// nanoseconds added to `seconds`
    pub nanoseconds: u64,
    /// seconds passed since a fixed point in time
    pub seconds: u64,
}

impl Ord for ClockTime {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.seconds
            .cmp(&other.seconds)
            .then(self.nanoseconds.cmp(&other.nanoseconds))
    }
}

impl PartialOrd for ClockTime {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl core::ops::Sub for ClockTime {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let nanoseconds = if self.nanoseconds < rhs.nanoseconds {
            self.nanoseconds + 1_000_000_000 - rhs.nanoseconds
        } else {
            self.nanoseconds - rhs.nanoseconds
        };
        let seconds = self.seconds
            - rhs.seconds
            - if self.nanoseconds < rhs.nanoseconds {
                1
            } else {
                0
            };
        Self {
            nanoseconds,
            seconds,
        }
    }
}

impl core::ops::Add for ClockTime {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let nanoseconds = self.nanoseconds + rhs.nanoseconds;
        let seconds = self.seconds + rhs.seconds + nanoseconds / 1_000_000_000;
        Self {
            nanoseconds: nanoseconds % 1_000_000_000,
            seconds,
        }
    }
}

trait ClockDevice {
    /// Returns the current time of the device with no relation to anything
    /// The system will use consequtive calls to determine the time
    fn get_time(&self) -> ClockTime;
    /// Returns the granularity of the device in nanoseconds, i.e. the smallest time unit it can measure
    /// Must be at least 1
    fn granularity(&self) -> u64;
    /// Returns true if the device needs to be calibration
    /// i.e. it doesn't count time correctly
    fn require_calibration(&self) -> bool;
}

pub fn init(bios_tables: &BiosTables) {
    let facp = bios_tables.rsdt.get_table::<Facp>();

    let century_reg = facp.map(|facp| facp.century);
    // TODO: use it later, and provide it to everyone who need it
    let rtc = Rtc::new(century_reg);
    let rtc_time = rtc.get_time();
    println!("Time now: {rtc_time}: UTC");

    let hpet_table = bios_tables.rsdt.get_table::<tables::Hpet>();

    if let Some(hpet_table) = hpet_table {
        hpet::init(hpet_table);
    } else {
        println!("HPET is not available!");
    }
}
