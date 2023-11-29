use core::{ffi, fmt, mem};

use crate::memory_management::memory_layout::{physical2virtual, MemSize};

#[repr(u32)]
#[derive(Debug, PartialEq, Eq)]
pub enum MemoryMapType {
    Available = 1,
    Reserved = 2,
    ACPIReclaimable = 3,
    ACPINvs = 4,
    BadMemory = 5,
    Undefined(u32),
}

#[derive(Debug)]
pub struct MemoryMap {
    pub base_addr: u64,
    pub length: u64,
    pub mem_type: MemoryMapType,
}

pub struct MemoryMapIter {
    memory_map_raw: *const MemoryMapsRaw,
    remaining: u32,
}

impl Iterator for MemoryMapIter {
    type Item = MemoryMap;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let ptr = physical2virtual(self.memory_map_raw as _) as *const MemoryMapsRaw;
        let mmap = unsafe { &*ptr };
        let memory_map = MemoryMap {
            base_addr: (mmap.base_addr_high as u64) << 32 | mmap.base_addr_low as u64,
            length: (mmap.length_high as u64) << 32 | mmap.length_low as u64,
            mem_type: match mmap.mem_type {
                1 => MemoryMapType::Available,
                2 => MemoryMapType::Reserved,
                3 => MemoryMapType::ACPIReclaimable,
                4 => MemoryMapType::ACPINvs,
                5 => MemoryMapType::BadMemory,
                n => MemoryMapType::Undefined(n),
            },
        };
        self.memory_map_raw =
            (self.memory_map_raw as u64).wrapping_add(mmap.size as u64 + 4) as *const MemoryMapsRaw;
        self.remaining = self.remaining.saturating_sub(mmap.size + 4);
        Some(memory_map)
    }
}

#[repr(C, packed(4))]
struct MemoryMapsRaw {
    size: u32,
    base_addr_low: u32,
    base_addr_high: u32,
    length_low: u32,
    length_high: u32,
    mem_type: u32,
}

#[derive(Debug, Clone)]
#[repr(C, packed(4))]
pub struct ModRaw {
    mod_start: u32,
    mod_end: u32,
    string: [u8; 4],
    reserved: u32,
}

pub struct ModsIter {
    mods_addr: *const ModRaw,
    remaining: u32,
}

impl Iterator for ModsIter {
    type Item = ModRaw;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let ptr = self.mods_addr;
        let mod_raw = unsafe { &*ptr };
        self.mods_addr =
            (self.mods_addr as u64).wrapping_add(mem::size_of::<ModRaw>() as u64) as *const ModRaw;
        self.remaining = self.remaining.saturating_sub(1);
        Some(mod_raw.clone())
    }
}

#[derive(Debug, Clone)]
pub struct VbeInfo {
    pub control_info: u32,
    pub mode_info: u32,
    pub mode: u16,
    pub interface_seg: u16,
    pub interface_off: u16,
    pub interface_len: u16,
}

