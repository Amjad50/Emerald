use core::{mem::size_of, slice};

use alloc::vec::Vec;

use crate::memory_management::memory_layout::physical2virtual_bios;

const BIOS_RO_MEM_START: usize = 0x000E0000;
const BIOS_RO_MEM_END: usize = 0x000FFFFF;

// Note: this requires allocation, so it should be called after the heap is initialized
pub fn get_bios_tables() -> Result<BiosTables, ()> {
    let mut tables = BiosTables::empty();

    // look for RSDP PTR
    let mut rsdp_ptr = physical2virtual_bios(BIOS_RO_MEM_START) as *const u8;
    let end = physical2virtual_bios(BIOS_RO_MEM_END) as *const u8;

    while rsdp_ptr < end {
        let str = unsafe { slice::from_raw_parts(rsdp_ptr, 8) };
        if str == b"RSD PTR " {
            // calculate checksum
            let sum = unsafe {
                slice::from_raw_parts(rsdp_ptr, 20)
                    .iter()
                    .fold(0u8, |acc, &x| acc.wrapping_add(x))
            };
            if sum == 0 {
                let rsdp_ref = unsafe { &*(rsdp_ptr as *const RsdpV0) };
                if rsdp_ref.revision >= 2 {
                    tables.rsdp.fill_from_v2(rsdp_ref);
                } else {
                    tables.rsdp.fill_from_v0(rsdp_ref);
                }
                break;
            }
        }
        rsdp_ptr = unsafe { rsdp_ptr.add(1) };
    }
    if rsdp_ptr == end {
        // TODO: report error, no RSDP found
        return Err(());
    }

    tables.rsdt = tables.rsdp.rdst();

    Ok(tables)
}

// used to copy
#[repr(C, packed)]
struct RsdpV0 {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
    // these are only v2, but its here to make copying easier
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

/// Represent v2 and above
#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct Rsdp {
    pub signature: [u8; 8],
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8,
    pub rsdt_address: u32,
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
    pub reserved: [u8; 3],
}

impl Rsdp {
    pub const fn empty() -> Self {
        Self {
            signature: [0; 8],
            checksum: 0,
            oem_id: [0; 6],
            revision: 0,
            rsdt_address: 0,
            length: 0,
            xsdt_address: 0,
            extended_checksum: 0,
            reserved: [0; 3],
        }
    }

    fn fill_from_v0(&mut self, v0: &RsdpV0) {
        self.signature = v0.signature;
        self.checksum = v0.checksum;
        self.oem_id = v0.oem_id;
        self.revision = v0.revision;
        self.rsdt_address = v0.rsdt_address;
    }

    fn fill_from_v2(&mut self, v2: &RsdpV0) {
        assert!(size_of::<Rsdp>() == size_of::<RsdpV0>());
        *self = unsafe { core::mem::transmute_copy(v2) }
    }

