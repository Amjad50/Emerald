use std::{path::PathBuf, process::Command};

pub struct RunConfig {
    iso_path: PathBuf,
    pub enable_debug_port: bool,
    pub enable_gdb: bool,
    pub enable_serial: bool,
    pub enable_graphics: bool,
}

#[allow(dead_code)]
impl RunConfig {
    pub fn new(iso_path: PathBuf) -> RunConfig {
        RunConfig {
            iso_path,
            enable_debug_port: false,
            enable_gdb: false,
            enable_serial: false,
            enable_graphics: true,
        }
    }

    pub fn with_debug_port(mut self, enable_debug_port: bool) -> Self {
        self.enable_debug_port = enable_debug_port;
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

    pub fn with_graphics(mut self, enable_graphics: bool) -> Self {
        self.enable_graphics = enable_graphics;
        self
    }

    pub fn run(self, extra_args: &[String]) -> anyhow::Result<i32> {
        let mut cmd = Command::new("qemu-system-x86_64");

        cmd.arg("-cdrom")
            .arg(self.iso_path)
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

        if self.enable_debug_port {
            cmd.arg("-device")
                .arg("isa-debug-exit,iobase=0xf4,iosize=0x04");
        }

        if !self.enable_graphics {
            cmd.arg("-display").arg("none");
        }

        cmd.args(extra_args);

        println!("[+] Running the kernel: {:?}", cmd);

        cmd.status()
            .map(|status| status.code().unwrap_or(1))
            .map_err(|e| e.into())
    }
}
