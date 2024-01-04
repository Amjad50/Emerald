use core::{
    any::Any,
    fmt,
    mem::{self, size_of},
    slice,
};

use alloc::{boxed::Box, vec::Vec};

use crate::{
    io::{ByteStr, HexArray},
    memory_management::{
        memory_layout::{align_down, align_up, virtual2physical, KERNEL_BASE, KERNEL_END, PAGE_4K},
        virtual_space,
    },
    multiboot2::MultiBoot2Info,
};

use super::aml::{parse_aml, AmlCode};

const BIOS_RO_MEM_START: usize = 0x000E0000;
const BIOS_RO_MEM_END: usize = 0x000FFFFF;

fn physical_to_acpi_memory(addr: usize, size: usize) -> usize {
    if addr < virtual2physical(KERNEL_END) {
        assert!(addr + size <= virtual2physical(KERNEL_END));
        addr + KERNEL_BASE
    } else {
        virtual_space::get_virtual_for_physical(addr as _, size as _) as usize
    }
}

// this is used after parsing headers and determining the size of the table
// it tells the `virtual_space` module to ensure that the new size is still
// mapped, otherwise try to allocate next blocks if they are free.
// in our case since its done immidiately after parsing the headers, we know it
// should work unless someone else is executing concurrently
// at the time of writing this comment, this is done in startup so there shouldn't be anyne else.
fn ensure_at_least_size(addr: usize, size: usize) {
    if addr < virtual2physical(KERNEL_END) {
        assert!(addr + size <= virtual2physical(KERNEL_END));
    } else {
        let virtual_start = align_down(addr, PAGE_4K);
        virtual_space::ensure_at_least_size(virtual_start as _, align_up(size, PAGE_4K) as _);
    }
}

// Note: this requires allocation, so it should be called after the heap is initialized
pub fn get_acpi_tables(multiboot_info: &MultiBoot2Info) -> Result<BiosTables, ()> {
    let rdsp = multiboot_info
        .get_most_recent_rsdp()
        .or_else(|| {
            // look for RSDP PTR
            let mut rsdp_ptr =
                physical_to_acpi_memory(BIOS_RO_MEM_START, mem::size_of::<Rsdp>()) as *const u8;
            let end = physical_to_acpi_memory(BIOS_RO_MEM_END, mem::size_of::<Rsdp>()) as *const u8;

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
                        let rsdp_ref = unsafe { &*(rsdp_ptr as *const RsdpV2) };
                        if rsdp_ref.rsdp_v1.revision >= 2 {
                            return Some(Rsdp::from_v2(rsdp_ref));
                        } else {
                            return Some(Rsdp::from_v1(&rsdp_ref.rsdp_v1));
                        }
                    }
                }
                rsdp_ptr = unsafe { rsdp_ptr.add(1) };
            }

            None
        })
        .ok_or(())?;

    Ok(BiosTables::new(rdsp))
}

#[repr(C, packed)]
pub struct RsdpV1 {
    signature: ByteStr<[u8; 8]>,
    checksum: u8,
    oem_id: ByteStr<[u8; 6]>,
    revision: u8,
    rsdt_address: u32,
}

// used to copy
#[repr(C, packed)]
pub struct RsdpV2 {
    rsdp_v1: RsdpV1,
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
    pub signature: ByteStr<[u8; 8]>,
    pub checksum: u8,
    pub oem_id: ByteStr<[u8; 6]>,
    pub revision: u8,
    pub rsdt_address: u32,
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
    pub reserved: [u8; 3],
}

impl Rsdp {
    pub fn from_v1(v0: &RsdpV1) -> Self {
        Self {
            signature: v0.signature,
            checksum: v0.checksum,
            oem_id: v0.oem_id,
            revision: v0.revision,
            rsdt_address: v0.rsdt_address,
            length: 0,
            xsdt_address: 0,
            extended_checksum: 0,
            reserved: [0; 3],
        }
    }

