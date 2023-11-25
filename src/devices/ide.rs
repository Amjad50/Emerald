use core::{hint, mem, sync::atomic::AtomicBool};

use crate::{
    cpu::{self, idt::InterruptStackFrame64, interrupts::apic},
    memory_management::memory_layout::MemSize,
    sync::spin::mutex::Mutex,
};

use super::pci::{self, PciDevice, PciDeviceConfig};

static mut IDE_DEVICES: [Option<IdeDevicePair>; 4] = [None, None, None, None];
static INTERRUPTS_SETUP: AtomicBool = AtomicBool::new(false);

pub fn try_register_ide_device(pci_device: &PciDeviceConfig) -> bool {
    let Some(ide_device) = IdeDevicePair::probe_init(pci_device) else {
        return false;
    };

    // SAFETY: we are muting only to add elements, and we are not accessing the old elements or changing thems
    let ide_devices = unsafe { &mut IDE_DEVICES };
    let slot = ide_devices.iter_mut().find(|x| x.is_none());

    if let Some(slot) = slot {
        *slot = Some(ide_device);
        true
    } else {
        panic!("No more IDE devices can be registered!");
    }
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
    // sepcific to SCSI
    pub const ERROR_SENSE_KEY: u8 = 0xF << 4;

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
    pub const COMMAND_DEVICE_RESET: u8 = 0x08;
    pub const COMMAND_PACKET: u8 = 0xA0;

    pub const PACKET_FEAT_DMA: u8 = 1 << 0;
    pub const PACKET_FEAT_DMA_DIR_FROM_DEVICE: u8 = 1 << 2;

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
        let mut status = self.read_command_block(ata::STATUS);
        while status & ata::STATUS_BUSY != 0 {
            hint::spin_loop();
            status = self.read_command_block(ata::STATUS);
        }
    }

    pub fn wait_until_can_command(&self) {
        let mut status = self.read_command_block(ata::STATUS);
        while status & (ata::STATUS_BUSY | ata::STATUS_DATA_REQUEST) != 0 {
            hint::spin_loop();
            status = self.read_command_block(ata::STATUS);
        }
    }

    pub fn read_data(&self) -> u16 {
        unsafe { cpu::io_in(self.command_block + ata::DATA) }
    }

    #[allow(dead_code)]
    pub fn write_data(&self, value: u16) {
        unsafe { cpu::io_out(self.command_block + ata::DATA, value) }
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
}

#[allow(dead_code)]
pub struct IdeDevicePair {
    // two devices per io port (selected from the DEVICE register)
    primary: [Option<Mutex<IdeDevice>>; 2],
    secondary: [Option<Mutex<IdeDevice>>; 2],
}

impl PciDevice for IdeDevicePair {
    fn probe_init(config: &PciDeviceConfig) -> Option<Self>
    where
        Self: Sized,
    {
        if let super::pci::PciDeviceType::MassStorageController(0x1, prog_if, ..) =
            config.device_type
        {
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
            let primary_io = if prog_if & pci_cfg::PROG_IF_PRIMARY != 0 {
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
            };
            let secondary_io = if prog_if & pci_cfg::PROG_IF_SECONDARY != 0 {
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
                    ide_interrupt_primary,
                    pci_cfg::DEFAULT_PRIMARY_INTERRUPT,
                    cpu::cpu(),
                );
                apic::assign_io_irq(
                    ide_interrupt_secondary,
                    pci_cfg::DEFAULT_SECONDARY_INTERRUPT,
                    cpu::cpu(),
                );
            }

            let primary_io = IdeIo {
                command_block: primary_io.0,
                control_block: primary_io.1,
            };
            let secondary_io = IdeIo {
                command_block: secondary_io.0,
                control_block: secondary_io.1,
            };

            Some(Self {
                primary: [
                    IdeDevice::init_new(master_io, primary_io, config, false).map(Mutex::new),
                    IdeDevice::init_new(master_io, primary_io, config, true).map(Mutex::new),
                ],
                secondary: [
                    IdeDevice::init_new(master_io, secondary_io, config, false).map(Mutex::new),
                    IdeDevice::init_new(master_io, secondary_io, config, true).map(Mutex::new),
                ],
            })
        } else {
            None
        }
    }

    fn device_name(&self) -> &'static str {
        "IDE"
    }
}

