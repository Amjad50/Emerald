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

    pub fn get_data_as_slice(&mut self, len: usize) -> Result<&[u8], AmlParseError> {
        if self.pos + len > self.buffer.len() {
            return Err(AmlParseError::UnexpectedEndOfCode);
        }

        let slice = &self.buffer[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
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
        resource_source_index: Option<u8>,
        resource_source: Option<Vec<u8>>,
    },
    Register {
        address_space: RegionSpace,
        bit_width: u8,
        offset: u8,
        address: u64,
        access_size: AccessType,
    },
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
            0x04 => {
                let data = parser.get_data_as_slice(data_len as usize)?;

                Some(ResourceMacro::VendorLarge {
                    data: data.to_vec(),
                })
            }
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
            0x09 => {
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

                let mut len_so_far = table_len as u16 * 4 + 2;
                let resource_source_index = if len_so_far < data_len {
                    Some(parser.get_next_byte()?)
                } else {
                    None
                };
                len_so_far += 1;

                let resource_source = if len_so_far < data_len {
                    let buffer =
                        parser.get_data_as_slice(data_len as usize - len_so_far as usize)?;
                    Some(buffer.to_vec())
                } else {
                    None
                };
                Some(ResourceMacro::Interrupt {
                    is_consumer: is_consumed,
                    edge_triggered,
                    active_low,
                    is_shared,
                    wake_capable,
                    interrupts,
                    resource_source_index,
                    resource_source,
                })
            }
            0x0A => {
                assert_eq!(data_len, 43);

                todo!()
            }
            _ => None,
        };

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

#[allow(clippy::too_many_arguments)]
fn display_interrupt_args<'a, 'b, 'r>(
    d: &'r mut AmlDisplayer<'a, 'b>,
    is_consumer: Option<bool>,
    wake_capable: bool,
    is_shared: bool,
    active_low: bool,
    edge_triggered: bool,
    resource_source_index: Option<u8>,
    resource_source: Option<&Vec<u8>>,
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
    if let Some(resource_source_index) = resource_source_index {
        d.paren_arg(|f| write!(f, "{}", resource_source_index));
    }
    if let Some(resource_source) = resource_source {
        d.paren_arg(|f| write!(f, "{:?}", ByteStr(resource_source)));
    }

    d
}

trait NumHexDisplay {
    fn fmt_hex(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl NumHexDisplay for u8 {
    fn fmt_hex(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:02X}", self)
    }
}

impl NumHexDisplay for u16 {
    fn fmt_hex(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:04X}", self)
    }
}

impl NumHexDisplay for u32 {
    fn fmt_hex(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08X}", self)
    }
}

impl NumHexDisplay for u64 {
    fn fmt_hex(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:016X}", self)
    }
}

fn display_memory_args<'a, 'b>(
    f: &'a mut fmt::Formatter<'b>,
    name: &str,
    is_read_write: bool,
    nums: &[&dyn NumHexDisplay],
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
                    None,
                    None,
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
                resource_source_index,
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
                    *resource_source_index,
                    resource_source.as_ref(),
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
