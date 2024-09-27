use tracing::info;

use crate::{
    acpi,
    cpu::{self},
    fs,
    io::console,
    process::scheduler,
};

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
