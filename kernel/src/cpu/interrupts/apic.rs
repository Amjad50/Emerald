use core::borrow::{Borrow, BorrowMut};

use alloc::vec::Vec;

use crate::{
    bios::tables::{
        self, BiosTables, DescriptorTableBody, InterruptControllerStruct, InterruptSourceOverride,
    },
    cpu::{self, idt::InterruptStackFrame64, Cpu, CPUID_FN_FEAT, CPUS, MAX_CPUS},
    memory_management::memory_layout::physical2virtual_io,
    sync::spin::mutex::Mutex,
};

use super::{
    allocate_basic_user_interrupt, allocate_user_interrupt, allocate_user_interrupt_all_saved,
    InterruptHandler,
};

const CPUID_FEAT_EDX_APIC: u32 = 1 << 9;

const APIC_BAR_ENABLED: u64 = 1 << 11;
const APIC_BASE_MASK: u64 = 0xFFFF_FFFF_FFFF_F000;
const DEFAULT_APIC_BASE: usize = 0xFEE0_0000;

static mut APIC: Mutex<Apic> = Mutex::new(Apic::empty());

pub fn init(bios_tables: &BiosTables) {
    disable_pic();
    unsafe {
        APIC.lock().init(bios_tables);
    }
}

fn disable_pic() {
    unsafe {
        cpu::io_out::<u8>(0x21, 0xFF);
        cpu::io_out::<u8>(0xA1, 0xFF);
    }
}

pub fn return_from_interrupt() {
    unsafe {
        APIC.lock().return_from_interrupt();
    }
}

pub fn assign_io_irq<H: InterruptHandler>(handler: H, interrupt_num: u8, cpu: &Cpu) {
    unsafe { APIC.lock().assign_io_irq(handler, interrupt_num, cpu) }
}

pub fn assign_io_irq_custom<H: InterruptHandler, F>(
    handler: H,
    interrupt_num: u8,
    cpu: &Cpu,
    modify_entry: F,
) where
    F: FnOnce(IoApicRedirectionBuilder) -> IoApicRedirectionBuilder,
{
    unsafe {
        APIC.lock()
            .assign_io_irq_custom(handler, interrupt_num, cpu, modify_entry)
    }
}

#[repr(C, align(4))]
struct ApicReg {
    reg: u32,
    pad: [u32; 3],
}

impl ApicReg {
    fn write(&mut self, value: u32) {
        unsafe { (self.reg.borrow_mut() as *mut u32).write_volatile(value) };
    }

    fn read(&self) -> u32 {
        unsafe { (self.reg.borrow() as *const u32).read_volatile() }
    }
}

#[repr(C)]
struct LocalVectorRegister {
    reg: ApicReg,
}

#[allow(dead_code)]
impl LocalVectorRegister {
    fn read(&self) -> LocalVectorRegisterBuilder {
        LocalVectorRegisterBuilder {
            reg: self.reg.read(),
        }
    }

    fn write(&mut self, builder: LocalVectorRegisterBuilder) {
        self.reg.write(builder.reg)
    }
}

const LVT_VECTOR_MASK: u32 = 0xFF;
const LVT_MESSAGE_TYPE_MASK: u32 = 0x7 << 8;
const LVT_TRIGGER_MODE_MASK: u32 = 1 << 15;
const LVT_MASK_MASK: u32 = 1 << 16;
const LVT_TIMER_MODE_MASK: u32 = 1 << 17;

const SPURIOUS_ENABLE: u32 = 1 << 8;

#[derive(Default, Clone, Copy)]
struct LocalVectorRegisterBuilder {
    reg: u32,
}

#[allow(dead_code)]
impl LocalVectorRegisterBuilder {
    fn with_vector(mut self, vector: u8) -> Self {
        self.reg = (self.reg & !LVT_VECTOR_MASK) | vector as u32;
        self
    }

    fn with_message_type(mut self, message_type: u8) -> Self {
        self.reg = (self.reg & !LVT_MESSAGE_TYPE_MASK) | ((message_type & 0x7) as u32) << 8;
        self
    }

    fn with_trigger_mode(mut self, trigger_mode: bool) -> Self {
        self.reg = (self.reg & !LVT_TRIGGER_MODE_MASK) | (trigger_mode as u32) << 15;
        self
    }

