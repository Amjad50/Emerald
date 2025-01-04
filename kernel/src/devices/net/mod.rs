mod e1000;
use super::pci::{self, PciDeviceConfig};

pub fn try_register_net_device(pci_device: &PciDeviceConfig) -> bool {
    let pci::PciDeviceType::NetworkController(0, 0, _) = pci_device.device_type else {
        return false;
    };
    e1000::try_register(pci_device)
}
