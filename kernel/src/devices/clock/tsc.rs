use core::sync::atomic::{AtomicU64, Ordering};

use crate::{cpu, devices::clock::NANOS_PER_SEC};

use super::ClockDevice;

const CALIBRATION_LOOPS: usize = 1000;

pub struct Tsc {
    start: AtomicU64,
    frequency_ns: AtomicU64,
}

impl Tsc {
    pub fn new(base: &dyn ClockDevice) -> Option<Self> {
        if unsafe { cpu::cpuid::cpuid!(cpu::cpuid::FN_FEAT).edx } & cpu::cpuid::FEAT_EDX_TSC == 0 {
            return None;
        }

        let tsc = Tsc {
            start: AtomicU64::new(0),
            frequency_ns: AtomicU64::new(0),
        };
        tsc.re_calibrate(base);
        Some(tsc)
    }

    pub fn re_calibrate(&self, base: &dyn ClockDevice) {
        // self.start = unsafe { cpu::read_tsc() };
        // self.frequency = unsafe { tsc_frequency() };

        let granularity = base.granularity();
        assert!(granularity > 0);

        // modify the loops based on the granularity, so we don't wait too long
        let loops = CALIBRATION_LOOPS / granularity as usize;
        let loops = loops.max(2); // if its too large, we don't want to wait forever

        eprintln!("Calibrating TSC with {} loops", loops);

        let tsc_start = unsafe { cpu::read_tsc() };
        let start_time = base.get_time();
        let mut base_time = start_time;

        for _ in 0..loops {
            loop {
                let new_time = base.get_time();
                if new_time > base_time {
                    base_time = new_time;
                    break;
                }
            }
        }

        let tsc_end = unsafe { cpu::read_tsc() };
        let end_time = base.get_time();

        let time = end_time - start_time;
        let tsc = tsc_end - tsc_start;

        let total_time_ns = time.seconds * NANOS_PER_SEC + time.nanoseconds;
        self.frequency_ns.store(
            ((tsc as u128 * NANOS_PER_SEC as u128) / total_time_ns as u128) as u64,
            Ordering::Relaxed,
        );
        self.start.store(tsc_start, Ordering::Relaxed);

        eprintln!(
            "TSC calibrated to {:.03} MHz",
            self.frequency_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0
        );
    }
}

impl ClockDevice for Tsc {
    fn name(&self) -> &'static str {
        "TSC"
    }

    fn get_time(&self) -> super::ClockTime {
        let tsc = unsafe { cpu::read_tsc() };
        let tsc = tsc - self.start.load(Ordering::Relaxed);
        let nanos = tsc / self.frequency_ns.load(Ordering::Relaxed);
        super::ClockTime {
            nanoseconds: nanos % NANOS_PER_SEC,
            seconds: nanos / NANOS_PER_SEC,
        }
    }

    fn granularity(&self) -> u64 {
        let granularity = NANOS_PER_SEC / self.frequency_ns.load(Ordering::Relaxed);
        if granularity == 0 {
            1
        } else {
            granularity
        }
    }

    fn require_calibration(&self) -> bool {
        true
    }

    fn rating(&self) -> u64 {
        100
    }
}
