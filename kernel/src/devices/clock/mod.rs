mod hpet;
mod rtc;
mod tsc;

use core::fmt;

use alloc::{sync::Arc, vec::Vec};

use crate::{
    acpi::tables::{self, BiosTables, Facp},
    cpu::{self},
    sync::{once::OnceLock, spin::mutex::Mutex},
};

use self::rtc::Rtc;

pub const NANOS_PER_SEC: u64 = 1_000_000_000;

static CLOCKS: OnceLock<Clock> = OnceLock::new();

pub fn clocks() -> &'static Clock {
    CLOCKS.get()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockTime {
    /// nanoseconds added to `seconds`
    pub nanoseconds: u64,
    /// seconds passed since a fixed point in time
    pub seconds: u64,
}

#[allow(dead_code)]
impl ClockTime {
    pub fn as_nanos(&self) -> u64 {
        self.seconds * NANOS_PER_SEC + self.nanoseconds
    }
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

/// Accurate always increasing time source
struct SystemTime {
    /// The time when the system was started
    start_unix: ClockTime,
    /// The last time we ticked the system time
    last_tick: ClockTime,
    /// The system time since the start
    startup_offset: ClockTime,
    /// device used to get the time
    device: Option<Arc<dyn ClockDevice>>,
}

impl SystemTime {
    fn new(rtc: &Rtc) -> Self {
        let time = rtc.get_time();
        // let device_time = device.get_time();

        let timestamp = time.seconds_since_unix_epoch().expect("Must be after 1970");
        println!("Time now: {time} - UTC");
        println!("System start timestamp: {}", timestamp);

        let start_unix = ClockTime {
            nanoseconds: 0,
            seconds: timestamp,
        };

        Self {
            start_unix,
            last_tick: ClockTime {
                nanoseconds: 0,
                seconds: 0,
            },
            startup_offset: ClockTime {
                nanoseconds: 0,
                seconds: 0,
            },
            device: None,
        }
    }

    fn tick(&mut self) {
        if let Some(device) = &self.device {
            let time = device.get_time();
            let diff = time - self.last_tick;
            self.startup_offset = self.startup_offset + diff;
            self.last_tick = time;
        }
    }

    /// Will update the device if this one is different
    fn update_device(&mut self, device: Arc<dyn ClockDevice>, rtc: &Rtc) {
        if let Some(current_device) = &self.device {
            if Arc::ptr_eq(&device, current_device) {
                return;
            }

            // switch the counters to use the new device
            let time = current_device.get_time();
            let new_time = device.get_time();
            let diff = time - self.last_tick;
            self.startup_offset = self.startup_offset + diff;

            self.device = Some(device);
            self.last_tick = new_time
        } else {
            // this is the first time, make sure we are aligned with rtc

            cpu::cpu().push_cli();

            let mut rtc_time = rtc.get_time();
            // wait for the next second to start
            loop {
                let new_rtc_time = rtc.get_time();
                if new_rtc_time.seconds != rtc_time.seconds {
                    rtc_time = new_rtc_time;
                    break;
                }
            }
            let device_time = device.get_time();

            let timestamp = rtc_time
                .seconds_since_unix_epoch()
                .expect("Must be after 1970");
            println!("Adjusted Time now: {rtc_time} - UTC");
            println!("Adjusted System start timestamp: {}", timestamp);

            self.last_tick = device_time;
            self.device = Some(device);
            self.startup_offset = ClockTime {
                nanoseconds: 0,
                seconds: 0,
            };
            self.start_unix = ClockTime {
                nanoseconds: 0,
                seconds: rtc_time
                    .seconds_since_unix_epoch()
                    .expect("Must be after 1970"),
            };

            cpu::cpu().pop_cli();
        }
    }

    fn time_since_startup(&self) -> ClockTime {
        self.startup_offset
    }

    fn time_since_unix_epoch(&self) -> ClockTime {
        self.start_unix + self.startup_offset
    }
}

#[allow(dead_code)]
pub struct Clock {
    /// devices sorted based on their rating
    // TODO: replace with read-write lock
    devices: Mutex<Vec<Arc<dyn ClockDevice>>>,
    /// Used to determine the outside world time and use it as a base
    rtc: Rtc,
    /// System time
    // TODO: replace with read-write lock
    system_time: Mutex<SystemTime>,
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
            system_time: Mutex::new(SystemTime::new(&rtc)),
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
        devs.sort_unstable_by_key(|device| -(device.rating() as i64));
        self.system_time
            .lock()
            .update_device(devs[0].clone(), &self.rtc);
    }

    #[allow(dead_code)]
    fn get_best_clock(&self) -> Option<Arc<dyn ClockDevice>> {
        self.devices.lock().first().map(Arc::clone)
    }

    fn get_best_for_calibration(&self) -> Option<Arc<dyn ClockDevice>> {
        self.devices
            .lock()
            .iter()
            .find(|device| !device.require_calibration())
            .map(Arc::clone)
    }

    #[allow(dead_code)]
    pub fn tick_system_time(&self) {
        self.system_time.lock().tick();
    }

    #[allow(dead_code)]
    pub fn time_since_startup(&self) -> ClockTime {
        // TODO: find a better way to do this
        let mut time = self.system_time.lock();
        time.tick();
        time.time_since_startup()
    }

    #[allow(dead_code)]
    pub fn time_since_unix_epoch(&self) -> ClockTime {
        // TODO: find a better way to do this
        let mut time = self.system_time.lock();
        time.tick();
        time.time_since_unix_epoch()
    }
}

pub fn init(bios_tables: &BiosTables) {
    let facp = bios_tables.rsdt.get_table::<Facp>();
    let century_reg = facp.map(|facp| facp.century);

    // create the clock
    CLOCKS
        .set(Clock::new(Rtc::new(century_reg)))
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
