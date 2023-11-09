use alloc::vec::Vec;

use crate::{
    bios::{
        self,
        tables::{self, DescriptorTableBody, InterruptControllerStruct, InterruptSourceOverride},
    },
    cpu::{self, idt::InterruptStackFrame64, outb, CPUID_FN_FEAT, CPUS, MAX_CPUS},
    memory_management::memory_layout::physical2virtual_io,
};

use super::allocate_user_interrupt;

const CPUID_FEAT_EDX_APIC: u32 = 1 << 9;

const APIC_BAR_MSR: u32 = 0x1B;
const APIC_BAR_ENABLED: u64 = 1 << 11;
const APIC_BASE_MASK: u64 = 0xFFFF_FFFF_FFFF_F000;
const DEFAULT_APIC_BASE: usize = 0xFEE0_0000;

static mut APIC: Apic = Apic::empty();

pub fn init() {
    disable_pic();
    unsafe {
        APIC.init();
    }
}

pub fn return_from_interrupt() {
    unsafe {
        APIC.return_from_interrupt();
    }
}

fn disable_pic() {
    unsafe {
        outb(0x21, 0xFF);
        outb(0xA1, 0xFF);
    }
}

#[repr(C, align(4))]
struct ApicReg {
    reg: u32,
    pad: [u32; 3],
}

impl ApicReg {
    fn write(&mut self, value: u32) {
        self.reg = value;
    }

    fn read(&self) -> u32 {
        self.reg
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

#[derive(Default)]
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

    fn with_timer_mode(mut self, timer_mode: bool) -> Self {
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

#[repr(C, packed)]
struct IoApicMmio {}

#[allow(dead_code)]
struct IoApic {
    id: u8,
    global_irq_base: u32,
    mmio: &'static mut IoApicMmio,
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
            mmio: unsafe {
                &mut *(physical2virtual_io(table.io_apic_address as _) as *mut IoApicMmio)
            },
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

    fn init(&mut self) {
        // do we have APIC in this cpu?
        let cpuid = unsafe { cpu::cpuid!(CPUID_FN_FEAT) };
        if cpuid.edx & CPUID_FEAT_EDX_APIC == 0 {
            panic!("APIC is not supported");
        }
        let apic_bar = unsafe { cpu::rdmsr(APIC_BAR_MSR) };
        if apic_bar & APIC_BAR_ENABLED == 0 {
            // enable APIC
            unsafe {
                cpu::wrmsr(APIC_BAR_MSR, apic_bar | APIC_BAR_ENABLED);
            }
            // recheck
            let apic_bar = unsafe { cpu::rdmsr(APIC_BAR_MSR) };
            if apic_bar & APIC_BAR_ENABLED == 0 {
                panic!("APIC is not enabled");
            }
        }
        let mut apic_address = (apic_bar & APIC_BASE_MASK) as usize;

        let bios_tables = bios::tables::get_bios_tables().unwrap();

        // process the MADT table
        let madt_table = bios_tables
            .rsdt
            .entries
            .into_iter()
            .find_map(|entry| {
                if let DescriptorTableBody::Apic(apic) = entry.body {
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

        for strct in madt_table.interrupt_controller_structs {
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
                    self.io_apics.push(s.into());
                }
                InterruptControllerStruct::InterruptSourceOverride(s) => {
                    self.source_overrides.push(s);
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

        self.disable_spurious_interrupt_vector();
        self.initialize_timer();
    }

    fn disable_spurious_interrupt_vector(&mut self) {
        unsafe {
            (*self.mmio).spurious_interrupt_vector.write(0x1FF);
        }
    }

    fn initialize_timer(&mut self) {
        let interrupt_num = allocate_user_interrupt(timer_handler);

        unsafe {
            // divide by 1
            (*self.mmio).timer_divide_configuration.write(0b1011);
            // just random value, this is based on the CPU clock speed
            // so its not accurate timing.
            (*self.mmio).timer_initial_count.write(0x1000000);
            // periodic mode, not masked, and with the allocated vector number
            let vector_table = LocalVectorRegisterBuilder::default()
                .with_timer_mode(true)
                .with_mask(false)
                .with_vector(interrupt_num);
            (*self.mmio).timer_local_vector_table.write(vector_table);
        }
    }

    fn return_from_interrupt(&self) {
        unsafe {
            (*self.mmio).end_of_interrupt.write(0);
        }
    }
}

extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame64) {
    return_from_interrupt();
}
