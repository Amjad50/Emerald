use core::{mem, ops};

use alloc::{borrow::Cow, string::String, sync::Arc, vec, vec::Vec};
use kernel_user_link::file::BlockingMode;

use crate::{
    devices::{
        ide::{self, IdeDeviceIndex, IdeDeviceType},
        Device, DEVICES_FILESYSTEM_CLUSTER_MAGIC,
    },
    memory_management::memory_layout::align_up,
    sync::{once::OnceLock, spin::mutex::Mutex},
};

use self::mbr::MbrRaw;

mod fat;
mod mbr;

static FILESYSTEM_MAPPING: Mutex<FileSystemMapping> = Mutex::new(FileSystemMapping {
    mappings: Vec::new(),
});

static EMPTY_FILESYSTEM: OnceLock<Arc<EmptyFileSystem>> = OnceLock::new();

pub fn empty_filesystem() -> Arc<EmptyFileSystem> {
    EMPTY_FILESYSTEM
        .get_or_init(|| Arc::new(EmptyFileSystem))
        .clone()
}

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
    start_cluster: u64,
    size: u64,
    device: Option<Arc<dyn Device>>,
}

impl INode {
    pub fn new_file(
        name: String,
        attributes: FileAttributes,
        start_cluster: u64,
        size: u64,
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

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    #[allow(dead_code)]
    pub fn attributes(&self) -> FileAttributes {
        self.attributes
    }

    pub fn start_cluster(&self) -> u64 {
        self.start_cluster
    }

    pub fn device(&self) -> Option<&Arc<dyn Device>> {
        self.device.as_ref()
    }
}

impl Drop for INode {
    fn drop(&mut self) {
        if let Some(device) = self.device.take() {
            device.close().expect("Failed to close device");
        }
    }
}

pub trait FileSystem: Send + Sync {
    fn open_root(&self) -> Result<INode, FileSystemError>;
    // TODO: don't use Vector please, use an iterator somehow
    fn open_dir(&self, path: &str) -> Result<Vec<INode>, FileSystemError>;
    fn read_dir(&self, inode: &INode) -> Result<Vec<INode>, FileSystemError>;
    fn read_file(
        &self,
        inode: &INode,
        position: u64,
        buf: &mut [u8],
    ) -> Result<u64, FileSystemError> {
        if inode.is_dir() {
            return Err(FileSystemError::IsDirectory);
        }
        if let Some(device) = inode.device() {
            assert!(inode.start_cluster == DEVICES_FILESYSTEM_CLUSTER_MAGIC);
            device.read(position, buf)
        } else {
            Err(FileSystemError::ReadNotSupported)
        }
    }

    fn write_file(&self, inode: &INode, position: u64, buf: &[u8]) -> Result<u64, FileSystemError> {
        if inode.is_dir() {
            return Err(FileSystemError::IsDirectory);
        }
        if let Some(device) = inode.device() {
            assert!(inode.start_cluster == DEVICES_FILESYSTEM_CLUSTER_MAGIC);
            device.write(position, buf)
        } else {
            Err(FileSystemError::WriteNotSupported)
        }
    }
}

pub struct EmptyFileSystem;

impl FileSystem for EmptyFileSystem {
    fn open_root(&self) -> Result<INode, FileSystemError> {
        Err(FileSystemError::FileNotFound)
    }

    fn open_dir(&self, _path: &str) -> Result<Vec<INode>, FileSystemError> {
        Err(FileSystemError::FileNotFound)
    }

