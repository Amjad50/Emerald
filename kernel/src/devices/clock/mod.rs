mod hpet;
mod rtc;
mod tsc;

use core::fmt;

use alloc::{sync::Arc, vec::Vec};

use crate::{
    acpi::tables::{self, BiosTables, Facp},
    sync::{once::OnceLock, spin::mutex::Mutex},
};

use self::rtc::Rtc;

const NANOS_PER_SEC: u64 = 1_000_000_000;

static CLOCKS: OnceLock<Clock> = OnceLock::new();

fn clocks() -> &'static Clock {
    CLOCKS.get()
}

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

trait ClockDevice: Send + Sync {
    /// Returns the name of the device
    fn name(&self) -> &'static str;
    /// Returns the current time of the device with no relation to anything
    /// The system will use consequtive calls to determine the time
    fn get_time(&self) -> ClockTime;
    /// Returns the granularity of the device in nanoseconds, i.e. the smallest time unit it can measure
    /// Must be at least 1
    fn granularity(&self) -> u64;
    /// Returns true if the device needs to be calibration
    /// i.e. it doesn't count time correctly
    fn require_calibration(&self) -> bool;
    /// Returns the rating of the device, i.e. how good it is
    /// The higher the better
    fn rating(&self) -> u64 {
        // default rating is 1
        1
    }
}

#[allow(dead_code)]
struct Clock {
    /// devices sorted based on their rating
    // TODO: replace with read-write lock
    devices: Mutex<Vec<Arc<dyn ClockDevice>>>,
    /// Used to determine the outside world time and use it as a base
    rtc: Rtc,
}

impl fmt::Debug for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Clock").finish()
    }
}

impl Clock {
    fn new(rtc: Rtc) -> Self {
        Self {
            devices: Mutex::new(Vec::new()),
            rtc,
        }
    }

    fn add_device(&self, device: Arc<dyn ClockDevice>) {
        println!(
            "Adding clock device: {}, rating: {}",
            device.name(),
            device.rating()
        );
        let mut devs = self.devices.lock();
        devs.push(device);
        devs.sort_unstable_by_key(|device| device.rating() as i64 * -1);
    }

    #[allow(dead_code)]
    fn get_best_clock(&self) -> Option<Arc<dyn ClockDevice>> {
        self.devices.lock().first().map(|device| Arc::clone(device))
    }

    fn get_best_for_calibration(&self) -> Option<Arc<dyn ClockDevice>> {
        self.devices
            .lock()
            .iter()
            .find(|device| !device.require_calibration())
            .map(|device| Arc::clone(device))
    }
}

pub fn init(bios_tables: &BiosTables) {
    let facp = bios_tables.rsdt.get_table::<Facp>();
    let century_reg = facp.map(|facp| facp.century);
    let rtc = Rtc::new(century_reg);
    let rtc_time = rtc.get_time();
    println!("Time now: {rtc_time}: UTC");

    // create the clock
    CLOCKS
        .set(Clock::new(rtc))
        .expect("Clock is already initialized");

    // init HPET
    let hpet_table = bios_tables.rsdt.get_table::<tables::Hpet>();
    if let Some(hpet_table) = hpet_table {
        clocks().add_device(hpet::init(hpet_table));
    } else {
        println!("HPET is not available!");
    }

    // init TSC
    if let Some(tsc) = tsc::Tsc::new(
        clocks()
            .get_best_for_calibration()
            .expect("Have a clock that can be used as a base for TSC calibration")
            .as_ref(),
    ) {
        clocks().add_device(Arc::new(tsc));
    }
}
