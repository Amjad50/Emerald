use crate::cpu;

use super::ApicGenericAddress;

#[allow(dead_code)]
pub mod flags {
    // Power Management Status Register flags
    pub const PM_STS_TMR: u16 = 1 << 0;
    pub const PM_STS_BM: u16 = 1 << 4;
    pub const PM_STS_GBL: u16 = 1 << 5;
    pub const PM_STS_PWRBTN: u16 = 1 << 8;
    pub const PM_STS_SLPBTN: u16 = 1 << 9;
    pub const PM_STS_RTC: u16 = 1 << 10;
    pub const PM_STS_PCIEXP_WAKE: u16 = 1 << 14;
    pub const PM_STS_WAK: u16 = 1 << 15;

    // Power Management Enable Register flags
    pub const PM_EN_TMR: u16 = 1 << 0;
    pub const PM_EN_GBL: u16 = 1 << 5;
    pub const PM_EN_PWRBTN: u16 = 1 << 8;
    pub const PM_EN_SLPBTN: u16 = 1 << 9;
    pub const PM_EN_RTC: u16 = 1 << 10;
    pub const PM_DIS_PCIEXP_WAKE: u16 = 1 << 14;

    // Power Management Control Register flags
    pub const PM_CTRL_SCI_EN: u16 = 1 << 0;
    pub const PM_CTRL_BM_RLD: u16 = 1 << 1;
    pub const PM_CTRL_GBL_RLS: u16 = 1 << 2;
    pub const PM_CTRL_SLP_EN: u16 = 1 << 13;
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct Facp {
    firmware_control: u32,
    pub dsdt: u32,
    reserved: u8,
    preferred_pm_profile: u8,
    sci_interrupt: u16,
    smi_command_port: u32,
    acpi_enable: u8,
    acpi_disable: u8,
    s4bios_req: u8,
    pstate_control: u8,
    pm1a_event_block: u32,
    pm1b_event_block: u32,
    pm1a_control_block: u32,
    pm1b_control_block: u32,
    pm2_control_block: u32,
    pm_timer_block: u32,
    gpe0_block: u32,
    gpe1_block: u32,
    pm1_event_length: u8,
    pm1_control_length: u8,
    pm2_control_length: u8,
    pm_timer_length: u8,
    gpe0_block_length: u8,
    gpe1_block_length: u8,
    gpe1_base: u8,
    cstate_control: u8,
    p_level2_latency: u16,
    p_level3_latency: u16,
    flush_size: u16,
    flush_stride: u16,
    duty_offset: u8,
    duty_width: u8,
    day_alarm: u8,
    month_alarm: u8,
    pub century: u8,
    iapc_boot_arch: u16,
    reserved2: u8,
    flags: u32,
    reset_reg: ApicGenericAddress,
    reset_value: u8,
    arm_boot_arch: u16,
    fadt_minor_version: u8,
    x_firmware_control: u64,
    x_dsdt: u64,
    x_pm1a_event_block: ApicGenericAddress,
    x_pm1b_event_block: ApicGenericAddress,
    x_pm1a_control_block: ApicGenericAddress,
    x_pm1b_control_block: ApicGenericAddress,
    x_pm2_control_block: ApicGenericAddress,
    x_pm_timer_block: ApicGenericAddress,
    x_gpe0_block: ApicGenericAddress,
    x_gpe1_block: ApicGenericAddress,
    sleep_control_reg: ApicGenericAddress,
    sleep_status_reg: ApicGenericAddress,
    hypervisor_vendor_id: u64,
}

#[allow(dead_code)]
impl Facp {
    pub fn sci_interrupt(&self) -> u8 {
        assert!(self.sci_interrupt < 256);
        assert!(self.sci_interrupt > 0);

        self.sci_interrupt as u8
    }

    fn smi_command_port(&self) -> u16 {
        assert!(self.smi_command_port > 0 && self.smi_command_port < 0xFFFF);
        self.smi_command_port as u16
    }

    #[inline]
    pub fn is_acpi_enabled(&self) -> bool {
        (self.read_pm1_control() & 1) != 0
    }

    pub fn enable_acpi(&self) {
        if self.is_acpi_enabled() {
            return;
        }
        unsafe {
            cpu::io_out(self.smi_command_port(), self.acpi_enable);
        }
    }

    fn access_io_write(&self, register: u32, alt_register: Option<u32>, length: u8, value: u32) {
        assert!(register > 0 && register < 0xFFFF);

        match length {
            2 => unsafe {
                cpu::io_out::<u16>(
                    register as u16,
                    value.try_into().expect("Should be in u16 range"),
                );
                if let Some(alt_register) = alt_register {
                    assert!(alt_register > 0 && alt_register < 0xFFFF);
                    cpu::io_out::<u16>(
                        alt_register as u16,
                        value.try_into().expect("Should be in u16 range"),
                    );
                }
            },
            4 => unsafe {
                cpu::io_out::<u32>(register as u16, value);
                if let Some(alt_register) = alt_register {
                    assert!(alt_register > 0 && alt_register < 0xFFFF);
                    cpu::io_out::<u32>(alt_register as u16, value);
                }
            },
            _ => {
                todo!("Can't handle `register length` of {}", length)
            }
        }
    }

    fn access_io_read(&self, register: u32, length: u8) -> u32 {
        assert!(register > 0 && register < 0xFFFF);

        match length {
            2 => unsafe { cpu::io_in::<u16>(register as u16) as u32 },
            4 => unsafe { cpu::io_in::<u32>(register as u16) },
            _ => {
                todo!("Can't handle `register length` of {}", length)
            }
        }
    }

