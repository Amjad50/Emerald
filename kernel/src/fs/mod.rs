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

/// This is not used at all, just an indicator in [`Directory::fetch_entries`]
pub(crate) const ANOTHER_FILESYSTEM_MAPPING_INODE_MAGIC: u64 = 0xf11356573e;
pub(crate) const NO_PARENT_DIR_SECTOR: u64 = 0xFFFF_FFFF_FFFF_FFFF;

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
pub struct FileAttributes(pub u8);

#[allow(dead_code)]
impl FileAttributes {
    pub const EMPTY: FileAttributes = FileAttributes(0);
    pub const READ_ONLY: FileAttributes = FileAttributes(0b0000_0001);
    pub const HIDDEN: FileAttributes = FileAttributes(0b0000_0010);
    pub const SYSTEM: FileAttributes = FileAttributes(0b0000_0100);
    pub const VOLUME_LABEL: FileAttributes = FileAttributes(0b0000_1000);
    pub const DIRECTORY: FileAttributes = FileAttributes(0b0001_0000);
    pub const ARCHIVE: FileAttributes = FileAttributes(0b0010_0000);

    pub fn read_only(self) -> bool {
        self.0 & Self::READ_ONLY.0 != 0
    }

    pub fn hidden(self) -> bool {
        self.0 & Self::HIDDEN.0 != 0
    }

    pub fn system(self) -> bool {
        self.0 & Self::SYSTEM.0 != 0
    }

    pub fn volume_label(self) -> bool {
        self.0 & Self::VOLUME_LABEL.0 != 0
    }

    pub fn directory(self) -> bool {
        self.0 & Self::DIRECTORY.0 != 0
    }

    pub fn archive(self) -> bool {
        self.0 & Self::ARCHIVE.0 != 0
    }
}

impl ops::BitOr for FileAttributes {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        FileAttributes(self.0 | rhs.0)
    }
}

impl ops::BitOrAssign for FileAttributes {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl ops::BitAnd for FileAttributes {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        FileAttributes(self.0 & rhs.0)
    }
}

#[derive(Debug, Clone)]
pub struct BaseNode {
    name: String,
    attributes: FileAttributes,
    start_cluster: u64,
    parent_dir_sector: u64,
    /// The position of this file in the parent directory
    /// the size of the sector shouldn't exceed 16 bits
    /// this is element wise and not byte wise
    parent_dir_index: u16,
}

impl BaseNode {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn start_cluster(&self) -> u64 {
        self.start_cluster
    }

    #[allow(dead_code)]
    pub fn attributes(&self) -> FileAttributes {
        self.attributes
    }

    #[allow(dead_code)]
    pub fn parent_dir_sector(&self) -> u64 {
        self.parent_dir_sector
    }

    #[allow(dead_code)]
    pub fn parent_dir_index(&self) -> u16 {
        self.parent_dir_index
    }
}

#[derive(Debug, Clone)]
pub struct FileNode {
    base: BaseNode,
    size: u64,
    device: Option<Arc<dyn Device>>,
}

impl FileNode {
    pub fn new_file(
        name: String,
        attributes: FileAttributes,
        start_cluster: u64,
        size: u64,
        parent_dir_sector: u64,
        parent_dir_index: u16,
    ) -> Self {
        assert!(!attributes.directory());
        Self {
            base: BaseNode {
                name,
                attributes,
                start_cluster,
                parent_dir_sector,
                parent_dir_index,
            },
            size,
            device: None,
        }
    }

