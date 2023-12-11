use super::{FromSyscallArgU64, SyscallArgError};

impl FromSyscallArgU64 for u64 {
    fn from_syscall_arg_u64(value: u64) -> Result<Self, SyscallArgError> {
        Ok(value)
    }
}

impl FromSyscallArgU64 for usize {
    fn from_syscall_arg_u64(value: u64) -> Result<Self, SyscallArgError> {
        Ok(value as usize)
    }
}

impl FromSyscallArgU64 for u32 {
    fn from_syscall_arg_u64(value: u64) -> Result<Self, SyscallArgError> {
        Ok(value as u32)
    }
}

impl FromSyscallArgU64 for u16 {
    fn from_syscall_arg_u64(value: u64) -> Result<Self, SyscallArgError> {
        Ok(value as u16)
    }
}

impl FromSyscallArgU64 for u8 {
    fn from_syscall_arg_u64(value: u64) -> Result<Self, SyscallArgError> {
        Ok(value as u8)
    }
}

impl FromSyscallArgU64 for *const u8 {
    fn from_syscall_arg_u64(value: u64) -> Result<Self, SyscallArgError> {
        Ok(value as *const u8)
    }
}

impl FromSyscallArgU64 for *mut u8 {
    fn from_syscall_arg_u64(value: u64) -> Result<Self, SyscallArgError> {
        Ok(value as *mut u8)
    }
}