    pub fn from_v2(v2: &RsdpV2) -> Self {
        Self {
            signature: v2.rsdp_v1.signature,
            checksum: v2.rsdp_v1.checksum,
            oem_id: v2.rsdp_v1.oem_id,
            revision: v2.rsdp_v1.revision,
            rsdt_address: v2.rsdp_v1.rsdt_address,
            length: v2.length,
            xsdt_address: v2.xsdt_address,
            extended_checksum: v2.extended_checksum,
            reserved: v2.reserved,
        }
    }

    // allocates a new RDST
    fn rdst(&self) -> Rsdt {
        let header =
            physical_to_acpi_memory(self.rsdt_address as _, mem::size_of::<DescriptionHeader>())
                as *const DescriptionHeader;
        let len = unsafe { (*header).length } as usize;
        ensure_at_least_size(header as _, len);

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
        let mut s = Rsdt {
            header: unsafe { *header },
            entries,
        };
        // add extra entries
        if let Some(facp) = s.get_table::<Facp>() {
            if facp.dsdt != 0 {
                s.entries
                    .push(DescriptorTable::from_physical_ptr(facp.dsdt));
            }
        }

        s
    }
}

#[derive(Debug, Clone)]
pub struct Rsdt {
    pub header: DescriptionHeader,
    entries: Vec<DescriptorTable>,
}

