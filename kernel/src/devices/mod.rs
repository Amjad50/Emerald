use core::fmt;

use alloc::{collections::BTreeMap, string::String, sync::Arc};
use tracing::info;

use crate::{
    fs::{
        self, DirTreverse, DirectoryNode, FileAttributes, FileNode, FileSystem, FileSystemError,
        Node,
    },
    power,
    sync::{once::OnceLock, spin::rwlock::RwLock},
};

use self::{
    keyboard_mouse::{KeyboardDeviceCreator, MouseDeviceCreator},
    pci::{PciDeviceConfig, PciDeviceProbeIterator},
};

pub mod clock;
pub mod ide;
pub mod keyboard_mouse;
pub mod net;
pub mod pci;
pub mod pipe;

static DEVICES: OnceLock<Devices> = OnceLock::new();

pub(crate) const DEVICES_FILESYSTEM_CLUSTER_MAGIC: u64 = 0xdef1ce5;
pub(crate) const DEVICES_FILESYSTEM_ROOT_INODE_MAGIC: u64 = 0xdef1ce55007;

#[derive(Debug)]
struct Devices {
    devices: RwLock<BTreeMap<String, Arc<dyn Device>>>,
}

impl Devices {
    fn register_device(&self, device: Arc<dyn Device>) {
        let mut devices = self.devices.write();
        assert!(
            !devices.contains_key(device.name()),
            "Device {} already registered",
            device.name()
        );
        info!("Registered {} device", device.name());
        devices.insert(String::from(device.name()), device);
    }

    fn unregister_all_devices(&self) {
        let mut devices = self.devices.write();

        devices.iter_mut().for_each(|(name, device)| {
            let _ = device.close();
            info!("Unregistering {} device", name);
        });

        devices.clear();
    }
}

pub trait Device: Sync + Send + fmt::Debug {
    fn name(&self) -> &str;
    fn read(&self, _offset: u64, _buf: &mut [u8]) -> Result<u64, FileSystemError> {
        Err(FileSystemError::ReadNotSupported)
    }
    fn write(&self, _offset: u64, _buf: &[u8]) -> Result<u64, FileSystemError> {
        Err(FileSystemError::WriteNotSupported)
    }
    fn set_size(&self, _size: u64) -> Result<(), FileSystemError> {
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

struct DevicesFilesystem;

impl FileSystem for DevicesFilesystem {
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
            for node in DEVICES.get().devices.read().iter().map(|(name, device)| {
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

    fn unmount(self: Arc<Self>) {
        // clean the devices
        DEVICES.get().unregister_all_devices();
    }
}

pub fn init_devices_mapping() {
    DEVICES
        .set(Devices {
            devices: RwLock::new(BTreeMap::new()),
        })
        .expect("Devices already initialized");

    // initialize builtin devices
    register_device(Arc::new(power::PowerDevice));

    fs::mapping::mount("/devices", Arc::new(DevicesFilesystem)).expect("Mapping failed");
}

pub fn register_device(device: Arc<dyn Device>) {
    DEVICES.get().register_device(device);
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
    ide::try_register_ide_device(pci_device) || net::try_register_net_device(pci_device)
    // add more devices here
}

/// Devices such as PS/2 keyboard, mouse, serial ports, etc.
pub fn init_legacy_devices() {
    keyboard_mouse::init_device();
    register_device(Arc::new(KeyboardDeviceCreator));
    register_device(Arc::new(MouseDeviceCreator));
}
