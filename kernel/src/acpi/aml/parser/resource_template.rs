use core::fmt;

use alloc::vec::Vec;

use crate::{acpi::aml::display::AmlDisplayer, io::ByteStr};

use super::{AccessType, AmlParseError, Buffer, RegionSpace};

#[allow(dead_code)]
mod consts {
    pub const END_TAG_FULL: u8 = 0x79;
}

pub struct Parser<'a> {
    buffer: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    pub fn get_next_byte(&mut self) -> Result<u8, AmlParseError> {
        if self.pos >= self.buffer.len() {
            return Err(AmlParseError::UnexpectedEndOfCode);
        }
        let byte = self.buffer[self.pos];
        self.pos += 1;
        Ok(byte)
    }

    pub fn get_next_u16(&mut self) -> Result<u16, AmlParseError> {
        let low = self.get_next_byte()? as u16;
        let high = self.get_next_byte()? as u16;

        Ok((high << 8) | low)
    }

    pub fn get_next_u32(&mut self) -> Result<u32, AmlParseError> {
        Ok(u32::from_le_bytes([
            self.get_next_byte()?,
            self.get_next_byte()?,
            self.get_next_byte()?,
            self.get_next_byte()?,
        ]))
    }

    pub fn get_next_u64(&mut self) -> Result<u64, AmlParseError> {
        Ok(u64::from_le_bytes([
            self.get_next_byte()?,
            self.get_next_byte()?,
            self.get_next_byte()?,
            self.get_next_byte()?,
            self.get_next_byte()?,
            self.get_next_byte()?,
            self.get_next_byte()?,
            self.get_next_byte()?,
        ]))
    }

    pub fn get_next_address_data<W: AddressWidth>(&mut self) -> Result<W, AmlParseError> {
        W::from_parser(self)
    }

    pub fn get_inner(&mut self, len: usize) -> Result<Self, AmlParseError> {
        if self.pos + len > self.buffer.len() {
            return Err(AmlParseError::UnexpectedEndOfCode);
        }

        let buffer = &self.buffer[self.pos..self.pos + len];
        self.pos += len;
        Ok(Parser { buffer, pos: 0 })
    }

    pub fn get_remaining_data(&mut self) -> Result<&[u8], AmlParseError> {
        if self.pos >= self.buffer.len() {
            return Err(AmlParseError::UnexpectedEndOfCode);
        }
        self.pos = self.buffer.len();
        Ok(&self.buffer[self.pos..])
    }

    pub fn is_done(&self) -> bool {
        self.pos >= self.buffer.len()
    }
}

#[derive(Debug, Clone)]
pub enum DmaSpeedType {
    Compatibility,
    TypeA,
    TypeB,
    TypeF,
}

#[derive(Debug, Clone)]
pub enum DmaTrasferType {
    Transfer8,
    Transfer16,
    Transfer8_16,
}

#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum DmaTransferWidth {
    Width8Bit,
    Width16Bit,
    Width32Bit,
    Width64Bit,
    Width128Bit,
    Width256Bit,
}

#[derive(Debug, Clone)]
pub enum SpaceMemoryType {
    NonCacheable,
    Cacheable,
    WriteCombining,
    // ignore in extended address space
    Prefetchable,
}

#[derive(Debug, Clone)]
pub enum SpaceMemoryRangeType {
    AddressRangeMemory,
    AddressRangeReserved,
    AddressRangeACPI,
    AddressRangeNVS,
}

#[derive(Debug, Clone)]
pub enum SpaceIoRangeType {
    NonISAOnlyRanges,
    ISAOnlyRanges,
    EntireRange,
}

#[derive(Debug, Clone)]
pub enum AddressSpaceType {
    Memory {
        is_read_write: bool,
        ty: SpaceMemoryType,
        range_ty: SpaceMemoryRangeType,
        is_type_translation: bool,
    },
    Io {
        is_type_translation: bool,
        range_ty: SpaceIoRangeType,
        is_sparse_translation: bool,
    },
    BusNumber,
    VendorDefined {
        value: u8,
        flags: u8,
    },
}

