use crate::cpu::{self, IoPortInt};

fn read_pci_config<T: IoPortInt>(bus: u8, dev: u8, func: u8, offset: u8) -> T {
    let address = 0x80000000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | (offset as u32);
    unsafe {
        cpu::io_out(0xCF8, address);
        cpu::io_in(0xCFC)
    }
}

fn write_pci_config<T: IoPortInt>(bus: u8, dev: u8, func: u8, offset: u8, value: T) {
    let address = 0x80000000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | (offset as u32);
    unsafe {
        cpu::io_out(0xCF8, address);
        cpu::io_out(0xCFC, value);
    }
}

#[allow(dead_code)]
mod regs {
    pub const VENDOR_ID: u8 = 0x00;
    pub const DEVICE_ID: u8 = 0x02;
    pub const COMMAND: u8 = 0x04;
    pub const STATUS: u8 = 0x06;
    pub const CLASS_DWORD: u8 = 0x08;
    pub const HEADER_TYPE: u8 = 0x0E;
    pub const BAR_START: u8 = 0x10;
    pub const CAPABILITIES_PTR: u8 = 0x34;
    pub const INTERRUPT_LINE: u8 = 0x3C;
    pub const INTERRUPT_PIN: u8 = 0x3D;
}

pub struct PciDevicePropeIterator {
    bus: u8,
    dev: u8,
    func: u8,
}

impl PciDevicePropeIterator {
    pub fn new() -> PciDevicePropeIterator {
        PciDevicePropeIterator {
            bus: 0,
            dev: 0,
            func: 0,
        }
    }
}

impl Iterator for PciDevicePropeIterator {
    type Item = PciDeviceConfig;

    fn next(&mut self) -> Option<Self::Item> {
        // loop until we find a valid device
        loop {
            while self.dev < 32 {
                while self.func < 8 {
                    let config = PciDeviceConfig::probe(self.bus, self.dev, self.func);
                    self.func += 1;
                    if config.is_some() {
                        return config;
                    }
                }
                self.func = 0;
                self.dev += 1;
            }
            self.dev = 0;
            let (bus, overflow) = self.bus.overflowing_add(1);
            if overflow {
                break;
            }
            self.bus = bus;
        }

        None
    }
}

#[derive(Debug)]
pub enum PciDeviceType {
    Unclassified(u8, u8, u8),
    MassStorageController(u8, u8, u8),
    NetworkController(u8, u8, u8),
    DisplayController(u8, u8, u8),
    MultimediaController(u8, u8, u8),
    MemoryController(u8, u8, u8),
    BridgeDevice(u8, u8, u8),
    SimpleCommunicationController(u8, u8, u8),
    BaseSystemPeripheral(u8, u8, u8),
    InputDeviceController(u8, u8, u8),
    DockingStation(u8, u8, u8),
    Processor(u8, u8, u8),
    SerialBusController(u8, u8, u8),
    WirelessController(u8, u8, u8),
    IntelligentController(u8, u8, u8),
    SatelliteCommunicationController(u8, u8, u8),
    EncryptionController(u8, u8, u8),
    SignalProcessingController(u8, u8, u8),
    ProcessingAccelerator(u8, u8, u8),
    NonEssentialInstrumentation(u8, u8, u8),
    CoProcessor(u8, u8, u8),
    Reserved(u8, u8, u8),
    Unassigned(u8, u8, u8),
}

