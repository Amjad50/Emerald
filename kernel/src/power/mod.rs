use tracing::{error, info};

use crate::{
    acpi,
    cpu::{self},
    devices::{keyboard_mouse, Device},
    fs,
    io::console,
    process::scheduler,
    sync::once::OnceLock,
};

static CURRENT_CMD: OnceLock<PowerCommand> = OnceLock::new();

#[derive(Debug, Copy, Clone)]
pub enum PowerCommand {
    Shutdown,
    Reboot,
}

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
                start_power_sequence(PowerCommand::Shutdown);
                Ok(buf.len() as u64)
            } else {
                Err(fs::FileSystemError::EndOfFile)
            }
        } else if let Some(rest) = buf.strip_prefix(b"reboot") {
            if rest.trim_ascii().is_empty() {
                start_power_sequence(PowerCommand::Reboot);
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
pub fn start_power_sequence(cmd: PowerCommand) {
    if let Err(current_cmd) = CURRENT_CMD.set(cmd) {
        error!("Power command already set: {current_cmd:?}, ignoring: {cmd:?}",);
        return;
    }
    match cmd {
        PowerCommand::Shutdown => {
            info!("Shutting down the system");
        }
        PowerCommand::Reboot => {
            info!("Rebooting the system");
        }
    }

    // tell the scheduler to initiate shutdown/reboot, the rest will be handled by
    // [`finish_power_sequence`]
    scheduler::stop_scheduler();
}

/// reverse of [`crate::kernel_main`]
/// Called by [`crate::kernel_main`] after all processes have exited and cleaned up
pub fn finish_power_sequence() -> ! {
    let cmd = CURRENT_CMD.try_get().expect("No power command set");

    console::tracing::shutdown_log_file();
    // unmount all filesystems
    fs::unmount_all();

    cpu::cpu().push_cli();
    match cmd {
        PowerCommand::Shutdown => {
            // shutdown through ACPI, state S5
            acpi::sleep(5).expect("Could not shutdown");
        }
        PowerCommand::Reboot => {
            // TODO: implement using the `reset_register` in ACPI if available
            //       not doing it now because for my qemu its not enabled,
            //       and using the below method is easier for now.
            info!("Rebooting the system using the keyboard controller");
            keyboard_mouse::reset_system();
        }
    }

    // if ACPI failed or woke up, halt the CPU
    loop {
        unsafe {
            cpu::halt();
        }
    }
}
