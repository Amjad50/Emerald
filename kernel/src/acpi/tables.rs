use core::{
    any::Any,
    fmt,
    mem::{self, size_of, MaybeUninit},
    slice,
};

use alloc::{boxed::Box, vec::Vec};

use crate::{
    io::{ByteStr, HexArray},
    memory_management::{
        memory_layout::{physical2virtual, virtual2physical, KERNEL_BASE, KERNEL_END},
        virtual_space,
    },
    multiboot2::MultiBoot2Info,
    sync::once::OnceLock,
};

use super::aml::{parse_aml, AmlCode};

const BIOS_RO_MEM_START: usize = 0x000E0000;
const BIOS_RO_MEM_END: usize = 0x000FFFFF;

/// # Safety
///
/// Must ensure the `physical_addr` is valid and point to correct DescriptionHeader
/// Must ensure that the `physical_address` is not used in virtual_space before calling this function
/// If any address is used, this may panic, and undefined behavior may occur due to aliasing memory referenced by other code
/// call `deallocate_acpi_mapping` after using any memory in the acpi region
unsafe fn get_acpi_header_with_len(physical_addr: usize) -> (*const DescriptionHeader, usize) {
    if physical_addr < virtual2physical(KERNEL_END) {
        assert!(
            physical_addr + mem::size_of::<DescriptionHeader>() <= virtual2physical(KERNEL_END)
        );

        let header_virtual = (physical_addr + KERNEL_BASE) as *const DescriptionHeader;
        let len = (*header_virtual).length as usize;
        assert!(physical_addr + len <= virtual2physical(KERNEL_END));
        return (header_virtual, len);
    } else {
        let header = virtual_space::get_virtual_for_physical(
            physical_addr as _,
            mem::size_of::<DescriptionHeader>() as _,
        ) as *const DescriptionHeader;

        let len = (*header).length as usize;
        // remove this entry
        virtual_space::deallocate_virtual_space(
            header as _,
            mem::size_of::<DescriptionHeader>() as _,
        );
        let header_ptr = virtual_space::get_virtual_for_physical(physical_addr as _, len as _)
            as *const DescriptionHeader;

        // check sum
        let struct_slice = slice::from_raw_parts(header_ptr as *const u8, len);
        let sum = struct_slice.iter().fold(0u8, |acc, &x| acc.wrapping_add(x));
        assert_eq!(sum, 0);

        // after this point, the header is valid and can be used safely

        (header_ptr, len)
    }
}

/// # Safety
///
/// The data pointed by `addr` will become invalid and must not be used.
///
/// This will affect all pages of memory that are touching the address space of `addr` and `size`
/// Thus, this should only be used when we are sure that we are not holding any references to any data of this space.
unsafe fn deallocate_acpi_mapping(addr: usize, size: usize) {
    if addr < virtual2physical(KERNEL_END) {
        assert!(addr + size <= virtual2physical(KERNEL_END));
    } else {
        virtual_space::deallocate_virtual_space(addr as _, size as _);
    }
}

/// Will fill the table from the header data, and zero out remaining bytes if any are left
///
/// # Safety
/// the pointer must be valid and point to a valid table
/// Also, <T> must be valid when some parts of it is zero
unsafe fn get_table_from_header<T>(header: *const DescriptionHeader) -> T {
    let data_ptr = header.add(1) as *const u8;
    let data_len = (*header).length as usize - size_of::<DescriptionHeader>();
    let data_slice = slice::from_raw_parts(data_ptr, data_len);

    let mut our_data_value = MaybeUninit::zeroed();
    let out_data_slice =
        slice::from_raw_parts_mut(our_data_value.as_mut_ptr() as *mut u8, mem::size_of::<T>());
    out_data_slice[..data_len].copy_from_slice(data_slice);

    our_data_value.assume_init()
}

/// Will fill the table from the header data, and zero out remaining bytes if any are left
///
/// # Safety
///
/// type <T> must be valid when some parts of it is zero
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
static BIOS_TABLES: OnceLock<Result<BiosTables, ()>> = OnceLock::new();

