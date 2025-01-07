mod e1000;

use crate::net::{MacAddress, NetworkError, NetworkPacket};

use super::pci::{self, PciDeviceConfig};
pub trait NetworkDevice {
    fn mac_address(&self) -> MacAddress;
    fn send(&self, data: &NetworkPacket) -> Result<(), NetworkError>;
    fn receive_into(&self, packet: &mut NetworkPacket) -> Result<bool, NetworkError>;
}

#[allow(dead_code)]
pub fn get_device() -> Option<&'static dyn NetworkDevice> {
    e1000::get_device()
}

pub fn try_register_net_device(pci_device: &PciDeviceConfig) -> bool {
    let pci::PciDeviceType::NetworkController(0, 0, _) = pci_device.device_type else {
        return false;
    };
    e1000::try_register(pci_device)
}
