use alloc::sync::Arc;
use tracing::warn;

use crate::{
    acpi,
    cpu::{
        self,
        idt::{InterruptAllSavedState, InterruptHandlerWithAllState},
        interrupts::apic,
    },
    devices::clock::{hardware_timer::pit, ClockTime, FEMTOS_PER_SEC, NANOS_PER_FEMTO},
    memory_management::virtual_space::VirtualSpace,
    sync::{once::OnceLock, spin::mutex::Mutex},
    utils::vcell::{RO, RW, WO},
};

use super::super::ClockDevice;

static HPET_CLOCK: OnceLock<Arc<Mutex<Hpet>>> = OnceLock::new();

pub fn init(hpet_table: &acpi::tables::Hpet) -> Arc<Mutex<Hpet>> {
    // make sure we don't get interrupted before `HPET_CLOCK`
    // is initialized
    cpu::cpu().push_cli();

    // just to make sure that we don't initialize it twice
    if HPET_CLOCK.try_get().is_some() {
        panic!("HPET already initialized");
    }

    let clock = HPET_CLOCK.get_or_init(|| {
        // only executed once
        let hpet = Hpet::new(hpet_table);
        Arc::new(Mutex::new(hpet))
    });

    cpu::cpu().pop_cli();

    clock.clone()
}

#[repr(C, packed(8))]
struct HpetInterruptStatus {
    status: RW<u32>,
    reserved: u32,
}

impl HpetInterruptStatus {
    fn set_interrupts_iter(&self) -> impl Iterator<Item = u8> {
        let s = self.status.read();
        (0..32).filter(move |bit| s & (1 << bit) != 0)
    }

    fn ack(&mut self, bit: u8) {
        assert!(bit < 32);
        unsafe { self.status.write(1 << bit) }
    }
}

#[derive(Clone, Copy, Debug)]
struct InterruptRouteCapabilityBitmap {
    bitmap: u32,
}

impl InterruptRouteCapabilityBitmap {
    fn is_set(&self, bit: u8) -> bool {
        self.bitmap & (1 << bit) != 0
    }

    fn enabled_routes(&self) -> impl Iterator<Item = u8> {
        let s = *self;
        (0..32).filter(move |bit| s.is_set(*bit))
    }
}

struct HpetTimerConfig {
    is_interrupt_level_triggered: bool,
    interrupt_enabled: bool,
    is_periodic: bool,
    is_periodic_capable: bool,
    is_64bit_capable: bool,
    timer_set_value: bool,
    force_32bit_mode: bool,
    interrupt_route: u8,
    interrupt_via_fsb: bool,
    fsb_capable: bool,
    interrupt_route_capabilities: InterruptRouteCapabilityBitmap,
}

impl HpetTimerConfig {
    fn new(data: u64) -> Self {
        Self {
            is_interrupt_level_triggered: data & (1 << 1) != 0,
            interrupt_enabled: data & (1 << 2) != 0,
            is_periodic: data & (1 << 3) != 0,
            is_periodic_capable: data & (1 << 4) != 0,
            is_64bit_capable: data & (1 << 5) != 0,
            timer_set_value: data & (1 << 6) != 0,
            force_32bit_mode: data & (1 << 7) != 0,
            interrupt_route: ((data >> 9) & 0x1F) as u8,
            interrupt_via_fsb: data & (1 << 14) != 0,
            fsb_capable: data & (1 << 15) != 0,
            interrupt_route_capabilities: InterruptRouteCapabilityBitmap {
                bitmap: ((data >> 32) & 0xFFFFFFFF) as u32,
            },
        }
    }

    fn as_u64(&self) -> u64 {
        let mut data = 0;
        if self.is_interrupt_level_triggered {
            data |= 1 << 1;
        }
        if self.interrupt_enabled {
            data |= 1 << 2;
        }
        if self.is_periodic {
            data |= 1 << 3;
        }
        if self.is_periodic_capable {
            data |= 1 << 4;
        }
        if self.is_64bit_capable {
            data |= 1 << 5;
        }
        if self.timer_set_value {
            data |= 1 << 6;
        }
        if self.force_32bit_mode {
            data |= 1 << 7;
        }
        data |= (self.interrupt_route as u64) << 9;
        if self.interrupt_via_fsb {
            data |= 1 << 14;
        }
        if self.fsb_capable {
            data |= 1 << 15;
        }
        data |= (self.interrupt_route_capabilities.bitmap as u64) << 32;

        data
    }
}

#[repr(C, align(8))]
struct HpetTimerMmio {
    config_and_capabilities: RW<u64>,
    comparator_value: WO<u64>,
    fsb_interrupt_route: RO<u64>,
    reserved: u64,
}

impl HpetTimerMmio {
    fn config(&self) -> HpetTimerConfig {
        HpetTimerConfig::new(self.config_and_capabilities.read())
    }

    fn set_config(&mut self, config: HpetTimerConfig) {
        unsafe { self.config_and_capabilities.write(config.as_u64()) };
    }

    fn write_comparator_value(&mut self, value: u64) {
        unsafe { self.comparator_value.write(value) };
    }
}

#[repr(C, align(8))]
struct HpetMmio {
    general_capabilities_id: RO<u64>,
    reserved0: u64,
    general_configuration: RW<u64>,
    reserved1: u64,
    general_interrupt_status: HpetInterruptStatus,
    reserved2: [u64; 25],
    main_counter_value: RO<u64>,
    reserved3: u64,
    timers: [HpetTimerMmio; 3],
}