impl IdeDevicePair {
    pub fn primary_interrupt_handler(&self) {
        self.primary.iter().for_each(|x| {
            if let Some(primary) = x {
                primary.lock().interrupt();
            }
        });
    }

    pub fn secondary_interrupt_handler(&self) {
        self.secondary.iter().for_each(|x| {
            if let Some(secondary) = x {
                secondary.lock().interrupt();
            }
        });
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IdeDeviceType {
    Ata,
    Atapi,
}

#[derive(Debug, Clone, Copy)]
struct AtaCommand {
    pub command: u8,
    pub drive: u8,
    pub lba: u64,
    pub sector_count: u16,
    pub features: u8,
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

    #[allow(dead_code)]
    pub fn with_second_drive(mut self, is_second: bool) -> Self {
        if is_second {
            self.drive |= 1 << 4;
        } else {
            self.drive &= !(1 << 4);
        }
        self
    }

    #[allow(dead_code)]
    pub fn with_lba(mut self, lba: u64) -> Self {
        self.lba = lba;
        self
    }

    #[allow(dead_code)]
    pub fn with_sector_count(mut self, sector_count: u16) -> Self {
        self.sector_count = sector_count;
        self
    }

    pub fn write(&self, io_port: &IdeIo) {
        io_port.write_command_block(ata::FEATURES, self.features);
        io_port.write_command_block(ata::SECTOR_COUNT, self.sector_count as u8);
        io_port.write_command_block(ata::LBA_LO, (self.lba & 0xFF) as u8);
        io_port.write_command_block(ata::LBA_MID, ((self.lba >> 8) & 0xFF) as u8);
        io_port.write_command_block(ata::LBA_HI, ((self.lba >> 16) & 0xFF) as u8);
        io_port.write_command_block(ata::DRIVE, self.drive);
        io_port.write_command_block(ata::COMMAND, self.command);
    }

    pub fn execute(&self, io_port: &IdeIo, data: &mut [u8]) -> Result<(), u8> {
        // must be even since we are receiving 16 bit words
        assert!(data.len() % 2 == 0);
        io_port.wait_until_can_command();
        self.write(io_port);
        io_port.wait_until_free();

        if io_port.read_command_block(ata::STATUS) & ata::STATUS_ERR != 0 {
            // error
            return Err(io_port.read_command_block(ata::ERROR));
        }

        // read data
        for i in 0..data.len() / 2 {
            let word = io_port.read_data();
            data[i * 2] = (word & 0xFF) as u8;
            data[i * 2 + 1] = ((word >> 8) & 0xFF) as u8;

            // TODO: maybe not best to check on every read, but when reading multiple sectors
            if io_port.read_command_block(ata::STATUS) & ata::STATUS_BUSY != 0 {
                io_port.wait_until_free();
            }
        }

        Ok(())
    }
}

#[repr(C, packed)]
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
    addional_supported: u16,
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
    reserved4: [u16; 6],
    obsolete11: u16,
    security_status: u16,
    vendor_specific: [u16; 31],
    reserved_cfa2: [u16; 8],
    device_nominal_form_factor: u16,
    data_set_management_trim_support: u16,
    additional_product_id: [u16; 4],
    reserved5: [u16; 2],
    current_media_serial_number: [u16; 30],
    sct_command_transport: u16,
    reserved6: [u16; 2],
    logical_sectors_alignment: u16,
    write_read_verify_sector_count_mode3: u32,
    write_read_verify_sector_count_mode2: u32,
    obsolete12: [u16; 3],
    nominal_media_rotation_rate: u16,
    reserved7: u16,
    obsolete13: u16,
    write_read_verify_feature_set_current_mode: u16,
    reserved8: u16,
    transport_major_version: u16,
    transport_minor_version: u16,
    reserved9: [u16; 6],
    extended_user_addressable_sectors: u64,
    min_blocks_per_download_microcode: u16,
    max_blocks_per_download_microcode: u16,
    reserved10: [u16; 19],
    integrity_word: u16,
}

impl CommandIdentifyDataRaw {
    fn is_valid(&self) -> bool {
        // check that the serial number is not empty
        self.serial_number.iter().any(|x| *x != 0)
    }