impl Rsdt {
    pub fn get_table<T: Any>(&self) -> Option<&T> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.body {
                DescriptorTableBody::Unknown(_) => None,
                DescriptorTableBody::Apic(a) => Some(a.as_ref() as &dyn Any),
                DescriptorTableBody::Facp(a) => Some(a.as_ref() as &dyn Any),
                DescriptorTableBody::Hpet(a) => Some(a.as_ref() as &dyn Any),
                DescriptorTableBody::Dsdt(a) => Some(a.as_ref() as &dyn Any),
                DescriptorTableBody::Bgrt(a) => Some(a.as_ref() as &dyn Any),
                DescriptorTableBody::Waet(a) => Some(a.as_ref() as &dyn Any),
            })
            .find_map(|obj| obj.downcast_ref::<T>())
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DescriptionHeader {
    pub signature: ByteStr<[u8; 4]>,
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: ByteStr<[u8; 6]>,
    pub oem_table_id: ByteStr<[u8; 8]>,
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

#[derive(Debug, Clone)]
pub struct DescriptorTable {
    pub header: DescriptionHeader,
    pub body: DescriptorTableBody,
}

impl DescriptorTable {
    pub fn from_physical_ptr(ptr: u32) -> Self {
        let header_ptr = physical_to_acpi_memory(ptr as _, mem::size_of::<DescriptionHeader>())
            as *const DescriptionHeader;

        let header = unsafe { &*header_ptr };
        let len = header.length as usize;
        ensure_at_least_size(header_ptr as _, len);

        let body = match &header.signature.0 {
            b"APIC" => DescriptorTableBody::Apic(Box::new(Apic::from_header(header))),
            b"FACP" => DescriptorTableBody::Facp(Box::new(Facp::from_header(header))),
            b"HPET" => DescriptorTableBody::Hpet(Box::new(Hpet::from_header(header))),
            b"DSDT" => DescriptorTableBody::Dsdt(Box::new(Dsdt::from_header(header))),
            b"BGRT" => DescriptorTableBody::Bgrt(Box::new(Bgrt::from_header(header))),
            b"WAET" => DescriptorTableBody::Waet(Box::new(Waet::from_header(header))),
            _ => DescriptorTableBody::Unknown(HexArray(
                unsafe {
                    slice::from_raw_parts(
                        header as *const DescriptionHeader as *const u8,
                        header.length as usize,
                    )
                }
                .to_vec(),
            )),
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
    Dsdt(Box<Dsdt>),
    Bgrt(Box<Bgrt>),
    Waet(Box<Waet>),
    Unknown(HexArray<Vec<u8>>),
}

#[derive(Debug, Clone)]
pub struct Apic {
    pub local_apic_address: u32,
    pub flags: u32,
    pub interrupt_controller_structs: Vec<InterruptControllerStruct>,
}

impl Apic {
    fn from_header(header: &DescriptionHeader) -> Self {
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
    Unknown {
        struct_type: u8,
        bytes: HexArray<Vec<u8>>,
    } = 255,
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
                bytes: HexArray(bytes.to_vec()),
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
    reset_reg: HexArray<[u8; 12]>,
    reset_value: u8,
    arm_boot_arch: u16,
    fadt_minor_version: u8,
    x_firmware_control: u64,
    x_dsdt: u64,
    x_pm1a_event_block: HexArray<[u8; 12]>,
    x_pm1b_event_block: HexArray<[u8; 12]>,
    x_pm1a_control_block: HexArray<[u8; 12]>,
    x_pm1b_control_block: HexArray<[u8; 12]>,
    x_pm2_control_block: HexArray<[u8; 12]>,
    x_pm_timer_block: HexArray<[u8; 12]>,
    x_gpe0_block: HexArray<[u8; 12]>,
    x_gpe1_block: HexArray<[u8; 12]>,
    sleep_control_reg: HexArray<[u8; 12]>,
    sleep_status_reg: HexArray<[u8; 12]>,
    hypervisor_vendor_id: u64,
}

impl Facp {
    fn from_header(header: &DescriptionHeader) -> Self {
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
    fn from_header(header: &DescriptionHeader) -> Self {
        let facp_ptr = unsafe { (header as *const DescriptionHeader).add(1) as *const u8 };
        let facp = unsafe { &*(facp_ptr as *const Hpet) };
        // SAFETY: I'm using this to copy from the same struct
        unsafe { core::mem::transmute_copy(facp) }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Dsdt {
    aml_code: AmlCode,
}

impl Dsdt {
    fn from_header(header: &DescriptionHeader) -> Self {
        let dsdt_ptr = unsafe { (header as *const DescriptionHeader).add(1) as *const u8 };
        let data_len = header.length as usize - size_of::<DescriptionHeader>();
        let data = unsafe { slice::from_raw_parts(dsdt_ptr, data_len) };
        let aml_code = parse_aml(data).unwrap();
        Self { aml_code }
    }
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct Bgrt {
    version: u16,
    status: u8,
    image_type: u8,
    pub image_address: u64,
    pub image_offset_x: u32,
    pub image_offset_y: u32,
}

impl Bgrt {
    fn from_header(header: &DescriptionHeader) -> Self {
        let bgrt_ptr = unsafe { (header as *const DescriptionHeader).add(1) as *const u8 };
        let bgrt = unsafe { &*(bgrt_ptr as *const Bgrt) };
        // SAFETY: I'm using this to copy from the same struct
        unsafe { core::mem::transmute_copy(bgrt) }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Waet {
    emulated_device_flags: u32,
}

impl Waet {
    fn from_header(header: &DescriptionHeader) -> Self {
        let waet_ptr = unsafe { (header as *const DescriptionHeader).add(1) as *const u32 };
        let flags = unsafe { *waet_ptr };
        Self {
            emulated_device_flags: flags,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BiosTables {
    pub rsdp: Rsdp,
    pub rsdt: Rsdt,
}

impl BiosTables {
    pub fn new(rsdp: Rsdp) -> Self {
        Self {
            rsdt: rsdp.rdst(),
            rsdp,
        }
    }
}

impl fmt::Display for BiosTables {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "RSDP: {:X?}", self.rsdp)?;
        writeln!(f, "RSDT: {:X?}", self.rsdt.header)?;
        for entry in &self.rsdt.entries {
            match entry.body {
                DescriptorTableBody::Dsdt(_) => {
                    // TODO: add cmdline arg to print DSDT (its very large, so don't by default)
                    // writeln!(f, "DSDT: ")?;
                    // entry.aml_code.display_with_depth(f, 1)?;
                }
                DescriptorTableBody::Unknown(_) => {
                    writeln!(f, "  {:X?}", entry.header)?;
                    writeln!(f, "  {:X?}", entry.body)?;
                }
                _ => {
                    writeln!(f, "{:X?}", entry.body)?;
                }
            }
        }
        Ok(())
    }
}
