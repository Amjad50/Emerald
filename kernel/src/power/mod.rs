use tracing::info;

use crate::{
    acpi,
    cpu::{self},
    devices::Device,
    fs,
    io::console,
    process::scheduler,
};

/// Power device
///
/// This device is used to control the power of the system,
/// its mostly used with `echo [cmd] > /devices/power`
/// such as:
/// - `echo shutdown > /devices/power` to shutdown the system.
#[derive(Debug)]
pub struct PowerDevice;

impl Device for PowerDevice {
    fn name(&self) -> &str {
        "power"
    }

    // This is needed to support the `echo shutdown > /dev/power`, as it will
    // open the file and truncate it to 0, then write to it.
    fn set_size(&self, size: u64) -> Result<(), fs::FileSystemError> {
        if size != 0 {
            // TODO: replace the errors with better ones
            return Err(fs::FileSystemError::EndOfFile);
        }

        Ok(())
    }

    // TODO: replace the errors with better ones
    fn write(&self, offset: u64, buf: &[u8]) -> Result<u64, fs::FileSystemError> {
        if offset != 0 {
            return Err(fs::FileSystemError::EndOfFile);
        }

        if let Some(rest) = buf.strip_prefix(b"shutdown") {
            if rest.trim_ascii().is_empty() {
                start_shutdown();
                Ok(buf.len() as u64)
            } else {
                Err(fs::FileSystemError::EndOfFile)
            }
        } else {
            Err(fs::FileSystemError::EndOfFile)
        }
    }
}

/// Start the shutdown process
pub fn start_shutdown() {
    info!("Shutting down...");

    // tell the scheduler to initiate shutdown, the rest will be handled by
    // [`shutdown_system`]
    scheduler::stop_scheduler();
}

/// reverse of [`crate::kernel_main`]
/// Called by [`crate::kernel_main`] after all processes have exited and cleaned up
pub fn shutdown_system() -> ! {
    console::tracing::shutdown_log_file();
    // unmount all filesystems
    fs::unmount_all();
    // shutdown through ACPI, state S5
    acpi::sleep(5).expect("Could not shutdown");

    // if ACPI failed or woke up, halt the CPU
    loop {
        unsafe {
            cpu::halt();
        }
    }
}
