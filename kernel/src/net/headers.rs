use core::fmt;

use crate::testing;

use super::{Ipv4Address, MacAddress, NetworkError, NetworkHeader};

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
            num => Err(NetworkError::UnsupporedArpOperation(num)),
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
#[allow(dead_code)]
pub enum IpProtocol {
    #[default]
    HopOpt = 0,
    Icmp = 1,
    Igmp = 2,
    Tcp = 6,
    Udp = 17,
    Rdp = 27,
    Encap = 41,
    Tlsp = 56,
    Ospf = 89,
    Sctp = 132,
}
impl IpProtocol {
    fn from_u8(buffer: u8) -> Result<Self, NetworkError> {
        match buffer {
            0 => Ok(IpProtocol::HopOpt),
            1 => Ok(IpProtocol::Icmp),
            2 => Ok(IpProtocol::Igmp),
            6 => Ok(IpProtocol::Tcp),
            17 => Ok(IpProtocol::Udp),
            27 => Ok(IpProtocol::Rdp),
            41 => Ok(IpProtocol::Encap),
            56 => Ok(IpProtocol::Tlsp),
            89 => Ok(IpProtocol::Ospf),
            132 => Ok(IpProtocol::Sctp),
            num => Err(NetworkError::UnsupporedIpProtocol(num)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ipv4Flags {
    pub dont_fragment: bool,
    pub more_fragments: bool,
}

impl Ipv4Flags {
    pub fn into_u16(self) -> u16 {
        ((self.dont_fragment as u16) << 1) | ((self.more_fragments as u16) << 2)
    }

    fn from_bits(flags: u16) -> Ipv4Flags {
        Ipv4Flags {
            dont_fragment: (flags & 0x2) != 0,
            more_fragments: (flags & 0x4) != 0,
        }
    }
}

/// `Ipv4Header` with some restrictions to thing we care about:
/// - no `Options`, so its always `Header Length = 5`
/// - `DSCP` is the default value `0`
/// - `ECN` is the default value `0`
#[derive(Clone, Copy, Debug, Default)]
pub struct Ipv4Header {
    pub payload_len: u16,
    pub id: u16,
    pub flags: Ipv4Flags,
    pub fragment_offset: u16,
    pub ttl: u8,
    pub protocol: IpProtocol,
    pub source_addr: Ipv4Address,
    pub dest_addr: Ipv4Address,
}

impl Ipv4Header {
    fn calc_checksum(data: &[u8]) -> u16 {
        let mut sum: u32 = 0;
        for chunk in data.chunks(2) {
            let word = u16::from_be_bytes([chunk[0], *chunk.get(1).unwrap_or(&0)]);
            sum += word as u32;
        }
        sum = (sum & 0xFFFF) + (sum >> 16);
        !(sum as u16)
    }
}

impl NetworkHeader for Ipv4Header {
    fn write_into_buffer(&self, buffer: &mut [u8]) -> Result<(), NetworkError> {
        assert!(self.payload_len < 0xFFFF - 20, "total len would overflow");
        assert!(
            self.fragment_offset <= 0x1FFF,
            "fragment offset is more than permitted"
        );

        buffer[0] = 5 | (4 << 4); // IP version 4, 5*32bit data
        buffer[1] = 0; // DSCP = 0, ECN = 0
        let total_len = self.payload_len + 20; // header len is 20 for now; no Options
        buffer[2..4].copy_from_slice(&total_len.to_be_bytes());
        buffer[4..6].copy_from_slice(&self.id.to_be_bytes());
        let flags_and_fragment_off =
            (self.fragment_offset & 0x1FFF) | (self.flags.into_u16() << 13);
        buffer[6..8].copy_from_slice(&flags_and_fragment_off.to_be_bytes());
        buffer[8] = self.ttl;
        buffer[9] = self.protocol as u8;
        buffer[10] = 0; // fill checksum with 0 during computation, refill it later
        buffer[11] = 0;
        buffer[12..16].copy_from_slice(self.source_addr.bytes());
        buffer[16..20].copy_from_slice(self.dest_addr.bytes());

        // put checksum
        let checksum = Self::calc_checksum(&buffer[..20]);
        buffer[10..12].copy_from_slice(&checksum.to_be_bytes());

        Ok(())
    }

    fn size(&self) -> usize {
        // TODO: implement `Options`
        20
    }

    fn read_from_buffer(&mut self, buffer: &[u8]) -> Result<usize, NetworkError> {
        if buffer.len() < self.size() {
            return Err(NetworkError::ReachedEndOfStream);
        }

        let checksum = Self::calc_checksum(&buffer[..20]);
        // TODO: looks like some systems send wrong checksum, so hard to make it strict
        // if checksum != 0 {
        //     return Err(NetworkError::InvalidChecksum);
        // }

        let total_len = u16::from_be_bytes(buffer[2..4].try_into().unwrap());
        if total_len < 20 {
            return Err(NetworkError::InvalidHeader);
        }
        self.payload_len = total_len - 20;
        self.id = u16::from_be_bytes(buffer[4..6].try_into().unwrap());
        let flags_and_fragment_off = u16::from_be_bytes(buffer[6..8].try_into().unwrap());
        self.fragment_offset = flags_and_fragment_off & 0x1FFF;
        self.flags = Ipv4Flags::from_bits(flags_and_fragment_off >> 13);
        self.ttl = buffer[8];
        self.protocol = IpProtocol::from_u8(buffer[9])?;
        self.source_addr = Ipv4Address(buffer[12..16].try_into().unwrap());
        self.dest_addr = Ipv4Address(buffer[16..20].try_into().unwrap());

        Ok(20)
    }
}

#[derive(Clone, Copy, Debug)]
// TODO: implement the rest
pub enum IcmpHeader {
    EchoReply,
    EchoRequest,
    Unknown { ty: u8, code: u8, extra: u32 },
}

impl Default for IcmpHeader {
    fn default() -> Self {
        Self::Unknown {
            ty: 0,
            code: 0,
            extra: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct IcmpHeaderAndData {
    pub header: IcmpHeader,
    pub data: Vec<u8>,
}

impl IcmpHeader {
    pub fn ty(&self) -> u8 {
        // This is exactly what the `EchoReply` is, but I guess its okay either way
        match self {
            IcmpHeader::EchoReply => 0,
            IcmpHeader::EchoRequest => 8,
            IcmpHeader::Unknown { ty, .. } => *ty,
        }
    }

    pub fn code(&self) -> u8 {
        match self {
            IcmpHeader::EchoReply | IcmpHeader::EchoRequest => 0,
            IcmpHeader::Unknown { code, .. } => *code,
        }
    }

    pub fn extra(&self) -> u32 {
        match self {
            IcmpHeader::EchoReply | IcmpHeader::EchoRequest => 0,
            IcmpHeader::Unknown { extra, .. } => *extra,
        }
    }

    fn from_header(ty: u8, code: u8, extra: u32) -> Self {
        match (ty, code) {
            (0, 0) => Self::EchoReply,
            (8, 0) => Self::EchoRequest,
            _ => Self::Unknown { ty, code, extra },
        }
    }
}

impl NetworkHeader for IcmpHeaderAndData {
    fn write_into_buffer(&self, buffer: &mut [u8]) -> Result<(), NetworkError> {
        buffer[0] = self.header.ty();
        buffer[1] = self.header.code();
        buffer[2] = 0; // reset checksum to 0;
        buffer[3] = 0;
        buffer[4..8].copy_from_slice(&self.header.extra().to_be_bytes());
        buffer[8..8 + self.data.len()].copy_from_slice(&self.data);

        let checksum = Ipv4Header::calc_checksum(&buffer[..8 + self.data.len()]);
        buffer[2..4].copy_from_slice(&checksum.to_be_bytes());

        Ok(())
    }

    fn size(&self) -> usize {
        8 + self.data.len()
    }

    fn read_from_buffer(&mut self, buffer: &[u8]) -> Result<usize, NetworkError> {
        if buffer.len() < 8 {
            return Err(NetworkError::ReachedEndOfStream);
        }

        let checksum = Ipv4Header::calc_checksum(&buffer);
        // TODO: looks like some systems send wrong checksum, so hard to make it strict
        // if checksum != 0 {
        //     return Err(NetworkError::InvalidChecksum);
        // }

        self.header = IcmpHeader::from_header(
            buffer[0],
            buffer[1],
            u32::from_be_bytes(buffer[4..8].try_into().unwrap()),
        );

        self.data.clear();
        self.data.extend_from_slice(&buffer[8..]);

        Ok(self.size())
    }
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