// Note: this requires allocation, so it should be called after the heap is initialized
pub fn get_acpi_tables(multiboot_info: &MultiBoot2Info) -> Result<&'static BiosTables, ()> {
    BIOS_TABLES
        .get_or_init(|| {
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
                                if rsdp_ref.rsdp_v1.revision >= 2 {
                                    return Some(Rsdp::from_v2(rsdp_ref));
                                } else {
                                    return Some(Rsdp::from_v1(&rsdp_ref.rsdp_v1));
                                }
                            }
                        }
                        // Safety: same as above, this pointer is mapped
                        rsdp_ptr = unsafe { rsdp_ptr.add(1) };
                    }

                    None
                })
                .ok_or(())?;

            // Safety: this is called only once and we are sure no other call is using the ACPI memory
            Ok(unsafe { BiosTables::new(rdsp) })
        })
        .as_ref()
        .map_err(|_| ())
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
        let (header_ptr, len) = get_acpi_header_with_len(self.rsdt_address as _);
        // copy the header as value, for later storing in the struct
        let header = *header_ptr;

        let entries_len = (len - size_of::<DescriptionHeader>()) / size_of::<u32>();
        let entries_ptr = header_ptr.add(1) as *const u32;
        // use slice of u8 since we can't use u32 since its not aligned
        let entries_slice = slice::from_raw_parts(entries_ptr as *const u8, entries_len * 4);
        // we copy the addresses here, we can't sadly use iter to iterate over them since inside `from_physical_ptr` we need to be
        // sure that we don't own any references to the ACPI memory regions
        let entries_ptrs = entries_slice
            .chunks(4)
            .map(|a| u32::from_le_bytes(a.try_into().unwrap()))
            .filter(|&a| a != 0)
            .collect::<Vec<_>>();

        // after this call, the `header` pointer is invalid
        // remove allocation so that they can be used below in `from_physical_ptr`
        // Safety: the `header_ptr` is never used after this call
        deallocate_acpi_mapping(header_ptr as _, len);

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
        let (header_ptr, len) = unsafe { get_acpi_header_with_len(ptr as _) };

        // this must not be used as a reference to the header when creating values, as these values use
        // data present after the header in memory
        let header_copy = *header_ptr;

        let body = match &header_copy.signature.0 {
            b"APIC" => DescriptorTableBody::Apic(Box::new(Apic::from_header(header_ptr))),
            b"FACP" => DescriptorTableBody::Facp(Box::new(get_table_from_header(header_ptr))),
            b"HPET" => DescriptorTableBody::Hpet(Box::new(get_table_from_header(header_ptr))),
            b"DSDT" => DescriptorTableBody::Dsdt(Box::new(Xsdt::from_header(header_ptr))),
            b"SSDT" => DescriptorTableBody::Ssdt(Box::new(Xsdt::from_header(header_ptr))),
            b"BGRT" => DescriptorTableBody::Bgrt(Box::new(get_table_from_header(header_ptr))),
            b"WAET" => DescriptorTableBody::Waet(Box::new(get_table_from_header(header_ptr))),
            b"SRAT" => DescriptorTableBody::Srat(Box::new(Srat::from_header(header_ptr))),
            _ => DescriptorTableBody::Unknown(HexArray(
                slice::from_raw_parts(header_ptr as *const u8, header_copy.length as usize)
                    .to_vec(),
            )),
        };
        // after this point, `header_ref` and `header_ptr` are invalid
        // Safety: the `header_ptr` is never used after this call
        deallocate_acpi_mapping(header_ptr as _, len);

        Self {
            header: header_copy,
            body,
        }
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
    unsafe fn from_header(header: *const DescriptionHeader) -> Self {
        let mut apic = Self {
            local_apic_address: 0,
            flags: 0,
            interrupt_controller_structs: Vec::new(),
        };
        let after_header = header.add(1) as *const u32;
        apic.local_apic_address = after_header.read_unaligned();
        apic.flags = after_header.add(1).read_unaligned();

        let mut ptr = after_header.add(2) as *const u8;
        let mut remaining = (*header).length - size_of::<DescriptionHeader>() as u32 - 8;
        while remaining > 0 {
            let struct_type = *ptr;
            let struct_len = *(ptr.add(1));
            let struct_bytes = slice::from_raw_parts(ptr.add(2), struct_len as usize - 2);
            apic.interrupt_controller_structs
                .push(InterruptControllerStruct::from_type_and_bytes(
                    struct_type,
                    struct_bytes,
                ));
            ptr = ptr.add(struct_len as usize);
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

#[derive(Debug, Clone)]
#[allow(dead_code)]
/// This is inside DSDT and SSDT
pub struct Xsdt {
    aml_code: AmlCode,
}

impl Xsdt {
    /// # Safety
    /// the pointer must be valid and point to a valid table
    unsafe fn from_header(header: *const DescriptionHeader) -> Self {
        let dsdt_ptr = header.add(1) as *const u8;
        let data_len = (*header).length as usize - size_of::<DescriptionHeader>();
        let data = slice::from_raw_parts(dsdt_ptr, data_len);
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
    /// # Safety
    /// the pointer must be valid and point to a valid table
    unsafe fn from_header(header: *const DescriptionHeader) -> Self {
        let mut srat = Self {
            reserved1: 0,
            reserved2: 0,
            static_resource_allocation: Vec::new(),
        };
        let after_header = header.add(1) as *const u32;
        srat.reserved1 = after_header.read_unaligned();
        srat.reserved2 = (after_header.add(1) as *const u64).read_unaligned();

        let mut ptr = after_header.add(3) as *const u8;
        let mut remaining = (*header).length - size_of::<DescriptionHeader>() as u32 - 12;
        while remaining > 0 {
            let struct_type = *ptr;
            let struct_len = *(ptr.add(1));
            let struct_bytes = slice::from_raw_parts(ptr.add(2), struct_len as usize - 2);
            srat.static_resource_allocation
                .push(StaticResourceAffinity::from_type_and_bytes(
                    struct_type,
                    struct_bytes,
                ));
            ptr = ptr.add(struct_len as usize);
            remaining -= struct_len as u32;
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
            match entry.body {
                DescriptorTableBody::Dsdt(_) | DescriptorTableBody::Ssdt(_) => {
                    writeln!(f, "{:X?}", entry.header)?;
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
