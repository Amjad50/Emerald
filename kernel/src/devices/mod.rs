use core::fmt;

use alloc::{collections::BTreeMap, string::String, sync::Arc};

use crate::{
    fs::{self, DirTreverse, FileAttributes, FileSystem, FileSystemError, INode},
    sync::{once::OnceLock, spin::rwlock::RwLock},
};

use self::{
    keyboard_mouse::{KeyboardDeviceCreator, MouseDeviceCreator},
    pci::{PciDeviceConfig, PciDevicePropeIterator},
};

pub mod clock;
pub mod ide;
pub mod keyboard_mouse;
pub mod pci;
pub mod pipe;

static DEVICES: OnceLock<Arc<RwLock<Devices>>> = OnceLock::new();

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
    /// Open the device.
    /// This tells the device manager that when opening a file with this device name, it should
    /// instead use the device returned by this function.
    /// if `None`, it will just use the device directly.
    fn try_create(&self) -> Option<Result<Arc<dyn Device>, FileSystemError>> {
        None
    }
}

impl FileSystem for RwLock<Devices> {
    fn open_root(&self) -> Result<INode, FileSystemError> {
        Ok(INode::new_file(
            String::from("/"),
            FileAttributes::DIRECTORY,
            DEVICES_FILESYSTEM_ROOT_INODE_MAGIC,
            0,
        ))
    }

    fn read_dir(
        &self,
        inode: &INode,
        handler: &mut dyn FnMut(INode) -> DirTreverse,
    ) -> Result<(), FileSystemError> {
        if !inode.is_dir() {
            return Err(FileSystemError::IsNotDirectory);
        }
        assert_eq!(inode.start_cluster(), DEVICES_FILESYSTEM_ROOT_INODE_MAGIC);

        if inode.name().is_empty() || inode.name() == "/" {
            for node in self.read().devices.iter().map(|(name, device)| {
                INode::new_device(name.clone(), FileAttributes::EMPTY, Some(device.clone()))
            }) {
                if let DirTreverse::Stop = handler(node) {
                    break;
                }
            }
            Ok(())
        } else {
            Err(FileSystemError::FileNotFound)
        }
    }
}

pub fn init_devices_mapping() {
    DEVICES
        .set(Arc::new(RwLock::new(Devices {
            devices: BTreeMap::new(),
        })))
        .expect("Devices already initialized");

    fs::mount("/devices", DEVICES.get().clone());
}

pub fn register_device(device: Arc<dyn Device>) {
    let mut devices = DEVICES.get().write();
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
    keyboard_mouse::init_device();
    register_device(Arc::new(KeyboardDeviceCreator));
    register_device(Arc::new(MouseDeviceCreator));
}