    // allocates a new RDST
    fn rdst(&self) -> Rsdt {
        let header = physical2virtual_bios(self.rsdt_address as _) as *const DescriptionHeader;
        let len = unsafe { (*header).length } as usize;
        let entries_len = (len - size_of::<DescriptionHeader>()) / size_of::<u32>();
        let entries_ptr = unsafe { header.add(1) as *const u32 };
        // use slice of u8 since we can't use u32 since its not aligned
        let entries = unsafe { slice::from_raw_parts(entries_ptr as *const u8, entries_len * 4) };
        let entries = entries
            .chunks(4)
            .map(|a| u32::from_le_bytes(a.try_into().unwrap()))
            .filter(|&a| a != 0)
            // SAEFTY: these entries are static and never change, and not null
            .map(DescriptorTable::from_physical_ptr)
            .collect();
        Rsdt {
            header: unsafe { *header },
            entries,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Rsdt {
    pub header: DescriptionHeader,
    pub entries: Vec<DescriptorTable>,
}

impl Rsdt {
    pub const fn empty() -> Self {
        Self {
            header: DescriptionHeader::empty(),
            entries: Vec::new(),
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DescriptionHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

impl DescriptionHeader {
    const fn empty() -> Self {
        Self {
            signature: [0; 4],
            length: 0,
            revision: 0,
            checksum: 0,
            oem_id: [0; 6],
            oem_table_id: [0; 8],
            oem_revision: 0,
            creator_id: 0,
            creator_revision: 0,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DescriptorTable {
    pub header: DescriptionHeader,
    pub body: DescriptorTableBody,
}

impl DescriptorTable {
    pub fn from_physical_ptr(ptr: u32) -> Self {
        let header = unsafe { &*(physical2virtual_bios(ptr as _) as *const DescriptionHeader) };
        let body = match &header.signature {
            b"APIC" => DescriptorTableBody::Apic(Apic::from_header(header)),
            _ => DescriptorTableBody::Unknown,
        };
        Self {
            header: *header,
            body,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DescriptorTableBody {
    Apic(Apic),
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Apic {
    pub local_apic_address: u32,
    pub flags: u32,
    pub interrupt_controller_structs: Vec<InterruptControllerStruct>,
}

impl Apic {
    fn from_header(header: &'static DescriptionHeader) -> Self {
        let mut apic = Self {
            local_apic_address: 0,
            flags: 0,
            interrupt_controller_structs: Vec::new(),
        };
        let after_header = unsafe { (header as *const DescriptionHeader).add(1) as *const u32 };
        apic.local_apic_address = unsafe { after_header.read_unaligned() };
        apic.flags = unsafe { after_header.add(1).read_unaligned() };

        let mut ptr = unsafe { after_header.add(2) as *const u8 };
        let mut remaining = header.length - size_of::<DescriptionHeader>() as u32 - 8;
        while remaining > 0 {
            let struct_type = unsafe { *ptr };
            let struct_len = unsafe { *(ptr.add(1)) };
            let struct_bytes =
                unsafe { slice::from_raw_parts(ptr.add(2), struct_len as usize - 2) };
            apic.interrupt_controller_structs
                .push(InterruptControllerStruct::from_type_and_bytes(
                    struct_type,
                    struct_bytes,
                ));
            ptr = unsafe { ptr.add(struct_len as usize) };
            remaining -= struct_len as u32;
        }
        apic
    }
}

#[repr(u8)]
#[derive(Debug, Clone)]
pub enum InterruptControllerStruct {
    ProcessorLocalApic(ProcessorLocalApic) = 0,
    IoApic(IoApic) = 1,
    InterruptSourceOverride(InterruptSourceOverride) = 2,
    NonMaskableInterrupt(NonMaskableInterrupt) = 3,
    LocalApicNmi(LocalApicNmi) = 4,
    LocalApicAddressOverride(LocalApicAddressOverride) = 5,
    Unknown { struct_type: u8, bytes: Vec<u8> } = 255,
}

// extract enum into outside structs
#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct ProcessorLocalApic {
    pub acpi_processor_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct IoApic {
    pub io_apic_id: u8,
    pub io_apic_address: u32,
    pub global_system_interrupt_base: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct InterruptSourceOverride {
    pub bus: u8,
    pub source: u8,
    pub global_system_interrupt: u32,
    pub flags: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct NonMaskableInterrupt {
    pub flags: u16,
    pub global_system_interrupt: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct LocalApicNmi {
    pub acpi_processor_uid: u8,
    pub flags: u16,
    pub local_apic_lint: u8,
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct LocalApicAddressOverride {
    pub local_apic_address: u64,
}

impl InterruptControllerStruct {
    fn from_type_and_bytes(struct_type: u8, bytes: &[u8]) -> Self {
        match struct_type {
            0 => {
                let acpi_processor_id = bytes[0];
                let apic_id = bytes[1];
                let flags = u32::from_le_bytes(bytes[2..6].try_into().unwrap());
                Self::ProcessorLocalApic(ProcessorLocalApic {
                    acpi_processor_id,
                    apic_id,
                    flags,
                })
            }
            1 => {
                let io_apic_id = bytes[0];
                let io_apic_address = u32::from_le_bytes(bytes[2..6].try_into().unwrap());
                let global_system_interrupt_base =
                    u32::from_le_bytes(bytes[6..10].try_into().unwrap());
                Self::IoApic(IoApic {
                    io_apic_id,
                    io_apic_address,
                    global_system_interrupt_base,
                })
            }
            2 => {
                let bus = bytes[0];
                let source = bytes[1];
                let global_system_interrupt = u32::from_le_bytes(bytes[2..6].try_into().unwrap());
                let flags = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
                Self::InterruptSourceOverride(InterruptSourceOverride {
                    bus,
                    source,
                    global_system_interrupt,
                    flags,
                })
            }
            3 => {
                let flags = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
                let global_system_interrupt = u32::from_le_bytes(bytes[2..6].try_into().unwrap());
                Self::NonMaskableInterrupt(NonMaskableInterrupt {
                    flags,
                    global_system_interrupt,
                })
            }
            4 => {
                let acpi_processor_id = bytes[0];
                let flags = u16::from_le_bytes(bytes[1..3].try_into().unwrap());
                let local_apic_lint = bytes[3];
                Self::LocalApicNmi(LocalApicNmi {
                    acpi_processor_uid: acpi_processor_id,
                    flags,
                    local_apic_lint,
                })
            }
            5 => {
                let local_apic_address = u64::from_le_bytes(bytes.try_into().unwrap());
                Self::LocalApicAddressOverride(LocalApicAddressOverride { local_apic_address })
            }
            _ => Self::Unknown {
                struct_type,
                bytes: bytes.to_vec(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct BiosTables {
    pub rsdp: Rsdp,
    pub rsdt: Rsdt,
}

impl BiosTables {
    pub const fn empty() -> Self {
        Self {
            rsdp: Rsdp::empty(),
            rsdt: Rsdt::empty(),
        }
    }
}