    fn with_mask(mut self, mask: bool) -> Self {
        self.reg = (self.reg & !LVT_MASK_MASK) | (mask as u32) << 16;
        self
    }

    fn with_periodic_timer(mut self, timer_mode: bool) -> Self {
        self.reg = (self.reg & !LVT_TIMER_MODE_MASK) | (timer_mode as u32) << 17;
        self
    }
}

#[repr(C, align(16))]
struct ApicMmio {
    _pad1: [ApicReg; 2],
    id: ApicReg,
    version: ApicReg,
    _pad2: [ApicReg; 4],
    task_priority: ApicReg,
    arbitration_priority: ApicReg,
    processor_priority: ApicReg,
    end_of_interrupt: ApicReg,
    remote_read: ApicReg,
    logical_destination: ApicReg,
    destination_format: ApicReg,
    spurious_interrupt_vector: ApicReg,
    in_service: [ApicReg; 8],
    trigger_mode: [ApicReg; 8],
    interrupt_request: [ApicReg; 8],
    error_status: ApicReg,
    _pad3: [ApicReg; 7],
    interrupt_command_low: ApicReg,
    interrupt_command_high: ApicReg,
    timer_local_vector_table: LocalVectorRegister,
    thermal_local_vector_table: LocalVectorRegister,
    performance_local_vector_table: LocalVectorRegister,
    lint0_local_vector_table: LocalVectorRegister,
    lint1_local_vector_table: LocalVectorRegister,
    error_local_vector_table: LocalVectorRegister,
    timer_initial_count: ApicReg,
    timer_current_count: ApicReg,
    _pad4: [ApicReg; 4],
    timer_divide_configuration: ApicReg,
    _pad5: ApicReg,
    extended_apic_features: ApicReg,
    extended_apic_control: ApicReg,
    specifc_end_of_interrupt: ApicReg,
    _pad6: [ApicReg; 5],
    interrupt_enable: [ApicReg; 8],
    extended_interrupt_local_vector_tables: [ApicReg; 4],
}

#[allow(dead_code)]
mod io_apic {
    pub const IO_APIC_ID: u32 = 0;
    pub const IO_APIC_VERSION: u32 = 1;
    pub const IO_APIC_ARBITRATION_ID: u32 = 2;
    pub const IO_APIC_REDIRECTION_TABLE: u32 = 0x10;

    pub const RDR_VECTOR_MASK: u64 = 0xFF;
    pub const RDR_DELIVERY_MODE_MASK: u64 = 0x7 << 8;
    pub const RDR_DESTINATION_MODE_MASK: u64 = 1 << 11;
    pub const RDR_DELIVERY_STATUS_MASK: u64 = 1 << 12;
    pub const RDR_PIN_POLARITY_MASK: u64 = 1 << 13;
    pub const RDR_REMOTE_IRR_MASK: u64 = 1 << 14;
    pub const RDR_TRIGGER_MODE_MASK: u64 = 1 << 15;
    pub const RDR_MASK_MASK: u64 = 1 << 16;
    pub const RDR_DESTINATION_PHYSICAL_MASK: u64 = 0x1F << 59;
    pub const RDR_DESTINATION_LOGICAL_MASK: u64 = 0xFF << 56;
}

#[allow(dead_code)]
enum DestinationType {
    Physical(u8),
    Logical(u8),
}

#[derive(Default, Clone, Copy)]
pub struct IoApicRedirectionBuilder {
    reg: u64,
}

#[allow(dead_code)]
impl IoApicRedirectionBuilder {
    fn with_vector(mut self, vector: u8) -> Self {
        self.reg = (self.reg & !io_apic::RDR_VECTOR_MASK) | vector as u64;
        self
    }

    fn with_delivery_mode(mut self, delivery_mode: u8) -> Self {
        self.reg =
            (self.reg & !io_apic::RDR_DELIVERY_MODE_MASK) | ((delivery_mode & 0x7) as u64) << 8;
        self
    }

    pub fn with_interrupt_polartiy_low(mut self, polarity: bool) -> Self {
        self.reg = (self.reg & !io_apic::RDR_PIN_POLARITY_MASK) | (polarity as u64) << 13;
        self
    }

