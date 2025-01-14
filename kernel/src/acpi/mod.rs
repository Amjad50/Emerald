mod aml;
pub mod tables;

use alloc::format;
use aml::{
    execution::{AmlExecutionError, ExecutionContext},
    Aml,
};
use tables::facp;
pub use tables::init_acpi_tables;
use tracing::{error, info, warn};

use crate::{
    cpu::{
        self,
        idt::{BasicInterruptHandler, InterruptStackFrame64},
        interrupts::apic,
    },
    power,
    sync::once::OnceLock,
};

static ACPI: OnceLock<Acpi> = OnceLock::new();

/// Setup interrupts and request ownership of ACPI
pub fn init() {
    ACPI.set(Acpi::init())
        .expect("ACPI was already initialized");

    info!("ACPI initialized");
}

pub fn sleep(ty: u8) -> Result<(), AcpiError> {
    ACPI.get().sleep(ty)
}

#[derive(Debug)]
pub enum AcpiError {
    InvalidSleepType,
    SleepTypeNotAvailable,
}

/// Stores some items and data related to ACPI
#[derive(Debug)]
struct Acpi {
    /// SLP_TYPa and SLP_TYPb data for \_S1_ until \_S5_
    slp_type_data: [Option<[u8; 2]>; 5],
}

impl Acpi {
    fn init() -> Acpi {
        Self::enable();

        let mut slp_type_data = [None, None, None, None, None];

        for table in tables::get_acpi_tables().rsdt.iter_tables::<tables::Xsdt>() {
            for (i, slp_data) in slp_type_data.iter_mut().enumerate() {
                if slp_data.is_some() {
                    continue;
                }

                if let Some(result) = fetch_s_array(&table.aml, &format!("\\_S{}_", i + 1)) {
                    *slp_data = Some(result);
                }
            }
        }

        info!("SLP_TYPa and SLP_TYPb data: {:?}", slp_type_data);

        Acpi { slp_type_data }
    }

    fn sleep(&self, ty: u8) -> Result<(), AcpiError> {
        if ty == 0 || ty > 5 {
            return Err(AcpiError::InvalidSleepType);
        }

        if let Some(slp_data) = &self.slp_type_data[ty as usize - 1] {
            let facp = tables::get_acpi_tables()
                .rsdt
                .get_table::<tables::Facp>()
                .expect("No Facp");

            let mut ctrl_a = facp.read_pm1_control_a();
            let ctrl_b = facp.read_pm1_control_b();

            ctrl_a = (ctrl_a & !facp::flags::PM_CTRL_SLP_TYP_MASK)
                | ((slp_data[0] as u16) << facp::flags::PM_CTRL_SLP_TYP_SHIFT);

            info!("Entering Sleep mode {ty}!");

            if let Some(mut ctrl_b) = ctrl_b {
                ctrl_b = (ctrl_b & !facp::flags::PM_CTRL_SLP_TYP_MASK)
                    | ((slp_data[1] as u16) << facp::flags::PM_CTRL_SLP_TYP_SHIFT);
                facp.write_pm1_control_b(ctrl_b);
            }

            facp.write_pm1_control_a(ctrl_a | facp::flags::PM_CTRL_SLP_EN);

            // poll WAK_STS until woken
            while facp.read_pm1_status() & facp::flags::PM_STS_WAK == 0 {
                core::hint::spin_loop();
            }

            Ok(())
        } else {
            Err(AcpiError::SleepTypeNotAvailable)
        }
    }

    fn enable() {
        let facp = tables::get_acpi_tables()
            .rsdt
            .get_table::<tables::Facp>()
            .expect("No Facp");

        // make sure we haven't already enabled ACPI
        assert!(!apic::is_irq_assigned(facp.sci_interrupt()));

        // disable the events first
        facp.write_pm1_enable(0);

        apic::assign_io_irq(
            acpi_handler as BasicInterruptHandler,
            facp.sci_interrupt(),
            cpu::cpu(),
        );

        if !facp.is_acpi_enabled() {
            facp.enable_acpi();
        } else {
            warn!("ACPI already enabled");
        }

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
    }
}

/// Halper function
fn fetch_s_array(aml: &Aml, name: &str) -> Option<[u8; 2]> {
    let mut ctx = ExecutionContext::default();
    match aml.execute(&mut ctx, name, &[]) {
        Ok(obj) => {
            let Some(package) = obj.as_package() else {
                error!("{} is not a package", name);
                return None;
            };

            if package.size() < 2 {
                error!("{} package is ony {} in size", name, package.size());
            }

            let mut slp_data = [0; 2];

            for (i, item) in package
                .iter()
                .take(2)
                .map(|obj| {
                    let Some(data) = obj.as_data() else {
                        error!("{:?} is not a data", obj);
                        return None;
                    };

                    let Some(int) = data.as_integer() else {
                        error!("{:?} is not an integer", data);
                        return None;
                    };

                    if let Some(mut int) = int.as_u8() {
                        if int & !7 != 0 {
                            warn!(
                                "Data {:02X} of {name:?} is more than 3 bits, truncating...",
                                int
                            );
                            int &= 7;
                        }
                        Some(int)
                    } else {
                        error!("{:?} is not a u8", int);
                        None
                    }
                })
                .enumerate()
            {
                if let Some(item) = item {
                    slp_data[i] = item;
                } else {
                    return None;
                }
            }

            Some(slp_data)
        }
        Err(AmlExecutionError::LableNotFound(_)) => None,
        Err(e) => {
            error!("Failed to execute AML for {name}: {:?}", e);
            None
        }
    }
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

        // TODO: handle shutdown setup
        power::start_power_sequence(power::PowerCommand::Shutdown);
    } else if pm1_event & facp::flags::PM_EN_TMR != 0 {
        facp.write_pm1_status(facp::flags::PM_EN_TMR);
        warn!("Timer ACPI event: {:X}", pm1_event);
    } else {
        warn!("Unknown ACPI event: {:X}", pm1_event);
    }

    apic::return_from_interrupt();
}