    fn is_dma_supported(&self) -> bool {
        self.capabilities[0] & (1 << 8) != 0
    }

    fn is_lba_supported(&self) -> bool {
        self.capabilities[0] & (1 << 9) != 0
    }

    #[allow(dead_code)]
    fn is_lba48_supported(&self) -> bool {
        self.command_set_supported_or_enabled[1] & (1 << 10) != 0
    }

    fn user_addressable_sectors(&self) -> u64 {
        let extended_number_of_sectors_supported = self.addional_supported & (1 << 3) != 0;

        if extended_number_of_sectors_supported {
            self.extended_user_addressable_sectors
        } else {
            self.user_addressable_sectors
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

    fn size_of_device(&self) -> MemSize {
        MemSize((self.user_addressable_sectors() * self.sector_size() as u64) as usize)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct IdeDevice {
    // if this is None, then DMA is not supported
    master_io: Option<u16>,
    io: IdeIo,
    pci_device: PciDeviceConfig,

    identify_data: CommandIdentifyDataRaw,
    device_type: IdeDeviceType,
}

impl IdeDevice {
    pub fn init_new(
        mut master_io: Option<u16>,
        io: IdeIo,
        pci_device: &PciDeviceConfig,
        second_device_select: bool,
    ) -> Option<Self> {
        // identify device
        let mut identify_data = [0u8; 512];
        let command =
            AtaCommand::new(ata::COMMAND_IDENTIFY).with_second_drive(second_device_select);

        let mut device_type = IdeDeviceType::Ata;

        if let Err(err) = command.execute(&io, &mut identify_data) {
            assert_eq!(err, ata::ERROR_ABORTED);
            let lbalo = io.read_command_block(ata::LBA_LO);
            let lbamid = io.read_command_block(ata::LBA_MID);
            let lbahi = io.read_command_block(ata::LBA_HI);

            // ATAPI
            if lbalo == 0x1 && lbamid == 0x14 && lbahi == 0xEB {
                let command = AtaCommand::new(ata::COMMAND_PACKET_IDENTIFY)
                    .with_second_drive(second_device_select);
                if let Err(err) = command.execute(&io, &mut identify_data) {
                    println!("Error: unknown ATAPI device aborted: Err={err:02x}",);
                    return None;
                }
                device_type = IdeDeviceType::Atapi;
            } else {
                println!(
                    "Error: unknown IDE device aborted: LBA={lbalo:02x}:{lbamid:02x}:{lbahi:02x}",
                );
                return None;
            }
        }

        assert!(mem::size_of::<CommandIdentifyDataRaw>() == identify_data.len());
        let identify_data: CommandIdentifyDataRaw = unsafe { core::mem::transmute(identify_data) };

        if !identify_data.is_valid() {
            // device is not valid
            return None;
        }

        if !identify_data.is_dma_supported() {
            // DMA is not supported
            master_io = None;
        }
        if !identify_data.is_lba_supported() {
            // panic so that its easier to catch
            panic!("IDE device does not support LBA mode");
        }

        println!(
            "Initialized IDE device({device_type:?}): size={}",
            identify_data.size_of_device()
        );

        Some(Self {
            master_io,
            io,
            pci_device: pci_device.clone(),
            identify_data,
            device_type,
        })
    }

    #[allow(dead_code)]
    pub fn size(&self) -> MemSize {
        self.identify_data.size_of_device()
    }

    pub fn interrupt(&mut self) {}
}

extern "x86-interrupt" fn ide_interrupt_primary(_stack_frame: InterruptStackFrame64) {
    let ide_devices = unsafe { &IDE_DEVICES };
    for ide_device in ide_devices.iter().filter_map(Option::as_ref) {
        ide_device.primary_interrupt_handler();
    }
    apic::return_from_interrupt();
}

extern "x86-interrupt" fn ide_interrupt_secondary(_stack_frame: InterruptStackFrame64) {
    let ide_devices = unsafe { &IDE_DEVICES };
    for ide_device in ide_devices.iter().filter_map(Option::as_ref) {
        ide_device.secondary_interrupt_handler();
    }
    apic::return_from_interrupt();
}
