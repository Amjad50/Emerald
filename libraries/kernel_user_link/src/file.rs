use core::{ffi::CStr, ops};

/// A blocking flag when dealing with files
/// When using [`crate::syscalls::SYS_OPEN`], Bit 0 of `flags` argument can be:
/// 0 - non-blocking
/// 1 - line buffered
///
/// In order to use `Block` mode, you need to issue `syscall_blocking_mode`
///  with the whole range of blocking modes available for usage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlockingMode {
    #[default]
    None,
    Line,
    Block(u32),
}

impl BlockingMode {
    pub fn from_flags(flags: u64) -> Self {
        match flags & 1 {
            0 => BlockingMode::None,
            1 => BlockingMode::Line,
            _ => unreachable!(),
        }
    }

    pub fn to_u64(&self) -> u64 {
        match self {
            BlockingMode::None => 0,
            BlockingMode::Line => 1,
            BlockingMode::Block(num) => (*num as u64) << 2 | 3,
        }
    }
}

impl TryFrom<u64> for BlockingMode {
    type Error = ();

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        let mode = value & 3;
        let rest = value >> 2;

        match mode {
            0 if rest == 0 => Ok(BlockingMode::None),
            1 if rest == 0 => Ok(BlockingMode::Line),
            3 if rest != 0 && rest <= 0xFFFF_FFFF => Ok(BlockingMode::Block(rest as u32)),
            _ => Err(()),
        }
    }
}

/// Will extract all the information from the flags, will return `None` if the argument
/// is invalid
pub fn parse_flags(flags: u64) -> Option<BlockingMode> {
    let blocking_mode = BlockingMode::from_flags(flags);
    let flags = flags & !1;
    // must be 0 at the end
    if flags == 0 {
        Some(blocking_mode)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub enum FileType {
    #[default]
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct FileStat {
    pub size: u64,
    pub file_type: FileType,
}

pub const MAX_FILENAME_LEN: usize = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DirFilename([u8; MAX_FILENAME_LEN + 1]);

impl Default for DirFilename {
    fn default() -> Self {
        Self([0; MAX_FILENAME_LEN + 1])
    }
}

impl DirFilename {
    pub fn as_cstr(&self) -> &CStr {
        CStr::from_bytes_until_nul(&self.0).unwrap()
    }
}

impl From<&str> for DirFilename {
    fn from(s: &str) -> Self {
        let mut name = [0; MAX_FILENAME_LEN + 1];
        let bytes = s.as_bytes();
        assert!(bytes.len() < MAX_FILENAME_LEN);
        name[..bytes.len()].copy_from_slice(bytes);
        Self(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct DirEntry {
    pub stat: FileStat,
    pub name: DirFilename,
}

impl DirEntry {
    pub fn filename_cstr(&self) -> &CStr {
        self.name.as_cstr()
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FileMeta {
    BlockingMode(BlockingMode) = 0,
    IsTerminal(bool) = 1,
}

impl FileMeta {
    pub fn to_u64_meta_id(&self) -> u64 {
        match self {
            FileMeta::BlockingMode(_) => 0,
            FileMeta::IsTerminal(_) => 1,
        }
    }

    pub fn inner_u64(&self) -> u64 {
        match self {
            FileMeta::BlockingMode(mode) => mode.to_u64(),
            FileMeta::IsTerminal(is_terminal) => *is_terminal as u64,
        }
    }
}

impl TryFrom<(u64, u64)> for FileMeta {
    type Error = ();

    fn try_from(value: (u64, u64)) -> Result<Self, Self::Error> {
        match value.0 {
            0 => Ok(FileMeta::BlockingMode(BlockingMode::try_from(value.1)?)),
            1 => Ok(FileMeta::IsTerminal(value.1 != 0)),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SeekWhence {
    Start = 0,
    Current = 1,
    End = 2,
}

impl TryFrom<u64> for SeekWhence {
    type Error = ();

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SeekWhence::Start),
            1 => Ok(SeekWhence::Current),
            2 => Ok(SeekWhence::End),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SeekFrom {
    pub offset: i64,
    pub whence: SeekWhence,
}

impl SeekFrom {
    pub fn new(offset: i64, whence: SeekWhence) -> Self {
        Self { offset, whence }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    create: bool,
    create_new: bool,
    truncate: bool,
    append: bool,
}

#[allow(dead_code)]
impl OpenOptions {
    pub const READ: Self = Self {
        read: true,
        write: false,
        create: false,
        create_new: false,
        truncate: false,
        append: false,
    };

    pub const WRITE: Self = Self {
        read: false,
        write: true,
        create: false,
        create_new: false,
        truncate: false,
        append: false,
    };

    pub const CREATE: Self = Self {
        read: false,
        write: false,
        create: true,
        create_new: false,
        truncate: false,
        append: false,
    };

    pub const CREATE_NEW: Self = Self {
        read: false,
        write: false,
        create: true,
        create_new: true,
        truncate: false,
        append: false,
    };

    pub const TRUNCATE: Self = Self {
        read: false,
        write: false,
        create: false,
        create_new: false,
        truncate: true,
        append: false,
    };

    pub const APPEND: Self = Self {
        read: false,
        write: false,
        create: false,
        create_new: false,
        truncate: false,
        append: true,
    };

    pub fn new() -> Self {
        Self {
            read: false,
            write: false,
            create: false,
            create_new: false,
            truncate: false,
            append: false,
        }
    }

    pub fn read(mut self, read: bool) -> Self {
        self.read = read;
        self
    }

    pub fn write(mut self, write: bool) -> Self {
        self.write = write;
        self
    }

    pub fn create(mut self, create: bool) -> Self {
        self.create = create;
        self
    }

    pub fn create_new(mut self, create_new: bool) -> Self {
        self.create_new = create_new;
        self
    }

    pub fn truncate(mut self, truncate: bool) -> Self {
        self.truncate = truncate;
        self
    }

    pub fn append(mut self, append: bool) -> Self {
        self.append = append;
        self
    }

    pub fn is_read(&self) -> bool {
        self.read
    }

    pub fn is_write(&self) -> bool {
        self.write || self.append
    }

    pub fn is_create(&self) -> bool {
        self.create
    }

    pub fn is_create_new(&self) -> bool {
        self.create_new
    }

    pub fn is_truncate(&self) -> bool {
        self.truncate
    }

    pub fn is_append(&self) -> bool {
        self.append
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new().read(true)
    }
}

impl ops::BitOr for OpenOptions {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            read: self.read | rhs.read,
            write: self.write | rhs.write,
            create: self.create | rhs.create,
            create_new: self.create_new | rhs.create_new,
            truncate: self.truncate | rhs.truncate,
            append: self.append | rhs.append,
        }
    }
}

impl ops::BitOrAssign for OpenOptions {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

impl ops::BitAnd for OpenOptions {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self {
            read: self.read & rhs.read,
            write: self.write & rhs.write,
            create: self.create & rhs.create,
            create_new: self.create_new & rhs.create_new,
            truncate: self.truncate & rhs.truncate,
            append: self.append & rhs.append,
        }
    }
}

impl ops::BitAndAssign for OpenOptions {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = *self & rhs;
    }
}