#[derive(Debug, Clone)]
pub enum FramebufferColorInfo {
    Indexed {
        palette_addr: u32,
        palette_num_colors: u16,
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
    fn from_color_info(ty: u8, color_info: [u8; 6]) -> Self {
        match ty {
            0 => Self::Indexed {
                palette_addr: u32::from_le_bytes([
                    color_info[0],
                    color_info[1],
                    color_info[2],
                    color_info[3],
                ]),
                palette_num_colors: u16::from_le_bytes([color_info[4], color_info[5]]),
            },
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

#[repr(C, packed(4))]
pub struct MultiBootInfoRaw {
    pub flags: u32,
    mem_lower: u32,
    mem_upper: u32,
    boot_device: [u8; 4],
    cmdline: u32,
    mods_count: u32,
    mods_addr: u32,
    syms: [u32; 4],
    mmap_length: u32,
    mmap_addr: u32,
    drives_length: u32,
    drives_addr: u32,
    config_table: u32,
    bootloader_name: u32,
    apm_table: u32,
    vbe_control_info: u32,
    vbe_mode_info: u32,
    vbe_mode: u16,
    vbe_interface_seg: u16,
    vbe_interface_off: u16,
    vbe_interface_len: u16,

    framebuffer_addr: u64,
    framebuffer_pitch: u32,
    framebuffer_width: u32,
    framebuffer_height: u32,
    framebuffer_bpp: u8,
    framebuffer_type: u8,
    color_info: [u8; 6],
}

impl MultiBootInfoRaw {
    // SAFETY: the caller must assure that the pointer passed is valid and properly aligned
    pub unsafe fn from_ptr(multiboot_info_ptr: usize) -> Self {
        // copy the multiboot info struct from the pointer
        core::ptr::read(multiboot_info_ptr as *const MultiBootInfoRaw)
    }

    pub fn lower_memory_size(&self) -> Option<u64> {
        if self.flags & 0b1 != 0 {
            Some(self.mem_lower as u64 * 1024)
        } else {
            None
        }
    }

    pub fn upper_memory_size(&self) -> Option<u64> {
        if self.flags & 0b1 != 0 {
            Some(self.mem_upper as u64 * 1024)
        } else {
            None
        }
    }

    pub fn boot_device(&self) -> Option<[u8; 4]> {
        if self.flags & 0b10 != 0 {
            Some(self.boot_device)
        } else {
            None
        }
    }

    pub fn cmdline(&self) -> Option<&str> {
        if self.flags & 0b100 != 0 {
            let ptr = physical2virtual(self.cmdline as _) as *const u8;
            unsafe {
                Some(
                    ffi::CStr::from_ptr(ptr as *const i8)
                        .to_str()
                        .expect("invalid utf8"),
                )
            }
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn mods(&self) -> Option<ModsIter> {
        if self.flags & 0b1000 != 0 && self.mods_count != 0 {
            Some(ModsIter {
                mods_addr: self.mods_addr as *const ModRaw,
                remaining: self.mods_count,
            })
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn syms(&self) -> Option<()> {
        if self.flags & 0b10000 != 0 {
            todo!("implement syms")
        } else {
            None
        }
    }

    pub fn memory_maps(&self) -> Option<MemoryMapIter> {
        if self.flags & 0b1000000 != 0 {
            Some(MemoryMapIter {
                memory_map_raw: self.mmap_addr as *const MemoryMapsRaw,
                remaining: self.mmap_length,
            })
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn drives(&self) -> Option<()> {
        if self.flags & 0b10000000 != 0 {
            todo!("implement drives")
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn config_table(&self) -> Option<()> {
        if self.flags & 0b100000000 != 0 {
            todo!("implement config_table")
        } else {
            None
        }
    }

    pub fn bootloader_name(&self) -> Option<&str> {
        if self.flags & 0b1000000000 != 0 {
            let ptr = physical2virtual(self.bootloader_name as _) as *const u8;

            unsafe {
                Some(
                    ffi::CStr::from_ptr(ptr as *const i8)
                        .to_str()
                        .expect("invalid utf8"),
                )
            }
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn apm_table(&self) -> Option<()> {
        if self.flags & 0b10000000000 != 0 {
            todo!("implement apm_table")
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn vbe(&self) -> Option<VbeInfo> {
        if self.flags & 0b100000000000 != 0 {
            Some(VbeInfo {
                control_info: self.vbe_control_info,
                mode_info: self.vbe_mode_info,
                mode: self.vbe_mode,
                interface_seg: self.vbe_interface_seg,
                interface_off: self.vbe_interface_off,
                interface_len: self.vbe_interface_len,
            })
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn framebuffer(&self) -> Option<Framebuffer> {
        if self.flags & 0b1000000000000 != 0 {
            Some(Framebuffer {
                addr: self.framebuffer_addr,
                pitch: self.framebuffer_pitch,
                width: self.framebuffer_width,
                height: self.framebuffer_height,
                bpp: self.framebuffer_bpp,
                color_info: FramebufferColorInfo::from_color_info(
                    self.framebuffer_type,
                    self.color_info,
                ),
            })
        } else {
            None
        }
    }
}

impl fmt::Display for MultiBootInfoRaw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Flags: {:012b}", self.flags)?;
        writeln!(
            f,
            "Lower memory Size: {:?}",
            self.lower_memory_size().map(MemSize)
        )?;
        writeln!(
            f,
            "Upper memory Size: {:?}",
            self.upper_memory_size().map(MemSize)
        )?;
        writeln!(f, "Boot device: {:X?}", self.boot_device())?;
        writeln!(f, "Cmdline: {:X?}", self.cmdline())?;
        if let Some(mods) = self.mods() {
            writeln!(f, "Mods:")?;
            for mod_ in mods {
                writeln!(
                    f,
                    "start={:010X}, end={:010X}, string={:X?}",
                    mod_.mod_start, mod_.mod_end, mod_.string
                )?;
            }
        } else {
            writeln!(f, "Mods: None")?;
        }
        if let Some(memory_maps) = self.memory_maps() {
            writeln!(f, "Memory maps:")?;
            for map in memory_maps {
                writeln!(
                    f,
                    "base={:010X}, len={:10}, ty={:?}",
                    map.base_addr,
                    MemSize(map.length),
                    map.mem_type
                )?;
            }
        } else {
            writeln!(f, "Memory maps: None")?;
        }
        writeln!(f, "Bootloader name: {:X?}", self.bootloader_name())?;
        writeln!(f, "VBE: {:X?}", self.vbe())?;
        writeln!(f, "Framebuffer: {:X?}", self.framebuffer())?;
        Ok(())
    }
}