impl AddressSpaceType {
    pub fn parse<W: AddressWidth>(value: u8, flags: u8) -> Result<Self, AmlParseError> {
        let result = match (W::width(), value) {
            (_, 0) => {
                let is_read_write = flags & 1 != 0;
                let ty = match (flags >> 1) & 0b11 {
                    0 => SpaceMemoryType::NonCacheable,
                    1 => SpaceMemoryType::Cacheable,
                    2 => SpaceMemoryType::WriteCombining,
                    3 => SpaceMemoryType::Prefetchable,
                    _ => unreachable!(),
                };
                let range_ty = match (flags >> 3) & 0b11 {
                    0 => SpaceMemoryRangeType::AddressRangeMemory,
                    1 => SpaceMemoryRangeType::AddressRangeReserved,
                    2 => SpaceMemoryRangeType::AddressRangeACPI,
                    3 => SpaceMemoryRangeType::AddressRangeNVS,
                    _ => unreachable!(),
                };
                let is_type_translation = (flags >> 5) & 1 != 0;

                Self::Memory {
                    is_read_write,
                    ty,
                    range_ty,
                    is_type_translation,
                }
            }
            (_, 1) => {
                let range_ty = match flags & 0b11 {
                    0 => return Err(AmlParseError::ReservedValue),
                    1 => SpaceIoRangeType::NonISAOnlyRanges,
                    2 => SpaceIoRangeType::ISAOnlyRanges,
                    3 => SpaceIoRangeType::EntireRange,
                    _ => unreachable!(),
                };
                let is_type_translation = (flags >> 4) & 1 != 0;
                let is_sparse_translation = (flags >> 5) & 1 != 0;

                Self::Io {
                    is_type_translation,
                    range_ty,
                    is_sparse_translation,
                }
            }
            // only word has bus number
            (2, 2) => Self::BusNumber,
            (_, 192..=255) => Self::VendorDefined { value, flags },
            _ => return Err(AmlParseError::ReservedValue),
        };

        Ok(result)
    }
}

#[derive(Debug, Clone)]
pub enum ResourceSourceOrTypeSpecificAttrs {
    ResourceSource(ResourceSource),
    TypeSpecificAttrs(u64),
}

#[derive(Debug, Clone)]
pub struct AddressSpace<W: AddressWidth> {
    ty: AddressSpaceType,
    is_consumer: bool,
    is_max_fixed: bool,
    is_min_fixed: bool,
    is_subtract_decode: bool,
    granularity: W,
    min: W,
    max: W,
    translation_offset: W,
    len: W,
    extra: ResourceSourceOrTypeSpecificAttrs,
}

impl<W: AddressWidth> AddressSpace<W> {
    fn parse_address_ranges_nums(parser: &mut Parser) -> Result<(W, W, W, W, W), AmlParseError> {
        let granularity = parser.get_next_address_data::<W>()?;
        let min = parser.get_next_address_data::<W>()?;
        let max = parser.get_next_address_data::<W>()?;
        let translation_offset = parser.get_next_address_data::<W>()?;
        let len = parser.get_next_address_data::<W>()?;

        Ok((granularity, min, max, translation_offset, len))
    }

    fn parse_start_flags(
        parser: &mut Parser,
    ) -> Result<(AddressSpaceType, bool, bool, bool, bool), AmlParseError> {
        let ty_byte = parser.get_next_byte()?;
        let flags = parser.get_next_byte()?;
        let ty_flags = parser.get_next_byte()?;

        let ty = AddressSpaceType::parse::<W>(ty_byte, ty_flags)?;
        let is_max_fixed = flags & (1 << 3) != 0;
        let is_min_fixed = flags & (1 << 2) != 0;
        let is_subtract_decode = flags & (1 << 1) != 0;
        let is_consumer = flags & 1 != 0;

        Ok((
            ty,
            is_consumer,
            is_max_fixed,
            is_min_fixed,
            is_subtract_decode,
        ))
    }

    pub fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let (ty, is_consumer, is_max_fixed, is_min_fixed, is_subtract_decode) =
            Self::parse_start_flags(parser)?;
        let (granularity, min, max, translation_offset, len) =
            Self::parse_address_ranges_nums(parser)?;

        let resource_source = ResourceSource::parse(parser)?;

