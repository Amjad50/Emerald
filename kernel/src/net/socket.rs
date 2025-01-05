use super::{NetworkError, NetworkPacket};

pub struct EthernetSocket {
    _private: (),
}

#[allow(dead_code)]
impl EthernetSocket {
    pub fn new() -> Self {
        Self { _private: () }
    }

    pub fn send(&self, packet: &NetworkPacket) -> Result<(), NetworkError> {
        crate::devices::net::get_device().unwrap().send(packet)
    }

    pub fn receive(&self, packet: &mut NetworkPacket) -> Result<bool, NetworkError> {
        crate::devices::net::get_device()
            .unwrap()
            .receive_into(packet)
    }
}
