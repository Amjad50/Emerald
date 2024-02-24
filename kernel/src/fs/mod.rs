use core::ops;

use alloc::{boxed::Box, string::String, sync::Arc, vec, vec::Vec};
use kernel_user_link::file::{BlockingMode, DirEntry, FileStat, FileType};

use crate::{
    devices::{
        ide::{self, IdeDeviceIndex, IdeDeviceType},
        Device, DEVICES_FILESYSTEM_CLUSTER_MAGIC,
    },
    sync::{once::OnceLock, spin::mutex::Mutex},
};

use self::{
    mbr::Mbr,
    path::{Component, Path},
};

mod fat;
mod mbr;
pub mod path;

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

    pub fn as_file_stat(&self) -> FileStat {
        FileStat {
            size: self.size(),
            file_type: if self.is_dir() {
                FileType::Directory
            } else {
                FileType::File
            },
        }
    }

    pub fn try_open_device(&mut self) -> Result<(), FileSystemError> {
        if let Some(device) = self.device.take() {
            self.device = Some(device.try_create().unwrap_or(Ok(device))?);
        }

        Ok(())
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
    fn open_dir(&self, path: &Path) -> Result<Vec<INode>, FileSystemError>;
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

    fn open_dir(&self, _path: &Path) -> Result<Vec<INode>, FileSystemError> {
        Err(FileSystemError::FileNotFound)
    }

    fn read_dir(&self, _inode: &INode) -> Result<Vec<INode>, FileSystemError> {
        Err(FileSystemError::FileNotFound)
    }
}

struct FileSystemMapping {
    mappings: Vec<(Box<Path>, Arc<dyn FileSystem>)>,
}

impl FileSystemMapping {
    /// Retrieves the file system mapping for a given path.
    ///
    /// This function iterates over the stored mappings in reverse order and returns the first
    /// file system and the stripped path for which the given path has a prefix that matches
    /// the file system's path.
    ///
    /// # Parameters
    ///
    /// * `path`: A reference to the path for which to find the file system mapping.
    ///
    /// # Returns
    ///
    /// * A tuple containing the stripped path and an `Arc` to the file system, if a matching mapping is found.
    /// * `FileSystemError::FileNotFound` if no matching mapping is found.
    ///
    /// # Examples
    ///
    /// ```
    /// // Assume fs_map has been populated with some mappings
    /// let mut fs_map = FileSystemMap::new();
    ///
    /// let path = Path::new("/some/path");
    /// match fs_map.get_mapping(&path) {
    ///     Ok((stripped_path, fs)) => {
    ///         println!("Found file system: {:?}", fs);
    ///         println!("Stripped path: {:?}", stripped_path);
    ///     }
    ///     Err(FileSystemError::FileNotFound) => {
    ///         println!("No file system found for path: {:?}", path);
    ///     }
    ///     _ => {}
    /// }
    /// ```
    fn get_mapping<'p>(
        &mut self,
        path: &'p Path,
    ) -> Result<(&'p Path, Arc<dyn FileSystem>), FileSystemError> {
        let (stripped_path, filesystem) = self
            .mappings
            .iter()
            // look from the back for best match
            .rev()
            .find_map(|(fs_path, fs)| Some((path.strip_prefix(fs_path).ok()?, fs.clone())))
            .ok_or(FileSystemError::FileNotFound)?;

        Ok((stripped_path, filesystem.clone()))
    }

    fn mount<P: AsRef<Path>>(&mut self, arg: P, filesystem: Arc<dyn FileSystem>) {
        // TODO: replace with error
        assert!(
            !self
                .mappings
                .iter()
                .any(|(fs_path, _)| fs_path.as_ref() == arg.as_ref()),
            "Mounting {:?} twice",
            arg.as_ref().display()
        );

        self.mappings.push((arg.as_ref().into(), filesystem));

        // must be kept sorted by length, so we can find the best/correct mapping,
        // as mappings can be inside each other in structure
        self.mappings
            .sort_unstable_by(|(a, _), (b, _)| a.as_str().len().cmp(&b.as_str().len()));
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
    MustBeAbsolute,
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
    OperationNotSupported,
    EndOfFile,
    BufferNotLargeEnough(usize),
}

fn get_mapping(path: &Path) -> Result<(&Path, Arc<dyn FileSystem>), FileSystemError> {
    FILESYSTEM_MAPPING.lock().get_mapping(path)
}