        Ok(Self {
            ty,
            is_consumer,
            is_max_fixed,
            is_min_fixed,
            is_subtract_decode,
            granularity,
            min,
            max,
            translation_offset,
            len,
            extra: ResourceSourceOrTypeSpecificAttrs::ResourceSource(resource_source),
        })
    }

    pub fn parse_extended(parser: &mut Parser) -> Result<Self, AmlParseError> {
        assert_eq!(W::width(), 8, "only support 64-bit address space");
        let (ty, is_consumer, is_max_fixed, is_min_fixed, is_subtract_decode) =
            Self::parse_start_flags(parser)?;

        let revision = parser.get_next_byte()?;
        assert_eq!(revision, 1, "only support revision 1");
        let reserved = parser.get_next_byte()?;
        if reserved != 0 {
            return Err(AmlParseError::ReservedValue);
        }

        let (granularity, min, max, translation_offset, len) =
            Self::parse_address_ranges_nums(parser)?;

        let type_specific_attribute = parser.get_next_u64()?;

        Ok(Self {
            ty,
            is_consumer,
            is_max_fixed,
            is_min_fixed,
            is_subtract_decode,
            granularity,
            min,
            max,
            translation_offset,
            len,
            extra: ResourceSourceOrTypeSpecificAttrs::TypeSpecificAttrs(type_specific_attribute),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ResourceSource {
    index: Option<u8>,
    source: Option<Vec<u8>>,
}

impl ResourceSource {
    fn empty() -> Self {
        Self {
            index: None,
            source: None,
        }
    }

    fn parse(parser: &mut Parser) -> Result<Self, AmlParseError> {
        let index = if !parser.is_done() {
            Some(parser.get_next_byte()?)
        } else {
            None
        };

        let source = if !parser.is_done() {
            Some(parser.get_remaining_data()?.to_vec())
        } else {
            None
        };

        Ok(Self { index, source })
    }

    fn display<'a, 'b, 'r>(&self, d: &'r mut AmlDisplayer<'a, 'b>) -> &'r mut AmlDisplayer<'a, 'b> {
        d.paren_arg(|f| {
            if let Some(index) = self.index {
                write!(f, "{}", index)
            } else {
                Ok(())
            }
        });
        d.paren_arg(|f| {
            if let Some(source) = &self.source {
                write!(f, "{:?}", ByteStr(source))
            } else {
                Ok(())
            }
        });

        d
    }
}

#[derive(Debug, Clone)]
pub enum ResourceMacro {
    Irq {
        wake_capable: bool,
        is_shared: bool,
        active_low: bool,
        edge_triggered: bool,
        irqs_mask: u16,
    },
    Dma {
        speed_ty: DmaSpeedType,
        is_bus_master: bool,
        transfer_type: DmaTrasferType,
        channels_mask: u8,
    },
    StartDependentFunctions {
        compatibility_priority: u8,
        performance_priority: u8,
    },
    EndDependentFunctions,
    Io {
        is_16_bit_decode: bool,
        min_addr: u16,
        max_addr: u16,
        alignment: u8,
        len: u8,
    },
    FixedIo {
        base: u16,
        len: u8,
    },
    FixedDma {
        dma_req: u16,
        channel: u16,
        transfer_width: DmaTransferWidth,
    },
    VendorShort {
        data: [u8; 6],
        len: u8,
    },
    VendorLarge {
        data: Vec<u8>,
    },
    Memory24 {
        is_read_write: bool,
        min_addr: u32,
        max_addr: u32,
        alignment: u16,
        len: u16,
    },
    Memory32Fixed {
        is_read_write: bool,
        base_addr: u32,
        len: u32,
    },
    Memory32 {
        is_read_write: bool,
        min_addr: u32,
        max_addr: u32,
        alignment: u32,
        len: u32,
    },
    Interrupt {
        is_consumer: bool,
        edge_triggered: bool,
        active_low: bool,
        is_shared: bool,
        wake_capable: bool,
        interrupts: Vec<u32>,
        resource_source: ResourceSource,
    },
    Register {
        address_space: RegionSpace,
        bit_width: u8,
        offset: u8,
        address: u64,
        access_size: AccessType,
    },
    AddressSpaceWord(AddressSpace<u16>),
    AddressSpaceDWord(AddressSpace<u32>),
    AddressSpaceQWord(AddressSpace<u64>),
    AddressSpaceExtended(AddressSpace<u64>),
}

impl ResourceMacro {
    fn try_parse_small_resource(
        parser: &mut Parser,
        first: u8,
    ) -> Result<Option<Self>, AmlParseError> {
        let result = match first {
            0x22 | 0x23 => {
                // contain extra 1 byte
                let contain_info = first == 0x23;

                let irqs_mask = parser.get_next_u16()?;

                let mut wake_capable = false;
                let mut is_shared = false;
                let mut active_low = false;
                let mut edge_triggered = true;

                if contain_info {
                    let flags = parser.get_next_byte()?;
                    edge_triggered = flags & 1 != 0;
                    active_low = flags & (1 << 3) != 0;
                    is_shared = flags & (1 << 4) != 0;
                    wake_capable = flags & (1 << 5) != 0;

                    if flags & (0b11 << 6) != 0 {
                        return Err(AmlParseError::ReservedFieldSet);
                    }
                }

                Some(ResourceMacro::Irq {
                    wake_capable,
                    is_shared,
                    active_low,
                    edge_triggered,
                    irqs_mask,
                })
            }
            0x2A => {
                // DMA
                let channels_mask = parser.get_next_byte()?;
                let flags = parser.get_next_byte()?;

                if flags & 0x80 != 0 {
                    return Err(AmlParseError::ReservedFieldSet);
                }

                let transfer_type = match flags & 0b11 {
                    0b00 => DmaTrasferType::Transfer8,
                    0b01 => DmaTrasferType::Transfer8_16,
                    0b10 => DmaTrasferType::Transfer16,
                    _ => return Err(AmlParseError::ReservedFieldSet),
                };
                let is_bus_master = (flags >> 2) & 1 == 1;
                let speed_ty = match (flags >> 5) & 0b11 {
                    0b00 => DmaSpeedType::Compatibility,
                    0b01 => DmaSpeedType::TypeA,
                    0b10 => DmaSpeedType::TypeB,
                    0b11 => DmaSpeedType::TypeF,
                    _ => unreachable!(),
                };

                Some(ResourceMacro::Dma {
                    speed_ty,
                    is_bus_master,
                    transfer_type,
                    channels_mask,
                })
            }
            0x30 | 0x31 => {
                let contain_priority_byte = first == 0x31;

                let mut compatibility_priority = 1;
                let mut performance_priority = 1;

                if contain_priority_byte {
                    let flags = parser.get_next_byte()?;
                    compatibility_priority = flags & 0b11;
                    performance_priority = (flags >> 2) & 0b11;

                    if flags & (0b1111 << 4) != 0 {
                        return Err(AmlParseError::ReservedFieldSet);
                    }
                }

                Some(ResourceMacro::StartDependentFunctions {
                    compatibility_priority,
                    performance_priority,
                })
            }
            0x38 => Some(ResourceMacro::EndDependentFunctions),
            0x47 => {
                let flags = parser.get_next_byte()?;
                let min_addr = parser.get_next_u16()?;
                let max_addr = parser.get_next_u16()?;
                let alignment = parser.get_next_byte()?;
                let len = parser.get_next_byte()?;

                Some(ResourceMacro::Io {
                    is_16_bit_decode: flags & 1 == 1,
                    min_addr,
                    max_addr,
                    alignment,
                    len,
                })
            }
            0x4B => {
                let base = parser.get_next_u16()? & 0x3FF;
                let len = parser.get_next_byte()?;

                Some(ResourceMacro::FixedIo { base, len })
            }
            0x55 => {
                let dma_req = parser.get_next_u16()?;
                let channel = parser.get_next_u16()?;

                let transfer_width = match parser.get_next_byte()? {
                    0 => DmaTransferWidth::Width8Bit,
                    1 => DmaTransferWidth::Width16Bit,
                    2 => DmaTransferWidth::Width32Bit,
                    3 => DmaTransferWidth::Width64Bit,
                    4 => DmaTransferWidth::Width128Bit,
                    5 => DmaTransferWidth::Width256Bit,
                    _ => return Err(AmlParseError::ReservedFieldSet),
                };

                Some(ResourceMacro::FixedDma {
                    dma_req,
                    channel,
                    transfer_width,
                })
            }
            0x71..=0x77 => {
                let len = (first & 0b111) - 1;

                let mut data = [0; 6];

                for d in data.iter_mut().take(len as usize) {
                    *d = parser.get_next_byte()?;
                }

                Some(ResourceMacro::VendorShort { data, len })
            }
            0x79 => {
                return Err(AmlParseError::ResourceTemplateReservedTag);
            }
            _ => None,
        };

        Ok(result)
    }

    fn try_parse_large_resource(
        parser: &mut Parser,
        first: u8,
    ) -> Result<Option<Self>, AmlParseError> {
        if first & 0x80 != 0x80 {
            return Ok(None);
        }

        let data_len = parser.get_next_u16()?;

        let mut parser = parser.get_inner(data_len as usize)?;

        let result = match first & 0x7F {
            0x00 | 0x03 | 0x13..=0x7F => {
                return Err(AmlParseError::ResourceTemplateReservedTag);
            }
            0x01 => {
                assert_eq!(data_len, 9);
                let flags = parser.get_next_byte()?;

                let min_addr = parser.get_next_u16()?;
                let max_addr = parser.get_next_u16()?;
                let alignment = parser.get_next_u16()?;
                let len = parser.get_next_u16()?;

                Some(ResourceMacro::Memory24 {
                    is_read_write: flags & 1 == 1,
                    min_addr: (min_addr as u32) << 8,
                    max_addr: (max_addr as u32) << 8,
                    alignment,
                    len,
                })
            }
            0x02 => {
                assert_eq!(data_len, 12);
                let mut address_space = parser.get_next_byte()?.into();

                if let RegionSpace::Other(other) = address_space {
                    if other == 0x7F {
                        address_space = RegionSpace::FFixedHW;
                    } else {
                        return Err(AmlParseError::ReservedFieldSet);
                    }
                }
                let bit_width = parser.get_next_byte()?;
                let offset = parser.get_next_byte()?;
                let access_size = parser.get_next_byte()?.try_into()?;
                let address = parser.get_next_u64()?;

                if let AccessType::Buffer = access_size {
                    return Err(AmlParseError::ReservedFieldSet);
                }

                Some(ResourceMacro::Register {
                    address_space,
                    bit_width,
                    offset,
                    address,
                    access_size,
                })
            }
            0x04 => Some(ResourceMacro::VendorLarge {
                data: parser.get_remaining_data()?.to_vec(),
            }),
            0x05 => {
                assert_eq!(data_len, 17);
                let flags = parser.get_next_byte()?;

                let min_addr = parser.get_next_u32()?;
                let max_addr = parser.get_next_u32()?;
                let alignment = parser.get_next_u32()?;
                let len = parser.get_next_u32()?;

                Some(ResourceMacro::Memory32 {
                    is_read_write: flags & 1 == 1,
                    min_addr,
                    max_addr,
                    alignment,
                    len,
                })
            }
            0x06 => {
                assert_eq!(data_len, 9);
                let flags = parser.get_next_byte()?;

                let base_addr = parser.get_next_u32()?;
                let len = parser.get_next_u32()?;

                Some(ResourceMacro::Memory32Fixed {
                    is_read_write: flags & 1 == 1,
                    base_addr,
                    len,
                })
            }
            0x07 => {
                assert!(data_len >= 23);
                Some(ResourceMacro::AddressSpaceDWord(
                    AddressSpace::<u32>::parse(&mut parser)?,
                ))
            }
            0x08 => {
                assert!(data_len >= 13);
                Some(ResourceMacro::AddressSpaceWord(AddressSpace::<u16>::parse(
                    &mut parser,
                )?))
            }
            0x09 => {
                assert!(data_len >= 6);

                let flags = parser.get_next_byte()?;
                let is_consumed = flags & 1 != 0;
                let edge_triggered = flags & (1 << 1) != 0;
                let active_low = flags & (1 << 2) != 0;
                let is_shared = flags & (1 << 3) != 0;
                let wake_capable = flags & (1 << 4) != 0;

                let table_len = parser.get_next_byte()?;

                let mut interrupts = Vec::with_capacity(table_len as usize);
                for _ in 0..table_len {
                    interrupts.push(parser.get_next_u32()?);
                }

                let resource_source = ResourceSource::parse(&mut parser)?;

                Some(ResourceMacro::Interrupt {
                    is_consumer: is_consumed,
                    edge_triggered,
                    active_low,
                    is_shared,
                    wake_capable,
                    interrupts,
                    resource_source,
                })
            }
            0x0A => {
                assert!(data_len >= 43);
                Some(ResourceMacro::AddressSpaceQWord(
                    AddressSpace::<u64>::parse(&mut parser)?,
                ))
            }
            0x0B => {
                assert!(data_len >= 53);
                Some(ResourceMacro::AddressSpaceExtended(
                    AddressSpace::<u64>::parse_extended(&mut parser)?,
                ))
            }
            // TODO: support rest of the resource types
            _ => None,
        };

        // still there is more data to parse
        if result.is_some() && !parser.is_done() {
            return Err(AmlParseError::InvalidResourceTemplate);
        }

        Ok(result)
    }

    pub fn try_parse_buffer(parser: &mut Parser) -> Result<Option<Self>, AmlParseError> {
        let first = parser.get_next_byte()?;

        if let Some(result) = Self::try_parse_small_resource(parser, first)? {
            return Ok(Some(result));
        };

        Self::try_parse_large_resource(parser, first)
    }
}

#[derive(Debug, Clone)]
pub struct ResourceTemplate {
    pub(super) items: Vec<ResourceMacro>,
}

impl ResourceTemplate {
    pub fn try_parse_buffer(buf: &Buffer) -> Result<Option<Self>, AmlParseError> {
        let data = buf.data.as_slice();
        // is this a resource template anyway?
        {
            if data.len() < 2 || data[data.len() - 2] != consts::END_TAG_FULL {
                return Ok(None);
            }

            let sum: u8 = buf.data.iter().fold(0, |a, b| a.wrapping_add(*b));
            // The checksum must match or the last element can be 0
            if sum != 0 && data.last() != Some(&0) {
                return Ok(None);
            }
        }

        let data = &data[0..buf.data.len() - 2];

        let mut parser = Parser {
            buffer: data,
            pos: 0,
        };

        let mut items = Vec::new();
        while !parser.is_done() {
            let Some(item) = ResourceMacro::try_parse_buffer(&mut parser)? else {
                return Ok(None);
            };

            items.push(item);
        }

        Ok(Some(ResourceTemplate { items }))
    }
}

pub trait AddressWidth {
    fn width() -> u8
    where
        Self: Sized;
    fn fmt_hex(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
    fn from_parser(parser: &mut Parser) -> Result<Self, AmlParseError>
    where
        Self: Sized;
}

impl AddressWidth for u16 {
    fn width() -> u8 {
        2
    }

    fn fmt_hex(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:04X}", self)
    }

    fn from_parser(parser: &mut Parser) -> Result<Self, AmlParseError> {
        parser.get_next_u16()
    }
}

impl AddressWidth for u32 {
    fn width() -> u8 {
        4
    }

    fn fmt_hex(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08X}", self)
    }

    fn from_parser(parser: &mut Parser) -> Result<Self, AmlParseError> {
        parser.get_next_u32()
    }
}