    pub fn with_trigger_mode_level(mut self, trigger_mode: bool) -> Self {
        self.reg = (self.reg & !io_apic::RDR_TRIGGER_MODE_MASK) | (trigger_mode as u64) << 15;
        self
    }

    pub fn with_mask(mut self, mask: bool) -> Self {
        self.reg = (self.reg & !io_apic::RDR_MASK_MASK) | (mask as u64) << 16;
        self
    }

    fn with_destination(mut self, destination: DestinationType) -> Self {
        match destination {
            DestinationType::Physical(d) => {
                assert!(d < 32, "physical destination is out of range");
                // clear the destination mode bit
                self.reg &= !io_apic::RDR_DESTINATION_MODE_MASK;
                self.reg = (self.reg & !io_apic::RDR_DESTINATION_PHYSICAL_MASK)
                    | (((d & 0x1F) as u64) << 59);
            }
            DestinationType::Logical(d) => {
                // set the destination mode bit
                self.reg |= io_apic::RDR_DESTINATION_MODE_MASK;
                self.reg = (self.reg & !io_apic::RDR_DESTINATION_LOGICAL_MASK) | ((d as u64) << 56);
            }
        }

        self
    }
}

#[repr(C, align(16))]
struct IoApicMmio {
    register_select: ApicReg,
    data: ApicReg,
    irq_pin_assersion: ApicReg,
    _pad: ApicReg,
    return_of_interrupt: ApicReg,
}

impl IoApicMmio {
    pub fn read_register(&mut self, register: u32) -> u32 {
        self.register_select.write(register);
        self.data.read()
    }

    pub fn write_register(&mut self, register: u32, value: u32) {
        self.register_select.write(register);
        self.data.write(value);
    }
}

#[allow(dead_code)]
struct IoApic {
    id: u8,
    global_irq_base: u32,
    mmio: *mut IoApicMmio,
}

impl IoApic {
    fn reset_all_interrupts(&mut self) {
        for i in 0..24 {
            let b = IoApicRedirectionBuilder::default()
                .with_vector(0)
                .with_mask(true);
            self.write_redirect_entry(i, b);
        }
    }

    #[allow(dead_code)]
    fn read_register(&self, register: u32) -> u32 {
        unsafe { (*self.mmio).read_register(register) }
    }

    fn write_register(&self, register: u32, value: u32) {
        unsafe { (*self.mmio).write_register(register, value) }
    }

    fn write_redirect_entry(&mut self, entry: u8, builder: IoApicRedirectionBuilder) {
        let lo = builder.reg as u32;
        let hi = (builder.reg >> 32) as u32;
        self.write_register(io_apic::IO_APIC_REDIRECTION_TABLE + entry as u32 * 2, lo);
        self.write_register(
            io_apic::IO_APIC_REDIRECTION_TABLE + entry as u32 * 2 + 1,
            hi,
        );
    }
}

impl From<tables::IoApic> for IoApic {
    fn from(table: tables::IoApic) -> Self {
        assert!(table.io_apic_address != 0, "IO APIC address is 0");
        assert!(
            table.io_apic_address & 0xF == 0,
            "IO APIC address is not aligned"
        );
        Self {
            id: table.io_apic_id,
            global_irq_base: table.global_system_interrupt_base,
            mmio: physical2virtual_io(table.io_apic_address as _) as *mut IoApicMmio,
        }
    }
}

struct Apic {
    mmio: *mut ApicMmio,
    n_cpus: usize,
    io_apics: Vec<IoApic>,
    source_overrides: Vec<InterruptSourceOverride>,
}

impl Apic {
    const fn empty() -> Self {
        Self {
            // we should call `init` first, but this is just in case.
            mmio: physical2virtual_io(DEFAULT_APIC_BASE) as *mut ApicMmio,
            n_cpus: 0,
            io_apics: Vec::new(),
            source_overrides: Vec::new(),
        }
    }

