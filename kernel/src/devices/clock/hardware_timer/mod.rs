//! Hardware Timer
//!
//! Timer drivers for components that act as timers/clocks
//! This includes the High Precision Event Timer (HPET) and Programmable Interval Timer (PIT).

use alloc::sync::Arc;
use hpet::Hpet;
use pit::Pit;
use tracing::warn;

use crate::{acpi, sync::spin::mutex::Mutex};

use super::ClockDevice;

mod hpet;
mod pit;

pub enum HardwareTimer {
    Hpet(Arc<Mutex<Hpet>>),
    Pit(Arc<Mutex<Pit>>),
}
impl HardwareTimer {
    pub fn init(hpet_table: Option<&acpi::tables::Hpet>) -> Arc<dyn ClockDevice> {
        Arc::new(match hpet_table {
            Some(hpet_table) => HardwareTimer::Hpet(hpet::init(hpet_table)),
            None => {
                warn!("HPET clock not found, falling back to PIT");
                HardwareTimer::Pit(pit::init())
            }
        })
    }
}

impl ClockDevice for HardwareTimer {
    fn name(&self) -> &'static str {
        match self {
            HardwareTimer::Hpet(t) => t.name(),
            HardwareTimer::Pit(t) => t.name(),
        }
    }

    fn get_time(&self) -> super::ClockTime {
        match self {
            HardwareTimer::Hpet(t) => t.get_time(),
            HardwareTimer::Pit(t) => t.get_time(),
        }
    }

    fn granularity(&self) -> u64 {
        match self {
            HardwareTimer::Hpet(t) => t.granularity(),
            HardwareTimer::Pit(t) => t.granularity(),
        }
    }

    fn require_calibration(&self) -> bool {
        match self {
            HardwareTimer::Hpet(t) => t.require_calibration(),
            HardwareTimer::Pit(t) => t.require_calibration(),
        }
    }

    fn rating(&self) -> u64 {
        match self {
            HardwareTimer::Hpet(t) => t.rating(),
            HardwareTimer::Pit(t) => t.rating(),
        }
    }
}
