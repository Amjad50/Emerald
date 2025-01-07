mod headers;
pub mod socket;

pub use headers::*;

use core::{any::Any, fmt};

use alloc::{boxed::Box, vec::Vec};

use crate::testing;

/// Represent a part of a network stack, and will
/// be written directly into the network DMA buffer
pub trait NetworkHeader: fmt::Debug + Any {
    fn write_into_buffer(&self, buffer: &mut [u8]) -> Result<(), NetworkError>;
    fn size(&self) -> usize;
    fn read_from_buffer(&mut self, buffer: &[u8]) -> Result<usize, NetworkError>;
    fn next_header(&self) -> Option<Box<dyn NetworkHeader>> {
        None
    }
    fn create() -> Self
    where
        Self: Default,
    {
        Default::default()
    }
}

#[derive(Default)]
pub struct NetworkPacket {
    headers: Vec<Box<dyn NetworkHeader>>,
    payload: Vec<u8>,
    should_build: bool,
}

impl fmt::Debug for NetworkPacket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NetworkPacket")
            .field("headers", &self.headers)
            .field("payload", &self.payload)
            .finish()
    }
}

#[allow(dead_code)]
impl NetworkPacket {
    pub fn buildable() -> Self {
        Self {
            should_build: true,
            ..Default::default()
        }
    }

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

    fn read_from_buffer_into(&mut self, buffer: &[u8]) -> Result<(), NetworkError> {
        let mut offset = 0;
        for header in self.headers.iter_mut() {
            offset += header.read_from_buffer(&buffer[offset..])?;
        }
        // remaining bytes are the payload
        self.payload.clear();
        self.payload.extend_from_slice(&buffer[offset..]);

        Ok(())
    }

    fn read_from_buffer_and_build(&mut self, buffer: &[u8]) -> Result<(), NetworkError> {
        self.clear();

        let mut ethernet = EthernetHeader::create();
        let mut off = ethernet.read_from_buffer(buffer)?;

        let mut next_header = ethernet.next_header();
        self.push(ethernet);

        while let Some(mut header) = next_header {
            off += header.read_from_buffer(&buffer[off..])?;
            next_header = header.next_header();
            self.headers.push(header);
        }

        self.payload.extend_from_slice(&buffer[off..]);

        Ok(())
    }

    pub fn read_from_buffer(&mut self, buffer: &[u8]) -> Result<(), NetworkError> {
        if self.should_build {
            self.read_from_buffer_and_build(buffer)
        } else {
            self.read_from_buffer_into(buffer)
        }
    }

    pub fn header<T: NetworkHeader>(&self) -> Option<&T> {
        self.headers
            .iter()
            .find_map(|h| (h.as_ref() as &dyn Any).downcast_ref::<T>())
    }

    pub fn header_mut<T: NetworkHeader>(&mut self) -> Option<&mut T> {
        self.headers
            .iter_mut()
            .find_map(|h| (h.as_mut() as &mut dyn Any).downcast_mut::<T>())
    }

    pub fn headers<T: NetworkHeader>(&self) -> impl Iterator<Item = &T> {
        self.headers
            .iter()
            .filter_map(|h| (h.as_ref() as &dyn Any).downcast_ref::<T>())
    }

    pub fn all_headers(&self) -> &[Box<dyn NetworkHeader>] {
        &self.headers
    }

    pub fn clear(&mut self) {
        self.headers.clear();
        self.payload.clear();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum NetworkError {
    ReachedEndOfStream,
    NotEnoughSpace,
    UnsupporedEtherType(u16),
    PacketTooLarge(usize),
    InvalidHeader,
    InvalidChecksum,
    UnsupporedIpProtocol(u8),
    UnsupporedArpOperation(u16),
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
