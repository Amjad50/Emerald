use core::{ffi, fmt, mem};

use crate::{
    acpi::tables::{Rsdp, RsdpV1, RsdpV2},
    io::NoDebug,
    memory_management::memory_layout::{align_up, MemSize, PAGE_4K},
};

#[repr(u32)]
#[derive(Debug, PartialEq, Eq)]
pub enum MemoryMapType {
    Available = 1,
    Reserved = 2,
    ACPIReclaimable = 3,
    ACPINonVolatile = 4,
    BadMemory = 5,
    Undefined(u32),
}

#[derive(Debug)]
pub struct MemoryMap {
    pub base_addr: u64,
    pub length: u64,
    pub mem_type: MemoryMapType,
}

impl fmt::Display for MemoryMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "range={:016X}..{:016X}, len={:4}, ty={:?}",
            self.base_addr,
            self.base_addr + self.length,
            MemSize(self.length),
            self.mem_type
        )
    }
}

struct MemoryMapTagRaw {
    entry_size: u32,
    _entry_version: u32,
}

#[derive(Clone, Debug)]
pub struct MemoryMapIter {
    remaining: usize,
    entry_size: u32,
    memory_map_raw: *const MemoryMapsRaw,
}

impl Iterator for MemoryMapIter {
    type Item = MemoryMap;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let ptr = self.memory_map_raw;
        let mmap = unsafe { &*ptr };
        let memory_map = MemoryMap {
            base_addr: mmap.base_addr,
            length: mmap.length,
            mem_type: match mmap.mem_type {
                1 => MemoryMapType::Available,
                2 => MemoryMapType::Reserved,
                3 => MemoryMapType::ACPIReclaimable,
                4 => MemoryMapType::ACPINonVolatile,
                5 => MemoryMapType::BadMemory,
                n => MemoryMapType::Undefined(n),
            },
        };
        self.memory_map_raw =
            (self.memory_map_raw as u64).wrapping_add(self.entry_size as _) as *const MemoryMapsRaw;
        self.remaining = self.remaining.saturating_sub(self.entry_size as _);
        Some(memory_map)
    }
}

#[repr(u32)]
#[derive(Debug, PartialEq, Eq)]
pub enum EfiMemoryMapType {
    Reserved = 0,
    LoaderCode = 1,
    LoaderData = 2,
    BootServicesCode = 3,
    BootServicesData = 4,
    RuntimeServicesCode = 5,
    RuntimeServicesData = 6,
    Conventional = 7,
    Unusable = 8,
    ACPIReclaimable = 9,
    ACPINonVolatile = 10,
    MemoryMappedIO = 11,
    MemoryMappedIOPortSpace = 12,
    PalCode = 13,
    PersistentMemory = 14,
    Undefined(u32),
}

#[derive(Debug)]
pub struct EfiMemoryMap {
    pub mem_type: EfiMemoryMapType,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attributes: u64,
}

impl fmt::Display for EfiMemoryMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "range={:016X}..{:016X}, (virt_start={:016X}), len={:4}, ty={:?}, attributes={:X}",
            self.physical_start,
            self.physical_start + self.number_of_pages * PAGE_4K as u64,
            self.virtual_start,
            MemSize(self.number_of_pages * PAGE_4K as u64),
            self.mem_type,
            self.attributes
        )
    }
}

#[repr(C, packed(4))]
struct EfiMemoryMapsRaw {
    pub mem_type: u64,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attributes: u64,
}

#[derive(Clone, Debug)]
pub struct EfiMemoryMapIter {
    remaining: usize,
    entry_size: u32,
    memory_map_raw: *const EfiMemoryMapsRaw,
}

impl Iterator for EfiMemoryMapIter {
    type Item = EfiMemoryMap;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let ptr = self.memory_map_raw;
        let mmap = unsafe { &*ptr };
        let memory_map = EfiMemoryMap {
            physical_start: mmap.physical_start,
            virtual_start: mmap.virtual_start,
            number_of_pages: mmap.number_of_pages,
            attributes: mmap.attributes,
            mem_type: match mmap.mem_type {
                0 => EfiMemoryMapType::Reserved,
                1 => EfiMemoryMapType::LoaderCode,
                2 => EfiMemoryMapType::LoaderData,
                3 => EfiMemoryMapType::BootServicesCode,
                4 => EfiMemoryMapType::BootServicesData,
                5 => EfiMemoryMapType::RuntimeServicesCode,
                6 => EfiMemoryMapType::RuntimeServicesData,
                7 => EfiMemoryMapType::Conventional,
                8 => EfiMemoryMapType::Unusable,
                9 => EfiMemoryMapType::ACPIReclaimable,
                10 => EfiMemoryMapType::ACPINonVolatile,
                11 => EfiMemoryMapType::MemoryMappedIO,
                12 => EfiMemoryMapType::MemoryMappedIOPortSpace,
                13 => EfiMemoryMapType::PalCode,
                14 => EfiMemoryMapType::PersistentMemory,
                n => EfiMemoryMapType::Undefined(n as _),
            },
        };
        self.memory_map_raw = (self.memory_map_raw as u64).wrapping_add(self.entry_size as _)
            as *const EfiMemoryMapsRaw;
        self.remaining = self.remaining.saturating_sub(self.entry_size as _);
        Some(memory_map)
    }
}

