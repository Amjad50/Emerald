use alloc::{collections::VecDeque, string::String, sync::Arc};

use crate::{
    fs::{self, FileAttributes, FileSystemError, INode},
    sync::spin::mutex::Mutex,
};

use super::Device;

pub fn create_pipe_pair() -> (fs::File, fs::File) {
    let pipe = Arc::new(Mutex::new(Pipe {
        buffer: VecDeque::new(),
    }));

    let inode = INode::new_device(String::from("pipe"), FileAttributes::EMPTY, Some(pipe));
    let in_file = fs::inode_to_file(
        inode,
        fs::empty_filesystem(),
        0,
        kernel_user_link::file::BlockingMode::Block(1),
    );
    // TODO: change permissions for the file, one is input and the other is output
    let out_file = in_file.clone();

    (in_file, out_file)
}

/// Pipe is a device that allows two processes to communicate with each other.
#[derive(Debug)]
pub struct Pipe {
    /// The buffer of the pipe.
    buffer: VecDeque<u8>,
}

impl Device for Mutex<Pipe> {
    fn name(&self) -> &str {
        "pipe"
    }

    fn read(&self, _offset: u32, buf: &mut [u8]) -> Result<u64, FileSystemError> {
        let mut pipe = self.lock();
        let mut bytes_read = 0;
        for byte in buf.iter_mut() {
            if let Some(b) = pipe.buffer.pop_back() {
                *byte = b;
                bytes_read += 1;
            } else {
                break;
            }
        }
        Ok(bytes_read as u64)
    }

    fn write(&self, _offset: u32, buf: &[u8]) -> Result<u64, FileSystemError> {
        let mut pipe = self.lock();
        for byte in buf.iter() {
            pipe.buffer.push_front(*byte);
        }
        Ok(buf.len() as u64)
    }
}
