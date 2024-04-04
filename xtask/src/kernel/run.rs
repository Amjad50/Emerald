use std::process::Command;

use crate::GlobalMeta;

pub struct RunConfig {
    pub enable_debug: bool,
    pub enable_gdb: bool,
    pub enable_serial: bool,
}

#[allow(dead_code)]
impl RunConfig {
    pub fn new() -> RunConfig {
        RunConfig {
            enable_debug: false,
            enable_gdb: false,
            enable_serial: false,
        }
    }

    pub fn with_debug(mut self, enable_debug: bool) -> Self {
        self.enable_debug = enable_debug;
        self
    }

    pub fn with_gdb(mut self, enable_gdb: bool) -> Self {
        self.enable_gdb = enable_gdb;
        self
    }

    pub fn with_serial(mut self, enable_serial: bool) -> Self {
        self.enable_serial = enable_serial;
        self
    }

    pub fn run(self, meta: &GlobalMeta, extra_args: &[String]) -> anyhow::Result<i32> {
        let mut cmd = Command::new("qemu-system-x86_64");

        cmd.arg("-cdrom")
            .arg(super::iso_path(meta))
            .arg("-m")
            .arg("512")
            .arg("-boot")
            .arg("d")
            .arg("-drive")
            .arg("format=raw,file=fat:rw:filesystem");

        if self.enable_serial {
            cmd.arg("-serial").arg("mon:stdio");
        }

        if self.enable_gdb {
            cmd.arg("-s").arg("-S");
        }

        if self.enable_debug {
            cmd.arg("-device")
                .arg("isa-debug-exit,iobase=0xf4,iosize=0x04");
        }

        cmd.args(extra_args);

        println!("[+] Running the kernel: {:?}", cmd);

        cmd.status()
            .map(|status| status.code().unwrap_or(1))
            .map_err(|e| e.into())
    }
}
