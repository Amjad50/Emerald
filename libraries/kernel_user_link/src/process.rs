#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SpawnFileMapping {
    pub src_fd: usize,
    pub dst_fd: usize,
}