impl AddressWidth for u64 {
    fn width() -> u8 {
        8
    }

    fn fmt_hex(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:016X}", self)
    }

    fn from_parser(parser: &mut Parser) -> Result<Self, AmlParseError> {
        parser.get_next_u64()
    }
}

#[allow(clippy::too_many_arguments)]
fn display_interrupt_args<'a, 'b, 'r>(
    d: &'r mut AmlDisplayer<'a, 'b>,
    is_consumer: Option<bool>,
    wake_capable: bool,
    is_shared: bool,
    active_low: bool,
    edge_triggered: bool,
    resource_source: &ResourceSource,
) -> &'r mut AmlDisplayer<'a, 'b> {
    if let Some(is_consumer) = is_consumer {
        d.paren_arg(|f| {
            f.write_str(if is_consumer {
                "ResourceConsumer"
            } else {
                "ResourceProducer"
            })
        });
    }

    d.paren_arg(|f| f.write_str(if edge_triggered { "Edge" } else { "Level" }))
        .paren_arg(|f| {
            f.write_str(if active_low {
                "ActiveLow"
            } else {
                "ActiveHigh"
            })
        })
        .paren_arg(|f| {
            f.write_str(if is_shared { "Shared" } else { "Exclusive" })?;
            if wake_capable {
                f.write_str("WakeCapable")
            } else {
                Ok(())
            }
        });

    resource_source.display(d);

    d
}

