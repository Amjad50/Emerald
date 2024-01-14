use core::fmt;

use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};

use crate::{
    fs::{self, path::Path, FileAttributes, FileSystem, FileSystemError, INode},
    io,
    sync::{once::OnceLock, spin::mutex::Mutex},
};

use self::pci::{PciDeviceConfig, PciDevicePropeIterator};

pub mod clock;
pub mod ide;
pub mod pci;
pub mod pipe;

// TODO: replace with rwlock
static DEVICES: OnceLock<Arc<Mutex<Devices>>> = OnceLock::new();

pub(crate) const DEVICES_FILESYSTEM_CLUSTER_MAGIC: u64 = 0xdef1ce5;
pub(crate) const DEVICES_FILESYSTEM_ROOT_INODE_MAGIC: u64 = 0xdef1ce55007;

#[derive(Debug)]
struct Devices {
    devices: BTreeMap<String, Arc<dyn Device>>,
}

pub trait Device: Sync + Send + fmt::Debug {
    fn name(&self) -> &str;
    fn read(&self, _offset: u64, _buf: &mut [u8]) -> Result<u64, FileSystemError> {
        Err(FileSystemError::ReadNotSupported)
    }
    fn write(&self, _offset: u64, _buf: &[u8]) -> Result<u64, FileSystemError> {
        Err(FileSystemError::WriteNotSupported)
    }
    /// Informs the device that it is closed.
    fn close(&self) -> Result<(), FileSystemError> {
        Ok(())
    }
    /// Informs the device that it is cloned.
    fn clone_device(&self) -> Result<(), FileSystemError> {
        Ok(())
    }
}

impl FileSystem for Mutex<Devices> {
    fn open_root(&self) -> Result<INode, FileSystemError> {
        Ok(INode::new_file(
            String::from("/"),
            FileAttributes::DIRECTORY,
            DEVICES_FILESYSTEM_ROOT_INODE_MAGIC,
            0,
        ))
    }

    fn open_dir(&self, path: &Path) -> Result<Vec<INode>, FileSystemError> {
        if path.is_root() || path.is_empty() {
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
        if !inode.is_dir() {
            return Err(FileSystemError::IsNotDirectory);
        }
        assert_eq!(inode.start_cluster(), DEVICES_FILESYSTEM_ROOT_INODE_MAGIC);
        self.open_dir(Path::new(inode.name()))
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
