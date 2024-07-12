mod aml;
pub mod tables;

use tables::facp;
pub use tables::init_acpi_tables;
use tracing::{info, warn};

use crate::cpu::{
    self,
    idt::{BasicInterruptHandler, InterruptStackFrame64},
    interrupts::apic,
};

/// Setup interrupts and request ownership of ACPI
pub fn setup_enable_acpi() {
    let facp = tables::get_acpi_tables()
        .rsdt
        .get_table::<tables::Facp>()
        .expect("No Facp");

    if facp.is_acpi_enabled() {
        warn!("ACPI already enabled");
        assert!(apic::is_irq_assigned(facp.sci_interrupt()));

        return;
    }

    // disable the events first
    facp.write_pm1_enable(0);

    apic::assign_io_irq(
        acpi_handler as BasicInterruptHandler,
        facp.sci_interrupt(),
        cpu::cpu(),
    );

    facp.enable_acpi();

    let mut i = 0;
    while !facp.is_acpi_enabled() && i < 10000 {
        i += 1;
        core::hint::spin_loop();
    }

    if !facp.is_acpi_enabled() {
        panic!("Failed to enable ACPI");
    }

    // enable all events except timer
    facp.write_pm1_enable(
        facp::flags::PM_EN_GBL
            | facp::flags::PM_EN_PWRBTN
            | facp::flags::PM_EN_SLPBTN
            | facp::flags::PM_EN_RTC,
    );

    info!("ACPI initialized");
}

extern "x86-interrupt" fn acpi_handler(_frame: InterruptStackFrame64) {
    let facp = tables::get_acpi_tables()
        .rsdt
        .get_table::<tables::Facp>()
        .expect("No Facp");

    let pm1_event = facp.read_pm1_status();

    if pm1_event & facp::flags::PM_EN_GBL != 0 {
        facp.write_pm1_status(facp::flags::PM_EN_GBL);
        warn!("Global ACPI event: {:X}", pm1_event);
    } else if pm1_event & facp::flags::PM_EN_SLPBTN != 0 {
        facp.write_pm1_status(facp::flags::PM_EN_SLPBTN);
        warn!("Sleep button ACPI event: {:X}", pm1_event);
    } else if pm1_event & facp::flags::PM_EN_RTC != 0 {
        facp.write_pm1_status(facp::flags::PM_EN_RTC);
        warn!("RTC ACPI event: {:X}", pm1_event);
    } else if pm1_event & facp::flags::PM_EN_PWRBTN != 0 {
        facp.write_pm1_status(facp::flags::PM_EN_PWRBTN);
        warn!("Power button ACPI event: {:X}", pm1_event);
    } else if pm1_event & facp::flags::PM_EN_TMR != 0 {
        facp.write_pm1_status(facp::flags::PM_EN_TMR);
        warn!("Timer ACPI event: {:X}", pm1_event);
    } else {
        warn!("Unknown ACPI event: {:X}", pm1_event);
    }

    apic::return_from_interrupt();
}
