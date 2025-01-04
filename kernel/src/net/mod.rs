use alloc::vec::Vec;

use crate::devices::net::MacAddress;

/// Represent a part of a network stack, and will
/// be written directly into the network DMA buffer
pub trait NetworkFrame {
    fn write_into_buffer(&self, buffer: &mut [u8]) -> Result<(), NetworkError>;
    fn size(&self) -> usize;
    fn from_buffer(buffer: &[u8]) -> Result<Self, NetworkError>
    where
        Self: Sized;
}

impl NetworkFrame for Vec<u8> {
    fn write_into_buffer(&self, buffer: &mut [u8]) -> Result<(), NetworkError> {
        buffer.copy_from_slice(self.as_slice());
        Ok(())
    }

    fn size(&self) -> usize {
        self.len()
    }

    fn from_buffer(buffer: &[u8]) -> Result<Self, NetworkError> {
        Ok(buffer.to_vec())
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug)]
pub enum EtherType {
    Ipv4 = 0x0800,
    Arp = 0x0806,
    WakeOnLan = 0x0842,
    Srp = 0x22EA,
    Avtp = 0x22F0,
    IetfThrill = 0x22F3,
    Rarp = 0x8035,
    VlanFrame = 0x8100,
    Slpp = 0x8102,
    Vlacp = 0x8103,
    Ipx = 0x8137,
    QnxQnet = 0x8204,
    Ipv6 = 0x86DD,
    EthernetFlowControl = 0x8808,
    PPPoEDiscovery = 0x8863,
    PPPoESession = 0x8864,
    AtaOverEthernet = 0x88A2,
    Lldp = 0x88CC,
    Mrp = 0x88E3,
    Ptp = 0x88F7,
    Prp = 0x88FB,
}

impl EtherType {
    pub fn to_be_bytes(self) -> [u8; 2] {
        (self as u16).to_be_bytes()
    }

    fn from_be_bytes(num: [u8; 2]) -> Result<Self, NetworkError> {
        match u16::from_be_bytes(num) {
            0x0800 => Ok(EtherType::Ipv4),
            0x0806 => Ok(EtherType::Arp),
            0x0842 => Ok(EtherType::WakeOnLan),
            0x22EA => Ok(EtherType::Srp),
            0x22F0 => Ok(EtherType::Avtp),
            0x22F3 => Ok(EtherType::IetfThrill),
            0x8035 => Ok(EtherType::Rarp),
            0x8100 => Ok(EtherType::VlanFrame),
            0x8102 => Ok(EtherType::Slpp),
            0x8103 => Ok(EtherType::Vlacp),
            0x8137 => Ok(EtherType::Ipx),
            0x8204 => Ok(EtherType::QnxQnet),
            0x86DD => Ok(EtherType::Ipv6),
            0x8808 => Ok(EtherType::EthernetFlowControl),
            0x8863 => Ok(EtherType::PPPoEDiscovery),
            0x8864 => Ok(EtherType::PPPoESession),
            0x88A2 => Ok(EtherType::AtaOverEthernet),
            0x88CC => Ok(EtherType::Lldp),
            0x88E3 => Ok(EtherType::Mrp),
            0x88F7 => Ok(EtherType::Ptp),
            0x88FB => Ok(EtherType::Prp),
            num => Err(NetworkError::UnsupporedEtherType(num)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EthernetPacket<T: NetworkFrame> {
    pub dest: MacAddress,
    pub src: MacAddress,
    pub ty: EtherType,
    pub data: T,
}

impl<T: NetworkFrame> NetworkFrame for EthernetPacket<T> {
    fn write_into_buffer(&self, buffer: &mut [u8]) -> Result<(), NetworkError> {
        buffer[0..6].copy_from_slice(self.dest.bytes());
        buffer[6..12].copy_from_slice(self.src.bytes());
        buffer[12..14].copy_from_slice(&self.ty.to_be_bytes());
        self.data.write_into_buffer(&mut buffer[14..])?;
        Ok(())
    }

    fn size(&self) -> usize {
        14 + self.data.size()
    }

    fn from_buffer(buffer: &[u8]) -> Result<Self, NetworkError>
    where
        Self: Sized,
    {
        if buffer.len() < 14 {
            return Err(NetworkError::InvalidFrameMissingData);
        }

        let dest = MacAddress::new(buffer[0..6].try_into().unwrap());
        let src = MacAddress::new(buffer[6..12].try_into().unwrap());
        let ty = EtherType::from_be_bytes(buffer[12..14].try_into().unwrap())?;

        Ok(Self {
            dest,
            src,
            ty,
            data: T::from_buffer(&buffer[14..])?,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum NetworkError {
    InvalidFrameMissingData,
    UnsupporedEtherType(u16),
}

pub struct EthernetSocket {
    _private: (),
}

#[allow(dead_code)]
impl EthernetSocket {
    pub fn new() -> Self {
        Self { _private: () }
    }

    pub fn send<T: NetworkFrame>(&self, packet: &EthernetPacket<T>) -> Result<(), NetworkError> {
        crate::devices::net::get_device().unwrap().send(packet);

        Ok(())
    }

    pub fn receive(&self) -> Result<Option<EthernetPacket<Vec<u8>>>, NetworkError> {
        Ok(crate::devices::net::get_device()
            .unwrap()
            .receive()
            .map(|data| EthernetPacket::from_buffer(&data).unwrap()))
    }
}
