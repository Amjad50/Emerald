pub mod facp;

pub use facp::Facp;

use core::{
    any::Any,
    fmt,
    mem::{self, MaybeUninit},
    slice,
};

use alloc::{boxed::Box, vec::Vec};
use byteorder::{ByteOrder, LittleEndian};

use crate::{
    cmdline::{self, LogAml},
    io::{ByteStr, HexArray},
    memory_management::{memory_layout::physical2virtual, virtual_space::VirtualSpace},
    multiboot2::MultiBoot2Info,
    sync::once::OnceLock,
};

use super::aml::Aml;

const BIOS_RO_MEM_START: u64 = 0x000E0000;
const BIOS_RO_MEM_END: u64 = 0x000FFFFF;

/// # Safety
///
/// Must ensure the `physical_addr` is valid and point to correct DescriptionHeader
/// Must ensure that the `physical_address` is not used in virtual_space before calling this function
/// We are using `VirtualSpace` on low kernel addresses (i.e. already mapped by the kernel).
/// Accessing these addresses manually without `VirtualSpace` may lead to undefined behavior due to aliasing memory referenced by other code
unsafe fn get_acpi_table_bytes(physical_addr: u64) -> (DescriptionHeader, VirtualSpace<[u8]>) {
    let header = VirtualSpace::<DescriptionHeader>::new(physical_addr).expect("Failed to map");
    let len = header.length as usize;
    let header_copy = *header;
    drop(header);

    let data_start_phys = physical_addr + mem::size_of::<DescriptionHeader>() as u64;
    let data_len = len - mem::size_of::<DescriptionHeader>();

    let header_data =
        VirtualSpace::<u8>::new_slice(data_start_phys, data_len).expect("Failed to get slice");

    // check sum
    let sum = header_copy
        .sum()
        .wrapping_add(header_data.iter().fold(0u8, |acc, &x| acc.wrapping_add(x)));
    assert_eq!(sum, 0);

    // after this point, the header is valid and can be used safely
    (header_copy, header_data)
}

/// Will fill the table from the header data, and zero out remaining bytes if any are left
///
/// # Safety
/// the pointer must be valid and point to a valid table
/// Also, `<T>` must be valid when some parts of it is zero
unsafe fn get_table_from_body<T>(body: &[u8]) -> T {
    let mut our_data_value = MaybeUninit::zeroed();
    let out_data_slice =
        slice::from_raw_parts_mut(our_data_value.as_mut_ptr() as *mut u8, mem::size_of::<T>());
    out_data_slice[..body.len()].copy_from_slice(body);

    our_data_value.assume_init()
}

/// Will fill the table from the header data, and zero out remaining bytes if any are left
///
/// # Safety
///
/// type `<T>` must be valid when some parts of it is zero
///
/// TODO: should this be unsafe?
fn get_struct_from_bytes<T>(data: &[u8]) -> T {
    assert_eq!(data.len(), mem::size_of::<T>());

    let mut our_data_value = MaybeUninit::zeroed();
    // Safety: it is safe to create a slice of bytes for the struct, since we know the pointer is valid
    let out_data_slice = unsafe {
        slice::from_raw_parts_mut(our_data_value.as_mut_ptr() as *mut u8, mem::size_of::<T>())
    };
    out_data_slice.copy_from_slice(data);

    // Safety: we are sure that the data is valid, since we assume that in the function doc,
    // assured from the caller
    unsafe { our_data_value.assume_init() }
}

// cache the tables
static BIOS_TABLES: OnceLock<BiosTables> = OnceLock::new();

