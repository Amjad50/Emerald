use core::{mem::size_of, slice};

use alloc::{boxed::Box, vec::Vec};

use crate::{
    memory_management::{
        memory_layout::{
            align_down, allocate_from_extra_kernel_pages, virtual2physical, KERNEL_BASE,
            KERNEL_END, PAGE_4K,
        },
        virtual_memory::{self, VirtualMemoryMapEntry},
    },
    sync::once::OnceLock,
};

const BIOS_RO_MEM_START: usize = 0x000E0000;
const BIOS_RO_MEM_END: usize = 0x000FFFFF;

static BIOS_MEMORY_MAPPER: OnceLock<BiosMemoryMapper> = OnceLock::new();

fn physical_to_bios_memory(addr: usize) -> usize {
    if addr < virtual2physical(KERNEL_END) {
        addr + KERNEL_BASE
    } else if let Some(mapper) = BIOS_MEMORY_MAPPER.try_get() {
        mapper.get_virtual(addr as _) as _
    } else {
        let mapper = BiosMemoryMapper::new(addr as _);
        let virtual_addr = mapper.get_virtual(addr as _) as _;
        BIOS_MEMORY_MAPPER
            .set(mapper)
            .expect("BIOS_MEMORY_MAPPER already set");
        virtual_addr
    }
}

// number of pages to map around the `prope/start` address
const BIOS_MEMORY_MAPPED_PAGES_AROUND: usize = 4;

#[derive(Debug)]
struct BiosMemoryMapper {
    start_physical: u64,
    start_virtual: u64,
    num_pages: usize,
}

impl BiosMemoryMapper {
    // we use `prope_addr` to know where to start from, generally this memory isn't very large
    // so we can just start pages around `prope` address and map from there
    pub fn new(prope_addr: u64) -> Self {
        let prope_page = align_down(prope_addr as _, PAGE_4K) as u64;
        const MEMORY_AROUND: u64 = BIOS_MEMORY_MAPPED_PAGES_AROUND as u64 * PAGE_4K as u64;
        assert!(prope_page > MEMORY_AROUND);
        assert!(prope_page < (usize::MAX as u64 - MEMORY_AROUND));
        let physical_start = prope_page - MEMORY_AROUND;
        let num_pages = BIOS_MEMORY_MAPPED_PAGES_AROUND * 2 + 1;

        let start_virtual = unsafe { allocate_from_extra_kernel_pages(num_pages) };

        virtual_memory::map_kernel(&VirtualMemoryMapEntry {
            virtual_address: start_virtual as u64,
            physical_address: Some(physical_start),
            size: num_pages as u64 * PAGE_4K as u64,
            flags: 0,
        });

        Self {
            start_physical: physical_start,
            start_virtual: start_virtual as u64,
            num_pages,
        }
    }

    pub fn get_virtual(&self, addr: u64) -> u64 {
        if addr >= self.start_physical
            && addr < self.start_physical + self.num_pages as u64 * PAGE_4K as u64
        {
            addr - self.start_physical + self.start_virtual
        } else {
            // for now I'm assuming we can start from the first address we try to map
            // and just map `BIOS_MEMORY_MAPPED_PAGES_AROUND` pages around it
            panic!(
                "bios address {:#X} not mapped, range: {:#X}-{:#X}",
                addr,
                self.start_physical,
                self.start_physical + self.num_pages as u64 * PAGE_4K as u64
            );
        }
    }
}

// Note: this requires allocation, so it should be called after the heap is initialized
pub fn get_bios_tables() -> Result<BiosTables, ()> {
    let mut tables = BiosTables::empty();

    // look for RSDP PTR
    let mut rsdp_ptr = physical_to_bios_memory(BIOS_RO_MEM_START) as *const u8;
    let end = physical_to_bios_memory(BIOS_RO_MEM_END) as *const u8;

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
        let header = physical_to_bios_memory(self.rsdt_address as _) as *const DescriptionHeader;
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
        let header = unsafe { &*(physical_to_bios_memory(ptr as _) as *const DescriptionHeader) };
        let body = match &header.signature {
            b"APIC" => DescriptorTableBody::Apic(Box::new(Apic::from_header(header))),
            b"FACP" => DescriptorTableBody::Facp(Box::new(Facp::from_header(header))),
            b"HPET" => DescriptorTableBody::Hpet(Box::new(Hpet::from_header(header))),
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
    Apic(Box<Apic>),
    Facp(Box<Facp>),
    Hpet(Box<Hpet>),
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

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct Facp {
    firmware_control: u32,
    dsdt: u32,
    reserved: u8,
    preferred_pm_profile: u8,
    sci_interrupt: u16,
    smi_command_port: u32,
    acpi_enable: u8,
    acpi_disable: u8,
    s4bios_req: u8,
    pstate_control: u8,
    pm1a_event_block: u32,
    pm1b_event_block: u32,
    pm1a_control_block: u32,
    pm1b_control_block: u32,
    pm2_control_block: u32,
    pm_timer_block: u32,
    gpe0_block: u32,
    gpe1_block: u32,
    pm1_event_length: u8,
    pm1_control_length: u8,
    pm2_control_length: u8,
    pm_timer_length: u8,
    gpe0_block_length: u8,
    gpe1_block_length: u8,
    gpe1_base: u8,
    cstate_control: u8,
    p_level2_latency: u16,
    p_level3_latency: u16,
    flush_size: u16,
    flush_stride: u16,
    duty_offset: u8,
    duty_width: u8,
    day_alarm: u8,
    month_alarm: u8,
    pub century: u8,
    iapc_boot_arch: u16,
    reserved2: u8,
    flags: u32,
    reset_reg: [u8; 12],
    reset_value: u8,
    arm_boot_arch: u16,
    fadt_minor_version: u8,
    x_firmware_control: u64,
    x_dsdt: u64,
    x_pm1a_event_block: [u8; 12],
    x_pm1b_event_block: [u8; 12],
    x_pm1a_control_block: [u8; 12],
    x_pm1b_control_block: [u8; 12],
    x_pm2_control_block: [u8; 12],
    x_pm_timer_block: [u8; 12],
    x_gpe0_block: [u8; 12],
    x_gpe1_block: [u8; 12],
    sleep_control_reg: [u8; 12],
    sleep_status_reg: [u8; 12],
    hypervisor_vendor_id: u64,
}

impl Facp {
    fn from_header(header: &'static DescriptionHeader) -> Self {
        let facp_ptr = unsafe { (header as *const DescriptionHeader).add(1) as *const u8 };
        let facp = unsafe { &*(facp_ptr as *const Facp) };
        // SAFETY: I'm using this to copy from the same struct
        unsafe { core::mem::transmute_copy(facp) }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ApicGenericAddress {
    pub address_space_id: u8,
    pub register_bit_width: u8,
    pub register_bit_offset: u8,
    pub reserved: u8,
    pub address: u64,
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct Hpet {
    pub event_timer_block_id: u32,
    pub base_address: ApicGenericAddress,
    pub hpet_number: u8,
    pub main_counter_minimum_clock_tick: u16,
    pub page_protection: u8,
}

impl Hpet {
    fn from_header(header: &'static DescriptionHeader) -> Self {
        let facp_ptr = unsafe { (header as *const DescriptionHeader).add(1) as *const u8 };
        let facp = unsafe { &*(facp_ptr as *const Hpet) };
        // SAFETY: I'm using this to copy from the same struct
        unsafe { core::mem::transmute_copy(facp) }
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