fn display_memory_args<'a, 'b>(
    f: &'a mut fmt::Formatter<'b>,
    name: &str,
    is_read_write: bool,
    nums: &[&dyn AddressWidth],
) -> AmlDisplayer<'a, 'b> {
    let mut d = AmlDisplayer::start(f, name);

    d.paren_arg(|f| {
        f.write_str(if is_read_write {
            "ReadWrite"
        } else {
            "ReadOnce"
        })
    });

    for num in nums {
        d.paren_arg(|f| num.fmt_hex(f));
    }

    d
}

impl fmt::Display for SpaceMemoryRangeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Display for SpaceIoRangeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Display for SpaceMemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl<W: AddressWidth> fmt::Display for AddressSpace<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let width_name = match W::width() {
            2 => "Word",
            4 => "DWord",
            8 => "QWord",
            _ => unreachable!(),
        };
        let space_name = match self.ty {
            AddressSpaceType::Memory { .. } => "Memory",
            AddressSpaceType::Io { .. } => "IO",
            AddressSpaceType::BusNumber => "BusNumber",
            AddressSpaceType::VendorDefined { .. } => "Space",
        };
        let write_min_fixed = |f: &mut fmt::Formatter<'_>| {
            if self.is_min_fixed {
                f.write_str("MinFixed")
            } else {
                f.write_str("MinNotFixed")
            }
        };
        let write_max_fixed = |f: &mut fmt::Formatter<'_>| {
            if self.is_max_fixed {
                f.write_str("MaxFixed")
            } else {
                f.write_str("MaxNotFixed")
            }
        };
        let write_decode = |f: &mut fmt::Formatter<'_>| {
            if self.is_subtract_decode {
                f.write_str("SubDecode")
            } else {
                f.write_str("PosDecode")
            }
        };

        f.write_str(width_name)?;
        f.write_str(space_name)?;
        let mut d = AmlDisplayer::start(f, "");

        if let AddressSpaceType::VendorDefined { value, .. } = self.ty {
            d.paren_arg(|f| write!(f, "0x{:X}", value));
        }
        d.paren_arg(|f| {
            f.write_str(if self.is_consumer {
                "ResourceConsumer"
            } else {
                "ResourceProducer"
            })
        });

        match &self.ty {
            AddressSpaceType::Memory {
                is_read_write, ty, ..
            } => {
                d.paren_arg(write_decode);
                d.paren_arg(write_min_fixed);
                d.paren_arg(write_max_fixed);
                d.paren_arg(|f| ty.fmt(f));
                d.paren_arg(|f| {
                    f.write_str(if *is_read_write {
                        "ReadWrite"
                    } else {
                        "ReadOnly"
                    })
                });
            }
            AddressSpaceType::Io { range_ty, .. } => {
                d.paren_arg(write_min_fixed);
                d.paren_arg(write_max_fixed);
                d.paren_arg(write_decode);
                d.paren_arg(|f| range_ty.fmt(f));
            }
            AddressSpaceType::BusNumber => {
                d.paren_arg(write_min_fixed);
                d.paren_arg(write_max_fixed);
                d.paren_arg(write_decode);
            }
            AddressSpaceType::VendorDefined { flags, .. } => {
                d.paren_arg(write_decode);
                d.paren_arg(write_min_fixed);
                d.paren_arg(write_max_fixed);
                d.paren_arg(|f| write!(f, "0x{:X}", flags));
            }
        }

        d.paren_arg(|f| self.granularity.fmt_hex(f));
        d.paren_arg(|f| self.min.fmt_hex(f));
        d.paren_arg(|f| self.max.fmt_hex(f));
        d.paren_arg(|f| self.translation_offset.fmt_hex(f));
        d.paren_arg(|f| self.len.fmt_hex(f));

        match &self.extra {
            ResourceSourceOrTypeSpecificAttrs::ResourceSource(resource_source) => {
                resource_source.display(&mut d);
            }
            ResourceSourceOrTypeSpecificAttrs::TypeSpecificAttrs(type_attrs) => {
                d.paren_arg(|f| write!(f, "0x{:8X}", type_attrs));
            }
        }

        match &self.ty {
            AddressSpaceType::Memory {
                range_ty,
                is_type_translation,
                ..
            } => {
                d.paren_arg(|f| range_ty.fmt(f));
                d.paren_arg(|f| {
                    f.write_str(if *is_type_translation {
                        "TypeTranslation"
                    } else {
                        "TypeStatic"
                    })
                });
            }
            AddressSpaceType::Io {
                is_type_translation,
                is_sparse_translation,
                ..
            } => {
                d.paren_arg(|f| {
                    f.write_str(if *is_type_translation {
                        "TypeTranslation"
                    } else {
                        "TypeStatic"
                    })
                });
                d.paren_arg(|f| {
                    f.write_str(if *is_sparse_translation {
                        "SparseTranslation"
                    } else {
                        "DenseTranslation"
                    })
                });
            }
            AddressSpaceType::BusNumber | AddressSpaceType::VendorDefined { .. } => {}
        }

        d.finish()
    }
}