// Note: this requires allocation, so it should be called after the heap is initialized
pub fn init_acpi_tables(multiboot_info: &MultiBoot2Info) -> &'static BiosTables {
    BIOS_TABLES.get_or_init(|| {
        let rdsp = multiboot_info
            .get_most_recent_rsdp()
            .or_else(|| {
                // look for RSDP PTR
                // this is inside the kernel low virtual range, so we can just convert to virtual directly without allocating space
                let mut rsdp_ptr = physical2virtual(BIOS_RO_MEM_START) as *const u8;
                let end = physical2virtual(BIOS_RO_MEM_END) as *const u8;

                while rsdp_ptr < end {
                    // Safety: this is a valid mapped range, as we are sure that the kernel is
                    // mapped since boot and we are inside the kernel lower range
                    let str = unsafe { slice::from_raw_parts(rsdp_ptr, 8) };
                    if str == b"RSD PTR " {
                        // calculate checksum
                        // Safety: same as above, this pointer is mapped
                        let sum = unsafe {
                            slice::from_raw_parts(rsdp_ptr, 20)
                                .iter()
                                .fold(0u8, |acc, &x| acc.wrapping_add(x))
                        };
                        if sum == 0 {
                            // Safety: same as above, this pointer is mapped
                            let rsdp_ref = unsafe { &*(rsdp_ptr as *const RsdpV2) };
                            return if rsdp_ref.rsdp_v1.revision >= 2 {
                                Some(Rsdp::from_v2(rsdp_ref))
                            } else {
                                Some(Rsdp::from_v1(&rsdp_ref.rsdp_v1))
                            };
                        }
                    }
                    // Safety: same as above, this pointer is mapped
                    rsdp_ptr = unsafe { rsdp_ptr.add(1) };
                }

                None
            })
            .expect("No RSDP found");

        // Safety: this is called only once and we are sure no other call is using the ACPI memory
        unsafe { BiosTables::new(rdsp) }
    })
}