pub struct Hpet {
    mmio: VirtualSpace<HpetMmio>,
}

impl Hpet {
    fn new(hpet: &acpi::tables::Hpet) -> Self {
        pit::disable();
        assert_eq!(hpet.base_address.address_space_id, 0); // memory space
        let mmio = unsafe { VirtualSpace::new(hpet.base_address.address).unwrap() };

        // enable the timer
        let mut s = Self { mmio };
        let clock_period = s.counter_clock_period();

        // setup interrupts for the first timer only for now
        let timer = &mut s.mmio.timers[0];
        let mut config = timer.config();
        assert!(config.is_periodic_capable); // must be periodic capable
        assert!(config.is_64bit_capable); // must be 64-bit capable

        config.is_interrupt_level_triggered = false;
        config.interrupt_enabled = true;
        config.is_periodic = true; // periodic
        config.force_32bit_mode = false; // don't force 32-bit mode
        config.interrupt_via_fsb = false; // don't use FSB
        let available_routes = config.interrupt_route_capabilities.enabled_routes();

        let mut first_available_route = None;
        let mut above_15_route = None;
        // check if we have available routes that are higher than 15, which
        // is the range of legacy ISA interrupts.
        // if we have any above those, it's best to use them
        // otherwise, we will use the first available route
        for route in available_routes {
            if first_available_route.is_none() && !apic::is_irq_assigned(route) {
                // we can use this route
                first_available_route = Some(route);
            }
            if above_15_route.is_none() && route > 15 {
                above_15_route = Some(route);
            }
            if first_available_route.is_some() && above_15_route.is_some() {
                break;
            }
        }

        let chosen_route = above_15_route
            .or(first_available_route)
            .expect("No available HPET route");

        config.interrupt_route = chosen_route;
        config.timer_set_value = true; // write the timer value
        timer.set_config(config);
        timer.write_comparator_value(FEMTOS_PER_SEC / clock_period);
        timer.write_comparator_value(FEMTOS_PER_SEC / clock_period);

        // setup ioapic
        apic::assign_io_irq(
            timer0_handler as InterruptHandlerWithAllState,
            chosen_route,
            cpu::cpu(),
        );

        s.set_enabled(true);
        // use normal routing
        s.set_enable_legacy_replacement_route(false);

        s
    }

    fn read_general_configuration(&self) -> u64 {
        self.mmio.general_configuration.read()
    }

    fn write_general_configuration(&mut self, value: u64) {
        unsafe { self.mmio.general_configuration.write(value) };
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        let mut config = self.read_general_configuration();
        if enabled {
            config |= 1;
        } else {
            config &= !1;
        }
        self.write_general_configuration(config);
    }

    pub fn set_enable_legacy_replacement_route(&mut self, enabled: bool) {
        let mut config = self.read_general_configuration();
        if enabled {
            config |= 1 << 1;
        } else {
            config &= !(1 << 1);
        }
        self.write_general_configuration(config);
    }

    /// Returns the number of femtoseconds per counter tick
    fn counter_clock_period(&self) -> u64 {
        (self.mmio.general_capabilities_id.read() >> 32) & 0xFFFFFFFF
    }

    fn current_counter(&self) -> u64 {
        // Safety: we know that the counter is 64-bit, aligned, valid pointer
        self.mmio.main_counter_value.read()
    }

    fn status_interrupts_iter(&self) -> impl Iterator<Item = u8> {
        self.mmio.general_interrupt_status.set_interrupts_iter()
    }

    fn ack_interrupt(&mut self, interrupt: u8) {
        self.mmio.general_interrupt_status.ack(interrupt);
    }
}

impl ClockDevice for Mutex<Hpet> {
    fn name(&self) -> &'static str {
        "HPET"
    }

    fn get_time(&self) -> ClockTime {
        let clock = self.lock();
        let counter = clock.current_counter();
        let femtos_per_tick = clock.counter_clock_period();
        let nanos_per_tick = femtos_per_tick / NANOS_PER_FEMTO;
        let seconds_divider = FEMTOS_PER_SEC / femtos_per_tick;
        let seconds = counter / seconds_divider;
        let nanoseconds = (counter % seconds_divider) * nanos_per_tick;

        ClockTime {
            seconds,
            nanoseconds,
        }
    }

    fn granularity(&self) -> u64 {
        let granularity = self.lock().counter_clock_period() / NANOS_PER_FEMTO;
        if granularity == 0 {
            1
        } else {
            granularity
        }
    }

    fn require_calibration(&self) -> bool {
        false
    }

    fn rating(&self) -> u64 {
        50
    }
}

extern "C" fn timer0_handler(_all_state: &mut InterruptAllSavedState) {
    let mut clock = HPET_CLOCK.get().as_ref().lock();

    // if we are level triggered, we must clear the interrupt bit
    if clock.mmio.timers[0].config().is_interrupt_level_triggered {
        if let Some(interrupt) = clock.status_interrupts_iter().next() {
            // clear the interrupt (must for level triggered interrupts)
            clock.ack_interrupt(interrupt);
        } else {
            warn!("Looks like we are getting PIT interrupt instead of HPET");
        }
    }

    apic::return_from_interrupt();
}
