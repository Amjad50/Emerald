use alloc::{collections::VecDeque, string::String, sync::Arc};
use kernel_user_link::file::BlockingMode;

use crate::{
    fs::{self, FileAttributes, FileSystemError, INode},
    sync::spin::mutex::Mutex,
};

use super::Device;

/// Create a connected pipe pair.
/// The first returned file is the read side of the pipe.
/// The second returned file is the write side of the pipe.
pub fn create_pipe_pair() -> (fs::File, fs::File) {
    let pipe = Arc::new(Mutex::new(InnerPipe {
        buffer: VecDeque::new(),
        read_side_available: true,
        write_side_available: true,
    }));

    let read_device = Arc::new(PipeSide {
        inner: pipe.clone(),
        is_read_side: true,
    });
    let write_device = Arc::new(PipeSide {
        inner: pipe.clone(),
        is_read_side: false,
    });

    let read_inode = INode::new_device(
        String::from("read_pipe"),
        FileAttributes::EMPTY,
        Some(read_device),
    );
    let write_inode = INode::new_device(
        String::from("write_pipe"),
        FileAttributes::EMPTY,
        Some(write_device),
    );
    let read_file = fs::inode_to_file(
        read_inode,
        fs::empty_filesystem(),
        0,
        BlockingMode::Block(1),
    );
    // no blocking for write
    let write_file = fs::inode_to_file(write_inode, fs::empty_filesystem(), 0, BlockingMode::None);

    (read_file, write_file)
}

/// Pipe is a device that allows two processes to communicate with each other.
#[derive(Debug)]
struct InnerPipe {
    /// The buffer of the pipe.
    buffer: VecDeque<u8>,
    read_side_available: bool,
    write_side_available: bool,
}

/// Represent one side of a pipe.
/// Check [`create_pipe_pair`] for more details.
#[derive(Debug)]
pub struct PipeSide {
    inner: Arc<Mutex<InnerPipe>>,
    is_read_side: bool,
}

impl Device for PipeSide {
    fn name(&self) -> &str {
        "pipe"
    }

    fn read(&self, _offset: u32, buf: &mut [u8]) -> Result<u64, FileSystemError> {
        if !self.is_read_side {
            return Err(FileSystemError::ReadNotSupported);
        }
        let mut pipe = self.inner.lock();
        if !pipe.write_side_available {
            return Err(FileSystemError::EndOfFile);
        }
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
        if self.is_read_side {
            return Err(FileSystemError::WriteNotSupported);
        }
        let mut pipe = self.inner.lock();
        if !pipe.read_side_available {
            return Err(FileSystemError::EndOfFile);
        }
        for byte in buf.iter() {
            pipe.buffer.push_front(*byte);
        }
        Ok(buf.len() as u64)
    }

    fn close(&self) -> Result<(), FileSystemError> {
        let mut pipe = self.inner.lock();
        if self.is_read_side {
            pipe.read_side_available = false;
        } else {
            pipe.write_side_available = false;
        }
        Ok(())
    }
}
