#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SpawnFileMapping {
    pub src_fd: usize,
    pub dst_fd: usize,
}

#[derive(Debug, Clone)]
pub struct ProcessMetadata {
    pub pid: u64,
    pub image_base: usize,
    pub image_size: usize,
    pub program_headers_offset: usize,
    pub eh_frame_adress: usize,
    pub eh_frame_size: usize,
}

impl ProcessMetadata {
    pub fn empty() -> ProcessMetadata {
        ProcessMetadata {
            pid: 0,
            image_base: 0,
            image_size: 0,
            program_headers_offset: 0,
            eh_frame_adress: 0,
            eh_frame_size: 0,
        }
    }
}

const PROCESS_METADATA_ADDR: *const ProcessMetadata =
    0xFFFF_FF7F_FFFF_E000 as *const ProcessMetadata;

pub fn process_metadata() -> &'static ProcessMetadata {
    unsafe { &*PROCESS_METADATA_ADDR }
}