#[repr(C, packed(4))]
struct MemoryMapsRaw {
    base_addr: u64,
    length: u64,
    mem_type: u32,
    reserved: u32,
}

#[derive(Debug, Clone)]
pub enum FramebufferColorInfo {
    Indexed {
        num_colors: u32,
        // TODO: add colors iter
    },
    Rgb {
        red_field_position: u8,
        red_mask_size: u8,
        green_field_position: u8,
        green_mask_size: u8,
        blue_field_position: u8,
        blue_mask_size: u8,
    },
    EgaText,
}

impl FramebufferColorInfo {
    fn from_color_info(ty: u8, color_info: &[u8]) -> Self {
        match ty {
            0 => {
                let num_colors = u32::from_le_bytes([
                    color_info[0],
                    color_info[1],
                    color_info[2],
                    color_info[3],
                ]);
                Self::Indexed { num_colors }
            }
            1 => Self::Rgb {
                red_field_position: color_info[0],
                red_mask_size: color_info[1],
                green_field_position: color_info[2],
                green_mask_size: color_info[3],
                blue_field_position: color_info[4],
                blue_mask_size: color_info[5],
            },
            2 => Self::EgaText,
            _ => panic!("unknown framebuffer color info type"),
        }
    }

    pub fn is_rgb(&self) -> bool {
        matches!(self, Self::Rgb { .. })
    }
}

#[repr(C)]
struct FramebufferRaw {
    addr: u64,
    pitch: u32,
    width: u32,
    height: u32,
    bpp: u8,
    framebuffer_type: u8,
    reserved: u16,
}

