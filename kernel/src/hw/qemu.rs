#![allow(dead_code)]

use crate::cpu;

const EXIT_FAILURE: u32 = 0; // since ((0 << 1) | 1) = 1.
const EXIT_SUCCESS: u32 = 1; // since ((1 << 1) | 1) = 3.

const IO_BASE: u16 = 0xf4;

pub enum ExitStatus {
    Success,
    Failure,
    Custom(u32),
}

pub fn exit(status: ExitStatus) -> ! {
    let code = match status {
        ExitStatus::Success => EXIT_SUCCESS,
        ExitStatus::Failure => EXIT_FAILURE,
        ExitStatus::Custom(code) => code,
    };

    println!("Exiting with code {}", code);

    unsafe {
        cpu::io_out(IO_BASE, code);
    }

    println!("Qemu did not exit, halting.");

    // If we didn't exit, just halt
    loop {
        unsafe {
            cpu::clear_interrupts();
            // this will never wake up, we disabled interrupt, (except maybe NMI?)
            cpu::halt();
        }
    }
}
