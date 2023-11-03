use core::{ffi, fmt};

use crate::memory_management::memory_layout::MemSize;

#[repr(u32)]
#[derive(Debug, PartialEq, Eq)]
pub enum MemoryMapType {
    Available = 1,
    Reserved = 2,
    ACPIReclaimable = 3,
    ACPINvs = 4,
    BadMemory = 5,
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
        let mmap = unsafe { &*self.memory_map_raw };
        let memory_map = MemoryMap {
            base_addr: (mmap.base_addr_high as u64) << 32 | mmap.base_addr_low as u64,
            length: (mmap.length_high as u64) << 32 | mmap.length_low as u64,
            mem_type: match mmap.mem_type {
                1 => MemoryMapType::Available,
                2 => MemoryMapType::Reserved,
                3 => MemoryMapType::ACPIReclaimable,
                4 => MemoryMapType::ACPINvs,
                5 => MemoryMapType::BadMemory,
                _ => panic!("unknown memory map type"),
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

    pub fn lower_memory_size(&self) -> Option<usize> {
        if self.flags & 0b1 != 0 {
            Some(self.mem_lower as usize * 1024)
        } else {
            None
        }
    }

    pub fn upper_memory_size(&self) -> Option<usize> {
        if self.flags & 0b1 != 0 {
            Some(self.mem_upper as usize * 1024)
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
            let ptr = self.cmdline as *const u8;
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
    pub fn mods(&self) -> Option<()> {
        if self.flags & 0b1000 != 0 {
            todo!("implement mods")
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
            let ptr = self.bootloader_name as *const u8;
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
    pub fn vbe(&self) -> Option<()> {
        if self.flags & 0b100000000000 != 0 {
            todo!("implement vbe")
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn framebuffer(&self) -> Option<()> {
        if self.flags & 0b1000000000000 != 0 {
            todo!("implement framebuffer")
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
        if let Some(memory_maps) = self.memory_maps() {
            writeln!(f, "Memory maps:")?;
            for map in memory_maps {
                writeln!(
                    f,
                    "base={:010X}, len={:10}, ty={:?}",
                    map.base_addr,
                    MemSize(map.length as usize),
                    map.mem_type
                )?;
            }
        } else {
            writeln!(f, "Memory maps: None")?;
        }
        writeln!(f, "Bootloader name: {:X?}", self.bootloader_name())?;
        Ok(())
    }
}
