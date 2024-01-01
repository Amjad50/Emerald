use core::{mem, ops};

use alloc::{borrow::Cow, string::String, sync::Arc, vec, vec::Vec};
use kernel_user_link::file::BlockingMode;

use crate::{
    devices::{
        ide::{self, IdeDeviceIndex, IdeDeviceType},
        Device, DEVICES_FILESYSTEM_CLUSTER_MAGIC,
    },
    memory_management::memory_layout::align_up,
    sync::spin::mutex::Mutex,
};

use self::mbr::MbrRaw;

mod fat;
mod mbr;

static FILESYSTEM_MAPPING: Mutex<FileSystemMapping> = Mutex::new(FileSystemMapping {
    mappings: Vec::new(),
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileAttributes {
    pub read_only: bool,
    pub hidden: bool,
    pub system: bool,
    pub volume_label: bool,
    pub directory: bool,
    pub archive: bool,
}

#[allow(dead_code)]
impl FileAttributes {
    pub const EMPTY: FileAttributes = FileAttributes {
        read_only: false,
        hidden: false,
        system: false,
        volume_label: false,
        directory: false,
        archive: false,
    };
    pub const READ_ONLY: FileAttributes = FileAttributes {
        read_only: true,
        hidden: false,
        system: false,
        volume_label: false,
        directory: false,
        archive: false,
    };
    pub const HIDDEN: FileAttributes = FileAttributes {
        read_only: false,
        hidden: true,
        system: false,
        volume_label: false,
        directory: false,
        archive: false,
    };
    pub const SYSTEM: FileAttributes = FileAttributes {
        read_only: false,
        hidden: false,
        system: true,
        volume_label: false,
        directory: false,
        archive: false,
    };
    pub const VOLUME_LABEL: FileAttributes = FileAttributes {
        read_only: false,
        hidden: false,
        system: false,
        volume_label: true,
        directory: false,
        archive: false,
    };
    pub const DIRECTORY: FileAttributes = FileAttributes {
        read_only: false,
        hidden: false,
        system: false,
        volume_label: false,
        directory: true,
        archive: false,
    };
    pub const ARCHIVE: FileAttributes = FileAttributes {
        read_only: false,
        hidden: false,
        system: false,
        volume_label: false,
        directory: false,
        archive: true,
    };
}

impl ops::BitOr for FileAttributes {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            read_only: self.read_only | rhs.read_only,
            hidden: self.hidden | rhs.hidden,
            system: self.system | rhs.system,
            volume_label: self.volume_label | rhs.volume_label,
            directory: self.directory | rhs.directory,
            archive: self.archive | rhs.archive,
        }
    }
}

#[derive(Debug, Clone)]
pub struct INode {
    name: String,
    attributes: FileAttributes,
    start_cluster: u32,
    size: u32,
    device: Option<Arc<dyn Device>>,
}

impl INode {
    pub fn new_file(
        name: String,
        attributes: FileAttributes,
        start_cluster: u32,
        size: u32,
    ) -> Self {
        Self {
            name,
            attributes,
            start_cluster,
            size,
            device: None,
        }
    }

    pub fn new_device(
        name: String,
        attributes: FileAttributes,
        device: Option<Arc<dyn Device>>,
    ) -> Self {
        Self {
            name,
            attributes,
            start_cluster: DEVICES_FILESYSTEM_CLUSTER_MAGIC,
            size: 0,
            device,
        }
    }

    pub fn is_dir(&self) -> bool {
        self.attributes.directory
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    #[allow(dead_code)]
    pub fn attributes(&self) -> FileAttributes {
        self.attributes
    }

    pub fn start_cluster(&self) -> u32 {
        self.start_cluster
    }

    pub fn device(&self) -> Option<&Arc<dyn Device>> {
        self.device.as_ref()
    }
}

pub trait FileSystem: Send + Sync {
    // TODO: don't use Vector please, use an iterator somehow
    fn open_dir(&self, path: &str) -> Result<Vec<INode>, FileSystemError>;
    fn read_dir(&self, inode: &INode) -> Result<Vec<INode>, FileSystemError>;
    fn read_file(
        &self,
        inode: &INode,
        position: u32,
        buf: &mut [u8],
    ) -> Result<u64, FileSystemError> {
        if let Some(device) = inode.device() {
            assert!(inode.start_cluster == DEVICES_FILESYSTEM_CLUSTER_MAGIC);
            device.read(position, buf)
        } else {
            Err(FileSystemError::ReadNotSupported)
        }
    }

