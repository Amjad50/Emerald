use std::{
    fs::File,
    io::{Error, Write},
};

use kernel_user_link::power::{POWER_DEVICE_PATH, SHUTDOWN_COMMAND};

pub fn shutdown() -> Result<(), Error> {
    let mut power_file = File::options().write(true).open(POWER_DEVICE_PATH)?;
    if power_file.write(SHUTDOWN_COMMAND)? != SHUTDOWN_COMMAND.len() {
        return Err(Error::new(
            std::io::ErrorKind::Other,
            "failed to write all bytes",
        ));
    }
    Ok(())
}