#[derive(Debug, Clone)]
pub struct Framebuffer {
    pub addr: u64,
    pub pitch: u32,
    pub width: u32,
    pub height: u32,
    pub bpp: u8,
    pub color_info: FramebufferColorInfo,
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct VbeControlInfo {
    pub signature: [u8; 4],
    pub version: u16,
    pub oem_str_ptr: u32,
    pub capabilities: u32,
    pub video_modes_ptr: u32,
    pub video_memory_size_blocks: u16,
    pub software_rev: u16,
    pub vendor: u32,
    pub product_name: u32,
    pub product_rev: u32,
    pub reserved: NoDebug<[u8; 222]>,
    pub oem_data: NoDebug<[u8; 256]>,
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct VbeModeInfo {
    pub attributes: u16,
    pub window_a_attributes: u8,
    pub window_b_attributes: u8,
    pub window_granularity: u16,
    pub window_size: u16,
    pub window_a_segment: u16,
    pub window_b_segment: u16,
    pub window_func_ptr: u32,
    pub bytes_per_scanline: u16,
    pub width: u16,
    pub height: u16,
    pub w_char: u8,
    pub y_char: u8,
    pub planes: u8,
    pub bpp: u8,
    pub banks: u8,
    pub memory_model: u8,
    pub bank_size: u8,
    pub image_pages: u8,
    pub reserved0: u8,
    pub red_mask_size: u8,
    pub red_field_position: u8,
    pub green_mask_size: u8,
    pub green_field_position: u8,
    pub blue_mask_size: u8,
    pub blue_field_position: u8,
    pub rsvd_mask_size: u8,
    pub rsvd_field_position: u8,
    pub direct_color_mode_attributes: u8,
    pub framebuffer_addr: u32,
    pub reserved1: NoDebug<[u8; 212]>,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct VbeInfo {
    pub mode: u16,
    pub interface_seg: u16,
    pub interface_off: u16,
    pub interface_len: u16,
    pub control_info: VbeControlInfo,
    pub mode_info: VbeModeInfo,
}

struct MultiBootTagRaw {
    ty: u32,
    size: u32,
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct BasicMemoryInfo {
    mem_lower: u32,
    mem_upper: u32,
}

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct AdvancedPowerManagementTable {
    version: u16,
    cseg: u16,
    offset: u32,
    cseg_16: u16,
    dseg: u16,
    flags: u16,
    cseg_len: u16,
    cseg_16_len: u16,
    dseg_len: u16,
}

#[derive(Debug, Clone)]
pub enum MultiBootTag<'a> {
    BootCommandLine {
        cmdline: &'a str,
    },
    BootLoaderName {
        name: &'a str,
    },
    BasicMemoryInfo(&'a BasicMemoryInfo),
    AdvancedPowerManagementTable(&'a AdvancedPowerManagementTable),
    ImageLoadBasePhysical {
        base_addr: u32,
    },
    MemoryMap(MemoryMapIter),
    EfiMemoryMap(EfiMemoryMapIter),
    ElfSymbols,
    BiosBootDevice {
        biosdev: u32,
        partition: u32,
        sub_partition: u32,
    },
    FrameBufferInfo(Framebuffer),
    OldRsdp(Rsdp),
    NewRsdp(Rsdp),
    Efi64SystemTablePtr {
        ptr: u64,
    },
    EfiBootServicesNotTerminated,
    Efi64ImageHandle {
        ptr: u64,
    },
    VbeInfo(&'a VbeInfo),
}

pub struct MultiBootTagIter<'a> {
    current: *const MultiBootTagRaw,
    remaining: usize,
    phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a> Iterator for MultiBootTagIter<'a> {
    type Item = MultiBootTag<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let ptr = self.current;
        let tag = unsafe { &*ptr };
        let tag_size = align_up(tag.size as _, 8);
        let next = unsafe { (ptr as *const u8).add(tag_size) as *const MultiBootTagRaw };
        self.remaining -= tag_size;
        self.current = next;
        let tag = match tag.ty {
            0 => {
                // end
                assert_eq!(tag.size as usize, mem::size_of::<MultiBootTagRaw>());
                assert_eq!(self.remaining, 0);
                return None;
            }
            1 => {
                let str_ptr = unsafe { ptr.add(1) as *const i8 };
                let cmdline =
                    unsafe { ffi::CStr::from_ptr(str_ptr).to_str().expect("invalid utf8") };
                MultiBootTag::BootCommandLine { cmdline }
            }
            2 => {
                let str_ptr = unsafe { ptr.add(1) as *const i8 };
                let name = unsafe { ffi::CStr::from_ptr(str_ptr).to_str().expect("invalid utf8") };
                MultiBootTag::BootLoaderName { name }
            }
            4 => {
                let tag = unsafe { &*(ptr.add(1) as *const BasicMemoryInfo) };
                MultiBootTag::BasicMemoryInfo(tag)
            }
            5 => {
                let tag = unsafe { ptr.add(1) as *const u32 };
                let data_slice = unsafe { core::slice::from_raw_parts(tag, 3) };
                MultiBootTag::BiosBootDevice {
                    biosdev: data_slice[0],
                    partition: data_slice[1],
                    sub_partition: data_slice[2],
                }
            }
            6 => {
                let mmap_tag = unsafe { &*(ptr.add(1) as *const MemoryMapTagRaw) };
                MultiBootTag::MemoryMap(MemoryMapIter {
                    remaining: tag.size as usize
                        - mem::size_of::<MultiBootTagRaw>()
                        - mem::size_of::<MemoryMapTagRaw>(),
                    entry_size: mmap_tag.entry_size,
                    memory_map_raw: unsafe { (mmap_tag as *const MemoryMapTagRaw).add(1) as _ },
                })
            }
            7 => {
                let vbe_tag = unsafe { &*(ptr.add(1) as *const VbeInfo) };
                MultiBootTag::VbeInfo(vbe_tag)
            }
            8 => {
                let frame_tag = unsafe { &*(ptr.add(1) as *const FramebufferRaw) };
                let color_info_start =
                    unsafe { (frame_tag as *const FramebufferRaw).add(1) as *const u8 };
                let remaining_size = tag.size as usize
                    - mem::size_of::<MultiBootTagRaw>()
                    - mem::size_of::<FramebufferRaw>();
                let color_info =
                    unsafe { core::slice::from_raw_parts(color_info_start, remaining_size) };
                MultiBootTag::FrameBufferInfo(Framebuffer {
                    addr: frame_tag.addr,
                    pitch: frame_tag.pitch,
                    width: frame_tag.width,
                    height: frame_tag.height,
                    bpp: frame_tag.bpp,
                    color_info: FramebufferColorInfo::from_color_info(
                        frame_tag.framebuffer_type,
                        color_info,
                    ),
                })
            }
            9 => {
                let _tag = unsafe { &*(ptr.add(1) as *const u32) };
                MultiBootTag::ElfSymbols
            }
            10 => {
                let tag = unsafe { &*(ptr.add(1) as *const AdvancedPowerManagementTable) };
                MultiBootTag::AdvancedPowerManagementTable(tag)
            }
            12 => {
                let efi64_ptr = unsafe { &*(ptr.add(1) as *const u64) };
                MultiBootTag::Efi64SystemTablePtr { ptr: *efi64_ptr }
            }
            14 => {
                let old_rsdp = unsafe { &*(ptr.add(1) as *const RsdpV1) };
                assert!(
                    tag.size as usize - mem::size_of::<MemoryMapTagRaw>()
                        == mem::size_of::<RsdpV1>(),
                );

                MultiBootTag::OldRsdp(Rsdp::from_v1(old_rsdp))
            }
            15 => {
                let new_rsdp = unsafe { &*(ptr.add(1) as *const RsdpV2) };
                assert!(
                    tag.size as usize - mem::size_of::<MemoryMapTagRaw>()
                        == mem::size_of::<RsdpV2>(),
                );

                MultiBootTag::NewRsdp(Rsdp::from_v2(new_rsdp))
            }
            17 => {
                let efi_mmap = unsafe { &*(ptr.add(1) as *const MemoryMapTagRaw) };

                MultiBootTag::EfiMemoryMap(EfiMemoryMapIter {
                    remaining: tag.size as usize
                        - mem::size_of::<MultiBootTagRaw>()
                        - mem::size_of::<MemoryMapTagRaw>(),
                    entry_size: efi_mmap.entry_size,
                    memory_map_raw: unsafe { (efi_mmap as *const MemoryMapTagRaw).add(1) as _ },
                })
            }
            18 => MultiBootTag::EfiBootServicesNotTerminated,
            20 => {
                let efi64_image_handle = unsafe { &*(ptr.add(1) as *const u64) };
                MultiBootTag::Efi64ImageHandle {
                    ptr: *efi64_image_handle,
                }
            }
            21 => {
                let tag = unsafe { &*(ptr.add(1) as *const u32) };
                MultiBootTag::ImageLoadBasePhysical { base_addr: *tag }
            }
            t => unimplemented!("tag {t}"),
        };
        Some(tag)
    }
}

#[repr(C, packed(4))]
pub struct MultiBoot2Info {
    total_size: u32,
    reserved: u32,
}

impl MultiBoot2Info {
    fn data_ptr(&self) -> *const u8 {
        unsafe { (self as *const Self as *const u8).add(8) }
    }

    #[allow(dead_code)]
    fn data_slice(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self.data_ptr(),
                self.total_size as usize - mem::size_of::<MultiBoot2Info>(),
            )
        }
    }

    pub fn end_address(&self) -> u64 {
        unsafe { (self as *const Self as *const u8).add(self.total_size as _) as _ }
    }

    pub fn tags(&self) -> MultiBootTagIter<'_> {
        MultiBootTagIter {
            current: unsafe { (self as *const Self as *const u8).add(8) as _ },
            remaining: self.total_size as usize - mem::size_of::<MultiBoot2Info>(),
            phantom: core::marker::PhantomData,
        }
    }

    pub fn cmdline(&self) -> Option<&str> {
        self.tags().find_map(|tag| match tag {
            MultiBootTag::BootCommandLine { cmdline } => Some(cmdline),
            _ => None,
        })
    }

    pub fn memory_maps(&self) -> Option<impl Iterator<Item = MemoryMap> + '_> {
        self.tags().find_map(|tag| match tag {
            MultiBootTag::MemoryMap(mmap) => Some(mmap),
            _ => None,
        })
    }

