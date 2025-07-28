use core::{
    fmt, hint, mem,
    ptr::{addr_of, addr_of_mut},
    sync::atomic::AtomicBool,
};

use alloc::sync::Arc;
use tracing::{error, info};

use crate::{
    cpu::{
        self,
        idt::{BasicInterruptHandler, InterruptStackFrame64},
        interrupts::apic,
    },
    memory_management::memory_layout::MemSize,
    sync::spin::mutex::Mutex,
};

use super::pci::{self, PciDevice, PciDeviceConfig, PciDeviceType, ProbeExtra};

static mut IDE_DEVICES: [Option<Arc<IdeDevice>>; 4] = [None, None, None, None];
static INTERRUPTS_SETUP: AtomicBool = AtomicBool::new(false);

pub fn try_register_ide_device(pci_device: &PciDeviceConfig) -> bool {
    let PciDeviceType::MassStorageController(..) = pci_device.device_type else {
        return false;
    };

    let mut found_device = false;
    for i in 0..2 {
        // i = 0 => primary
        // i = 1 => secondary
        for j in 0..2 {
            // j = 0 => master
            // j = 1 => slave
            let extra = ProbeExtra { args: [i, j, 0, 0] };
            let Some(ide_device) = IdeDevice::probe_init(pci_device, extra) else {
                continue;
            };

            // SAFETY: we are muting only to add elements, and we are not accessing the old elements or changing them
            let ide_devices = unsafe { addr_of_mut!(IDE_DEVICES).as_mut().unwrap() };
            let slot = ide_devices.iter_mut().find(|x| x.is_none());

            if let Some(slot) = slot {
                // must be done after initializing the heap, i.e. after virtual memory
                *slot = Some(Arc::new(ide_device));
                found_device = true;
            } else {
                panic!("No more IDE devices can be registered!");
            }
        }
    }

    found_device
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdeDeviceType {
    Ata,
    Atapi,
}

#[derive(Debug, Clone, Copy)]
pub struct IdeDeviceIndex {
    pub ty: IdeDeviceType,
    pub index: usize,
}

pub fn get_ide_device(index: IdeDeviceIndex) -> Option<Arc<IdeDevice>> {
    let ide_devices = unsafe { addr_of!(IDE_DEVICES).as_ref().unwrap() };
    let mut passed = 0;
    if index.index < ide_devices.len() {
        for ide_device in ide_devices.iter().filter_map(Option::as_ref) {
            if ide_device.device_type == index.ty {
                if passed == index.index {
                    return Some(ide_device.clone());
                }
                passed += 1;
            }
        }
    }
    None
}

#[allow(dead_code)]
mod pci_cfg {
    // program interface
    pub const PROG_IF_PRIMARY: u8 = 1 << 0;
    pub const PROG_IF_PRIMARY_PROGRAMMABLE: u8 = 1 << 1;
    pub const PROG_IF_SECONDARY: u8 = 1 << 2;
    pub const PROG_IF_SECONDARY_PROGRAMMABLE: u8 = 1 << 3;
    pub const PROG_IF_MASTER: u8 = 1 << 7;

    pub const DEFAULT_PRIMARY_IO: (u16, u16) = (0x1F0, 0x3F6);
    pub const DEFAULT_SECONDARY_IO: (u16, u16) = (0x170, 0x376);

    // commands
    pub const CMD_IO_SPACE: u16 = 1 << 0;
    pub const CMD_MEM_SPACE: u16 = 1 << 1;
    pub const CMD_BUS_MASTER: u16 = 1 << 2;
    pub const CMD_INT_DISABLE: u16 = 1 << 10;

    // interrupts
    pub const CFG_CONTROL_REG: u8 = 0x40;
    pub const DEFAULT_PRIMARY_INTERRUPT: u8 = 14;
    pub const DEFAULT_SECONDARY_INTERRUPT: u8 = 15;
}

#[allow(dead_code)]
mod ata {
    pub const DATA: u16 = 0x0;

    pub const ERROR: u16 = 0x1;
    pub const FEATURES: u16 = 0x1;

    pub const SECTOR_COUNT: u16 = 0x2;
    pub const LBA_LO: u16 = 0x3;
    pub const LBA_MID: u16 = 0x4;
    pub const LBA_HI: u16 = 0x5;

    pub const DRIVE: u16 = 0x6;

    pub const COMMAND: u16 = 0x7;
    pub const STATUS: u16 = 0x7;

    pub const ALT_STATUS: u16 = 0x0;
    pub const DEV_CONTROL: u16 = 0x0;

    pub const ERROR_ILL_LENGTH: u8 = 1 << 0;
    pub const ERROR_END_OF_MEDIA: u8 = 1 << 1;
    pub const ERROR_ABORTED: u8 = 1 << 2;
    pub const ERROR_ID_NOT_FOUND: u8 = 1 << 4;
    pub const ERROR_UNCORRECTABLE: u8 = 1 << 6;
    pub const ERROR_BAD_BLOCK: u8 = 1 << 7;
    // specific to SCSI
    pub const ERROR_SENSE_KEY: u8 = 0xF << 4;

    pub const SENSE_OK: u8 = 0x0;
    pub const SENSE_RECOVERED_ERROR: u8 = 0x1;
    pub const SENSE_NOT_READY: u8 = 0x2;
    pub const SENSE_MEDIUM_ERROR: u8 = 0x3;
    pub const SENSE_HARDWARE_ERROR: u8 = 0x4;
    pub const SENSE_ILLEGAL_REQUEST: u8 = 0x5;
    pub const SENSE_UNIT_ATTENTION: u8 = 0x6;
    pub const SENSE_DATA_PROTECT: u8 = 0x7;
    pub const SENSE_BLANK_CHECK: u8 = 0x8;
    pub const SENSE_VENDOR_SPECIFIC: u8 = 0x9;
    pub const SENSE_COPY_ABORTED: u8 = 0xA;
    pub const SENSE_ABORTED_COMMAND: u8 = 0xB;
    pub const SENSE_VOLUME_OVERFLOW: u8 = 0xD;
    pub const SENSE_MISCOMPARE: u8 = 0xE;

    pub const STATUS_ERR: u8 = 1 << 0;
    pub const STATUS_SENSE_DATA: u8 = 1 << 1;
    pub const STATUS_ALIGN_ERR: u8 = 1 << 2;
    pub const STATUS_DATA_REQUEST: u8 = 1 << 3;
    pub const STATUS_DEFERRED_WRITE_ERR: u8 = 1 << 4;
    pub const STATUS_DRIVE_FAULT: u8 = 1 << 5;
    pub const STATUS_READY: u8 = 1 << 6;
    pub const STATUS_BUSY: u8 = 1 << 7;

    pub const COMMAND_IDENTIFY: u8 = 0xEC;
    pub const COMMAND_PACKET_IDENTIFY: u8 = 0xA1;
    pub const COMMAND_READ_SECTORS: u8 = 0x20;
    pub const COMMAND_WRITE_SECTORS: u8 = 0x30;
    pub const COMMAND_DEVICE_RESET: u8 = 0x08;
    pub const COMMAND_PACKET: u8 = 0xA0;

    pub const PACKET_FEAT_DMA: u8 = 1 << 0;
    pub const PACKET_FEAT_DMA_DIR_FROM_DEVICE: u8 = 1 << 2;

    pub const PACKET_CMD_READ_10: u8 = 0x28;
    pub const PACKET_CMD_INQUIRY: u8 = 0x12;
    pub const PACKET_CMD_TEST_UNIT_READY: u8 = 0x00;
    pub const PACKET_CMD_READ_CAPACITY: u8 = 0x25;

    pub const PACKET_INQUIRY_VITAL_DATA: u8 = 0x1;
    pub const PACKET_VITAL_PAGE_SUPPORTED_PAGES: u8 = 0x00;
    pub const PACKET_VITAL_PAGE_DEVICE_IDENTIFY: u8 = 0x83;
    pub const PACKET_VITAL_PAGE_BLOCK_LIMITS: u8 = 0xB0;
    pub const PACKET_VITAL_PAGE_BLOCK_LIMITS_SIZE: u16 = 64;

    // size of a sector in bytes
    pub const DEFAULT_SECTOR_SIZE: u32 = 512;
}

#[derive(Debug, Clone, Copy)]
pub struct IdeIo {
    pub command_block: u16,
    pub control_block: u16,
}

impl IdeIo {
    // until not busy
    pub fn wait_until_free(&self) {
        let mut status = self.read_status();
        while status & ata::STATUS_BUSY != 0 {
            hint::spin_loop();
            status = self.read_status();
        }
    }

    pub fn wait_until_can_command(&self) -> Result<(), u8> {
        self.wait_until_free();

        let mut status = self.read_status();
        while status & (ata::STATUS_BUSY | ata::STATUS_DATA_REQUEST) != 0 {
            if status & ata::STATUS_ERR != 0 {
                return Err(self.read_error());
            }
            hint::spin_loop();
            status = self.read_status();
        }
        Ok(())
    }

    pub fn read_data(&self) -> u16 {
        unsafe { cpu::io_in(self.command_block + ata::DATA) }
    }

    pub fn write_data(&self, value: u16) {
        unsafe { cpu::io_out(self.command_block + ata::DATA, value) }
    }

    pub fn read_status(&self) -> u8 {
        self.read_command_block(ata::STATUS)
    }

    /// Note that this is only valid if the device is not busy
    /// and has the `ERROR` status bit set
    pub fn read_error(&self) -> u8 {
        self.read_command_block(ata::ERROR)
    }

    // ATAPI only
    pub fn read_sense_key(&self) -> u8 {
        self.read_error() >> 4
    }

    pub fn read_command_block(&self, offset: u16) -> u8 {
        unsafe { cpu::io_in(self.command_block + offset) }
    }

    pub fn write_command_block(&self, offset: u16, value: u8) {
        unsafe { cpu::io_out(self.command_block + offset, value) }
    }

    #[allow(dead_code)]
    pub fn read_control_block(&self, offset: u16) -> u8 {
        unsafe { cpu::io_in(self.control_block + offset) }
    }

    #[allow(dead_code)]
    pub fn write_control_block(&self, offset: u16, value: u8) {
        unsafe { cpu::io_out(self.control_block + offset, value) }
    }

    pub fn read_data_block(&self, data: &mut [u8]) -> Result<(), u8> {
        if self.read_status() & ata::STATUS_ERR != 0 {
            // error
            return Err(self.read_error());
        }

        // read data
        for i in 0..data.len() / 2 {
            if i.is_multiple_of(256) {
                self.wait_until_free();
            }

            let word = self.read_data();
            data[i * 2] = (word & 0xFF) as u8;
            data[i * 2 + 1] = ((word >> 8) & 0xFF) as u8;
        }

        self.wait_until_free();

        // TODO: replace with error
        assert_eq!(self.read_status() & ata::STATUS_DATA_REQUEST, 0);

        Ok(())
    }

    pub fn write_data_block(&self, data: &[u8]) -> Result<(), u8> {
        if self.read_status() & ata::STATUS_ERR != 0 {
            // error
            return Err(self.read_error());
        }

        // write data
        for i in 0..data.len() / 2 {
            if i.is_multiple_of(256) {
                self.wait_until_free();
            }

            let word = (data[i * 2] as u16) | ((data[i * 2 + 1] as u16) << 8);

            self.write_data(word);
        }

        self.wait_until_free();

        // TODO: replace with error
        assert_eq!(self.read_status() & ata::STATUS_DATA_REQUEST, 0);

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct AtaCommand {
    command: u8,
    drive: u8,
    lba: u64,
    sector_count: u8,
    features: u8,
}

impl AtaCommand {
    pub fn new(command: u8) -> Self {
        Self {
            command,
            drive: 0x40, // always have the 6th bit set (LBA mode)
            lba: 0,
            sector_count: 0,
            features: 0,
        }
    }

    #[allow(dead_code)]
    pub fn with_features(mut self, features: u8) -> Self {
        self.features = features;
        self
    }

    pub fn with_second_drive(mut self, is_second: bool) -> Self {
        if is_second {
            self.drive |= 1 << 4;
        } else {
            self.drive &= !(1 << 4);
        }
        self
    }

    pub fn with_lba(mut self, lba: u64) -> Self {
        self.lba = lba;
        self
    }

    pub fn with_sector_count(mut self, sector_count: u8) -> Self {
        self.sector_count = sector_count;
        self
    }

    pub fn write(&self, io_port: &IdeIo) {
        io_port.write_command_block(ata::FEATURES, self.features);
        io_port.write_command_block(ata::SECTOR_COUNT, self.sector_count);
        io_port.write_command_block(ata::LBA_LO, (self.lba & 0xFF) as u8);
        io_port.write_command_block(ata::LBA_MID, ((self.lba >> 8) & 0xFF) as u8);
        io_port.write_command_block(ata::LBA_HI, ((self.lba >> 16) & 0xFF) as u8);
        io_port.write_command_block(ata::DRIVE, self.drive);
        io_port.write_command_block(ata::COMMAND, self.command);
    }

    pub fn execute_read(&self, io_port: &IdeIo, data: &mut [u8]) -> Result<(), u8> {
        // must be even since we are receiving 16 bit words
        assert_eq!(data.len() % 2, 0);
        io_port.wait_until_can_command()?;
        self.write(io_port);

        io_port.read_data_block(data)
    }

    pub fn execute_write(&self, io_port: &IdeIo, data: &[u8]) -> Result<(), u8> {
        // must be even since we are sending 16 bit words
        assert_eq!(data.len() % 2, 0);
        io_port.wait_until_can_command()?;
        self.write(io_port);

        io_port.write_data_block(data)
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum AtapiDmaDirection {
    FromDevice,
    ToDevice,
    None,
}

#[derive(Debug, Clone, Copy)]
struct AtapiPacketCommand {
    drive: u8,
    dma: AtapiDmaDirection,
    lba: u64,
    params: [u8; 32],

    params_index: usize,
}

impl AtapiPacketCommand {
    pub fn new(command: u8) -> Self {
        let mut params = [0; 32];
        params[0] = command;
        Self {
            drive: 0x40, // always have the 6th bit set (LBA mode)
            dma: AtapiDmaDirection::None,
            lba: 0xFFFF00, // byte count limit of 0xFFFF (should be ignored if not needed)
            params,
            params_index: 1,
        }
    }

    #[allow(dead_code)]
    pub fn with_dma(mut self, dma: AtapiDmaDirection) -> Self {
        self.dma = dma;
        self
    }

    pub fn with_second_drive(mut self, is_second: bool) -> Self {
        if is_second {
            self.drive |= 1 << 4;
        } else {
            self.drive &= !(1 << 4);
        }
        self
    }

    #[allow(dead_code)]
    pub fn with_byte_count_limit(mut self, byte_count_limit: u16) -> Self {
        self.lba = (byte_count_limit as u64) << 8;
        self
    }

    pub fn push_param(mut self, param: u8) -> Self {
        self.params[self.params_index] = param;
        self.params_index += 1;
        self
    }

    // msb first, lsb last
    pub fn push_param_u16(self, param: u16) -> Self {
        self.push_param((param >> 8) as u8)
            .push_param((param & 0xFF) as u8)
    }

    // msb first, lsb last
    pub fn push_param_u32(self, param: u32) -> Self {
        self.push_param((param >> 24) as u8)
            .push_param((param >> 16) as u8)
            .push_param((param >> 8) as u8)
            .push_param((param & 0xFF) as u8)
    }

    pub fn write_packet_command(&self, io_port: &IdeIo) {
        let features = match self.dma {
            AtapiDmaDirection::FromDevice => {
                ata::PACKET_FEAT_DMA | ata::PACKET_FEAT_DMA_DIR_FROM_DEVICE
            }
            AtapiDmaDirection::ToDevice => ata::PACKET_FEAT_DMA,
            AtapiDmaDirection::None => 0,
        };

        io_port.write_command_block(ata::FEATURES, features);
        io_port.write_command_block(ata::SECTOR_COUNT, 0);
        io_port.write_command_block(ata::LBA_LO, (self.lba & 0xFF) as u8);
        io_port.write_command_block(ata::LBA_MID, ((self.lba >> 8) & 0xFF) as u8);
        io_port.write_command_block(ata::LBA_HI, ((self.lba >> 16) & 0xFF) as u8);
        io_port.write_command_block(ata::DRIVE, self.drive);
        io_port.write_command_block(ata::COMMAND, ata::COMMAND_PACKET);
    }

    pub fn execute(&self, io_port: &IdeIo, data: &mut [u8]) -> Result<(), u8> {
        // must be even since we are receiving 16 bit words
        assert_eq!(data.len() % 2, 0);
        io_port.wait_until_can_command()?;
        self.write_packet_command(io_port);
        io_port.wait_until_free();

        if io_port.read_status() & ata::STATUS_ERR != 0 {
            // ATAPI uses SENSE key
            return Err(io_port.read_sense_key());
        }

        // write command
        let mut param_count = self.params_index;
        // make sure its even
        if param_count & 1 == 1 {
            param_count += 1;
        }
        // SAFETY: we are sure that the params are aligned to u16
        for &param in unsafe { self.params[..param_count].align_to::<u16>().1 } {
            io_port.write_data(param);
        }
        // if for some reason it expects more data
        // push until satisfied
        // since `DATA_REQUEST` is also used to denote that there is data present
        // we might miss that the device is sending us data and not waiting for more data from us,
        // we should break if that's the case
        let mut max = 32;
        while io_port.read_status() & ata::STATUS_DATA_REQUEST != 0 {
            io_port.write_data(0);
            max -= 1;
            if max == 0 {
                break;
            }
        }

        // convert error to sense key
        io_port.read_data_block(data).map_err(|e| e >> 4)
    }
}

#[repr(C, packed(2))]
#[derive(Debug)]
struct CommandIdentifyDataRaw {
    general_config: u16,
    obsolete1: u16,
    specific_config: u16,
    obsolete2: [u16; 4],
    reserved_cfa1: [u16; 2],
    obsolete3: u16,
    serial_number: [u8; 20],
    obsolete4: [u16; 3],
    firmware_revision: [u8; 8],
    model_number: [u8; 40],
    // Bits 7:0 of this word define the maximum number of logical sectors
    // per DRQ data block that the device supports for READ MULTIPLE
    // commands (see 7.26), READ MULTIPLE EXT commands (see 7.27),
    // WRITE MULTIPLE commands (see 7.64), WRITE MULTIPLE EXT
    // commands (see 7.65), and WRITE MULTIPLE EXT FUA commands (see 7.66).
    //
    // For SATA devices, bits 7:0 shall be set to 16 or less.
    max_sectors_per_multiple_commands: u16,
    trusted_computing_features: u16,
    capabilities: [u16; 2],
    obsolete6: [u16; 2],
    unk_53: u16,
    obsolete7: [u16; 5],
    unk_59: u16,
    user_addressable_sectors_28_mode: u32,
    obsolete8: u16,
    unk_63: u16,
    unk_64: u16,
    min_multiword_dma_transfer_cycle_time: u16,
    recommended_multiword_dma_transfer_cycle_time: u16,
    min_pio_transfer_cycle_time_no_flow_control: u16,
    min_pio_transfer_cycle_time_with_ioready: u16,
    additional_supported: u16,
    reserved: u16,
    // reserved fir IDENTIFY PACKET DEVICE command
    reserved2: [u16; 4],
    queue_depth: u16,
    serial_ata_capabilities: [u16; 2],
    serial_ata_features_supported: u16,
    serial_ata_features_enabled: u16,
    major_version: u16,
    minor_version: u16,
    command_set_supported_or_enabled: [u16; 6],
    ultra_dma_modes: u16,
    unk_89: u16,
    unk_90: u16,
    current_apm_level: u16,
    master_password_id: u16,
    hardware_reset_result: u16,
    obsolete9: u16,
    stream_min_request_size: u16,
    stream_dma_time: u16,
    stream_access_latency: u16,
    stream_performance_granularity: u32,
    user_addressable_sectors: u64,
    streaming_transfer_time: u16,
    max_blocks_per_data_set_management: u16,
    physical_logical_sector_size: u16,
    interseek_delay_for_iso_7779: u16,
    world_wide_name: [u16; 4],
    reserved3: [u16; 4],
    obsolete10: u16,
    logical_sector_size: u32,
    command_set_supported_or_enabled2: [u16; 2],
    reserved4: [u16; 4],
    atapi_byte_count_behavior: u16,
    reserved5: u16,
    obsolete11: u16,
    security_status: u16,
    vendor_specific: [u16; 31],
    reserved_cfa2: [u16; 8],
    device_nominal_form_factor: u16,
    data_set_management_trim_support: u16,
    additional_product_id: [u16; 4],
    reserved6: [u16; 2],
    current_media_serial_number: [u16; 30],
    sct_command_transport: u16,
    reserved7: [u16; 2],
    logical_sectors_alignment: u16,
    write_read_verify_sector_count_mode3: u32,
    write_read_verify_sector_count_mode2: u32,
    obsolete12: [u16; 3],
    nominal_media_rotation_rate: u16,
    reserved8: u16,
    obsolete13: u16,
    write_read_verify_feature_set_current_mode: u16,
    reserved9: u16,
    transport_major_version: u16,
    transport_minor_version: u16,
    reserved10: [u16; 6],
    extended_user_addressable_sectors: u64,
    min_blocks_per_download_microcode: u16,
    max_blocks_per_download_microcode: u16,
    reserved11: [u16; 19],
    integrity_word: u16,
}

impl CommandIdentifyDataRaw {
    fn is_valid(&self) -> bool {
        // check the `general_config` is valid
        // check that the serial number is not empty
        // and not all is 0xFF
        ((self.general_config >> 8) != 0xFF && (self.general_config >> 8) != 0x7F)
            && self.serial_number.iter().any(|x| *x != 0)
            && self.serial_number.iter().any(|x| *x != 0xFF)
    }

    fn is_dma_supported(&self) -> bool {
        self.capabilities[0] & (1 << 8) != 0
    }

    fn is_lba_supported(&self) -> bool {
        self.capabilities[0] & (1 << 9) != 0
    }

    fn is_lba48_supported(&self) -> bool {
        self.command_set_supported_or_enabled[1] & (1 << 10) != 0
    }

    fn user_addressable_sectors(&self) -> u64 {
        if self.is_lba48_supported() {
            let extended_number_of_sectors_supported = self.additional_supported & (1 << 3) != 0;

            if extended_number_of_sectors_supported {
                self.extended_user_addressable_sectors
            } else {
                self.user_addressable_sectors
            }
        } else {
            self.user_addressable_sectors_28_mode as u64
        }
    }

    // Return the size of the logical sector in bytes
    fn sector_size(&self) -> u32 {
        let large_logical_sector_supported = self.physical_logical_sector_size & (1 << 12) != 0;
        if large_logical_sector_supported && self.logical_sector_size != 0 {
            assert!(self.logical_sector_size >= 256);
            // the value here is in bytes
            self.logical_sector_size * 2
        } else {
            // default value
            ata::DEFAULT_SECTOR_SIZE
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IdeError {
    DeviceError(u8),
    UnalignedSize,
    BoundsExceeded,
}

impl fmt::Display for IdeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            IdeError::DeviceError(err) => write!(f, "IDE device error: {err}"),
            IdeError::UnalignedSize => write!(f, "unaligned size"),
            IdeError::BoundsExceeded => write!(f, "bounds exceeded"),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct IdeDevice {
    device_impl: Mutex<IdeDeviceImpl>,
    device_type: IdeDeviceType,
    number_of_sectors: u64,
    sector_size: u32,

    second_device_select: bool,
}

impl IdeDevice {
    fn init_new(
        master_io: Option<u16>,
        io: IdeIo,
        pci_device: &PciDeviceConfig,
        second_device_select: bool,
    ) -> Option<Self> {
        IdeDeviceImpl::init_new(master_io, io, pci_device, second_device_select)
    }

    pub fn interrupt(&self) {
        self.device_impl.lock().interrupt();
    }

    pub fn is_primary(&self) -> bool {
        !self.second_device_select
    }

    pub fn is_secondary(&self) -> bool {
        self.second_device_select
    }

    #[allow(dead_code)]
    pub fn number_of_sectors(&self) -> u64 {
        self.number_of_sectors
    }

    pub fn sector_size(&self) -> u32 {
        self.sector_size
    }

    pub fn read_sync(&self, mut start_sector: u64, mut data: &mut [u8]) -> Result<(), IdeError> {
        let sector_size = self.sector_size as u64;
        let buffer_len = data.len() as u64;

        if !buffer_len.is_multiple_of(sector_size) {
            return Err(IdeError::UnalignedSize);
        }
        let mut number_of_sectors = buffer_len / sector_size;

        if start_sector
            .checked_add(number_of_sectors)
            .ok_or(IdeError::BoundsExceeded)?
            >= self.number_of_sectors
        {
            return Err(IdeError::BoundsExceeded);
        }

        if self.device_type == IdeDeviceType::Ata {
            let mut device = self.device_impl.lock();

            while number_of_sectors != 0 {
                let num_now = number_of_sectors.min(255);
                assert!(number_of_sectors >= num_now);
                number_of_sectors -= num_now;

                let (now_data, afterward) = data.split_at_mut((num_now * sector_size) as usize);

                device
                    .read_sync_ata(start_sector, num_now, now_data)
                    .map_err(IdeError::DeviceError)?;

                start_sector += num_now;
                data = afterward;
            }

            Ok(())
        } else {
            self.device_impl
                .lock()
                .read_sync_atapi(start_sector, number_of_sectors, data)
                .map_err(IdeError::DeviceError)
        }
    }

    pub fn write_sync(&self, mut start_sector: u64, mut data: &[u8]) -> Result<(), IdeError> {
        let sector_size = self.sector_size as u64;
        let buffer_len = data.len() as u64;

        if !buffer_len.is_multiple_of(sector_size) {
            return Err(IdeError::UnalignedSize);
        }
        let mut number_of_sectors = buffer_len / sector_size;

        if start_sector
            .checked_add(number_of_sectors)
            .ok_or(IdeError::BoundsExceeded)?
            >= self.number_of_sectors
        {
            return Err(IdeError::BoundsExceeded);
        }

        if self.device_type == IdeDeviceType::Ata {
            let mut device = self.device_impl.lock();

            while number_of_sectors != 0 {
                let num_now = number_of_sectors.min(255);
                assert!(number_of_sectors >= num_now);
                number_of_sectors -= num_now;

                let (now_data, afterward) = data.split_at((num_now * sector_size) as usize);

                device
                    .write_sync_ata(start_sector, num_now, now_data)
                    .map_err(IdeError::DeviceError)?;

                start_sector += num_now;
                data = afterward;
            }

            Ok(())
        } else {
            todo!("write_sync for ATAPI");
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct IdeDeviceImpl {
    // if this is None, then DMA is not supported
    master_io: Option<u16>,
    io: IdeIo,
    pci_device: PciDeviceConfig,
    identify_data: CommandIdentifyDataRaw,
    second_device_select: bool,
}

impl IdeDeviceImpl {
    fn init_new(
        mut master_io: Option<u16>,
        io: IdeIo,
        pci_device: &PciDeviceConfig,
        second_device_select: bool,
    ) -> Option<IdeDevice> {
        // identify device
        let mut identify_data = [0u8; 512];
        let command =
            AtaCommand::new(ata::COMMAND_IDENTIFY).with_second_drive(second_device_select);

        let mut device_type = IdeDeviceType::Ata;

        if let Err(err) = command.execute_read(&io, &mut identify_data) {
            assert_ne!(err & ata::ERROR_ABORTED, 0);
            let lbalo = io.read_command_block(ata::LBA_LO);
            let lbamid = io.read_command_block(ata::LBA_MID);
            let lbahi = io.read_command_block(ata::LBA_HI);

            // ATAPI
            if lbalo == 0x1 && lbamid == 0x14 && lbahi == 0xEB {
                // check that the device is running
                let command = AtapiPacketCommand::new(ata::PACKET_CMD_TEST_UNIT_READY)
                    .with_second_drive(second_device_select);
                if let Err(err) = command.execute(&io, &mut []) {
                    if err == ata::SENSE_NOT_READY {
                        // device not ready (i.e. not present)
                    } else {
                        error!("unknown ATAPI device error: Err={err:02x}");
                    }
                    return None;
                }
            } else {
                error!("unknown IDE device aborted: LBA={lbalo:02x}:{lbamid:02x}:{lbahi:02x}",);
                return None;
            }

            // here we know we are running an ATAPI device
            let command = AtaCommand::new(ata::COMMAND_PACKET_IDENTIFY)
                .with_second_drive(second_device_select);
            if let Err(err) = command.execute_read(&io, &mut identify_data) {
                error!("unknown ATAPI device aborted: Err={err:02x}",);
                return None;
            }
            device_type = IdeDeviceType::Atapi;
        }

        assert_eq!(
            mem::size_of::<CommandIdentifyDataRaw>(),
            identify_data.len()
        );
        let identify_data: CommandIdentifyDataRaw = unsafe { mem::transmute(identify_data) };

        if !identify_data.is_valid() {
            // device is not valid
            return None;
        }

        if !identify_data.is_dma_supported() {
            // DMA is not supported
            master_io = None;
        }
        if !identify_data.is_lba_supported() {
            // panic so that it's easier to catch
            panic!("IDE device does not support LBA mode");
        }

        let number_of_sectors;
        let sector_size;
        match device_type {
            IdeDeviceType::Ata => {
                number_of_sectors = identify_data.user_addressable_sectors();
                sector_size = identify_data.sector_size();
            }
            IdeDeviceType::Atapi => {
                let mut capacity_data = [0u8; 8];
                let command = AtapiPacketCommand::new(ata::PACKET_CMD_READ_CAPACITY)
                    .with_second_drive(second_device_select);
                command.execute(&io, &mut capacity_data).unwrap();

                // data returned is in big endian

                // the number of sectors is the last 4 bytes
                // this denotes the maximum addressable sector, so we add 1
                number_of_sectors = u32::from_be_bytes([
                    capacity_data[0],
                    capacity_data[1],
                    capacity_data[2],
                    capacity_data[3],
                ]) as u64
                    + 1;

                sector_size = u32::from_be_bytes([
                    capacity_data[4],
                    capacity_data[5],
                    capacity_data[6],
                    capacity_data[7],
                ]);
            }
        }

        info!(
            "Initialized IDE device({device_type:?}): size={} ({number_of_sectors} x {sector_size})",
            MemSize(number_of_sectors * sector_size as u64),
        );

        Some(IdeDevice {
            device_impl: Mutex::new(Self {
                master_io,
                io,
                pci_device: pci_device.clone(),
                identify_data,
                second_device_select,
            }),
            device_type,
            number_of_sectors,
            sector_size,
            second_device_select,
        })
    }

    fn read_sync_ata(
        &mut self,
        start_sector: u64,
        len_sectors: u64,
        data: &mut [u8],
    ) -> Result<(), u8> {
        assert!(len_sectors <= u8::MAX as u64);
        // the buffer is enough to hold the data (see read_sync)
        let command = AtaCommand::new(ata::COMMAND_READ_SECTORS)
            .with_lba(start_sector)
            .with_sector_count(len_sectors as u8)
            .with_second_drive(self.second_device_select);

        command.execute_read(&self.io, data)
    }

    fn read_sync_atapi(
        &mut self,
        start_sector: u64,
        len_sectors: u64,
        data: &mut [u8],
    ) -> Result<(), u8> {
        assert!(len_sectors <= u16::MAX as u64);
        assert!(start_sector <= u32::MAX as u64);
        // the buffer is enough to hold the data (see read_sync)
        let command = AtapiPacketCommand::new(ata::PACKET_CMD_READ_10)
            .with_second_drive(self.second_device_select)
            .push_param(0) // flags
            .push_param_u32(start_sector as u32) // lba
            .push_param(0) // group number
            .push_param_u16(len_sectors as u16) // transfer length
            .push_param(0); // control

        command.execute(&self.io, data)
    }

    fn write_sync_ata(
        &mut self,
        start_sector: u64,
        len_sectors: u64,
        data: &[u8],
    ) -> Result<(), u8> {
        assert!(len_sectors <= u8::MAX as u64);
        // the buffer is enough to hold the data (see write_sync)
        let command = AtaCommand::new(ata::COMMAND_WRITE_SECTORS)
            .with_lba(start_sector)
            .with_sector_count(len_sectors as u8)
            .with_second_drive(self.second_device_select);

        command.execute_write(&self.io, data)
    }

    fn interrupt(&mut self) {
        // acknowledge interrupt
        self.io.read_status();
    }
}

impl PciDevice for IdeDevice {
    fn probe_init(config: &PciDeviceConfig, extra: ProbeExtra) -> Option<Self>
    where
        Self: Sized,
    {
        if let PciDeviceType::MassStorageController(0x1, prog_if, ..) = config.device_type {
            let support_dma = prog_if & pci_cfg::PROG_IF_MASTER != 0;
            let mut command = config.read_command();
            command |= pci_cfg::CMD_IO_SPACE;
            if support_dma {
                // enable bus master
                command |= pci_cfg::CMD_BUS_MASTER;
            }
            config.write_command(command);
            let command = config.read_command();
            // make sure we have at least IO space enabled
            if command & pci_cfg::CMD_IO_SPACE == 0 {
                return None;
            }

            // get the IO ports to use
            let io_port = if extra.args[0] == 0 {
                // primary
                if prog_if & pci_cfg::PROG_IF_PRIMARY != 0 {
                    if let Some(primary_io) = config.base_address[0].get_io() {
                        let control_block_io = config.base_address[1].get_io().unwrap().0;
                        (primary_io.0, control_block_io)
                    } else {
                        // the IO ports are not set
                        if prog_if & pci_cfg::PROG_IF_PRIMARY_PROGRAMMABLE != 0 {
                            // we can program them to the default
                            let (bar0, bar1) = pci_cfg::DEFAULT_PRIMARY_IO;
                            // 1 means IO space
                            // FIXME: this makes the `config` has inconsistent state
                            config.write_config(pci::reg::BAR0, bar0 | 1);
                            config.write_config(pci::reg::BAR1, bar1 | 1);
                        }
                        pci_cfg::DEFAULT_PRIMARY_IO
                    }
                } else {
                    pci_cfg::DEFAULT_PRIMARY_IO
                }
            } else {
                // secondary
                if prog_if & pci_cfg::PROG_IF_SECONDARY != 0 {
                    if let Some(secondary_io) = config.base_address[2].get_io() {
                        let control_block_io = config.base_address[3].get_io().unwrap().0;
                        (secondary_io.0, control_block_io)
                    } else {
                        // the IO ports are not set
                        if prog_if & pci_cfg::PROG_IF_SECONDARY_PROGRAMMABLE != 0 {
                            // we can program them to the default
                            let (bar2, bar3) = pci_cfg::DEFAULT_SECONDARY_IO;
                            // 1 means IO space
                            // FIXME: this makes the `config` has inconsistent state
                            config.write_config(pci::reg::BAR2, bar2 | 1);
                            config.write_config(pci::reg::BAR3, bar3 | 1);
                        }
                        pci_cfg::DEFAULT_SECONDARY_IO
                    }
                } else {
                    pci_cfg::DEFAULT_SECONDARY_IO
                }
            };

            let master_io = if support_dma {
                if let Some(master_io) = config.base_address[4].get_io() {
                    Some(master_io.0)
                } else {
                    // the IO ports are not set
                    panic!("DMA is supported but the IO ports are not set")
                }
            } else {
                None
            };

            // setup interrupts if not already done
            if !INTERRUPTS_SETUP.swap(true, core::sync::atomic::Ordering::SeqCst) {
                // setup ide interrupt
                // TODO: we are assuming that this is the interrupt address.
                //       at least, can't find a specific place on all specs for to know for sure if its using
                //       legacy interrupts or something else
                apic::assign_io_irq(
                    ide_interrupt_primary as BasicInterruptHandler,
                    pci_cfg::DEFAULT_PRIMARY_INTERRUPT,
                    &cpu::cpu(),
                );
                apic::assign_io_irq(
                    ide_interrupt_secondary as BasicInterruptHandler,
                    pci_cfg::DEFAULT_SECONDARY_INTERRUPT,
                    &cpu::cpu(),
                );
            }

            let io_port = IdeIo {
                command_block: io_port.0,
                control_block: io_port.1,
            };

            let is_secondary = extra.args[1] == 1;
            IdeDevice::init_new(master_io, io_port, config, is_secondary)
        } else {
            None
        }
    }

    fn device_name(&self) -> &'static str {
        "IDE"
    }
}

extern "x86-interrupt" fn ide_interrupt_primary(_stack_frame: InterruptStackFrame64) {
    let ide_devices = unsafe { addr_of!(IDE_DEVICES).as_ref().unwrap() };
    for ide_device in ide_devices.iter().filter_map(Option::as_ref) {
        if ide_device.is_primary() {
            ide_device.interrupt()
        }
    }
    apic::return_from_interrupt();
}

extern "x86-interrupt" fn ide_interrupt_secondary(_stack_frame: InterruptStackFrame64) {
    let ide_devices = unsafe { addr_of!(IDE_DEVICES).as_ref().unwrap() };
    for ide_device in ide_devices.iter().filter_map(Option::as_ref) {
        if ide_device.is_secondary() {
            ide_device.interrupt()
        }
    }
    apic::return_from_interrupt();
}
