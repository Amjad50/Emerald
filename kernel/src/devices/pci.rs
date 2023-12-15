use core::fmt;

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
pub mod reg {
    pub const VENDOR_ID: u8 = 0x00;
    pub const DEVICE_ID: u8 = 0x02;
    pub const COMMAND: u8 = 0x04;
    pub const STATUS: u8 = 0x06;
    pub const CLASS_DWORD: u8 = 0x08;
    pub const HEADER_TYPE: u8 = 0x0E;
    pub const BAR0: u8 = 0x10;
    pub const BAR1: u8 = 0x14;
    pub const BAR2: u8 = 0x18;
    pub const BAR3: u8 = 0x1C;
    pub const BAR4: u8 = 0x20;
    pub const BAR5: u8 = 0x24;
    pub const SUBSYSTEM_VENDOR_ID: u8 = 0x2C;
    pub const SUBSYSTEM_ID: u8 = 0x2E;
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

#[repr(u8)]
#[derive(Debug, Clone)]
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
    Reserved(u8, u8, u8, u8),
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
            0x14..=0x3F => Self::Reserved(class_id, subclass_id, prog_if, revision_id),
            0x40 => Self::CoProcessor(subclass_id, prog_if, revision_id),
            0x41..=0xFE => Self::Reserved(class_id, subclass_id, prog_if, revision_id),
            0xFF => Self::Unassigned(subclass_id, prog_if, revision_id),
        }
    }
}