    pub fn new_device(name: String, attributes: FileAttributes, device: Arc<dyn Device>) -> Self {
        assert!(!attributes.directory());
        Self {
            base: BaseNode {
                name,
                attributes,
                start_cluster: DEVICES_FILESYSTEM_CLUSTER_MAGIC,
                parent_dir_sector: NO_PARENT_DIR_SECTOR,
                parent_dir_index: 0,
            },
            size: 0,
            device: Some(device),
        }
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub(self) fn set_size(&mut self, size: u64) {
        self.size = size;
    }

    pub fn try_open_device(&mut self) -> Result<(), FileSystemError> {
        if let Some(device) = self.device.take() {
            self.device = Some(device.try_create().unwrap_or(Ok(device))?);
        }

        Ok(())
    }
}

impl ops::Deref for FileNode {
    type Target = BaseNode;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl Drop for FileNode {
    fn drop(&mut self) {
        if let Some(device) = self.device.take() {
            device.close().expect("Failed to close device");
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectoryNode {
    base: BaseNode,
}

impl DirectoryNode {
    pub fn without_parent(name: String, attributes: FileAttributes, start_cluster: u64) -> Self {
        Self::new(name, attributes, start_cluster, NO_PARENT_DIR_SECTOR, 0)
    }

    pub fn new(
        name: String,
        attributes: FileAttributes,
        start_cluster: u64,
        parent_dir_sector: u64,
        parent_dir_index: u16,
    ) -> Self {
        assert!(attributes.directory());
        Self {
            base: BaseNode {
                name,
                attributes,
                start_cluster,
                parent_dir_sector,
                parent_dir_index,
            },
        }
    }
}

impl ops::Deref for DirectoryNode {
    type Target = BaseNode;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

/// A node of the filesystem, it can be anything, a file, a device or a directory
#[derive(Debug, Clone)]
pub enum Node {
    File(FileNode),
    Directory(DirectoryNode),
}

impl From<FileNode> for Node {
    fn from(file: FileNode) -> Self {
        Self::File(file)
    }
}

impl From<DirectoryNode> for Node {
    fn from(dir: DirectoryNode) -> Self {
        Self::Directory(dir)
    }
}

impl Node {
    pub fn new(
        name: String,
        attributes: FileAttributes,
        start_cluster: u64,
        size: u64,
        parent_dir_sector: u64,
        parent_dir_index: u16,
    ) -> Self {
        if attributes.directory() {
            Self::Directory(DirectoryNode::new(
                name,
                attributes,
                start_cluster,
                parent_dir_sector,
                parent_dir_index,
            ))
        } else {
            Self::File(FileNode::new_file(
                name,
                attributes,
                start_cluster,
                size,
                parent_dir_sector,
                parent_dir_index,
            ))
        }
    }

    pub fn size(&self) -> u64 {
        match self {
            Self::File(file) => file.size,
            Self::Directory(_) => 0,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::File(file) => &file.name,
            Self::Directory(dir) => &dir.name,
        }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, Self::Directory(_))
    }

    pub fn into_dir(self) -> Result<DirectoryNode, FileSystemError> {
        match self {
            Self::Directory(dir) => Ok(dir),
            Self::File(_) => Err(FileSystemError::IsNotDirectory),
        }
    }

    pub fn into_file(self) -> Result<FileNode, FileSystemError> {
        match self {
            Self::File(file) => Ok(file),
            Self::Directory(_) => Err(FileSystemError::IsDirectory),
        }
    }

    #[allow(dead_code)]
    pub fn attributes(&self) -> FileAttributes {
        match self {
            Self::File(file) => file.attributes,
            Self::Directory(dir) => dir.attributes,
        }
    }

    pub fn as_file_stat(&self) -> FileStat {
        FileStat {
            size: self.size(),
            file_type: match self {
                Self::File(_) => FileType::File,
                Self::Directory(_) => FileType::Directory,
            },
        }
    }

    pub fn try_open_device(&mut self) -> Result<(), FileSystemError> {
        if let Self::File(file) = self {
            file.try_open_device()?;
        }

        Ok(())
    }
}

// This is some sort of cache or extra metadata the filesystem
// use to help implement the filesystem and improve performance
#[derive(Debug, Default)]
pub struct AccessHelper {
    current_cluster: u64,
    cluster_index: u64,
}

pub enum DirTreverse {
    Continue,
    Stop,
}

pub trait FileSystem: Send + Sync {
    fn open_root(&self) -> Result<DirectoryNode, FileSystemError>;
    fn read_dir(
        &self,
        inode: &DirectoryNode,
        handler: &mut dyn FnMut(Node) -> DirTreverse,
    ) -> Result<(), FileSystemError>;

    fn read_file(
        &self,
        inode: &FileNode,
        position: u64,
        buf: &mut [u8],
        _access_helper: &mut AccessHelper,
    ) -> Result<u64, FileSystemError> {
        if let Some(device) = &inode.device {
            assert!(inode.start_cluster == DEVICES_FILESYSTEM_CLUSTER_MAGIC);
            device.read(position, buf)
        } else {
            Err(FileSystemError::ReadNotSupported)
        }
    }

    fn write_file(
        &self,
        inode: &mut FileNode,
        position: u64,
        buf: &[u8],
        _access_helper: &mut AccessHelper,
    ) -> Result<u64, FileSystemError> {
        if let Some(device) = &inode.device {
            assert!(inode.start_cluster == DEVICES_FILESYSTEM_CLUSTER_MAGIC);
            device.write(position, buf)
        } else {
            Err(FileSystemError::WriteNotSupported)
        }
    }

    fn close_file(
        &self,
        _inode: &FileNode,
        _access_helper: AccessHelper,
    ) -> Result<(), FileSystemError> {
        Ok(())
    }

    fn set_file_size(&self, _inode: &mut FileNode, _size: u64) -> Result<(), FileSystemError> {
        Err(FileSystemError::OperationNotSupported)
    }
}

pub struct EmptyFileSystem;

impl FileSystem for EmptyFileSystem {
    fn open_root(&self) -> Result<DirectoryNode, FileSystemError> {
        Err(FileSystemError::FileNotFound)
    }

    fn read_dir(
        &self,
        _inode: &DirectoryNode,
        _handler: &mut dyn FnMut(Node) -> DirTreverse,
    ) -> Result<(), FileSystemError> {
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

    fn get_all_matching_mappings<'a, 'p>(
        &'a mut self,
        path: &'p Path,
    ) -> impl Iterator<Item = (&'a Path, Arc<dyn FileSystem>)>
    where
        'p: 'a,
    {
        self.mappings.iter().filter_map(move |(fs_path, fs)| {
            let stripped = fs_path.strip_prefix(path).ok()?;
            if stripped.is_empty() {
                return None;
            }
            Some((stripped, fs.clone()))
        })
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
    CouldNotSetFileLength,
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
) -> Result<(Arc<dyn FileSystem>, Node), FileSystemError> {
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
                return filesystem
                    .open_root()
                    .map(|inode| (filesystem, inode.into()));
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

    let full_path = parent.join(filename);
    let root = filesystem.open_root()?;
    if full_path.is_empty() || full_path.is_root() {
        return Ok((filesystem, root.into()));
    }

    let open_component_inode =
        |inode: &DirectoryNode, component: &str| -> Result<Node, FileSystemError> {
            let mut entry = None;
            filesystem.read_dir(inode, &mut |inode| {
                if inode.name() == component {
                    entry = Some(inode);
                    DirTreverse::Stop
                } else {
                    DirTreverse::Continue
                }
            })?;
            entry.ok_or(FileSystemError::FileNotFound)
        };

    let mut dir = root;
    for component in parent.components() {
        let component = match component {
            Component::RootDir | Component::CurDir => {
                continue;
            }
            keep @ (Component::ParentDir | Component::Normal(_)) => keep.as_str(),
        };
        if component.is_empty() {
            continue;
        }
        let entry = open_component_inode(&dir, component)?;
        if let Node::Directory(dir_node) = entry {
            dir = dir_node;
        } else {
            return Err(FileSystemError::IsNotDirectory);
        }
    }

    // open the file inside `dir`
    let mut entry = open_component_inode(&dir, filename)?;
    if opening_dir {
        if entry.is_dir() {
            Ok((filesystem, entry))
        } else {
            Err(FileSystemError::IsNotDirectory)
        }
    } else {
        // open the device if it is a device
        entry.try_open_device()?;
        Ok((filesystem, entry))
    }
}

/// A handle to a file, it has the inode which controls the properties of the node in the filesystem
pub struct File {
    filesystem: Arc<dyn FileSystem>,
    path: Box<Path>,
    inode: FileNode,
    position: u64,
    is_terminal: bool,
    blocking_mode: BlockingMode,
    access_helper: AccessHelper,
}

/// A handle to a directory, it has the inode which controls the properties of the node in the filesystem
#[allow(dead_code)]
pub struct Directory {
    inode: DirectoryNode,
    path: Box<Path>,
    position: u64,
    dir_entries: Option<Vec<Node>>,
    filesystem: Arc<dyn FileSystem>,
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

        Self::from_inode(inode.into_file()?, path, filesystem, 0, blocking_mode)
    }

    pub fn from_inode<P: AsRef<Path>>(
        inode: FileNode,
        path: P,
        filesystem: Arc<dyn FileSystem>,
        position: u64,
        blocking_mode: BlockingMode,
    ) -> Result<Self, FileSystemError> {
        Ok(Self {
            filesystem,
            path: path.as_ref().into(),
            inode,
            position,
            is_terminal: false,
            blocking_mode,
            access_helper: AccessHelper::default(),
        })
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<u64, FileSystemError> {
        let count = match self.blocking_mode {
            BlockingMode::None => self.filesystem.read_file(
                &self.inode,
                self.position,
                buf,
                &mut self.access_helper,
            )?,
            BlockingMode::Line => {
                // read until \n or \0
                let mut i = 0;
                loop {
                    let mut char_buf = 0;
                    let read_byte = self.filesystem.read_file(
                        &self.inode,
                        self.position,
                        core::slice::from_mut(&mut char_buf),
                        &mut self.access_helper,
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
                    let read_byte = self.filesystem.read_file(
                        &self.inode,
                        self.position,
                        buf,
                        &mut self.access_helper,
                    );

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
        let written = self.filesystem.write_file(
            &mut self.inode,
            self.position,
            buf,
            &mut self.access_helper,
        )?;
        self.position += written;
        Ok(written)
    }

    pub fn seek(&mut self, position: u64) -> Result<(), FileSystemError> {
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

    pub fn size(&self) -> u64 {
        self.inode.size()
    }

    pub fn current_position(&self) -> u64 {
        self.position
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
            access_helper: AccessHelper::default(),
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

impl Drop for File {
    fn drop(&mut self) {
        self.filesystem
            .close_file(&self.inode, core::mem::take(&mut self.access_helper))
            .expect("Failed to close file");
    }
}

#[allow(dead_code)]
impl Directory {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, FileSystemError> {
        let (filesystem, inode) = open_inode(path.as_ref())?;

        Self::from_inode(inode.into_dir()?, path, filesystem, 0)
    }

    pub fn from_inode<P: AsRef<Path>>(
        inode: DirectoryNode,
        path: P,
        filesystem: Arc<dyn FileSystem>,
        position: u64,
    ) -> Result<Self, FileSystemError> {
        Ok(Self {
            path: path.as_ref().into(),
            inode,
            position,
            dir_entries: None,
            filesystem,
        })
    }

    fn fetch_entries(&mut self) -> Result<(), FileSystemError> {
        if self.dir_entries.is_none() {
            let mut dir_entries = Vec::new();
            self.filesystem.read_dir(&self.inode, &mut |entry| {
                dir_entries.push(entry);
                DirTreverse::Continue
            })?;
            // add entries from the root mappings
            for (path, _fs) in FILESYSTEM_MAPPING
                .lock()
                .get_all_matching_mappings(&self.path)
            {
                // only add path with one component
                if path.components().count() == 1 {
                    dir_entries.push(
                        DirectoryNode::without_parent(
                            path.components().next().unwrap().as_str().into(),
                            FileAttributes::DIRECTORY,
                            ANOTHER_FILESYSTEM_MAPPING_INODE_MAGIC,
                        )
                        .into(),
                    );
                }
            }

            self.dir_entries = Some(dir_entries);
        }

        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&mut self, entries: &mut [DirEntry]) -> Result<usize, FileSystemError> {
        self.fetch_entries()?;

        let dir_entries = self
            .dir_entries
            .as_ref()
            .expect("Entries must be initialized");

        let mut i = 0;
        while i < entries.len() {
            if self.position >= dir_entries.len() as u64 {
                break;
            }

            let entry = &dir_entries[self.position as usize];
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
            dir_entries: None, // allow for refetch
            filesystem: self.filesystem.clone(),
        }
    }
}

#[allow(dead_code)]
impl FilesystemNode {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, FileSystemError> {
        let (filesystem, inode) = open_inode(path.as_ref())?;

        match inode {
            Node::File(file) => Ok(Self::File(File::from_inode(
                file,
                path,
                filesystem,
                0,
                BlockingMode::None,
            )?)),
            Node::Directory(directory) => Ok(Self::Directory(Directory::from_inode(
                directory, path, filesystem, 0,
            )?)),
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