    pub fn framebuffer(&self) -> Option<Framebuffer> {
        self.tags().find_map(|tag| match tag {
            MultiBootTag::FrameBufferInfo(fb) => Some(fb),
            _ => None,
        })
    }

    pub fn vbe_info(&self) -> Option<&VbeInfo> {
        self.tags().find_map(|tag| match tag {
            MultiBootTag::VbeInfo(vbe) => Some(vbe),
            _ => None,
        })
    }

    pub fn get_most_recent_rsdp(&self) -> Option<Rsdp> {
        let mut ret_rdsp: Option<Rsdp> = None;
        for tag in self.tags() {
            match tag {
                MultiBootTag::OldRsdp(rsdp) | MultiBootTag::NewRsdp(rsdp) => {
                    // only override if new is higher version
                    if ret_rdsp.is_none() || ret_rdsp.as_ref().unwrap().revision < rsdp.revision {
                        ret_rdsp = Some(rsdp);
                    }
                }
                _ => {}
            }
        }
        ret_rdsp
    }
}

impl fmt::Display for MultiBoot2Info {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Multiboot2:")?;
        for tag in self.tags() {
            match tag {
                MultiBootTag::MemoryMap(mmap) => {
                    writeln!(f, "  MemoryMap:")?;
                    for memory in mmap {
                        writeln!(f, "    {}", memory)?;
                    }
                }
                MultiBootTag::EfiMemoryMap(mmap) => {
                    writeln!(f, "  EfiMemoryMap:")?;
                    for memory in mmap {
                        writeln!(f, "    {}", memory)?;
                    }
                }
                t => writeln!(f, "  {:X?}", t)?,
            }
        }
        Ok(())
    }
}
