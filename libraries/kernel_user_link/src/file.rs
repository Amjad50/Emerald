/// A blocking flag when dealing with files
/// When using [`crate::syscalls::SYS_OPEN`], Bit 0 of `flags` argument can be:
/// 0 - non-blocking
/// 1 - line buffered
///
/// In order to use `Block` mode, you need to issue a special syscall to modify the
/// properties of the file blocking mode
/// TODO: add this syscall
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingMode {
    None,
    Line,
    Block(usize),
}

impl BlockingMode {
    pub fn from_flags(flags: u64) -> Self {
        match flags & 1 {
            0 => BlockingMode::None,
            1 => BlockingMode::Line,
            _ => unreachable!(),
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
