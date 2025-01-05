pub mod socket;

use core::{any::Any, fmt};

use alloc::{boxed::Box, vec::Vec};

use crate::testing;

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

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct MacAddress(pub [u8; 6]);

#[allow(dead_code)]
impl MacAddress {
    pub const BROADCAST: MacAddress = MacAddress([0xFF; 6]);

    fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn parse(s: &str) -> Option<Self> {
        let mut bytes = [0; 6];
        let mut iter = s.split(':');
        for b in &mut bytes {
            *b = u8::from_str_radix(iter.next()?, 16).ok()?;
        }
        Some(MacAddress(bytes))
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

        self.dest = MacAddress(buffer[0..6].try_into().unwrap());
        self.src = MacAddress(buffer[6..12].try_into().unwrap());
        self.ty = EtherType::from_be_bytes(buffer[12..14].try_into().unwrap())?;

        Ok(14)
    }
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Ipv4Address(pub [u8; 4]);

#[allow(dead_code)]
impl Ipv4Address {
    pub fn bytes(&self) -> &[u8; 4] {
        &self.0
    }

    pub fn parse(s: &str) -> Option<Self> {
        let mut bytes = [0; 4];
        let mut iter = s.split('.');
        for b in &mut bytes {
            *b = str::parse(iter.next()?).ok()?;
        }
        Some(Ipv4Address(bytes))
    }
}

impl fmt::Debug for Ipv4Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3],)
    }
}

impl ArpProtocol for Ipv4Address {
    const PROTOCOL: EtherType = EtherType::Ipv4;
    const LENGTH: u8 = 4;

    fn bytes(&self) -> &[u8] {
        &self.0
    }

    fn new(bytes: &[u8]) -> Option<Self> {
        Some(Ipv4Address(bytes.try_into().ok()?))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u16)]
pub enum ArpOperation {
    #[default]
    Request = 1,
    Reply = 2,
    RequestReverse = 3,
    ReplyReverse = 4,
}

impl ArpOperation {
    pub fn to_be_bytes(self) -> [u8; 2] {
        (self as u16).to_be_bytes()
    }

    pub fn from_be_bytes(num: [u8; 2]) -> Result<Self, NetworkError> {
        match u16::from_be_bytes(num) {
            1 => Ok(ArpOperation::Request),
            2 => Ok(ArpOperation::Reply),
            3 => Ok(ArpOperation::RequestReverse),
            4 => Ok(ArpOperation::ReplyReverse),
            num => Err(NetworkError::UnsupporedEtherType(num)),
        }
    }
}

pub trait ArpProtocol: fmt::Debug {
    const PROTOCOL: EtherType;
    const LENGTH: u8;

    fn bytes(&self) -> &[u8];
    fn new(bytes: &[u8]) -> Option<Self>
    where
        Self: Sized;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Assume:
/// - `HardwareType` is always `Ethernet == 1`
/// - `HardwareLength` is always `6` because `Ethernet`
pub struct ArpHeader<T: ArpProtocol> {
    pub operation: ArpOperation,
    pub sender_hw_addr: MacAddress,
    pub sender_protocol_addr: T,
    pub target_hw_addr: MacAddress,
    pub target_protocol_addr: T,
}

impl<T: ArpProtocol + 'static> NetworkHeader for ArpHeader<T> {
    fn write_into_buffer(&self, buffer: &mut [u8]) -> Result<(), NetworkError> {
        buffer[0..2].copy_from_slice(&1u16.to_be_bytes()); // Ethernet
        buffer[2..4].copy_from_slice(&T::PROTOCOL.to_be_bytes());
        buffer[4] = 6; // Ethernet
        buffer[5] = T::LENGTH;
        buffer[6..8].copy_from_slice(&self.operation.to_be_bytes());
        buffer[8..14].copy_from_slice(self.sender_hw_addr.bytes());
        buffer[14..14 + T::LENGTH as usize].copy_from_slice(self.sender_protocol_addr.bytes());

        let off = 14 + T::LENGTH as usize;
        buffer[off..off + 6].copy_from_slice(self.target_hw_addr.bytes());

        let off = off + 6;
        buffer[off..off + T::LENGTH as usize].copy_from_slice(self.target_protocol_addr.bytes());

        Ok(())
    }

    fn size(&self) -> usize {
        14 + T::LENGTH as usize * 2 + 6
    }

    fn read_from_buffer(&mut self, buffer: &[u8]) -> Result<usize, NetworkError> {
        if buffer.len() < self.size() {
            return Err(NetworkError::ReachedEndOfStream);
        }

        self.operation = ArpOperation::from_be_bytes(buffer[6..8].try_into().unwrap())?;
        self.sender_hw_addr = MacAddress(buffer[8..14].try_into().unwrap());
        self.sender_protocol_addr =
            T::new(&buffer[14..14 + T::LENGTH as usize]).expect("Not enough space");

        let off = 14 + T::LENGTH as usize;
        self.target_hw_addr = MacAddress(buffer[off..off + 6].try_into().unwrap());

        let off = off + 6;
        self.target_protocol_addr =
            T::new(&buffer[off..off + T::LENGTH as usize]).expect("Not enough space");

        Ok(off + T::LENGTH as usize)
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
        MacAddress([0x00, 0x01, 0x02, 0x03, 0x04, 0x05])
    );
    assert_eq!(header.src, MacAddress([0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B]));
    assert_eq!(header.ty, EtherType::Ipv4);
}

#[macro_rules_attribute::apply(testing::test)]
fn test_parse_packet() {
    let mut packet = NetworkPacket::default();
    packet.push(EthernetHeader {
        dest: MacAddress([0x00, 0x01, 0x02, 0x03, 0x04, 0x05]),
        src: MacAddress([0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B]),
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
        MacAddress([0x00, 0x01, 0x02, 0x03, 0x04, 0x05])
    );
    assert_eq!(header.src, MacAddress([0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B]));
    assert_eq!(header.ty, EtherType::Ipv4);
}
