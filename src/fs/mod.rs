use core::mem;

use alloc::vec;

use crate::{
    devices::ide::{self, IdeDeviceIndex, IdeDeviceType},
    memory_management::memory_layout::align_up,
};

use self::mbr::MbrRaw;

mod mbr;

#[derive(Debug)]
pub enum FileSystemError {
    PartitionTableNotFound,
    DeviceNotFound,
    DiskReadError { sector: u64, error: ide::IdeError },
}

/// Loads the hard disk specified in the argument
/// it will load the first partition (MBR) if any, otherwise it will treat the whole disk
/// as one partition
///
/// Creates a new filesystem mapping for `/` and the filesystem found
pub fn init_filesystem(hard_disk_index: usize) -> Result<(), FileSystemError> {
    let ide_index = IdeDeviceIndex {
        ty: IdeDeviceType::Ata,
        index: hard_disk_index,
    };

    let device = ide::get_ide_device(ide_index).ok_or(FileSystemError::DeviceNotFound)?;

    let size = align_up(mem::size_of::<MbrRaw>(), device.sector_size() as usize);
    let mut sectors = vec![0; size];

    device
        .read_sync(0, &mut sectors)
        .map_err(|e| FileSystemError::DiskReadError {
            sector: 0,
            error: e,
        })?;

    // SAFETY: This is a valid allocated memory and the size is at least the size of MbrRaw
    let mbr = unsafe { &*(sectors.as_ptr() as *const MbrRaw) };

    if mbr.is_valid() {
        // found MBR

        let first_partition = &mbr.partition_table[0];
        println!("Found MBR, first partition: {:?}", first_partition);
        Ok(())
    } else {
        Err(FileSystemError::PartitionTableNotFound)
    }
}