    fn init(&mut self, bios_tables: &BiosTables) {
        // do we have APIC in this cpu?
        let cpuid = unsafe { cpu::cpuid!(CPUID_FN_FEAT) };
        if cpuid.edx & CPUID_FEAT_EDX_APIC == 0 {
            panic!("APIC is not supported");
        }
        let apic_bar = unsafe { cpu::msr::read(cpu::msr::APIC_BASE) };
        if apic_bar & APIC_BAR_ENABLED == 0 {
            // enable APIC
            unsafe {
                cpu::msr::write(cpu::msr::APIC_BASE, apic_bar | APIC_BAR_ENABLED);
            }
            // recheck
            let apic_bar = unsafe { cpu::msr::read(cpu::msr::APIC_BASE) };
            if apic_bar & APIC_BAR_ENABLED == 0 {
                panic!("APIC is not enabled");
            }
        }
        let mut apic_address = (apic_bar & APIC_BASE_MASK) as usize;

        // process the MADT table
        let madt_table = bios_tables
            .rsdt
            .entries
            .iter()
            .find_map(|entry| {
                if let DescriptorTableBody::Apic(apic) = &entry.body {
                    Some(apic)
                } else {
                    None
                }
            })
            .expect("MADT table not found");

        if madt_table.local_apic_address as usize != apic_address {
            println!(
                "WARNING: MADT table has a different APIC address (CPU:{:X}, MADT:{:X}), using MADT...",
                apic_address, madt_table.local_apic_address
            );
            apic_address = madt_table.local_apic_address as usize;
        }

        for strct in &madt_table.interrupt_controller_structs {
            match strct {
                InterruptControllerStruct::ProcessorLocalApic(s) => {
                    if s.flags & 1 == 0 {
                        // this is a disabled processor
                        continue;
                    }
                    if self.n_cpus >= MAX_CPUS {
                        println!(
                            "WARNING: too many CPUs, have {MAX_CPUS} already, ignoring the rest"
                        );
                    } else {
                        // initialize the CPUs
                        // SAFETY: this is safe
                        unsafe {
                            CPUS[self.n_cpus].init(self.n_cpus, s.apic_id);
                        }
                        self.n_cpus += 1;
                    }
                }
                InterruptControllerStruct::IoApic(s) => {
                    self.io_apics.push(s.clone().into());
                }
                InterruptControllerStruct::InterruptSourceOverride(s) => {
                    self.source_overrides.push(s.clone());
                }
                InterruptControllerStruct::NonMaskableInterrupt(_) => todo!(),
                InterruptControllerStruct::LocalApicNmi(s) => {
                    // for now, make sure we have the values we need
                    assert!(s.acpi_processor_uid == 0xFF);
                    assert!(s.flags == 0);
                    assert!(s.local_apic_lint == 1);
                }
                InterruptControllerStruct::LocalApicAddressOverride(s) => {
                    apic_address = s.local_apic_address as usize;
                }
                InterruptControllerStruct::Unknown { struct_type, bytes } => {
                    println!(
                        "WARNING: unknown interrupt controller struct type {:#X} with {:#X} bytes",
                        struct_type,
                        bytes.len()
                    );
                }
            }
        }

        assert!(
            self.n_cpus > 0,
            "no CPUs found in the MADT table, cannot continue"
        );
        assert!(
            !self.io_apics.is_empty(),
            "no IO APICs found in the MADT table, cannot continue"
        );
        assert!(apic_address != 0, "APIC address is 0, cannot continue");
        assert!(apic_address & 0xF == 0, "APIC address is not aligned");
        self.mmio = physical2virtual_io(apic_address) as *mut ApicMmio;

        // reset all interrupts
        self.io_apics.iter_mut().for_each(|io_apic| {
            io_apic.reset_all_interrupts();
        });

        self.initialize_spurious_interrupt();
        self.disable_local_interrupts();
        self.initialize_timer();
        self.setup_error_interrupt();
        // ack any pending interrupts
        self.return_from_interrupt();
    }

    fn return_from_interrupt(&self) {
        unsafe {
            (*self.mmio).end_of_interrupt.write(0);
        }
    }

    fn initialize_spurious_interrupt(&mut self) {
        let interrupt_num = allocate_basic_user_interrupt(spurious_handler);
        unsafe {
            // 1 << 8, to enable spurious interrupts
            (*self.mmio)
                .spurious_interrupt_vector
                .write(SPURIOUS_ENABLE | interrupt_num as u32);
        }
    }

