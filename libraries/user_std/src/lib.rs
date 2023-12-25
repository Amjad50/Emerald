#![no_std]

pub mod alloc;
pub mod io;
mod sync;

pub use kernel_user_link::syscalls::SyscallArgError;
pub use kernel_user_link::syscalls::SyscallError;
