use core::sync::atomic::{AtomicU16, AtomicU64, Ordering};

use alloc::sync::Arc;

use crate::{
    cpu::{
        self,
        idt::{BasicInterruptHandler, InterruptStackFrame64},
        interrupts::apic,
    },
    devices::clock::{ClockDevice, ClockTime, FEMTOS_PER_SEC, NANOS_PER_FEMTO},
    sync::once::OnceLock,
};

static PIT_CLOCK: OnceLock<Arc<Pit>> = OnceLock::new();

const PIT_TICK_PERIOD_FEMTOS: u64 = 838095345;
const PIT_TICK_PERIOD_NANOS: u64 = PIT_TICK_PERIOD_FEMTOS / NANOS_PER_FEMTO;

#[allow(dead_code)]
pub mod pit_io {
    // Port Addresses
    pub const PORT_CONTROL: u16 = 0x43;
    pub const PORT_CHANNEL_0: u16 = 0x40;
    pub const PORT_CHANNEL_1: u16 = 0x41;
    pub const PORT_CHANNEL_2: u16 = 0x42;

    // Channel Selection
    pub const SELECT_CHANNEL_0: u8 = 0b00 << 6;
    pub const SELECT_CHANNEL_1: u8 = 0b01 << 6;
    pub const SELECT_CHANNEL_2: u8 = 0b10 << 6;
    pub const SELECT_READ_BACK: u8 = 0b11 << 6;

    // Access Modes
    pub const ACCESS_LATCH_COUNT: u8 = 0b00 << 4;
    pub const ACCESS_LOBYTE: u8 = 0b01 << 4;
    pub const ACCESS_HIBYTE: u8 = 0b10 << 4;
    pub const ACCESS_LOBYTE_HIBYTE: u8 = 0b11 << 4;

    // Operating Modes
    pub const MODE_INTERRUPT_ON_TERMINAL_COUNT: u8 = 0b000 << 1;
    pub const MODE_HARDWARE_RETRIGGERABLE_ONE_SHOT: u8 = 0b001 << 1;
    pub const MODE_RATE_GENERATOR: u8 = 0b010 << 1;
    pub const MODE_SQUARE_WAVE_GENERATOR: u8 = 0b011 << 1;
    pub const MODE_SOFTWARE_TRIGGERED_STROBE: u8 = 0b100 << 1;
    pub const MODE_HARDWARE_TRIGGERED_STROBE: u8 = 0b101 << 1;

    // BCD/Binary Mode
    pub const MODE_BINARY: u8 = 0b0;
    pub const MODE_BCD: u8 = 0b1;

    pub const DEFAULT_INTERRUPT: u8 = 0;
}

pub fn disable() {
    // disable PIT (timer)
    unsafe {
        // Disable the PIT, we are using HPET instead
        // Not sure if this is an intended way to do it, but what we do here is:
        // 1. Select channel 0 (main one)
        // 2. Set access mode to lobyte (we only need this)
        // 3. Set operating mode to `interrupt on terminal count` (one shot)
        // 4. Reload value with 1 only, which will just trigger the interrupt immediately
        //    and then never again.
        //
        // Docs on Mode 0 (interrupt on terminal count):
        //  the mode/command register is written the output signal goes low and the PIT waits
        //  for the reload register to be set by software.
        //  When the current count decrements from one to zero, the output goes high and remains
        //  high until another mode/command register is written or the reload register is set again.
        //
        // How this works is that we select this mode, with the reload value of 1, which will
        // trigger the interrupt immediately, and then never again.
        // Since we don't have interrupt handler now, we just ignore it.
        cpu::io_out(
            pit_io::PORT_CONTROL,
            pit_io::SELECT_CHANNEL_0
                | pit_io::ACCESS_LOBYTE
                | pit_io::MODE_INTERRUPT_ON_TERMINAL_COUNT
                | pit_io::MODE_BINARY,
        );
        cpu::io_out(pit_io::PORT_CHANNEL_0, 1u8);
    }
}