    pub fn write_pm1_status(&self, value: u16) {
        if !self.x_pm1a_event_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        let pm1_evt_part_len = self.pm1_event_length / 2;
        let alt_reg = if self.pm1b_event_block == 0 {
            None
        } else {
            Some(self.pm1b_event_block)
        };

        self.access_io_write(
            self.pm1a_event_block,
            alt_reg,
            pm1_evt_part_len,
            value as u32,
        )
    }

    pub fn read_pm1_status(&self) -> u16 {
        if !self.x_pm1a_event_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        let pm1_evt_part_len = self.pm1_event_length / 2;
        self.access_io_read(self.pm1a_event_block, pm1_evt_part_len)
            .try_into()
            .expect("Should be in u16 range")
    }

    pub fn write_pm1_enable(&self, value: u16) {
        if !self.x_pm1a_event_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        let pm1_evt_part_len = self.pm1_event_length / 2;
        let alt_reg = if self.pm1b_event_block == 0 {
            None
        } else {
            Some(self.pm1b_event_block + pm1_evt_part_len as u32)
        };

        self.access_io_write(
            self.pm1a_event_block + pm1_evt_part_len as u32,
            alt_reg,
            pm1_evt_part_len,
            value as u32,
        )
    }

    pub fn read_pm1_enable(&self) -> u16 {
        if !self.x_pm1a_event_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        let pm1_evt_part_len = self.pm1_event_length / 2;
        self.access_io_read(
            self.pm1a_event_block + pm1_evt_part_len as u32,
            pm1_evt_part_len,
        )
        .try_into()
        .expect("Should be in u16 range")
    }

    pub fn write_pm1_control(&self, value: u16) {
        if !self.x_pm1a_control_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        let alt_reg = if self.pm1b_control_block == 0 {
            None
        } else {
            Some(self.pm1b_control_block)
        };
        self.access_io_write(
            self.pm1a_control_block,
            alt_reg,
            self.pm1_control_length,
            value as u32,
        )
    }

    pub fn read_pm1_control(&self) -> u16 {
        if !self.x_pm1a_control_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        self.access_io_read(self.pm1a_control_block, self.pm1_control_length)
            .try_into()
            .expect("Should be in u16 range")
    }

    pub fn read_pm_timer(&self) -> Option<u32> {
        if !self.x_pm_timer_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        if self.pm_timer_block == 0 {
            return None;
        }
        assert_eq!(self.pm_timer_length, 4);

        Some(self.access_io_read(self.pm_timer_block, self.pm_timer_length))
    }

    pub fn write_pm2_control(&self, value: u16) -> Option<()> {
        if !self.x_pm2_control_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        if self.pm2_control_block == 0 {
            return None;
        }

        self.access_io_write(
            self.pm2_control_block,
            Some(self.pm2_control_block),
            self.pm2_control_length,
            value as u32,
        );

        Some(())
    }

    pub fn read_pm2_control(&self) -> Option<u16> {
        if !self.x_pm2_control_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        if self.pm2_control_block == 0 {
            return None;
        }

        Some(self.access_io_read(self.pm2_control_block, self.pm2_control_length) as u16)
    }

    fn gpe_reg_write(register: u32, length: u8, mut value: u32) {
        assert!(register > 0 && register < 0xFFFF);
        assert!(length <= 4);

        // GPE is accessible by bytes regardless of the length
        for i in 0..length as u16 {
            unsafe {
                cpu::io_out::<u8>(register as u16 + i, (value & 0xff) as u8);
            }

            value >>= 8;
        }
    }

    fn gpe_reg_read(register: u32, length: u8) -> u32 {
        assert!(register > 0 && register < 0xFFFF);
        assert!(length <= 4);

        let mut result = 0;

        for i in 0..length as u16 {
            let s = unsafe { cpu::io_in::<u8>(register as u16 + i) as u32 };
            result |= s << (i * 8);
        }

        result
    }

    pub fn write_gpe_0_event_status(&self, value: u32) -> Option<()> {
        if !self.x_gpe0_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        if self.gpe0_block == 0 {
            return None;
        }

        Self::gpe_reg_write(self.gpe0_block, self.gpe0_block_length / 2, value);

        Some(())
    }

    pub fn read_gpe_0_event_status(&self) -> Option<u32> {
        if !self.x_gpe0_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        if self.gpe0_block == 0 {
            return None;
        }

        Some(Self::gpe_reg_read(
            self.gpe0_block,
            self.gpe0_block_length / 2,
        ))
    }

    pub fn write_gpe_0_event_enable(&self, value: u16) -> Option<()> {
        if !self.x_gpe0_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        if self.gpe0_block == 0 {
            return None;
        }

        Self::gpe_reg_write(
            self.gpe0_block + (self.gpe0_block_length / 2) as u32,
            self.gpe0_block_length / 2,
            value as u32,
        );

        Some(())
    }

    pub fn read_gpe_0_event_enable(&self) -> Option<u32> {
        if !self.x_gpe0_block.is_zero() {
            todo!("implement GenericAddress access");
        }

        if self.gpe0_block == 0 {
            return None;
        }

        Some(Self::gpe_reg_read(
            self.gpe0_block + (self.gpe0_block_length / 2) as u32,
            self.gpe0_block_length / 2,
        ))
    }
}
