#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ClockTime {
    pub seconds: u64,
    pub nanoseconds: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum ClockType {
    /// Real time clock, this follows the unix time
    RealTime = 0,
    /// Monotonic system time, this is based on the system time since boot
    SystemTime = 1,
}

impl TryFrom<u64> for ClockType {
    type Error = ();

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ClockType::RealTime),
            1 => Ok(ClockType::SystemTime),
            _ => Err(()),
        }
    }
}
