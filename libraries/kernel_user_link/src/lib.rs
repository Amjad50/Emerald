#![no_std]

pub mod clock;
pub mod file;
pub mod graphics;
pub mod keyboard;
pub mod mouse;
pub mod power;
pub mod process;
pub mod syscalls;

pub const FD_STDIN: usize = 0;
pub const FD_STDOUT: usize = 1;
pub const FD_STDERR: usize = 2;
