/// A blocking flag when dealing with files
/// When using [`crate::syscalls::SYS_OPEN`], Bit 0 of `flags` argument can be:
/// 0 - non-blocking
/// 1 - line buffered
///
/// In order to use `Block` mode, you need to issue `syscall_blocking_mode`
///  with the whole range of blocking modes available for usage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingMode {
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

    pub fn from_blocking_mode_num(blocking_mode: u64) -> Option<BlockingMode> {
        let mode = blocking_mode & 3;
        let rest = blocking_mode >> 2;

        match mode {
            0 if rest == 0 => Some(BlockingMode::None),
            1 if rest == 0 => Some(BlockingMode::Line),
            3 if rest != 0 && rest <= 0xFFFF_FFFF => Some(BlockingMode::Block(rest as u32)),
            _ => None,
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

pub fn parse_blocking_mode(blocking_mode: u64) -> Option<BlockingMode> {
    BlockingMode::from_blocking_mode_num(blocking_mode)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum FileType {
    File,
    Directory,
}

impl Default for FileType {
    fn default() -> Self {
        FileType::File
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct FileStat {
    pub size: u64,
    pub file_type: FileType,
}