impl PciDeviceType {
    pub fn new(class_dword: u32) -> Self {
        let class_id = (class_dword >> 24) as u8;
        let subclass_id = ((class_dword >> 16) & 0xFF) as u8;
        let prog_if = ((class_dword >> 8) & 0xFF) as u8;
        let revision_id = (class_dword & 0xFF) as u8;

        match class_id {
            0x00 => Self::Unclassified(subclass_id, prog_if, revision_id),
            0x01 => Self::MassStorageController(subclass_id, prog_if, revision_id),
            0x02 => Self::NetworkController(subclass_id, prog_if, revision_id),
            0x03 => Self::DisplayController(subclass_id, prog_if, revision_id),
            0x04 => Self::MultimediaController(subclass_id, prog_if, revision_id),
            0x05 => Self::MemoryController(subclass_id, prog_if, revision_id),
            0x06 => Self::BridgeDevice(subclass_id, prog_if, revision_id),
            0x07 => Self::SimpleCommunicationController(subclass_id, prog_if, revision_id),
            0x08 => Self::BaseSystemPeripheral(subclass_id, prog_if, revision_id),
            0x09 => Self::InputDeviceController(subclass_id, prog_if, revision_id),
            0x0A => Self::DockingStation(subclass_id, prog_if, revision_id),
            0x0B => Self::Processor(subclass_id, prog_if, revision_id),
            0x0C => Self::SerialBusController(subclass_id, prog_if, revision_id),
            0x0D => Self::WirelessController(subclass_id, prog_if, revision_id),
            0x0E => Self::IntelligentController(subclass_id, prog_if, revision_id),
            0x0F => Self::SatelliteCommunicationController(subclass_id, prog_if, revision_id),
            0x10 => Self::EncryptionController(subclass_id, prog_if, revision_id),
            0x11 => Self::SignalProcessingController(subclass_id, prog_if, revision_id),
            0x12 => Self::ProcessingAccelerator(subclass_id, prog_if, revision_id),
            0x13 => Self::NonEssentialInstrumentation(subclass_id, prog_if, revision_id),
            0x14..=0x3F => Self::Reserved(subclass_id, prog_if, revision_id),
            0x40 => Self::CoProcessor(subclass_id, prog_if, revision_id),
            0x41..=0xFE => Self::Reserved(subclass_id, prog_if, revision_id),
            0xFF => Self::Unassigned(subclass_id, prog_if, revision_id),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PciBarAddress {
    Memory(u64, bool),
    Io(u16),
    None,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct PciBar {
    address: PciBarAddress,
    size: u64,
}

impl PciBar {
    pub const fn empty() -> PciBar {
        PciBar {
            address: PciBarAddress::None,
            size: 0,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct PciDeviceConfig {
    bus: u8,
    dev: u8,
    func: u8,
    vendor_id: u16,
    device_id: u16,
    device_type: PciDeviceType,
    header_type: u8,
    base_address: [PciBar; 6],
    interrupt_line: u8,
    interrupt_pin: u8,
    capabilities_ptr: Option<u8>,
}

impl PciDeviceConfig {
    pub fn probe(bus: u8, dev: u8, func: u8) -> Option<PciDeviceConfig> {
        let vendor_id = read_pci_config(bus, dev, func, regs::VENDOR_ID);
        if vendor_id == 0xFFFF {
            return None;
        }
        let device_id = read_pci_config(bus, dev, func, regs::DEVICE_ID);

        let class_dword = read_pci_config(bus, dev, func, regs::CLASS_DWORD);
        let device_type = PciDeviceType::new(class_dword);

        let header_type = read_pci_config(bus, dev, func, regs::HEADER_TYPE);
        // make sure first we are reading the intended header type
        if header_type & 0x7F != 0x00 {
            return None;
        }
        // standard header
        let mut base_address = [PciBar::empty(); 6];
        let mut i = 0;
        while i < 6 {
            let bar_v = read_pci_config::<u32>(bus, dev, func, regs::BAR_START + i * 4);
            if bar_v == 0 {
                i += 1;
                continue;
            }

            if bar_v & 1 == 1 {
                // IO
                let old_bar = bar_v & 0xFFFF_FFFC;

                write_pci_config(bus, dev, func, regs::BAR_START + i * 4, 0xFFFF_FFFCu32);
                let bar = read_pci_config::<u32>(bus, dev, func, regs::BAR_START + i * 4);
                write_pci_config(bus, dev, func, regs::BAR_START + i * 4, old_bar);
                let size = (!(bar & 0xFFFF_FFFC) + 1) as u64;

                base_address[i as usize] = PciBar {
                    address: PciBarAddress::Io(old_bar as u16),
                    size,
                };
            } else {
                // Memory
                let old_bar = bar_v & 0xFFFF_FFF0;

                write_pci_config(bus, dev, func, regs::BAR_START + i * 4, 0xFFFF_FFF0u32);
                let bar = read_pci_config::<u32>(bus, dev, func, regs::BAR_START + i * 4);
                write_pci_config(bus, dev, func, regs::BAR_START + i * 4, old_bar);
                let size = (!(bar & 0xFFFF_FFF0) + 1) as u64;

                let prefetchable = (bar_v & 0x8) == 0x8;
                let ty = bar_v & 0x6;
                let address = match ty {
                    0x0 => {
                        // 32-bit
                        PciBarAddress::Memory(old_bar as u64, prefetchable)
                    }
                    0x2 => {
                        // 64-bit
                        assert!(i < 5);
                        let bar_2 =
                            read_pci_config::<u32>(bus, dev, func, regs::BAR_START + (i + 1) * 4);
                        i += 1;

                        let whole_bar = (bar_2 as u64) << 32 | (old_bar as u64);
                        PciBarAddress::Memory(whole_bar, prefetchable)
                    }
                    _ => panic!("Reserved bar memory type 1, BAR{i}=0x{bar_v:08X}"),
                };

                base_address[i as usize] = PciBar { address, size };
            }
            i += 1;
        }
        let interrupt_info = read_pci_config::<u16>(bus, dev, func, regs::INTERRUPT_LINE);
        let interrupt_line = interrupt_info as u8;
        let interrupt_pin = (interrupt_info >> 8) as u8;

        let capabilities_ptr = read_pci_config(bus, dev, func, regs::CAPABILITIES_PTR);
        let capabilities_ptr = if capabilities_ptr == 0 {
            None
        } else {
            Some(capabilities_ptr)
        };

        Some(PciDeviceConfig {
            bus,
            dev,
            func,
            vendor_id,
            device_id,
            device_type,
            header_type,
            base_address,
            interrupt_line,
            interrupt_pin,
            capabilities_ptr,
        })
    }
}