impl fmt::Display for DmaSpeedType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Display for DmaTrasferType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Display for DmaTransferWidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Display for ResourceMacro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceMacro::Irq {
                wake_capable,
                is_shared,
                active_low,
                edge_triggered,
                irqs_mask,
            } => {
                let mut d = AmlDisplayer::start(f, "IRQ");
                display_interrupt_args(
                    &mut d,
                    None,
                    *wake_capable,
                    *is_shared,
                    *active_low,
                    *edge_triggered,
                    &ResourceSource::empty(),
                )
                .finish_paren_arg();

                for i in 0..15 {
                    if irqs_mask & (1 << i) != 0 {
                        d.body_field(|f| write!(f, "{}", i));
                    }
                }

                d.at_least_empty_body().finish()
            }
            ResourceMacro::Dma {
                speed_ty,
                is_bus_master,
                transfer_type,
                channels_mask,
            } => {
                let mut d = AmlDisplayer::start(f, "DMA");
                d.paren_arg(|f| speed_ty.fmt(f))
                    .paren_arg(|f| {
                        f.write_str(if *is_bus_master {
                            "BusMaster"
                        } else {
                            "NotBusMaster"
                        })
                    })
                    .paren_arg(|f| transfer_type.fmt(f));

                for i in 0..7 {
                    if channels_mask & (1 << i) != 0 {
                        d.body_field(|f| write!(f, "{}", i));
                    }
                }

                d.at_least_empty_body().finish()
            }
            // TODO: this is not correct, it should have a list of terms inside it, but meh, do it later
            ResourceMacro::StartDependentFunctions {
                compatibility_priority,
                performance_priority,
            } => AmlDisplayer::start(f, "StartDependentFn")
                .paren_arg(|f| write!(f, "{}", compatibility_priority))
                .paren_arg(|f| write!(f, "{}", performance_priority))
                .finish(),
            ResourceMacro::EndDependentFunctions => AmlDisplayer::start(f, "EndDependentFn")
                .at_least_empty_paren_arg()
                .finish(),
            ResourceMacro::Io {
                is_16_bit_decode,
                min_addr,
                max_addr,
                alignment,
                len,
            } => AmlDisplayer::start(f, "IO")
                .paren_arg(|f| {
                    f.write_str(if *is_16_bit_decode {
                        "Decode16"
                    } else {
                        "Decode10"
                    })
                })
                .paren_arg(|f| write!(f, "0x{:04X}", min_addr))
                .paren_arg(|f| write!(f, "0x{:04X}", max_addr))
                .paren_arg(|f| write!(f, "0x{:02X}", alignment))
                .paren_arg(|f| write!(f, "0x{:02X}", len))
                .finish(),
            ResourceMacro::FixedIo { base, len } => AmlDisplayer::start(f, "FixedIO")
                .paren_arg(|f| write!(f, "0x{:04X}", base))
                .paren_arg(|f| write!(f, "0x{:02X}", len))
                .finish(),
            ResourceMacro::FixedDma {
                dma_req,
                channel,
                transfer_width,
            } => AmlDisplayer::start(f, "FixedDMA")
                .paren_arg(|f| write!(f, "0x{:04X}", dma_req))
                .paren_arg(|f| write!(f, "0x{:04X}", channel))
                .paren_arg(|f| transfer_width.fmt(f))
                .finish(),
            ResourceMacro::VendorShort { data, len } => {
                let mut d = AmlDisplayer::start(f, "VendorShort");
                d.at_least_empty_paren_arg();

                for e in data.iter().take(*len as usize) {
                    d.body_field(|f| write!(f, "0x{:02X}", e));
                }

                d.finish()
            }
            ResourceMacro::VendorLarge { data } => {
                let mut d = AmlDisplayer::start(f, "VendorLarge");
                d.at_least_empty_paren_arg();

                for e in data {
                    d.body_field(|f| write!(f, "0x{:02X}", e));
                }

                d.finish()
            }
            ResourceMacro::Memory24 {
                is_read_write,
                min_addr,
                max_addr,
                alignment,
                len,
            } => display_memory_args(
                f,
                "Memory24",
                *is_read_write,
                &[min_addr, max_addr, alignment, len],
            )
            .finish(),
            ResourceMacro::Memory32Fixed {
                is_read_write,
                base_addr,
                len,
            } => {
                display_memory_args(f, "Memory32Fixed", *is_read_write, &[base_addr, len]).finish()
            }
            ResourceMacro::Memory32 {
                is_read_write,
                min_addr,
                max_addr,
                alignment,
                len,
            } => display_memory_args(
                f,
                "Memory32",
                *is_read_write,
                &[min_addr, max_addr, alignment, len],
            )
            .finish(),
            ResourceMacro::Interrupt {
                is_consumer,
                edge_triggered,
                active_low,
                is_shared,
                wake_capable,
                interrupts,
                resource_source,
            } => {
                let mut d = AmlDisplayer::start(f, "Interrupt");
                display_interrupt_args(
                    &mut d,
                    Some(*is_consumer),
                    *wake_capable,
                    *is_shared,
                    *active_low,
                    *edge_triggered,
                    resource_source,
                )
                .finish_paren_arg();

                for i in interrupts {
                    d.body_field(|f| write!(f, "0x{:08X}", i));
                }

                d.at_least_empty_body().finish()
            }
            ResourceMacro::Register {
                address_space,
                bit_width,
                offset,
                address,
                access_size,
            } => AmlDisplayer::start(f, "Register")
                .paren_arg(|f| address_space.fmt(f))
                .paren_arg(|f| write!(f, "0x{:02X}", bit_width))
                .paren_arg(|f| write!(f, "0x{:02X}", offset))
                .paren_arg(|f| write!(f, "0x{:08X}", address))
                .paren_arg(|f| write!(f, "{}", access_size.clone() as u8))
                .finish(),
            ResourceMacro::AddressSpaceWord(address_space) => address_space.fmt(f),
            ResourceMacro::AddressSpaceDWord(address_space) => address_space.fmt(f),
            ResourceMacro::AddressSpaceQWord(address_space) => address_space.fmt(f),
            ResourceMacro::AddressSpaceExtended(address_space) => address_space.fmt(f),
        }
    }
}

impl fmt::Display for ResourceTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = AmlDisplayer::start(f, "ResourceTemplate");
        d.at_least_empty_paren_arg();

        for item in &self.items {
            d.body_field(|f| item.fmt(f));
        }

        d.finish()
    }
}
