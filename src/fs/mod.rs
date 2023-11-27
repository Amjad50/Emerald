use core::mem;

use alloc::{borrow::Cow, string::String, vec, vec::Vec};

use crate::{
    devices::ide::{self, IdeDeviceIndex, IdeDeviceType},
    memory_management::memory_layout::align_up,
    sync::spin::mutex::Mutex,
};

use self::{fat::DirectoryEntry, mbr::MbrRaw};

mod fat;
mod mbr;

static FILESYSTEM_MAPPING: Mutex<FileSystemMapping> = Mutex::new(FileSystemMapping {
    mappings: Vec::new(),
});

struct FileSystemMapping {
    mappings: Vec<(String, fat::FatFilesystem)>,
}

#[derive(Debug)]
pub enum FileSystemError {
    PartitionTableNotFound,
    DeviceNotFound,
    DiskReadError { sector: u64, error: ide::IdeError },
    FatError(fat::FatError),
    FileNotFound,
    InvalidPath,
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

    // SAFETY: This is a valid allocated memory
    let mbr = unsafe { &*(sectors.as_ptr() as *const MbrRaw) };

    if mbr.is_valid() {
        // found MBR

        let first_partition = &mbr.partition_table[0];
        let filesystem = fat::load_fat_filesystem(
            ide_index,
            first_partition.start_lba,
            first_partition.size_in_sectors,
        )?;
        println!(
            "Mapping / to FAT filesystem {:?}",
            filesystem.volume_label()
        );
        FILESYSTEM_MAPPING
            .lock()
            .mappings
            .push((String::from("/"), filesystem));

        Ok(())
    } else {
        Err(FileSystemError::PartitionTableNotFound)
    }
}

#[allow(dead_code)]
pub fn ls_dir(path: &str) -> Result<Vec<DirectoryEntry>, FileSystemError> {
    let mut path = Cow::from(path);

    if !path.ends_with('/') {
        path += "/";
    }

    let mappings = FILESYSTEM_MAPPING.lock();

    let (prefix, filesystem) = mappings
        .mappings
        .iter()
        // look from the back for best match
        .rev()
        .find(|(fs_path, _)| path.starts_with(fs_path))
        .ok_or(FileSystemError::FileNotFound)?;

    let prefix_len = if prefix.ends_with('/') {
        // keep the last /, so we can open the directory
        prefix.len() - 1
    } else {
        prefix.len()
    };
    let path = &path[prefix_len..];
    println!("Found filesystem {:?} for path {:?}", filesystem, path);

    Ok(filesystem.open_dir(path)?.collect())
}
