use alloc::vec::Vec;

use crate::{
    bios::{
        self,
        tables::{self, DescriptorTableBody, InterruptControllerStruct, InterruptSourceOverride},
    },
    cpu::{self, outb, CPUID_FN_FEAT, CPUS, MAX_CPUS},
    memory_management::memory_layout::physical2virtual_io,
};

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

fn disable_pic() {
    unsafe {
        outb(0x21, 0xFF);
        outb(0xA1, 0xFF);
    }
}

#[repr(C, packed)]
struct ApicMmio {}

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
    mmio: &'static mut ApicMmio,
    n_cpus: usize,
    io_apics: Vec<IoApic>,
    source_overrides: Vec<InterruptSourceOverride>,
}

impl Apic {
    const fn empty() -> Self {
        Self {
            // we should call `init` first, but this is just in case.
            mmio: unsafe { &mut *(physical2virtual_io(DEFAULT_APIC_BASE) as *mut ApicMmio) },
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
                    if self.n_cpus > MAX_CPUS {
                        println!(
                            "WARNING: too many CPUs, have {MAX_CPUS} already, ignoring the rest"
                        );
                    }
                    // initialize the CPUs
                    // SAFETY: this is safe
                    unsafe {
                        CPUS[self.n_cpus].id = self.n_cpus;
                        CPUS[self.n_cpus].apic_id = s.apic_id as usize;
                    }
                    self.n_cpus += 1;
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
        self.mmio = unsafe { &mut *(physical2virtual_io(apic_address) as *mut ApicMmio) };
    }
}
