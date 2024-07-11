use core::sync::atomic::{AtomicU64, Ordering};

use tracing::info;

use crate::{cpu, devices::clock::NANOS_PER_SEC};

use super::ClockDevice;

const fn cycles_to_ns(cycles: u64, nanos_per_cycle_scaled: u64) -> u64 {
    (((cycles as u128) * (nanos_per_cycle_scaled as u128)) >> 64) as u64
}

struct SyncPoint {
    nanos: u64,
    cycles: u64,
}

pub struct Tsc {
    /// Nanoseconds offset that we started counting from
    /// it may be negative, and thus we use `wrapping_add` and `wrapping_sub`
    start_time: AtomicU64,
    /// Frequency of the TSC, how many nano seconds per cycle
    /// scaled because
    nanos_per_cycle_scaled: AtomicU64,
    /// The latency of reading the TSC
    rd_tsc_call_latency: u64,
}

impl Tsc {
    pub fn new(base: &dyn ClockDevice) -> Option<Self> {
        if unsafe { cpu::cpuid::cpuid!(cpu::cpuid::FN_FEAT).edx } & cpu::cpuid::FEAT_EDX_TSC == 0 {
            return None;
        }
        let mut rd_tsc_call_latency = u64::MAX;
        for _ in 0..100 {
            let t1 = unsafe { cpu::read_tsc() };
            let t2 = unsafe { cpu::read_tsc() };
            rd_tsc_call_latency = rd_tsc_call_latency.min(t2 - t1);
        }

        let tsc = Tsc {
            start_time: AtomicU64::new(0),
            nanos_per_cycle_scaled: AtomicU64::new(0),
            rd_tsc_call_latency,
        };
        tsc.calibrate(base);
        Some(tsc)
    }

    fn get_device_delay(&self, base: &dyn ClockDevice) -> u64 {
        // measure clock latency
        let mut device_latency = u64::MAX;
        for _ in 0..100 {
            let t1 = unsafe { cpu::read_tsc() };
            let _ = base.get_time();
            let t2 = unsafe { cpu::read_tsc() };
            device_latency = device_latency.min(t2 - t1);
        }
        // subtract the latency from the TSC latency
        device_latency -= self.rd_tsc_call_latency;
        device_latency
    }

    fn get_sync_time_point(&self, base: &dyn ClockDevice, device_latency: u64) -> SyncPoint {
        let good_latency = device_latency + device_latency / 2;
        let mut min_cycles = u64::MAX;

        let mut ns = 0;
        let mut cycles = 0;

        for _ in 0..10 {
            let t1 = unsafe { cpu::read_tsc() };
            let device_time = base.get_time();
            let t2 = unsafe { cpu::read_tsc() };
            let diff_tsc = t2 - t1;
            if diff_tsc >= min_cycles {
                continue;
            }
            min_cycles = diff_tsc;

            ns = device_time.seconds * NANOS_PER_SEC + device_time.nanoseconds;
            cycles = t1 + self.rd_tsc_call_latency;

            if diff_tsc <= good_latency {
                break;
            }
        }

        SyncPoint { nanos: ns, cycles }
    }

    // part of the implementation is taken from `yb303/tsc_clock`
    //
    // MIT License
    //
    // Copyright (c) 2020 yb303
    //
    // Permission is hereby granted, free of charge, to any person obtaining a copy
    // of this software and associated documentation files (the "Software"), to deal
    // in the Software without restriction, including without limitation the rights
    // to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    // copies of the Software, and to permit persons to whom the Software is
    // furnished to do so, subject to the following conditions:
    //
    // The above copyright notice and this permission notice shall be included in all
    // copies or substantial portions of the Software.
    //
    // THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    // IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    // FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    // AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    // LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    // OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
    // SOFTWARE.
    pub fn calibrate(&self, base: &dyn ClockDevice) {
        let device_latency = self.get_device_delay(base);

        let granularity = base.granularity();
        assert!(granularity > 0);

        // at least 1ms (1000_000ns), and no more than 1s (1_000_000_000ns)
        let sleep_time = (granularity * 1000).max(1_000_000).min(NANOS_PER_SEC);
        info!("Calibrating TSC with sleep time: {}ns", sleep_time);

        let start_point = self.get_sync_time_point(base, device_latency);
        // sleep
        {
            let mut time = base.get_time();
            let start_ns = time.seconds * NANOS_PER_SEC + time.nanoseconds;
            while time.seconds * NANOS_PER_SEC + time.nanoseconds - start_ns < sleep_time {
                time = base.get_time();
            }
        }
        let end_point = self.get_sync_time_point(base, device_latency);

        let ns_diff = end_point.nanos - start_point.nanos;
        let cycles_diff = end_point.cycles - start_point.cycles;

        let scaled_ns_per_cycle = ((ns_diff as u128) << 64) / cycles_diff as u128;
        assert!(scaled_ns_per_cycle.leading_zeros() >= 64);
        let scaled_ns_per_cycle = scaled_ns_per_cycle as u64;

        let start_ns = start_point
            .nanos
            .wrapping_sub(cycles_to_ns(start_point.cycles, scaled_ns_per_cycle));
        self.start_time.store(start_ns, Ordering::Relaxed);
        self.nanos_per_cycle_scaled
            .store(scaled_ns_per_cycle, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    fn recalibrate(&self, base: &dyn ClockDevice) {
        let device_latency = self.get_device_delay(base);

        let end_point = self.get_sync_time_point(base, device_latency);

        let expected_nanos = self.time_nanos_since_start(end_point.cycles);
        let diff = expected_nanos - end_point.nanos;

        // If the difference is more than 50ms, we need to recalibrate
        if diff > 50_000_000 {
            info!("TSC recalibration needed, diff: {}ns", diff);
            self.calibrate(base);
        }
        let start_ns = end_point
            .nanos
            .wrapping_sub(self.cycles_to_time_nanos(end_point.cycles));
        self.start_time.store(start_ns, Ordering::Relaxed);
    }

    fn time_nanos_since_start(&self, cycles: u64) -> u64 {
        self.start_time
            .load(Ordering::Relaxed)
            .wrapping_add(self.cycles_to_time_nanos(cycles))
    }

    fn cycles_to_time_nanos(&self, cycles: u64) -> u64 {
        cycles_to_ns(cycles, self.nanos_per_cycle_scaled.load(Ordering::Relaxed))
    }
}

impl ClockDevice for Tsc {
    fn name(&self) -> &'static str {
        "TSC"
    }

    fn get_time(&self) -> super::ClockTime {
        let tsc = unsafe { cpu::read_tsc() };

        let nanos = self.time_nanos_since_start(tsc);
        super::ClockTime {
            seconds: nanos / NANOS_PER_SEC,
            nanoseconds: nanos % NANOS_PER_SEC,
        }
    }

    fn granularity(&self) -> u64 {
        let n = self.cycles_to_time_nanos(1);
        if n > 0 {
            n
        } else {
            1
        }
    }

    fn require_calibration(&self) -> bool {
        true
    }

    fn rating(&self) -> u64 {
        100
    }
}
