use super::{FromSyscallArgU64, SyscallArgError};

macro_rules! impl_convert_for_args {
    ($($typ:ty),*) => {
        $(
            impl FromSyscallArgU64 for $typ {
                fn from_syscall_arg_u64(value: u64) -> Result<Self, SyscallArgError> {
                    Ok(value as Self)
                }
            }
        )*
    };
}

impl_convert_for_args![
    i64,
    i32,
    i16,
    i8,
    isize,
    u64,
    u32,
    u16,
    u8,
    usize,
    *const u8,
    *mut u8,
    *const u64,
    *mut u64
];
