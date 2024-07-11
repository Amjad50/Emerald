use alloc::sync::Arc;

use crate::{
    cpu::{
        self,
        idt::{BasicInterruptHandler, InterruptStackFrame64},
        interrupts::apic,
    },
    devices::clock::{ClockDevice, ClockTime, FEMTOS_PER_SEC, NANOS_PER_FEMTO},
    sync::{once::OnceLock, spin::mutex::Mutex},
};

static PIT_CLOCK: OnceLock<Arc<Mutex<Pit>>> = OnceLock::new();

const PIT_TICK_PERIOD_FEMTOS: u64 = 838095345;
const PIT_TICK_PERIOD_NANOS: u64 = PIT_TICK_PERIOD_FEMTOS / NANOS_PER_FEMTO;

#[allow(dead_code)]
pub mod pit {
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
            pit::PORT_CONTROL,
            pit::SELECT_CHANNEL_0
                | pit::ACCESS_LOBYTE
                | pit::MODE_INTERRUPT_ON_TERMINAL_COUNT
                | pit::MODE_BINARY,
        );
        cpu::io_out(pit::PORT_CHANNEL_0, 1u8);
    }
}

pub fn init() -> Arc<Mutex<Pit>> {
    // make sure we don't get interrupted before `PIT_CLOCK`
    // is initialized
    cpu::cpu().push_cli();

    // just to make sure that we don't initialize it twice
    if PIT_CLOCK.try_get().is_some() {
        panic!("PIT already initialized");
    }

    let clock = PIT_CLOCK.get_or_init(|| {
        let pit = Pit::new();
        Arc::new(Mutex::new(pit))
    });

    cpu::cpu().pop_cli();

    clock.clone()
}

pub struct Pit {}

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
                pit::PORT_CONTROL,
                pit::SELECT_CHANNEL_0
                    | pit::ACCESS_LOBYTE_HIBYTE
                    | pit::MODE_SQUARE_WAVE_GENERATOR
                    | pit::MODE_BINARY,
            );

            cpu::io_out(pit::PORT_CHANNEL_0, (RELOAD_VALUE & 0xFF) as u8);
            cpu::io_out(pit::PORT_CHANNEL_0, (RELOAD_VALUE >> 8) as u8);
        }

        apic::assign_io_irq(
            pit_interrupt as BasicInterruptHandler,
            pit::DEFAULT_INTERRUPT,
            cpu::cpu(),
        );

        Pit {}
    }

    fn read_counter(&self) -> u64 {
        // SAFETY: this is unsafe because it does I/O operations
        // on the hardware
        unsafe {
            cpu::io_out(
                pit::PORT_CONTROL,
                pit::SELECT_CHANNEL_0 | pit::ACCESS_LATCH_COUNT | pit::MODE_BINARY,
            );

            let lo = cpu::io_in::<u8>(pit::PORT_CHANNEL_0) as u64;
            let hi = cpu::io_in::<u8>(pit::PORT_CHANNEL_0) as u64;

            (hi << 8) | lo
        }
    }
}

impl ClockDevice for Mutex<Pit> {
    fn name(&self) -> &'static str {
        "PIT"
    }

    fn get_time(&self) -> ClockTime {
        let clock = self.lock();
        let counter = clock.read_counter();
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
    // nothing to do here really
    apic::return_from_interrupt();
}
