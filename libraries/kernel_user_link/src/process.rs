#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SpawnFileMapping {
    pub src_fd: u64,
    pub dst_fd: u64,
}