    fn write_file(&self, inode: &INode, position: u32, buf: &[u8]) -> Result<u64, FileSystemError> {
        if let Some(device) = inode.device() {
            assert!(inode.start_cluster == DEVICES_FILESYSTEM_CLUSTER_MAGIC);
            device.write(position, buf)
        } else {
            Err(FileSystemError::WriteNotSupported)
        }
    }
}

struct FileSystemMapping {
    mappings: Vec<(String, Arc<dyn FileSystem>)>,
}

impl FileSystemMapping {
    fn get_mapping<'p>(
        &mut self,
        path: &'p str,
    ) -> Result<(&'p str, Arc<dyn FileSystem>), FileSystemError> {
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
    ReadNotSupported,
    WriteNotSupported,
}

pub fn mount(arg: &str, filesystem: Arc<dyn FileSystem>) {
    let mut mappings = FILESYSTEM_MAPPING.lock();

    assert!(
        !mappings.mappings.iter().any(|(fs_path, _)| fs_path == arg),
        "Mounting {} twice",
        arg
    );

    let base = String::from(arg);
    let mapping = if arg.ends_with('/') { base } else { base + "/" };

    mappings.mappings.push((mapping, filesystem));
    // must be kept sorted by length, so we can find the best/correct mapping faster
    mappings
        .mappings
        .sort_unstable_by(|(a, _), (b, _)| a.len().cmp(&b.len()));
}

/// Loads the hard disk specified in the argument
/// it will load the first partition (MBR) if any, otherwise it will treat the whole disk
/// as one partition
///
/// Creates a new filesystem mapping for `/` and the filesystem found
pub fn create_disk_mapping(hard_disk_index: usize) -> Result<(), FileSystemError> {
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
        mount("/", Arc::new(Mutex::new(filesystem)));

        Ok(())
    } else {
        Err(FileSystemError::PartitionTableNotFound)
    }
}

#[allow(dead_code)]
pub fn ls_dir(path: &str) -> Result<Vec<INode>, FileSystemError> {
    let mut path = Cow::from(path);

    if !path.ends_with('/') {
        path += "/";
    }

    let (new_path, filesystem) = FILESYSTEM_MAPPING.lock().get_mapping(&path)?;
    filesystem.open_dir(new_path)
}

#[allow(dead_code)]
pub(crate) fn open(path: &str) -> Result<File, FileSystemError> {
    open_blocking(path, BlockingMode::None)
}

#[allow(dead_code)]
pub(crate) fn open_blocking(
    path: &str,
    blocking_mode: BlockingMode,
) -> Result<File, FileSystemError> {
    let last_slash = path.rfind('/');

    let (parent_dir, basename) = match last_slash {
        Some(index) => path.split_at(index + 1),
        None => return Err(FileSystemError::InvalidPath),
    };

    let (parent_dir, filesystem) = FILESYSTEM_MAPPING.lock().get_mapping(parent_dir)?;

    let filesystem_clone = filesystem.clone();
    for entry in filesystem.open_dir(parent_dir)? {
        if entry.name() == basename {
            return Ok(File {
                filesystem: filesystem_clone,
                path: String::from(path),
                inode: entry,
                position: 0,
                blocking_mode,
            });
        }
    }

    Err(FileSystemError::FileNotFound)
}

pub(crate) fn inode_to_file(
    inode: INode,
    filesystem: Arc<dyn FileSystem>,
    position: u64,
    blocking_mode: BlockingMode,
) -> File {
    File {
        filesystem,
        // TODO: this is just the filename I think, not the full path
        path: String::from(inode.name()),
        inode,
        position,
        blocking_mode,
    }
}

#[derive(Clone)]
pub struct File {
    filesystem: Arc<dyn FileSystem>,
    path: String,
    inode: INode,
    position: u64,
    blocking_mode: BlockingMode,
}

#[allow(dead_code)]
impl File {
    pub fn read(&mut self, buf: &mut [u8]) -> Result<u64, FileSystemError> {
        let count = match self.blocking_mode {
            BlockingMode::None => {
                self.filesystem
                    .read_file(&self.inode, self.position as u32, buf)?
            }
            BlockingMode::Line => {
                // read until \n or \0
                let mut i = 0;
                loop {
                    let mut char_buf = 0;
                    let read_byte = self.filesystem.read_file(
                        &self.inode,
                        self.position as u32,
                        core::slice::from_mut(&mut char_buf),
                    )?;

                    // only put if we can, otherwise, eat the byte and continue
                    if read_byte == 1 {
                        if i < buf.len() {
                            buf[i] = char_buf;
                            i += 1;
                        }
                        if char_buf == b'\n' || char_buf == b'\0' {
                            break;
                        }
                    }
                }
                i as u64
            }
            BlockingMode::Block(_size) => {
                todo!("BlockingMode::Block")
            }
        };

        self.position += count;
        Ok(count)
    }

    pub fn write(&mut self, _buf: &[u8]) -> Result<u64, FileSystemError> {
        let written = self
            .filesystem
            .write_file(&self.inode, self.position as u32, _buf)?;
        self.position += written;
        Ok(written)
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

    pub fn is_blocking(&self) -> bool {
        self.blocking_mode != BlockingMode::None
    }
}