pub fn init() -> Arc<Pit> {
    // make sure we don't get interrupted before `PIT_CLOCK`
    // is initialized
    cpu::cpu().push_cli();

    // just to make sure that we don't initialize it twice
    if PIT_CLOCK.try_get().is_some() {
        panic!("PIT already initialized");
    }

    let clock = PIT_CLOCK.get_or_init(|| Arc::new(Pit::new()));

    cpu::cpu().pop_cli();

    clock.clone()
}

pub struct Pit {
    total_counter: AtomicU64,
    last_counter: AtomicU16,
}

impl Pit {
    pub fn new() -> Pit {
        // this will be 0x10000 which is the highest value
        // we don't care about the interrupt, and instead we just want to use
        // the counter to know the clock timer, so a larger value is better
        // since it will give us more time to check the value without being wrapped.
        const RELOAD_VALUE: u16 = 0x0000;

        // SAFETY: this is unsafe because it does I/O operations
        // on the hardware
        unsafe {
            cpu::io_out(
                pit_io::PORT_CONTROL,
                pit_io::SELECT_CHANNEL_0
                    | pit_io::ACCESS_LOBYTE_HIBYTE
                    | pit_io::MODE_RATE_GENERATOR
                    | pit_io::MODE_BINARY,
            );

            cpu::io_out(pit_io::PORT_CHANNEL_0, (RELOAD_VALUE & 0xFF) as u8);
            cpu::io_out(pit_io::PORT_CHANNEL_0, (RELOAD_VALUE >> 8) as u8);
        }

        apic::assign_io_irq(
            pit_interrupt as BasicInterruptHandler,
            pit_io::DEFAULT_INTERRUPT,
            &cpu::cpu(),
        );

        Pit {
            last_counter: AtomicU16::new(0),
            total_counter: AtomicU64::new(0),
        }
    }

    fn read_counter(&self) -> u16 {
        let lo;
        let hi;

        cpu::cpu().push_cli();

        // SAFETY: this is unsafe because it does I/O operations
        // on the hardware
        unsafe {
            cpu::io_out(
                pit_io::PORT_CONTROL,
                pit_io::SELECT_CHANNEL_0 | pit_io::ACCESS_LATCH_COUNT | pit_io::MODE_BINARY,
            );

            lo = cpu::io_in::<u8>(pit_io::PORT_CHANNEL_0) as u16;
            hi = cpu::io_in::<u8>(pit_io::PORT_CHANNEL_0) as u16;
        }
        cpu::cpu().pop_cli();

        (hi << 8) | lo
    }

    fn get_elapsed(&self) -> u64 {
        let counter = self.read_counter();

        self.last_counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |_| Some(counter))
            .unwrap()
            .wrapping_sub(counter) as u64
    }

    /// Ticks and returns the total number of ticks since creation
    fn tick_total_counter(&self) -> u64 {
        self.total_counter
            .fetch_add(self.get_elapsed(), Ordering::Relaxed);

        self.total_counter.load(Ordering::Relaxed)
    }
}

impl ClockDevice for Pit {
    fn name(&self) -> &'static str {
        "PIT"
    }

    fn get_time(&self) -> ClockTime {
        let counter = self.tick_total_counter();

        let seconds_divider = FEMTOS_PER_SEC / PIT_TICK_PERIOD_FEMTOS;
        let seconds = counter / seconds_divider;
        let nanoseconds = (counter % seconds_divider) * PIT_TICK_PERIOD_NANOS;

        ClockTime {
            seconds,
            nanoseconds,
        }
    }

    fn granularity(&self) -> u64 {
        PIT_TICK_PERIOD_FEMTOS / NANOS_PER_FEMTO
    }

    fn require_calibration(&self) -> bool {
        false
    }

    fn rating(&self) -> u64 {
        40
    }
}

extern "x86-interrupt" fn pit_interrupt(_stack_frame: InterruptStackFrame64) {
    PIT_CLOCK.get().tick_total_counter();

    // nothing to do here really
    apic::return_from_interrupt();
}
