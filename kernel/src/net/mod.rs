pub mod socket;

use core::{any::Any, fmt};

use alloc::{boxed::Box, vec::Vec};

use crate::{devices::net::MacAddress, testing};

/// Represent a part of a network stack, and will
/// be written directly into the network DMA buffer
pub trait NetworkHeader: fmt::Debug + Any {
    fn write_into_buffer(&self, buffer: &mut [u8]) -> Result<(), NetworkError>;
    fn size(&self) -> usize;
    fn read_from_buffer(&mut self, buffer: &[u8]) -> Result<usize, NetworkError>;
    fn create() -> Self
    where
        Self: Default,
    {
        Default::default()
    }
}

#[derive(Debug, Default)]
pub struct NetworkPacket {
    headers: Vec<Box<dyn NetworkHeader>>,
    payload: Vec<u8>,
}

#[allow(dead_code)]
impl NetworkPacket {
    pub fn push<T: NetworkHeader + 'static>(&mut self, header: T) -> &mut Self {
        self.headers.push(Box::new(header));
        self
    }

    pub fn push_empty<T: NetworkHeader + Default + 'static>(&mut self) -> &mut Self {
        self.headers.push(Box::new(T::create()));
        self
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn set_payload(&mut self, payload: Vec<u8>) {
        self.payload = payload;
    }

    pub fn extend_payload(&mut self, payload: &[u8]) {
        self.payload.extend_from_slice(payload);
    }

    pub fn size(&self) -> usize {
        self.headers.iter().map(|h| h.size()).sum::<usize>() + self.payload.len()
    }

    pub fn write_into_buffer(&self, buffer: &mut [u8]) -> Result<usize, NetworkError> {
        if buffer.len() < self.size() {
            return Err(NetworkError::NotEnoughSpace);
        }

        let mut offset = 0;
        for header in self.headers.iter() {
            header.write_into_buffer(&mut buffer[offset..])?;
            offset += header.size();
        }

        let payload_len = self.payload.len();

        buffer[offset..offset + payload_len].copy_from_slice(&self.payload);

        Ok(offset + payload_len)
    }

    pub fn read_from_buffer(&mut self, buffer: &[u8]) -> Result<(), NetworkError> {
        let mut offset = 0;
        for header in self.headers.iter_mut() {
            offset += header.read_from_buffer(&buffer[offset..])?;
        }
        // remaining bytes are the payload
        self.payload.clear();
        self.payload.extend_from_slice(&buffer[offset..]);

        Ok(())
    }

    pub fn header<T: NetworkHeader>(&self) -> Option<&T> {
        self.headers
            .iter()
            .find_map(|h| (h.as_ref() as &dyn Any).downcast_ref::<T>())
    }

    pub fn headers<T: NetworkHeader>(&self) -> impl Iterator<Item = &T> {
        self.headers
            .iter()
            .filter_map(|h| (h.as_ref() as &dyn Any).downcast_ref::<T>())
    }

    pub fn all_headers(&self) -> &[Box<dyn NetworkHeader>] {
        &self.headers
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EtherType {
    #[default]
    Unknown = 0x0000,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EthernetHeader {
    pub dest: MacAddress,
    pub src: MacAddress,
    pub ty: EtherType,
}

impl NetworkHeader for EthernetHeader {
    fn write_into_buffer(&self, buffer: &mut [u8]) -> Result<(), NetworkError> {
        buffer[0..6].copy_from_slice(self.dest.bytes());
        buffer[6..12].copy_from_slice(self.src.bytes());
        buffer[12..14].copy_from_slice(&self.ty.to_be_bytes());
        Ok(())
    }

    fn size(&self) -> usize {
        14
    }

    fn read_from_buffer(&mut self, buffer: &[u8]) -> Result<usize, NetworkError> {
        if buffer.len() < 14 {
            return Err(NetworkError::ReachedEndOfStream);
        }

        self.dest = MacAddress::new(buffer[0..6].try_into().unwrap());
        self.src = MacAddress::new(buffer[6..12].try_into().unwrap());
        self.ty = EtherType::from_be_bytes(buffer[12..14].try_into().unwrap())?;

        Ok(14)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkError {
    ReachedEndOfStream,
    NotEnoughSpace,
    UnsupporedEtherType(u16),
    PacketTooLarge(usize),
}

#[macro_rules_attribute::apply(testing::test)]
fn test_parse_ethernet_header() {
    let mut header = EthernetHeader::default();
    let buffer = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, // dest
        0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, // src
        0x08, 0x00, // type
    ];

    assert_eq!(header.read_from_buffer(&buffer), Ok(14));
    assert_eq!(
        header.dest,
        MacAddress::new([0x00, 0x01, 0x02, 0x03, 0x04, 0x05])
    );
    assert_eq!(
        header.src,
        MacAddress::new([0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B])
    );
    assert_eq!(header.ty, EtherType::Ipv4);
}

#[macro_rules_attribute::apply(testing::test)]
fn test_parse_packet() {
    let mut packet = NetworkPacket::default();
    packet.push(EthernetHeader {
        dest: MacAddress::new([0x00, 0x01, 0x02, 0x03, 0x04, 0x05]),
        src: MacAddress::new([0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B]),
        ty: EtherType::Ipv4,
    });

    let mut buffer = [0; 256];
    assert_eq!(packet.write_into_buffer(&mut buffer), Ok(14));

    let mut packet2 = NetworkPacket::default();
    packet2.push_empty::<EthernetHeader>();
    packet2.read_from_buffer(&buffer).unwrap();

    assert_eq!(packet2.payload().len(), 256 - 14);
    assert!(packet2.payload().iter().all(|&x| x == 0));

    assert_eq!(packet2.all_headers().len(), 1);
    let header = packet2.header::<EthernetHeader>().unwrap();
    assert_eq!(header, packet.header::<EthernetHeader>().unwrap());
    assert_eq!(
        header.dest,
        MacAddress::new([0x00, 0x01, 0x02, 0x03, 0x04, 0x05])
    );
    assert_eq!(
        header.src,
        MacAddress::new([0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B])
    );
    assert_eq!(header.ty, EtherType::Ipv4);
}
