mod types_conversions;

/// must be one of user interrupts, i.e. 0x20+
///
/// This is the number of the interrupts that we are going to use between the
/// user-kernel
pub const SYSCALL_INTERRUPT_NUMBER: u8 = 0xFE;

pub const NUM_SYSCALLS: usize = 13;

mod numbers {
    pub const SYS_OPEN: u64 = 0;
    pub const SYS_WRITE: u64 = 1;
    pub const SYS_READ: u64 = 2;
    pub const SYS_CLOSE: u64 = 3;
    pub const SYS_BLOCKING_MODE: u64 = 4;
    pub const SYS_EXIT: u64 = 5;
    pub const SYS_SPAWN: u64 = 6;
    pub const SYS_INC_HEAP: u64 = 7;
    pub const SYS_CREATE_PIPE: u64 = 8;
    pub const SYS_WAIT_PID: u64 = 9;
    pub const SYS_STAT: u64 = 10;
    pub const SYS_OPEN_DIR: u64 = 11;
    pub const SYS_READ_DIR: u64 = 12;
}
pub use numbers::*;

/// Creates a syscall, the first argument is the syscall number (in RAX), then the arguments are as follows
/// RCX, RDX, RSI, RDI, R8, R9, R10 (7 arguments max)
#[macro_export]
macro_rules! call_syscall {
    ($syscall_num:expr $(,)?) => {
        call_syscall!(@step $syscall_num; { }; { })
    };
    ($syscall_num:expr, $($args:expr),* $(,)?) => {
        call_syscall!(@step $syscall_num; { }; {$($args),*})
    };
    (@step $syscall_num: expr; {$($generated:tt)*}; {}) => {
        call_syscall!(@final $syscall_num, {$($generated)*})
    };
    (@step $syscall_num: expr; {$($generated:tt)*}; {$one:expr}) => {
        call_syscall!(@step $syscall_num; {in("rcx") $one, $($generated)*}; {})
    };
    (@step $syscall_num: expr; {$($generated:tt)*}; {$one:expr, $two:expr}) => {
        call_syscall!(@step $syscall_num; {in("rdx") $two, $($generated)*}; {$one})
    };
    (@step $syscall_num: expr; {$($generated:tt)*}; {$one:expr, $two:expr, $three:expr}) => {
        call_syscall!(@step $syscall_num; {in("rsi") $three, $($generated)*}; {$one, $two})
    };
    (@step $syscall_num: expr; {$($generated:tt)*}; {$one:expr, $two:expr, $three:expr, $four:expr}) => {
        call_syscall!(@step $syscall_num; {in("rdi") $four, $($generated)*}; {$one, $two, $three})
    };
    (@step $syscall_num: expr; {$($generated:tt)*}; {$one:expr, $two:expr, $three:expr, $four:expr, $five:expr}) => {
        call_syscall!(@step $syscall_num; {in("r8") $five, $($generated)*}; {$one, $two, $three, $four})
    };
    (@step $syscall_num: expr; {$($generated:tt)*}; {$one:expr, $two:expr, $three:expr, $four:expr, $five:expr, $six:expr}) => {
        call_syscall!(@step $syscall_num; {in("r9") $six, $($generated)*}; {$one, $two, $three, $four, $five})
    };
    (@step $syscall_num: expr; {$($generated:tt)*}; {$one:expr, $two:expr, $three:expr, $four:expr, $five:expr, $six:expr, $seven:expr}) => {
        call_syscall!(@step $syscall_num; {in("r10") $seven, $($generated)*}; {$one, $two, $three, $four, $five, $six})
    };
    (@step $syscall_num: expr; {$($generated:tt)*}; {$($args:expr),*}) => {
        compile_error!("Too many arguments for syscall")
    };
    (@final $syscall_num: expr, {$($generated:tt)*}) => {
        {
            let result: u64;
            ::core::arch::asm!("int 0xFE",
                            inout("rax") $syscall_num => result,
                            $($generated)*
                            options(nomem, nostack, preserves_flags));
            $crate::syscalls::syscall_result_from_u64(result)
        }
    };
}