    fn read_dir(&self, _inode: &INode) -> Result<Vec<INode>, FileSystemError> {
        Err(FileSystemError::FileNotFound)
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
    EndOfFile,
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

/// Open the inode of a path, this include directories and files.
pub(crate) fn open_inode(path: &str) -> Result<(Arc<dyn FileSystem>, INode), FileSystemError> {
    let last_slash = path.rfind('/');

    let (parent_dir, mut basename) = match last_slash {
        Some(index) => path.split_at(index + 1),
        None => return Err(FileSystemError::InvalidPath),
    };

    let (mut parent_dir, filesystem) = FILESYSTEM_MAPPING.lock().get_mapping(parent_dir)?;

    let mut opening_dir = false;
    // if no basename, this is a directory (either root or inner directory)
    if basename == "" {
        if parent_dir == "/" || parent_dir == "" {
            // we are opening the root of this filesystem
            return filesystem.open_root().map(|inode| (filesystem, inode));
        } else {
            // we are opening a folder in this filesystem
            // split the `parent_dir` again
            // remove the last slash first so we can find the one after it
            parent_dir = &parent_dir[..parent_dir.len() - 1];
            let last_slash = parent_dir.rfind('/');
            match last_slash {
                Some(index) => {
                    basename = &parent_dir[index + 1..];
                    parent_dir = &parent_dir[..index + 1];
                }
                None => return Err(FileSystemError::InvalidPath),
            }

            // we are opening a folder (i.e. the path ends with /)
            opening_dir = true;
        }
    }

    for entry in filesystem.open_dir(parent_dir)? {
        if entry.name() == basename {
            // if this is a file, return error if we requst a directory (using "/")
            if !entry.is_dir() && opening_dir {
                return Err(FileSystemError::IsNotDirectory);
            }
            return Ok((filesystem, entry));
        }
    }

    Err(FileSystemError::FileNotFound)
}

pub struct File {
    filesystem: Arc<dyn FileSystem>,
    path: String,
    inode: INode,
    position: u64,
    blocking_mode: BlockingMode,
}

impl File {
    pub fn open(path: &str) -> Result<Self, FileSystemError> {
        Self::open_blocking(path, BlockingMode::None)
    }
    pub fn open_blocking(path: &str, blocking_mode: BlockingMode) -> Result<Self, FileSystemError> {
        let (filesystem, inode) = open_inode(path)?;

        if inode.is_dir() {
            return Err(FileSystemError::IsDirectory);
        }

        Ok(Self {
            filesystem,
            path: String::from(path),
            inode,
            position: 0,
            blocking_mode,
        })
    }

    pub fn from_inode(
        inode: INode,
        filesystem: Arc<dyn FileSystem>,
        position: u64,
        blocking_mode: BlockingMode,
    ) -> Self {
        Self {
            filesystem,
            // TODO: this is just the filename I think, not the full path
            path: String::from(inode.name()),
            inode,
            position,
            blocking_mode,
        }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<u64, FileSystemError> {
        let count = match self.blocking_mode {
            BlockingMode::None => self.filesystem.read_file(&self.inode, self.position, buf)?,
            BlockingMode::Line => {
                // read until \n or \0
                let mut i = 0;
                loop {
                    let mut char_buf = 0;
                    let read_byte = self.filesystem.read_file(
                        &self.inode,
                        self.position,
                        core::slice::from_mut(&mut char_buf),
                    );

                    let read_byte = match read_byte {
                        Ok(read_byte) => read_byte,
                        Err(FileSystemError::EndOfFile) => {
                            // if we reached the end of the file, we return i
                            return Ok(i as u64);
                        }
                        Err(e) => return Err(e),
                    };

                    // only put if we can, otherwise, eat the byte and continue
                    if read_byte == 1 {
                        if i < buf.len() {
                            buf[i] = char_buf;
                            i += 1;
                        }
                        if char_buf == b'\n' || char_buf == b'\0' {
                            break;
                        }
                    } else {
                        // TODO: add IO waiting
                        for _ in 0..100 {
                            core::hint::spin_loop();
                        }
                    }
                }
                i as u64
            }
            BlockingMode::Block(size) => {
                // TODO: support block size > 1
                assert!(size == 1, "Only block size 1 is supported");

                // try to read until we have something
                loop {
                    let read_byte = self.filesystem.read_file(&self.inode, self.position, buf);

                    let read_byte = match read_byte {
                        Ok(read_byte) => read_byte,
                        Err(FileSystemError::EndOfFile) => {
                            // if we reached the end of the file, we return 0
                            break 0;
                        }
                        Err(e) => return Err(e),
                    };

                    // only if the result is not 0, we can return
                    if read_byte != 0 {
                        break read_byte;
                    }
                    // otherwise we wait
                    // TODO: add IO waiting
                    for _ in 0..100 {
                        core::hint::spin_loop();
                    }
                }
            }
        };

        self.position += count;
        Ok(count)
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<u64, FileSystemError> {
        let written = self
            .filesystem
            .write_file(&self.inode, self.position, buf)?;
        self.position += written;
        Ok(written)
    }

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

    pub fn set_blocking(&mut self, blocking_mode: BlockingMode) {
        self.blocking_mode = blocking_mode;
    }

    /// This is a move verbose method than `Clone::clone`, as I want it to be
    /// more explicit to the user that this is not a normal `clone` operation.
    pub fn clone_inherit(&self) -> Self {
        let s = Self {
            filesystem: self.filesystem.clone(),
            path: self.path.clone(),
            inode: self.inode.clone(),
            position: 0,
            blocking_mode: self.blocking_mode,
        };

        // inform the device of a clone operation
        s.inode.device.as_ref().map(|device| {
            device
                .clone_device()
                // TODO: maybe use error handling instead
                .expect("Failed to clone device for file")
        });

        s
    }
}
