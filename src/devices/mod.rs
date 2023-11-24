use self::pci::{PciDeviceConfig, PciDevicePropeIterator};

pub mod ide;
pub mod pci;

pub fn register_devices() {
    let pci_device_iter = PciDevicePropeIterator::new();
    for device in pci_device_iter {
        if probe_driver(&device) {
            println!(
                "Driver found for device: {:04X}:{:04X} - {}",
                device.vendor_id, device.device_id, device.device_type
            );
        } else {
            println!(
                "No driver found for device: {:04X}:{:04X} - {}",
                device.vendor_id, device.device_id, device.device_type
            );
        }
    }
}

pub fn probe_driver(pci_device: &PciDeviceConfig) -> bool {
    ide::try_register_ide_device(pci_device)
    // add more devices here
}