pub fn mount(arg: &str, filesystem: Arc<dyn FileSystem>) {
    FILESYSTEM_MAPPING.lock().mount(arg, filesystem);
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

    let mbr = Mbr::try_create_from_disk(&device)?;

    // load the first partition for now
    let first_partition = &mbr.partition_table[0];
    let filesystem = fat::load_fat_filesystem(
        device,
        first_partition.start_lba,
        first_partition.size_in_sectors,
    )?;
    println!(
        "Mapping / to FAT filesystem {:?} ({:?}), parition_type: 0x{:02X}",
        filesystem.volume_label(),
        filesystem.fat_type(),
        first_partition.partition_type
    );
    mount("/", Arc::new(Mutex::new(filesystem)));

    Ok(())
}

/// Open the inode of a path, this include directories and files.
///
/// This function must be called with an absolute path. Otherwise it will return [`FileSystemError::MustBeAbsolute`].
pub(crate) fn open_inode<P: AsRef<Path>>(
    path: P,
) -> Result<(Arc<dyn FileSystem>, INode), FileSystemError> {
    if !path.as_ref().is_absolute() {
        // this is an internal kernel only result, this function must be called with an absolute path
        return Err(FileSystemError::MustBeAbsolute);
    }
    let (remaining, filesystem) = get_mapping(path.as_ref())?;

    let opening_dir = path.as_ref().has_last_separator();
    let mut comp = remaining.components();
    let filename;
    let parent;
    // remove leading `.` and `..` from the path
    // TODO: this actually causes a bug where doing `/devices/../../..` will boil to `/devices` and not `/` as it should\
    //       Fix it
    loop {
        match comp.next_back() {
            Some(Component::Normal(segment)) => {
                filename = segment;
                parent = comp.as_path();
                break;
            }
            Some(Component::RootDir) | None => {
                // we reached root, return it directly
                return filesystem.open_root().map(|inode| (filesystem, inode));
            }
            Some(Component::CurDir) => {
                // ignore
            }
            Some(Component::ParentDir) => {
                // ignore
                // drop next component
                // FIXME: there is a bug here, `/welcome/../..` will be treated as `/welcome`
                //       as the first `..` will drop the second `..`
                //       implement better handling of this and make it global, probably in [`Path`]
                comp.next_back();
            }
        }
    }

    for mut entry in filesystem.open_dir(parent)? {
        if entry.name() == filename {
            // if this is a file, return error if we requst a directory (using "/")
            if !entry.is_dir() && opening_dir {
                return Err(FileSystemError::IsNotDirectory);
            }
            // if this is a device, open it
            entry.try_open_device()?;
            return Ok((filesystem, entry));
        }
    }

    Err(FileSystemError::FileNotFound)
}

/// A handle to a file, it has the inode which controls the properties of the node in the filesystem
pub struct File {
    filesystem: Arc<dyn FileSystem>,
    path: Box<Path>,
    inode: INode,
    position: u64,
    is_terminal: bool,
    blocking_mode: BlockingMode,
}

/// A handle to a directory, it has the inode which controls the properties of the node in the filesystem
#[allow(dead_code)]
pub struct Directory {
    inode: INode,
    path: Box<Path>,
    position: u64,
    // TODO: replace by iter so that new files can be added in the middle
    dir_entries: Vec<INode>,
    // now we don't need the filesystem, but if we implement dynamic directory read, we will
    // filesystem: Arc<dyn FileSystem>,
}

/// A node in the filesystem, can be a file or a directory
#[allow(dead_code)]
#[repr(u8)]
pub enum FilesystemNode {
    File(File),
    Directory(Directory),
}

#[allow(dead_code)]
impl File {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, FileSystemError> {
        Self::open_blocking(path, BlockingMode::None)
    }

    pub fn open_blocking<P: AsRef<Path>>(
        path: P,
        blocking_mode: BlockingMode,
    ) -> Result<Self, FileSystemError> {
        let (filesystem, inode) = open_inode(path.as_ref())?;

        Self::from_inode(inode, path, filesystem, 0, blocking_mode)
    }

