#![no_std]
// used to weak link libm functions
// this is temporary, waiting for PRs to be merged
// into `compiler-builtins`
// https://github.com/rust-lang/compiler-builtins/pull/577
#![feature(linkage)]

pub mod alloc;
pub mod clock;
pub mod graphics;
pub mod io;
pub mod process;
mod sync;

pub use kernel_user_link::syscalls::SyscallArgError;
pub use kernel_user_link::syscalls::SyscallError;
