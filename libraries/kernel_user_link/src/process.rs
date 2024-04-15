#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SpawnFileMapping {
    pub src_fd: usize,
    pub dst_fd: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PriorityLevel {
    VeryLow = 1,
    Low = 2,
    Normal = 3,
    High = 4,
    VeryHigh = 5,
}

impl PriorityLevel {
    pub fn from_u64(value: u64) -> Option<Self> {
        match value {
            1 => Some(Self::VeryLow),
            2 => Some(Self::Low),
            3 => Some(Self::Normal),
            4 => Some(Self::High),
            5 => Some(Self::VeryHigh),
            _ => None,
        }
    }

    pub fn to_u64(self) -> u64 {
        self as u64
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProcessMetadata {
    pub pid: u64,
    pub image_base: usize,
    pub image_size: usize,
    pub program_headers_offset: usize,
    pub eh_frame_address: usize,
    pub eh_frame_size: usize,
    pub text_address: usize,
    pub text_size: usize,
}

impl ProcessMetadata {
    pub fn empty() -> Self {
        Self::default()
    }
}

const PROCESS_METADATA_ADDR: *const ProcessMetadata =
    0xFFFF_FF7F_FFFF_E000 as *const ProcessMetadata;

pub fn process_metadata() -> &'static ProcessMetadata {
    unsafe { &*PROCESS_METADATA_ADDR }
}
