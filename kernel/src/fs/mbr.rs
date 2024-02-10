use core::mem;

use alloc::vec;

use crate::{devices::ide::IdeDevice, io::NoDebug, memory_management::memory_layout::align_up};

use super::FileSystemError;

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct PartitionEntry {
    pub bootable: u8,
    pub start_head: u8,
    pub start_sector: u8,
    pub start_cylinder: u8,
    pub partition_type: u8,
    pub end_head: u8,
    pub end_sector: u8,
    pub end_cylinder: u8,
    pub start_lba: u32,
    pub size_in_sectors: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct Mbr {
    pub boot_code: NoDebug<[u8; 220]>,
    pub original_first_physical_drive: u8,
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
    pub extended_boot_code: NoDebug<[u8; 216]>,
    pub disk_signature: u32,
    pub copy_protection: u16,
    pub partition_table: [PartitionEntry; 4],
    pub signature: u16,
}

impl Mbr {
    pub fn try_create_from_disk(device: &IdeDevice) -> Result<Self, FileSystemError> {
        let size = align_up(mem::size_of::<Self>(), device.sector_size() as usize);
        let mut sectors = vec![0; size];

        device
            .read_sync(0, &mut sectors)
            .map_err(|e| FileSystemError::DiskReadError {
                sector: 0,
                error: e,
            })?;

        // SAFETY: This is a valid allocated memory
        let mbr = unsafe { &*(sectors.as_ptr() as *const Mbr) };

        // if valid
        if mbr.signature == 0xAA55 {
            Ok(mbr.clone())
        } else {
            Err(FileSystemError::PartitionTableNotFound)
        }
    }
}