/// Get the syscall arguments from the interrupt state, the arguments come from
/// the registers RCX, RDX, RSI, RDI, R8, R9, R10
#[macro_export]
macro_rules! sys_arg {
    ($num:tt, $context_struct:expr) => {
        sys_arg!(@impl $num, $context_struct => u64)
    };
    ($num:tt, $context_struct:expr => $func:ident($ty:ty)) => {
        sys_arg!($num, $context_struct => $ty).and_then($func)
    };
    ($num:tt, $context_struct:expr => $ty:ty) => {
        syscall_arg_to_u64::<$ty>(sys_arg!(@impl $num, $context_struct))
    };
    (@impl 0, $context_struct:expr) => {
        $context_struct.rcx
    };
    (@impl 1, $context_struct:expr) => {
        $context_struct.rdx
    };
    (@impl 2, $context_struct:expr) => {
        $context_struct.rsi
    };
    (@impl 3, $context_struct:expr) => {
        $context_struct.rdi
    };
    (@impl 4, $context_struct:expr) => {
        $context_struct.r8
    };
    (@impl 5, $context_struct:expr) => {
        $context_struct.r9
    };
    (@impl 6, $context_struct:expr) => {
        $context_struct.r10
    };
    (@impl $rest:tt, $context_struct:expr) => {
        compile_error!("Not valid argument number")
    };
}

#[macro_export]
macro_rules! to_arg_err {
    ($num:tt, $err:expr) => {
        to_arg_err!(@impl $num, $err)
    };
    (@impl 0, $err:expr) => {
        $crate::syscalls::SyscallError::InvalidArgument(::core::option::Option::Some($err), None, None, None, None, None, None)
    };
    (@impl 1, $err:expr) => {
        $crate::syscalls::SyscallError::InvalidArgument(None, ::core::option::Option::Some($err), None, None, None, None, None)
    };
    (@impl 2, $err:expr) => {
        $crate::syscalls::SyscallError::InvalidArgument(None, None, ::core::option::Option::Some($err), None, None, None, None)
    };
    (@impl 3, $err:expr) => {
        $crate::syscalls::SyscallError::InvalidArgument(None, None, None, ::core::option::Option::Some($err), None, None, None)
    };
    (@impl 4, $err:expr) => {
        $crate::syscalls::SyscallError::InvalidArgument(None, None, None, None, ::core::option::Option::Some($err), None, None)
    };
    (@impl 5, $err:expr) => {
        $crate::syscalls::SyscallError::InvalidArgument(None, None, None, None, None, ::core::option::Option::Some($err), None)
    };
    (@impl 6, $err:expr) => {
        $crate::syscalls::SyscallError::InvalidArgument(None, None, None, None, None, None, ::core::option::Option::Some($err))
    };
    (@impl $rest:tt, $err:expr) => {
        compile_error!("Not valid argument number")
    };
}

#[macro_export]
macro_rules! verify_args {
    () => {((), (), (), (), (), (), ())};
    ($arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr, $arg7:expr) => {
        match ($arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $arg7) {
            (
                Ok(a),
                Ok(b),
                Ok(c),
                Ok(d),
                Ok(e),
                Ok(f),
                Ok(g),
            ) => {(a, b, c, d, e, f, g)}
            err => {
                return $crate::syscalls::SyscallResult::Err(
                    $crate::syscalls::SyscallError::InvalidArgument(
                        err.0.err(), err.1.err(), err.2.err(), err.3.err(), err.4.err(), err.5.err(), err.6.err(),
                    ),
                )
            }
        }
    };
    ($arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr, $arg7:expr, $extra:expr) => {
        compile_error!("Too many arguments for syscall")
    };
    // general
    ($($args:expr),* $(,)?) => {
        verify_args!($($args ,)* Ok(()))
    };
}