    /// disable the Local interrupts 0 and 1
    fn disable_local_interrupts(&mut self) {
        unsafe {
            let vector_table = LocalVectorRegisterBuilder::default().with_mask(true);
            (*self.mmio).lint0_local_vector_table.write(vector_table);
            (*self.mmio).lint1_local_vector_table.write(vector_table);
        }
    }

    fn initialize_timer(&mut self) {
        let interrupt_num = allocate_user_interrupt_all_saved(super::handlers::apic_timer_handler);

        unsafe {
            // divide by 1
            (*self.mmio).timer_divide_configuration.write(0b1011);
            // just random value, this is based on the CPU clock speed
            // so its not accurate timing.
            (*self.mmio).timer_initial_count.write(0x1000000);
            // periodic mode, not masked, and with the allocated vector number
            let vector_table = LocalVectorRegisterBuilder::default()
                .with_periodic_timer(true)
                .with_mask(false)
                .with_vector(interrupt_num);
            (*self.mmio).timer_local_vector_table.write(vector_table);
        }
    }

    fn setup_error_interrupt(&mut self) {
        // clear the error status and write 0 to it
        unsafe {
            // 1- clear the error status
            (*self.mmio).error_status.write(0);
            // 2- write 0 to it (yes, we have to do this twice)
            (*self.mmio).error_status.write(0);
        }

        let interrupt_num = allocate_basic_user_interrupt(error_interrupt_handler);
        unsafe {
            // not masked, and with the allocated vector number
            let vector_table = LocalVectorRegisterBuilder::default()
                .with_mask(false)
                .with_vector(interrupt_num);
            (*self.mmio).error_local_vector_table.write(vector_table);
        }
    }

    fn assign_io_irq<H: InterruptHandler>(&mut self, handler: H, irq_num: u8, cpu: &Cpu) {
        self.assign_io_irq_custom(handler, irq_num, cpu, |b| b)
    }

    fn assign_io_irq_custom<H: InterruptHandler, F>(
        &mut self,
        handler: H,
        irq_num: u8,
        cpu: &Cpu,
        modify_entry: F,
    ) where
        F: FnOnce(IoApicRedirectionBuilder) -> IoApicRedirectionBuilder,
    {
        assert!(cpu.id < self.n_cpus, "CPU ID is out of range");
        assert!(irq_num < 24, "interrupt number is out of range");

        // if we have override mapping for this interrupt, use it.
        let int_override = self
            .source_overrides
            .iter()
            .find(|int_override| int_override.source == irq_num);
        let mut interrupt_num = irq_num as u32;
        if let Some(int_override) = int_override {
            interrupt_num = int_override.global_system_interrupt;
        }
        let io_apic = self
            .io_apics
            .iter_mut()
            .find(|io_apic| {
                io_apic.global_irq_base <= interrupt_num
                    && interrupt_num < io_apic.global_irq_base + 24
            })
            .expect("Could not find IO APIC for the interrupt");

        // the location of where we want to
        let entry_in_ioapic = interrupt_num - io_apic.global_irq_base;

        let vector_num = allocate_user_interrupt(handler);

        let b = IoApicRedirectionBuilder::default()
            .with_vector(vector_num)
            .with_delivery_mode(0) // fixed
            .with_interrupt_polartiy_low(false) // active high
            .with_trigger_mode_level(false) // edge
            .with_mask(false) // not masked
            .with_destination(DestinationType::Physical(cpu.apic_id));

        let b = modify_entry(b);

        io_apic.write_redirect_entry(entry_in_ioapic as u8, b);
    }
}

extern "x86-interrupt" fn spurious_handler(_frame: InterruptStackFrame64) {
    println!("Spurious interrupt");
    return_from_interrupt();
}

extern "x86-interrupt" fn error_interrupt_handler(_frame: InterruptStackFrame64) {
    let error_status = unsafe { (*APIC.lock().mmio).error_status.read() };
    println!("APIC error: {:#X}", error_status);
    // clear the error
    unsafe {
        (*APIC.lock().mmio).error_status.write(0);
    }
    return_from_interrupt();
}
