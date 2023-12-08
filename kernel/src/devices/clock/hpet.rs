use crate::{
    bios::{self},
    cpu::{
        self,
        idt::{InterruptAllSavedState, InterruptHandlerWithAllState},
        interrupts::apic,
    },
    memory_management::memory_layout::physical2virtual_io,
};

use super::HPET_CLOCK;

const LEGACY_PIT_IO_PORT_CONTROL: u16 = 0x43;
const LEGACY_PIT_IO_PORT_CHANNEL_0: u16 = 0x40;

const ONE_SECOND_IN_FEMTOSECONDS: u64 = 1_000_000_000_000_000;

fn disable_pit() {
    // disable PIT (timer)
    unsafe {
        // The value being written:
        // 0x32 = 0001 0000
        //        |||| ||||
        //        |||| |||+- BCD/binary mode: 0 == 16-bit binary (not important)
        //        |||| +++-- Operating mode: 0b000 == interrupt on terminal count
        //        ||++------ Access mode: 0b01 == lobyte only
        //        ++-------- Select channel: 0 == channel 0
        //
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
        cpu::io_out(LEGACY_PIT_IO_PORT_CONTROL, 0x10u8);
        cpu::io_out(LEGACY_PIT_IO_PORT_CHANNEL_0, 1u8);
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C, packed(8))]
struct HpetInterruptStatus {
    status: u32,
    reserved: u32,
}

impl HpetInterruptStatus {
    fn is_set(&self, bit: u8) -> bool {
        assert!(bit < 32);
        unsafe { (&self.status as *const u32).read_volatile() & (1 << bit) != 0 }
    }

    fn set_interrupts_iter(&self) -> impl Iterator<Item = u8> {
        let s = *self;
        (0..32).filter(move |bit| s.is_set(*bit))
    }

    fn ack(&mut self, bit: u8) {
        assert!(bit < 32);
        unsafe { (&mut self.status as *mut u32).write_volatile(1 << bit) }
    }
}

#[derive(Clone, Copy, Debug)]
struct InterruptRouteCapabilityBitmap {
    bitmap: u32,
}

#[allow(dead_code)]
impl InterruptRouteCapabilityBitmap {
    fn is_set(&self, bit: u8) -> bool {
        self.bitmap & (1 << bit) != 0
    }

    fn enabled_rounts(&self) -> impl Iterator<Item = u8> {
        let s = *self;
        (0..32).filter(move |bit| s.is_set(*bit))
    }
}

struct HpetTimerConfig {
    is_interrupt_level_trigerred: bool,
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
    fn read(raw_ptr: &u64) -> Self {
        let data = unsafe { (raw_ptr as *const u64).read_volatile() };
        Self {
            is_interrupt_level_trigerred: data & (1 << 1) != 0,
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

    fn write(self, raw_ptr: &mut u64) {
        let mut data = 0;
        if self.is_interrupt_level_trigerred {
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

        unsafe { (raw_ptr as *mut u64).write_volatile(data) };
    }
}

#[derive(Debug)]
#[repr(C, align(8))]
struct HpetTimerMmio {
    config_and_capabilities: u64,
    comparator_value: u64,
    fsb_interrupt_route: u64,
    reserved: u64,
}

impl HpetTimerMmio {
    fn config(&self) -> HpetTimerConfig {
        HpetTimerConfig::read(&self.config_and_capabilities)
    }

    fn set_config(&mut self, config: HpetTimerConfig) {
        config.write(&mut self.config_and_capabilities);
    }

    fn write_comparator_value(&mut self, value: u64) {
        unsafe { (&mut self.comparator_value as *mut u64).write_volatile(value) };
    }
}

#[derive(Debug)]
#[repr(C, align(8))]
struct HpetMmio {
    general_capabilities_id: u64,
    reserved0: u64,
    general_configuration: u64,
    reserved1: u64,
    general_interrupt_status: HpetInterruptStatus,
    reserved2: [u64; 25],
    main_counter_value: u64,
    reserved3: u64,
    timers: [HpetTimerMmio; 3],
}

#[derive(Debug)]
pub struct Hpet {
    mmio: &'static mut HpetMmio,
}

impl Hpet {
    pub fn initialize_from_bios_table(hpet: &bios::tables::Hpet) -> Option<Self> {
        disable_pit();

        assert!(hpet.base_address.address_space_id == 0); // memory space
        let mmio =
            unsafe { &mut *(physical2virtual_io(hpet.base_address.address as _) as *mut HpetMmio) };

        // enable the timer
        let mut s = Self { mmio };
        let clock_period = s.counter_clock_period();

        // setup interrupts for the first timer only for now
        let timer = &mut s.mmio.timers[0];
        let mut config = timer.config();
        assert!(config.is_periodic_capable); // must be periodic capable
        assert!(config.is_64bit_capable); // must be 64-bit capable

        config.is_interrupt_level_trigerred = true;
        config.interrupt_enabled = true;
        config.is_periodic = true; // periodic
        config.force_32bit_mode = false; // don't force 32-bit mode
        config.interrupt_via_fsb = false; // don't use FSB
        let interrupt_route = config
            .interrupt_route_capabilities
            .enabled_rounts()
            .next()
            .unwrap();
        config.interrupt_route = interrupt_route;
        config.timer_set_value = true; // write the timer value
        timer.set_config(config);
        timer.write_comparator_value(ONE_SECOND_IN_FEMTOSECONDS / clock_period);
        timer.write_comparator_value(ONE_SECOND_IN_FEMTOSECONDS / clock_period);

        // setup ioapic
        apic::assign_io_irq_custom(
            timer0_handler as InterruptHandlerWithAllState,
            interrupt_route,
            cpu::cpu(),
            |entry| entry.with_trigger_mode_level(true),
        );

        s.set_enabled(true);
        // use normal routing
        s.set_enable_legacy_replacement_route(false);

        Some(s)
    }

    fn read_general_configuration(&self) -> u64 {
        unsafe { (&self.mmio.general_configuration as *const u64).read_volatile() }
    }

    fn write_general_configuration(&mut self, value: u64) {
        unsafe { (&mut self.mmio.general_configuration as *mut u64).write_volatile(value) }
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
        (self.mmio.general_capabilities_id >> 32) & 0xFFFFFFFF
    }

    #[allow(dead_code)]
    fn current_counter(&self) -> u64 {
        // Safety: we know that the counter is 64-bit, aligned, valid pointer
        unsafe { (&self.mmio.main_counter_value as *const u64).read_volatile() }
    }

    fn status_interrupts_iter(&self) -> impl Iterator<Item = u8> {
        self.mmio.general_interrupt_status.set_interrupts_iter()
    }

    fn ack_interrupt(&mut self, interrupt: u8) {
        self.mmio.general_interrupt_status.ack(interrupt);
    }
}

extern "cdecl" fn timer0_handler(_all_state: &mut InterruptAllSavedState) {
    let mut clock = HPET_CLOCK.get().as_ref().unwrap().lock();

    let interrupt = clock.status_interrupts_iter().next().unwrap();

    // clear the interrupt (must for level triggered interrupts)
    clock.ack_interrupt(interrupt);

    apic::return_from_interrupt();
}
