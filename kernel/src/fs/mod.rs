use core::mem;

use alloc::{borrow::Cow, string::String, sync::Arc, vec, vec::Vec};

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

type Filesystem = Arc<Mutex<fat::FatFilesystem>>;

struct FileSystemMapping {
    mappings: Vec<(String, Filesystem)>,
}

impl FileSystemMapping {
    fn get_mapping<'p>(&mut self, path: &'p str) -> Result<(&'p str, Filesystem), FileSystemError> {
        let (prefix, filesystem) = self
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

        Ok((path, filesystem.clone()))
    }
}

#[derive(Debug)]
pub enum FileSystemError {
    PartitionTableNotFound,
    DeviceNotFound,
    DiskReadError {
        sector: u64,
        error: ide::IdeError,
    },
    FatError(fat::FatError),
    FileNotFound,
    InvalidPath,
    IsNotDirectory,
    IsDirectory,
    InvalidOffset,
    /// Unlike InvalidInput, this typically means that the operation parameters were valid,
    ///  however the error was caused by malformed input data.
    ///
    /// For example, a function that reads a file into a string will error with InvalidData
    /// if the file’s contents are not valid `UTF-8`.
    InvalidData,
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
            "Mapping / to FAT filesystem {:?} ({:?})",
            filesystem.volume_label(),
            filesystem.fat_type()
        );
        FILESYSTEM_MAPPING
            .lock()
            .mappings
            .push((String::from("/"), Arc::new(Mutex::new(filesystem))));

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

    let (new_path, filesystem) = FILESYSTEM_MAPPING.lock().get_mapping(&path)?;
    let filesystem = filesystem.lock();
    Ok(filesystem.open_dir(new_path)?.collect())
}

#[allow(dead_code)]
pub(crate) fn open(path: &str) -> Result<File, FileSystemError> {
    let last_slash = path.rfind('/');

    let (parent_dir, basename) = match last_slash {
        Some(index) => path.split_at(index + 1),
        None => return Err(FileSystemError::InvalidPath),
    };

    let (parent_dir, filesystem) = FILESYSTEM_MAPPING.lock().get_mapping(parent_dir)?;
    let filesystem_guard = filesystem.lock();

    for entry in filesystem_guard.open_dir(parent_dir)? {
        if entry.name() == basename {
            drop(filesystem_guard);
            return Ok(File {
                filesystem,
                path: String::from(path),
                inode: entry.inode().clone(),
                position: 0,
            });
        }
    }

    Err(FileSystemError::FileNotFound)
}

#[allow(dead_code)]
pub struct File {
    filesystem: Filesystem,
    path: String,
    inode: fat::INode,
    position: u64,
}

#[allow(dead_code)]
impl File {
    pub fn read(&mut self, buf: &mut [u8]) -> Result<u64, FileSystemError> {
        let filesystem_guard = self.filesystem.lock();
        let read = filesystem_guard.read_file(&self.inode, self.position as u32, buf)?;
        self.position += read;
        Ok(read)
    }

    #[allow(dead_code)]
    pub fn seek(&mut self, position: u64) -> Result<(), FileSystemError> {
        if position > self.inode.size() as u64 {
            return Err(FileSystemError::InvalidOffset);
        }
        self.position = position;
        Ok(())
    }

    pub fn filesize(&self) -> u64 {
        self.inode.size() as u64
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn read_to_end(&mut self) -> Result<Vec<u8>, FileSystemError> {
        let mut buf = vec![0; self.inode.size() as usize];
        let mut position = 0;
        loop {
            let read = self.read(&mut buf[position..])?;
            if read == 0 {
                break;
            }
            position += read as usize;
        }
        Ok(buf)
    }

    pub fn read_to_string(&mut self) -> Result<String, FileSystemError> {
        let buf = self.read_to_end()?;
        String::from_utf8(buf).map_err(|_| FileSystemError::InvalidData)
    }
}
