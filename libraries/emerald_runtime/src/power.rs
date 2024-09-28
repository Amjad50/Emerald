use std::{
    fs::File,
    io::{Error, Write},
};

use kernel_user_link::power::{POWER_DEVICE_PATH, REBOOT_COMMAND, SHUTDOWN_COMMAND};

pub enum PowerCommand {
    Shutdown,
    Reboot,
}

impl PowerCommand {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "shutdown" => Some(Self::Shutdown),
            "reboot" => Some(Self::Reboot),
            _ => None,
        }
    }

    pub fn run(&self) -> Result<(), Error> {
        let cmd = match self {
            Self::Shutdown => SHUTDOWN_COMMAND,
            Self::Reboot => REBOOT_COMMAND,
        };
        let mut power_file = File::options().write(true).open(POWER_DEVICE_PATH)?;
        if power_file.write(cmd)? != cmd.len() {
            return Err(Error::new(
                std::io::ErrorKind::Other,
                "failed to write all bytes",
            ));
        }
        Ok(())
    }
}