pub trait FromSyscallArgU64 {
    fn from_syscall_arg_u64(value: u64) -> Result<Self, SyscallArgError>
    where
        Self: Sized;
}

pub fn syscall_arg_to_u64<T: FromSyscallArgU64>(value: u64) -> Result<T, SyscallArgError> {
    T::from_syscall_arg_u64(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[non_exhaustive]
pub enum SyscallArgError {
    // 0 is valid (used by Option::None)
    GeneralInvalid = 1,
    InvalidUserPointer = 2,
    NotValidUtf8 = 3,
    InvalidHeapIncrement = 4,
    DuplicateFileMappings = 5,
}

impl SyscallArgError {
    fn try_from(value: u8) -> Result<Option<Self>, ()> {
        match value {
            0 => Ok(None),
            1 => Ok(Some(SyscallArgError::GeneralInvalid)),
            2 => Ok(Some(SyscallArgError::InvalidUserPointer)),
            3 => Ok(Some(SyscallArgError::NotValidUtf8)),
            4 => Ok(Some(SyscallArgError::InvalidHeapIncrement)),
            5 => Ok(Some(SyscallArgError::DuplicateFileMappings)),
            _ => Err(()),
        }
    }
}

#[repr(align(8))]
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
#[non_exhaustive]
pub enum SyscallError {
    SyscallNotFound = 0,
    InvalidErrorCode(u64) = 1,
    CouldNotOpenFile = 2,
    InvalidFileIndex = 3,
    CouldNotWriteToFile = 4,
    CouldNotReadFromFile = 5,
    CouldNotLoadElf = 6,
    CouldNotAllocateProcess = 7,
    HeapRangesExceeded = 8,
    EndOfFile = 9,
    FileNotFound = 10,
    PidNotFound = 11,
    ProcessStillRunning = 12,
    IsNotDirectory = 13,
    IsDirectory = 14,
    InvalidArgument(
        Option<SyscallArgError>,
        Option<SyscallArgError>,
        Option<SyscallArgError>,
        Option<SyscallArgError>,
        Option<SyscallArgError>,
        Option<SyscallArgError>,
        Option<SyscallArgError>,
    ),
}

pub type SyscallResult = Result<u64, SyscallError>;

impl From<SyscallError> for SyscallResult {
    fn from(error: SyscallError) -> Self {
        SyscallResult::Err(error)
    }
}

/// Creates an error u64 value for syscall, each byte controls the error for that
/// argument, if the byte is 0, then the argument is valid, otherwise it is
/// invalid. and it represent the error code
/// the error value contains up to 7 arguments errors, the most significant bit
/// indicate a negative number.
///
/// This function will always set the msb to 1
fn create_syscall_error(
    arg1: Option<SyscallArgError>,
    arg2: Option<SyscallArgError>,
    arg3: Option<SyscallArgError>,
    arg4: Option<SyscallArgError>,
    arg5: Option<SyscallArgError>,
    arg6: Option<SyscallArgError>,
    arg7: Option<SyscallArgError>,
) -> u64 {
    let mut error = 0u64;
    error |= arg1.map(|e| e as u64).unwrap_or(0);
    error |= arg2.map(|e| e as u64).unwrap_or(0) << 8;
    error |= arg3.map(|e| e as u64).unwrap_or(0) << 16;
    error |= arg4.map(|e| e as u64).unwrap_or(0) << 24;
    error |= arg5.map(|e| e as u64).unwrap_or(0) << 32;
    error |= arg6.map(|e| e as u64).unwrap_or(0) << 40;
    error |= arg7.map(|e| e as u64).unwrap_or(0) << 48;
    error |= 1 << 63;
    error
}

#[inline(always)]
pub fn syscall_handler_wrapper<F>(syscall_number: u64, f: F) -> u64
where
    F: FnOnce() -> SyscallResult,
{
    if syscall_number >= NUM_SYSCALLS as u64 {
        return syscall_result_to_u64(SyscallResult::Err(SyscallError::SyscallNotFound));
    }
    let result = f();
    syscall_result_to_u64(result)
}

pub fn syscall_result_to_u64(result: SyscallResult) -> u64 {
    match result {
        SyscallResult::Ok(value) => {
            assert!(
                value & (1 << 63) == 0,
                "syscall result should not have msb set"
            );
            value
        }
        SyscallResult::Err(error) => {
            let err_upper = match error {
                SyscallError::SyscallNotFound => -1i64 as u64,
                SyscallError::InvalidErrorCode(code) => code,
                SyscallError::InvalidArgument(arg1, arg2, arg3, arg4, arg5, arg6, arg7) => {
                    create_syscall_error(arg1, arg2, arg3, arg4, arg5, arg6, arg7)
                }
                SyscallError::CouldNotOpenFile => 2 << 56,
                SyscallError::InvalidFileIndex => 3 << 56,
                SyscallError::CouldNotWriteToFile => 4 << 56,
                SyscallError::CouldNotReadFromFile => 5 << 56,
                SyscallError::CouldNotLoadElf => 6 << 56,
                SyscallError::CouldNotAllocateProcess => 7 << 56,
                SyscallError::HeapRangesExceeded => 8 << 56,
                SyscallError::EndOfFile => 9 << 56,
                SyscallError::FileNotFound => 10 << 56,
                SyscallError::PidNotFound => 11 << 56,
                SyscallError::ProcessStillRunning => 12 << 56,
                SyscallError::IsNotDirectory => 13 << 56,
                SyscallError::IsDirectory => 14 << 56,
            };

            err_upper | (1 << 63)
        }
    }
}

pub fn syscall_result_from_u64(value: u64) -> SyscallResult {
    if value & (1 << 63) == 0 {
        SyscallResult::Ok(value)
    } else {
        // remove last bit
        let value = value & !(1 << 63);
        // last byte
        let err_byte = (value >> 56) as u8;

        let invalid_error_code = |_| -> SyscallError { SyscallError::InvalidErrorCode(value) };

        let err = match err_byte {
            0 => {
                let arg1 =
                    SyscallArgError::try_from((value & 0xFF) as u8).map_err(invalid_error_code)?;
                let arg2 = SyscallArgError::try_from(((value >> 8) & 0xFF) as u8)
                    .map_err(invalid_error_code)?;
                let arg3 = SyscallArgError::try_from(((value >> 16) & 0xFF) as u8)
                    .map_err(invalid_error_code)?;
                let arg4 = SyscallArgError::try_from(((value >> 24) & 0xFF) as u8)
                    .map_err(invalid_error_code)?;
                let arg5 = SyscallArgError::try_from(((value >> 32) & 0xFF) as u8)
                    .map_err(invalid_error_code)?;
                let arg6 = SyscallArgError::try_from(((value >> 40) & 0xFF) as u8)
                    .map_err(invalid_error_code)?;
                let arg7 = SyscallArgError::try_from(((value >> 48) & 0xFF) as u8)
                    .map_err(invalid_error_code)?;

                SyscallError::InvalidArgument(arg1, arg2, arg3, arg4, arg5, arg6, arg7)
            }
            1 => SyscallError::InvalidErrorCode(value),
            2 => SyscallError::CouldNotOpenFile,
            3 => SyscallError::InvalidFileIndex,
            4 => SyscallError::CouldNotWriteToFile,
            5 => SyscallError::CouldNotReadFromFile,
            6 => SyscallError::CouldNotLoadElf,
            7 => SyscallError::CouldNotAllocateProcess,
            8 => SyscallError::HeapRangesExceeded,
            9 => SyscallError::EndOfFile,
            10 => SyscallError::FileNotFound,
            11 => SyscallError::PidNotFound,
            12 => SyscallError::ProcessStillRunning,
            13 => SyscallError::IsNotDirectory,
            14 => SyscallError::IsDirectory,
            _ => invalid_error_code(()),
        };
        SyscallResult::Err(err)
    }
}
