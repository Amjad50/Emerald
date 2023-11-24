use self::pci::{PciDeviceConfig, PciDevicePropeIterator};

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

pub fn probe_driver(_pci_device: &PciDeviceConfig) -> bool {
    // add more devices here
    false
}
