#![no_std]

pub mod syscalls;

pub const FD_STDIN: usize = 0;
pub const FD_STDOUT: usize = 1;
pub const FD_STDERR: usize = 2;