    pub fn from_inode<P: AsRef<Path>>(
        inode: INode,
        path: P,
        filesystem: Arc<dyn FileSystem>,
        position: u64,
        blocking_mode: BlockingMode,
    ) -> Result<Self, FileSystemError> {
        if inode.is_dir() {
            return Err(FileSystemError::IsDirectory);
        }

        Ok(Self {
            filesystem,
            path: path.as_ref().into(),
            inode,
            position,
            is_terminal: false,
            blocking_mode,
        })
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<u64, FileSystemError> {
        assert!(!self.inode.is_dir());

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
        assert!(!self.inode.is_dir());

        let written = self
            .filesystem
            .write_file(&self.inode, self.position, buf)?;
        self.position += written;
        Ok(written)
    }

    pub fn seek(&mut self, position: u64) -> Result<(), FileSystemError> {
        assert!(!self.inode.is_dir());

        if position > self.inode.size() {
            return Err(FileSystemError::InvalidOffset);
        }
        self.position = position;
        Ok(())
    }

    pub fn filesize(&self) -> u64 {
        self.inode.size()
    }

    pub fn path(&self) -> &Path {
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

    pub fn is_blocking(&self) -> bool {
        self.blocking_mode != BlockingMode::None
    }

    pub fn blocking_mode(&self) -> BlockingMode {
        self.blocking_mode
    }

    pub fn set_blocking(&mut self, blocking_mode: BlockingMode) {
        self.blocking_mode = blocking_mode;
    }

    pub fn is_terminal(&self) -> bool {
        self.is_terminal
    }

    pub fn set_terminal(&mut self, is_terminal: bool) {
        self.is_terminal = is_terminal;
    }

    /// This is a move verbose method than `Clone::clone`, as I want it to be
    /// more explicit to the user that this is not a normal `clone` operation.
    pub fn clone_inherit(&self) -> Self {
        let s = Self {
            filesystem: self.filesystem.clone(),
            path: self.path.clone(),
            inode: self.inode.clone(),
            position: 0,
            is_terminal: self.is_terminal,
            blocking_mode: self.blocking_mode,
        };

        // inform the device of a clone operation
        if let Some(device) = s.inode.device.as_ref() {
            device
                .clone_device()
                // TODO: maybe use error handling instead
                .expect("Failed to clone device for file")
        }

        s
    }
}

#[allow(dead_code)]
impl Directory {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, FileSystemError> {
        let (filesystem, inode) = open_inode(path.as_ref())?;

        Self::from_inode(inode, path, filesystem, 0)
    }

    pub fn from_inode<P: AsRef<Path>>(
        inode: INode,
        path: P,
        filesystem: Arc<dyn FileSystem>,
        position: u64,
    ) -> Result<Self, FileSystemError> {
        if !inode.is_dir() {
            return Err(FileSystemError::IsNotDirectory);
        }

        // TODO: read dynamically, not at creation, as we sometimes don't use this (e.g. current_dir in `Process`)
        let dir_entries = filesystem.read_dir(&inode)?;

        Ok(Self {
            path: path.as_ref().into(),
            inode,
            position,
            dir_entries,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&mut self, entries: &mut [DirEntry]) -> Result<usize, FileSystemError> {
        assert!(self.inode.is_dir());

        let mut i = 0;
        while i < entries.len() {
            if self.position >= self.dir_entries.len() as u64 {
                break;
            }

            let entry = &self.dir_entries[self.position as usize];
            entries[i] = DirEntry {
                stat: entry.as_file_stat(),
                name: entry.name().into(),
            };
            i += 1;
            self.position += 1;
        }

        Ok(i)
    }
}

impl Clone for Directory {
    fn clone(&self) -> Self {
        Self {
            inode: self.inode.clone(),
            path: self.path.clone(),
            position: 0,
            dir_entries: self.dir_entries.clone(),
        }
    }
}

#[allow(dead_code)]
impl FilesystemNode {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, FileSystemError> {
        let (filesystem, inode) = open_inode(path.as_ref())?;

        if inode.is_dir() {
            Ok(Self::Directory(Directory::from_inode(
                inode, path, filesystem, 0,
            )?))
        } else {
            Ok(Self::File(File::from_inode(
                inode,
                path,
                filesystem,
                0,
                BlockingMode::None,
            )?))
        }
    }

    pub fn inode(&self) -> &INode {
        match self {
            Self::File(file) => &file.inode,
            Self::Directory(dir) => &dir.inode,
        }
    }

    pub fn as_file(&self) -> Result<&File, FileSystemError> {
        match self {
            Self::File(file) => Ok(file),
            Self::Directory(_) => Err(FileSystemError::IsDirectory),
        }
    }

    pub fn as_file_mut(&mut self) -> Result<&mut File, FileSystemError> {
        match self {
            Self::File(file) => Ok(file),
            Self::Directory(_) => Err(FileSystemError::IsDirectory),
        }
    }

    pub fn as_dir_mut(&mut self) -> Result<&mut Directory, FileSystemError> {
        match self {
            Self::File(_) => Err(FileSystemError::IsNotDirectory),
            Self::Directory(dir) => Ok(dir),
        }
    }
}

impl From<File> for FilesystemNode {
    fn from(file: File) -> Self {
        Self::File(file)
    }
}

impl From<Directory> for FilesystemNode {
    fn from(dir: Directory) -> Self {
        Self::Directory(dir)
    }
}
