use core::fmt;

use alloc::{collections::BTreeMap, string::String, sync::Arc};
use tracing::info;

use crate::{
    fs::{
        self, DirTreverse, DirectoryNode, FileAttributes, FileNode, FileSystem, FileSystemError,
        Node,
    },
    sync::{once::OnceLock, spin::rwlock::RwLock},
};

use self::{
    keyboard_mouse::{KeyboardDeviceCreator, MouseDeviceCreator},
    pci::{PciDeviceConfig, PciDeviceProbeIterator},
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
    fn open_root(&self) -> Result<DirectoryNode, FileSystemError> {
        Ok(DirectoryNode::without_parent(
            String::from("/"),
            FileAttributes::DIRECTORY,
            DEVICES_FILESYSTEM_ROOT_INODE_MAGIC,
        ))
    }

    fn read_dir(
        &self,
        inode: &DirectoryNode,
        handler: &mut dyn FnMut(Node) -> DirTreverse,
    ) -> Result<(), FileSystemError> {
        assert_eq!(inode.start_cluster(), DEVICES_FILESYSTEM_ROOT_INODE_MAGIC);

        if inode.name().is_empty() || inode.name() == "/" {
            for node in self.read().devices.iter().map(|(name, device)| {
                FileNode::new_device(name.clone(), FileAttributes::EMPTY, device.clone()).into()
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

    fn number_global_refs(&self) -> usize {
        // we have `DEVICES` mutex globally stored
        1
    }
}

pub fn init_devices_mapping() {
    DEVICES
        .set(Arc::new(RwLock::new(Devices {
            devices: BTreeMap::new(),
        })))
        .expect("Devices already initialized");

    fs::mapping::mount("/devices", DEVICES.get().clone()).expect("Mapping failed");
}

pub fn register_device(device: Arc<dyn Device>) {
    let mut devices = DEVICES.get().write();
    assert!(
        !devices.devices.contains_key(device.name()),
        "Device {} already registered",
        device.name()
    );
    info!("Registered {} device", device.name());
    devices.devices.insert(String::from(device.name()), device);
}

pub fn probe_pci_devices() {
    let pci_device_iter = PciDeviceProbeIterator::new();
    for device in pci_device_iter {
        if probe_pci_driver(&device) {
            info!(
                "[{:02X}.{:02X}.{:02X}] Driver found for device: {:04X}:{:04X} - {}",
                device.bus,
                device.dev,
                device.func,
                device.vendor_id,
                device.device_id,
                device.device_type
            );
        } else {
            info!(
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
