use core::fmt;

use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};

use crate::{
    fs::{self, FileAttributes, FileSystem, FileSystemError, INode},
    io,
    sync::{once::OnceLock, spin::mutex::Mutex},
};

use self::pci::{PciDeviceConfig, PciDevicePropeIterator};

pub mod clock;
pub mod ide;
pub mod pci;

// TODO: replace with rwlock
static DEVICES: OnceLock<Arc<Mutex<Devices>>> = OnceLock::new();

pub(crate) const DEVICES_FILESYSTEM_CLUSTER_MAGIC: u32 = 0xdef1ce5;

#[derive(Debug)]
struct Devices {
    devices: BTreeMap<String, Arc<dyn Device>>,
}

pub trait Device: Sync + Send + fmt::Debug {
    fn name(&self) -> &str;
    fn read(&self, _offset: u32, _buf: &mut [u8]) -> Result<u64, FileSystemError> {
        Err(FileSystemError::ReadNotSupported)
    }
    fn write(&self, _offset: u32, _buf: &[u8]) -> Result<u64, FileSystemError> {
        Err(FileSystemError::WriteNotSupported)
    }
}

impl FileSystem for Mutex<Devices> {
    fn open_dir(&self, path: &str) -> Result<Vec<INode>, FileSystemError> {
        if path == "/" {
            Ok(self
                .lock()
                .devices
                .iter()
                .map(|(name, device)| {
                    INode::new_device(name.clone(), FileAttributes::EMPTY, Some(device.clone()))
                })
                .collect())
        } else {
            Err(FileSystemError::FileNotFound)
        }
    }

    fn read_dir(&self, inode: &INode) -> Result<Vec<INode>, FileSystemError> {
        assert_eq!(inode.start_cluster(), DEVICES_FILESYSTEM_CLUSTER_MAGIC);
        self.open_dir(inode.name())
    }
}

pub fn init_devices_mapping() {
    DEVICES
        .set(Arc::new(Mutex::new(Devices {
            devices: BTreeMap::new(),
        })))
        .expect("Devices already initialized");

    fs::mount("/devices", DEVICES.get().clone());
}

#[allow(dead_code)]
pub fn register_device(device: Arc<dyn Device>) {
    let mut devices = DEVICES.get().lock();
    assert!(
        !devices.devices.contains_key(device.name()),
        "Device {} already registered",
        device.name()
    );
    devices.devices.insert(String::from(device.name()), device);
}

pub fn prope_pci_devices() {
    let pci_device_iter = PciDevicePropeIterator::new();
    for device in pci_device_iter {
        if probe_pci_driver(&device) {
            println!(
                "[{:02X}.{:02X}.{:02X}] Driver found for device: {:04X}:{:04X} - {}",
                device.bus,
                device.dev,
                device.func,
                device.vendor_id,
                device.device_id,
                device.device_type
            );
        } else {
            println!(
                "[{:02X}.{:02X}.{:02X}] No driver found for device: {:04X}:{:04X} - {}",
                device.bus,
                device.dev,
                device.func,
                device.vendor_id,
                device.device_id,
                device.device_type
            );
        }
    }
}

pub fn probe_pci_driver(pci_device: &PciDeviceConfig) -> bool {
    ide::try_register_ide_device(pci_device)
    // add more devices here
}

/// Devices such as PS/2 keyboard, mouse, serial ports, etc.
pub fn init_legacy_devices() {
    io::keyboard::init_keyboard()
}
