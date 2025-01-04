mod e1000;

use core::fmt;

use alloc::vec::Vec;

use crate::net::NetworkFrame;

use super::pci::{self, PciDeviceConfig};

#[derive(Clone, Copy)]
pub struct MacAddress([u8; 6]);

impl MacAddress {
    pub fn new(bytes: [u8; 6]) -> Self {
        MacAddress(bytes)
    }

    pub fn bytes(&self) -> &[u8; 6] {
        &self.0
    }
}

impl fmt::Debug for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

pub trait NetworkDevice {
    fn mac_address(&self) -> MacAddress;
    fn send(&self, data: &dyn NetworkFrame);
    fn receive(&self) -> Option<Vec<u8>>;
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