impl fmt::Display for PciDeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Reserved(class, subclass, prog, rev) => {
                write!(
                    f,
                    "Reserved({:02X}.{:02X}.{:02X}.{:02X})",
                    class, subclass, prog, rev
                )
            }
            Self::Unassigned(subclass, prog, rev) => {
                write!(f, "Unassigned({subclass:02X}.{prog:02X}.{rev:02X})")
            }
            Self::Unclassified(subclass, prog, rev) => {
                write!(f, "Unclassified({subclass:02X}.{prog:02X}.{rev:02X})")
            }
            Self::MassStorageController(subclass, prog, rev) => {
                write!(
                    f,
                    "MassStorageController({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::NetworkController(subclass, prog, rev) => {
                write!(f, "NetworkController({subclass:02X}.{prog:02X}.{rev:02X})")
            }
            Self::DisplayController(subclass, prog, rev) => {
                write!(f, "DisplayController({subclass:02X}.{prog:02X}.{rev:02X})")
            }
            Self::MultimediaController(subclass, prog, rev) => {
                write!(
                    f,
                    "MultimediaController({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::MemoryController(subclass, prog, rev) => {
                write!(f, "MemoryController({subclass:02X}.{prog:02X}.{rev:02X})")
            }
            Self::BridgeDevice(subclass, prog, rev) => {
                write!(f, "BridgeDevice({subclass:02X}.{prog:02X}.{rev:02X})")
            }
            Self::SimpleCommunicationController(subclass, prog, rev) => {
                write!(
                    f,
                    "SimpleCommunicationController({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::BaseSystemPeripheral(subclass, prog, rev) => {
                write!(
                    f,
                    "BaseSystemPeripheral({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::InputDeviceController(subclass, prog, rev) => {
                write!(
                    f,
                    "InputDeviceController({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::DockingStation(subclass, prog, rev) => {
                write!(f, "DockingStation({subclass:02X}.{prog:02X}.{rev:02X})")
            }
            Self::Processor(subclass, prog, rev) => {
                write!(f, "Processor({subclass:02X}.{prog:02X}.{rev:02X})")
            }
            Self::SerialBusController(subclass, prog, rev) => {
                write!(
                    f,
                    "SerialBusController({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }

            Self::WirelessController(subclass, prog, rev) => {
                write!(f, "WirelessController({subclass:02X}.{prog:02X}.{rev:02X})")
            }
            Self::IntelligentController(subclass, prog, rev) => {
                write!(
                    f,
                    "IntelligentController({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::SatelliteCommunicationController(subclass, prog, rev) => {
                write!(
                    f,
                    "SatelliteCommunicationController({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::EncryptionController(subclass, prog, rev) => {
                write!(
                    f,
                    "EncryptionController({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::SignalProcessingController(subclass, prog, rev) => {
                write!(
                    f,
                    "SignalProcessingController({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::ProcessingAccelerator(subclass, prog, rev) => {
                write!(
                    f,
                    "ProcessingAccelerator({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::NonEssentialInstrumentation(subclass, prog, rev) => {
                write!(
                    f,
                    "NonEssentialInstrumentation({subclass:02X}.{prog:02X}.{rev:02X})"
                )
            }
            Self::CoProcessor(subclass, prog, rev) => {
                write!(f, "CoProcessor({subclass:02X}.{prog:02X}.{rev:02X})")
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum PciBar {
    Memory {
        addr: u64,
        size: u64,
        prefetchable: bool,
    },
    Io {
        addr: u16,
        size: u64,
    },
    None,
}

#[allow(dead_code)]
impl PciBar {
    pub fn get_io(&self) -> Option<(u16, u64)> {
        match self {
            Self::Io { addr, size } => Some((*addr, *size)),
            _ => None,
        }
    }

    pub fn get_memory(&self) -> Option<(u64, u64, bool)> {
        match self {
            Self::Memory {
                addr,
                size,
                prefetchable,
            } => Some((*addr, *size, *prefetchable)),
            _ => None,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PciDeviceConfig {
    pub(super) bus: u8,
    pub(super) dev: u8,
    pub(super) func: u8,
    pub(super) vendor_id: u16,
    pub(super) device_id: u16,
    pub(super) device_type: PciDeviceType,
    pub(super) header_type: u8,
    pub(super) base_address: [PciBar; 6],
    pub(super) interrupt_line: u8,
    pub(super) interrupt_pin: u8,
    pub(super) capabilities_ptr: Option<u8>,
}

impl PciDeviceConfig {
    pub fn probe(bus: u8, dev: u8, func: u8) -> Option<PciDeviceConfig> {
        let vendor_id = read_pci_config(bus, dev, func, reg::VENDOR_ID);
        if vendor_id == 0xFFFF {
            return None;
        }
        let device_id = read_pci_config(bus, dev, func, reg::DEVICE_ID);

        let class_dword = read_pci_config(bus, dev, func, reg::CLASS_DWORD);
        let device_type = PciDeviceType::new(class_dword);

        let header_type = read_pci_config(bus, dev, func, reg::HEADER_TYPE);
        // make sure first we are reading the intended header type
        if header_type & 0x7F != 0x00 {
            return None;
        }
        // standard header
        let mut base_address = [PciBar::None; 6];
        let mut i = 0;
        while i < 6 {
            let bar_v = read_pci_config::<u32>(bus, dev, func, reg::BAR0 + i * 4);
            if bar_v == 0 {
                i += 1;
                continue;
            }

            if bar_v & 1 == 1 {
                // IO
                let old_bar = bar_v & 0xFFFF_FFFC;

                write_pci_config(bus, dev, func, reg::BAR0 + i * 4, 0xFFFF_FFFCu32);
                let bar = read_pci_config::<u32>(bus, dev, func, reg::BAR0 + i * 4);
                write_pci_config(bus, dev, func, reg::BAR0 + i * 4, old_bar);
                let size = (!(bar & 0xFFFF_FFFC) + 1) as u64;

                base_address[i as usize] = PciBar::Io {
                    addr: old_bar as u16,
                    size,
                };
            } else {
                // Memory
                let old_bar = bar_v & 0xFFFF_FFF0;

                write_pci_config(bus, dev, func, reg::BAR0 + i * 4, 0xFFFF_FFF0u32);
                let bar = read_pci_config::<u32>(bus, dev, func, reg::BAR0 + i * 4);
                write_pci_config(bus, dev, func, reg::BAR0 + i * 4, old_bar);
                let size = (!(bar & 0xFFFF_FFF0) + 1) as u64;

                let prefetchable = (bar_v & 0x8) == 0x8;
                let ty = (bar_v & 0x6) >> 1;
                match ty {
                    0x0 => {
                        // 32-bit
                        base_address[i as usize] = PciBar::Memory {
                            addr: old_bar as u64,
                            size,
                            prefetchable,
                        }
                    }
                    0x2 => {
                        // 64-bit
                        assert!(i < 5);
                        let bar_2 = read_pci_config::<u32>(bus, dev, func, reg::BAR0 + (i + 1) * 4);
                        i += 1;

                        let whole_bar = (bar_2 as u64) << 32 | (old_bar as u64);
                        base_address[i as usize] = PciBar::Memory {
                            addr: whole_bar,
                            size,
                            prefetchable,
                        };
                        // store it in the two bars
                        base_address[(i - 1) as usize] = base_address[i as usize];
                    }
                    _ => panic!("Reserved bar memory type {ty}, BAR{i}=0x{bar_v:08X}"),
                };
            }
            i += 1;
        }
        let interrupt_info = read_pci_config::<u16>(bus, dev, func, reg::INTERRUPT_LINE);
        let interrupt_line = interrupt_info as u8;
        let interrupt_pin = (interrupt_info >> 8) as u8;

        let capabilities_ptr = read_pci_config(bus, dev, func, reg::CAPABILITIES_PTR);
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

    pub fn read_config<T: IoPortInt>(&self, offset: u8) -> T {
        read_pci_config(self.bus, self.dev, self.func, offset)
    }

    pub fn write_config<T: IoPortInt>(&self, offset: u8, value: T) {
        write_pci_config(self.bus, self.dev, self.func, offset, value);
    }

    #[allow(dead_code)]
    pub fn write_command(&self, value: u16) {
        self.write_config(reg::COMMAND, value);
    }

    #[allow(dead_code)]
    pub fn read_command(&self) -> u16 {
        self.read_config(reg::COMMAND)
    }

    #[allow(dead_code)]
    pub fn write_status(&self, value: u16) {
        self.write_config(reg::STATUS, value);
    }

    #[allow(dead_code)]
    pub fn read_status(&self) -> u16 {
        self.read_config(reg::STATUS)
    }
}

// Some extra args for probing
// this is used since some PCI devices might produce multiple devices
// so we use this to select which one to probe and store them individually
// so that its easier to interact with them
pub struct PropeExtra {
    pub args: [u64; 4],
}

pub trait PciDevice {
    fn probe_init(config: &PciDeviceConfig, extra: PropeExtra) -> Option<Self>
    where
        Self: Sized;
    fn device_name(&self) -> &'static str;
}