pub fn get_acpi_tables() -> &'static BiosTables {
    BIOS_TABLES.get()
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

    /// allocates a new RDST
    ///
    /// # Safety
    ///
    /// This should only be called once and not overlapping with any operation done to the region containing ACPI tables
    /// this uses virtual space for the regions that the `rsdt` is inside and all its other children structures
    unsafe fn rdst(&self) -> Rsdt {
        // Safety: here we are the first
        let (header, body_bytes) = get_acpi_table_bytes(self.rsdt_address as _);

        // we copy the addresses here, we can't sadly use iter to iterate over them since inside `from_physical_ptr` we need to be
        // sure that we don't own any references to the ACPI memory regions in the `VirtualSpace`
        let entries_ptrs = body_bytes
            .chunks(4)
            .map(|a| u32::from_le_bytes(a.try_into().unwrap()))
            .filter(|&a| a != 0)
            .collect::<Vec<_>>();

        // deallocate the virtual space memory, so we can use it again below if regions overlap
        drop(body_bytes);

        let entries = entries_ptrs
            .into_iter()
            // Safety: `from_physical_ptr` require we don't overlap usage of ACPI memory, we are `deallocating` the memory above
            //         before going into this function, and it will handle its own deallocation, so we are safe on that side
            .map(|p| unsafe { DescriptorTable::from_physical_ptr(p) })
            .collect();

        let mut s = Rsdt { header, entries };
        // add extra entries
        if let Some(facp) = s.get_table::<Facp>() {
            if facp.dsdt != 0 {
                // Safety: same as above, we are sure that we are not overlapping with any ACPI memory
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
                DescriptorTableBody::Ssdt(a) => Some(a.as_ref() as &dyn Any),
                DescriptorTableBody::Bgrt(a) => Some(a.as_ref() as &dyn Any),
                DescriptorTableBody::Waet(a) => Some(a.as_ref() as &dyn Any),
                DescriptorTableBody::Srat(a) => Some(a.as_ref() as &dyn Any),
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

impl DescriptionHeader {
    pub fn sum(&self) -> u8 {
        let mut sum = 0u8;
        let ptr = self as *const Self as *const u8;
        for i in 0..mem::size_of::<Self>() {
            sum = sum.wrapping_add(unsafe { ptr.add(i).read() });
        }
        sum
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorTable {
    pub header: DescriptionHeader,
    pub body: DescriptorTableBody,
}

impl DescriptorTable {
    /// # Safety
    ///
    /// This should not overlap any reference to the ACPI memory, it will own a reference to virtual space
    /// that points to the physical address, and then yields the reference before it returns.
    /// Thus it must never be called concurrently as well
    pub unsafe fn from_physical_ptr(ptr: u32) -> Self {
        // Safety: here we are relying on the caller to ensure that the `ptr` is valid and no one is using ACPI memory
        let (header, body_bytes) = unsafe { get_acpi_table_bytes(ptr as _) };

        let body = match &header.signature.0 {
            b"APIC" => DescriptorTableBody::Apic(Box::new(Apic::from_body_bytes(&body_bytes))),
            b"FACP" => DescriptorTableBody::Facp(Box::new(get_table_from_body(&body_bytes))),
            b"HPET" => DescriptorTableBody::Hpet(Box::new(get_table_from_body(&body_bytes))),
            b"DSDT" => DescriptorTableBody::Dsdt(Box::new(Xsdt::from_body_bytes(&body_bytes))),
            b"SSDT" => DescriptorTableBody::Ssdt(Box::new(Xsdt::from_body_bytes(&body_bytes))),
            b"BGRT" => DescriptorTableBody::Bgrt(Box::new(get_table_from_body(&body_bytes))),
            b"WAET" => DescriptorTableBody::Waet(Box::new(get_table_from_body(&body_bytes))),
            b"SRAT" => DescriptorTableBody::Srat(Box::new(Srat::from_body_bytes(&body_bytes))),
            _ => DescriptorTableBody::Unknown(HexArray(body_bytes.to_vec())),
        };

        Self { header, body }
    }
}

#[derive(Debug, Clone)]
pub enum DescriptorTableBody {
    Apic(Box<Apic>),
    Facp(Box<Facp>),
    Hpet(Box<Hpet>),
    Dsdt(Box<Xsdt>),
    Ssdt(Box<Xsdt>),
    Bgrt(Box<Bgrt>),
    Waet(Box<Waet>),
    Srat(Box<Srat>),
    Unknown(HexArray<Vec<u8>>),
}

#[derive(Debug, Clone)]
pub struct Apic {
    pub local_apic_address: u32,
    pub flags: u32,
    pub interrupt_controller_structs: Vec<InterruptControllerStruct>,
}

impl Apic {
    /// # Safety
    /// the pointer must be valid and point to a valid table
    fn from_body_bytes(body: &[u8]) -> Self {
        let mut apic = Self {
            local_apic_address: LittleEndian::read_u32(body),
            flags: LittleEndian::read_u32(&body[4..]),
            interrupt_controller_structs: Vec::new(),
        };

        let mut remaining_body = &body[8..];
        let mut remaining = body.len() - 8;
        while remaining > 0 {
            let struct_type = remaining_body[0];
            let struct_len = remaining_body[1];
            let struct_bytes = &remaining_body[2..struct_len as usize];
            apic.interrupt_controller_structs
                .push(InterruptControllerStruct::from_type_and_bytes(
                    struct_type,
                    struct_bytes,
                ));
            remaining -= struct_len as usize;
            remaining_body = &remaining_body[struct_len as usize..];
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

impl InterruptControllerStruct {
    fn from_type_and_bytes(struct_type: u8, bytes: &[u8]) -> Self {
        match struct_type {
            0 => Self::ProcessorLocalApic(get_struct_from_bytes(bytes)),
            1 => Self::IoApic(get_struct_from_bytes(bytes)),
            2 => Self::InterruptSourceOverride(get_struct_from_bytes(bytes)),
            3 => Self::NonMaskableInterrupt(get_struct_from_bytes(bytes)),
            4 => Self::LocalApicNmi(get_struct_from_bytes(bytes)),
            5 => Self::LocalApicAddressOverride(get_struct_from_bytes(bytes)),
            _ => Self::Unknown {
                struct_type,
                bytes: HexArray(bytes.to_vec()),
            },
        }
    }
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
    pub reserved: u8,
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
    pub reserved: u16,
    pub local_apic_address: u64,
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
impl ApicGenericAddress {
    fn is_zero(&self) -> bool {
        self.address == 0
            && self.address_space_id == 0
            && self.register_bit_offset == 0
            && self.register_bit_width == 0
            && self.reserved == 0
    }
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

#[derive(Debug, Clone)]
#[allow(dead_code)]
/// This is inside DSDT and SSDT
pub struct Xsdt {
    aml: Aml,
}

impl Xsdt {
    fn from_body_bytes(body: &[u8]) -> Self {
        let aml_code = Aml::parse(body).unwrap();
        Self { aml: aml_code }
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

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Waet {
    emulated_device_flags: u32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Srat {
    reserved1: u32,
    reserved2: u64,
    static_resource_allocation: Vec<StaticResourceAffinity>,
}

impl Srat {
    fn from_body_bytes(body: &[u8]) -> Self {
        let mut srat = Self {
            reserved1: LittleEndian::read_u32(body),
            reserved2: LittleEndian::read_u64(&body[4..]),
            static_resource_allocation: Vec::new(),
        };

        let mut remaining_body = &body[12..];

        let mut remaining = body.len() - 12;
        while remaining > 0 {
            let struct_type = remaining_body[0];
            let struct_len = remaining_body[1];
            let struct_bytes = &remaining_body[2..struct_len as usize];
            srat.static_resource_allocation
                .push(StaticResourceAffinity::from_type_and_bytes(
                    struct_type,
                    struct_bytes,
                ));
            remaining -= struct_len as usize;
            remaining_body = &remaining_body[struct_len as usize..];
        }
        srat
    }
}

#[repr(u8)]
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StaticResourceAffinity {
    ProcessorLocalAcpi(ProcessorLocalAcpiAffinity) = 0,
    MemoryAffinity(MemoryAffinity) = 1,
    ProcessorLocalX2Apic(ProcessorLocalX2ApicAffinity) = 2,
    GiccAffinity(GiccAffinity) = 3,
    GicInterruptTranslationService(GicInterruptTranslationServiceAffinity) = 4,
    GenericInitiatorAffinity(GenericInitiatorAffinity) = 5,
    Unknown {
        struct_type: u8,
        bytes: HexArray<Vec<u8>>,
    } = 255,
}

impl StaticResourceAffinity {
    fn from_type_and_bytes(struct_type: u8, bytes: &[u8]) -> Self {
        match struct_type {
            0 => Self::ProcessorLocalAcpi(get_struct_from_bytes(bytes)),
            1 => Self::MemoryAffinity(get_struct_from_bytes(bytes)),
            2 => Self::ProcessorLocalX2Apic(get_struct_from_bytes(bytes)),
            3 => Self::GiccAffinity(get_struct_from_bytes(bytes)),
            4 => Self::GicInterruptTranslationService(get_struct_from_bytes(bytes)),
            5 => Self::GenericInitiatorAffinity(get_struct_from_bytes(bytes)),
            _ => Self::Unknown {
                struct_type,
                bytes: HexArray(bytes.to_vec()),
            },
        }
    }
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct ProcessorLocalAcpiAffinity {
    proximity_domain_low: u8,
    apic_id: u8,
    flags: u32,
    local_sapic_eid: u8,
    proximity_domain_high: [u8; 3],
    clock_domain: u32,
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct MemoryAffinity {
    proximity_domain: u32,
    reserved1: u16,
    base_address_low: u32,
    base_address_high: u32,
    length_low: u32,
    length_high: u32,
    reserved2: u32,
    flags: u32,
    reserved3: u64,
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct ProcessorLocalX2ApicAffinity {
    reserved1: u16,
    proximity_domain: u32,
    x2apic_id: u32,
    flags: u32,
    clock_domain: u32,
    reserved2: u32,
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct GiccAffinity {
    proximity_domain: u32,
    acpi_processor_uid: u32,
    flags: u32,
    clock_domain: u32,
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct GicInterruptTranslationServiceAffinity {
    proximity_domain: u32,
    reserved1: u16,
    its_id: u32,
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct GenericInitiatorAffinity {
    reserved1: u8,
    device_handle_type: u8,
    proximity_domain: u32,
    device_handle: [u8; 16],
    flags: u32,
    reserved2: u32,
}

#[derive(Debug, Clone)]
pub struct BiosTables {
    pub rsdp: Rsdp,
    pub rsdt: Rsdt,
}

impl BiosTables {
    /// # Safety
    ///
    /// This should only be called once and not overlapping with any operation done to the region containing ACPI tables
    pub unsafe fn new(rsdp: Rsdp) -> Self {
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
            match &entry.body {
                DescriptorTableBody::Dsdt(data) | DescriptorTableBody::Ssdt(data) => {
                    writeln!(f, "{:X?}", entry.header)?;

                    match cmdline::cmdline().log_aml {
                        LogAml::Normal => {
                            writeln!(f, "AML: ")?;
                            data.aml.code().display_with_depth(f, 1)?;
                        }
                        LogAml::Structured => {
                            writeln!(f, "AML: ")?;
                            data.aml.structured().display_with_depth(f, 1)?;
                        }
                        LogAml::Off => {}
                    }
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
