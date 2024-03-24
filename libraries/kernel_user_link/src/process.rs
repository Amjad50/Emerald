#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SpawnFileMapping {
    pub src_fd: usize,
    pub dst_fd: usize,
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
