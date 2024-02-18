use core::mem::MaybeUninit;

pub use kernel_user_link::graphics::{FrameBufferInfo, GraphicsCommand};
use kernel_user_link::{
    call_syscall,
    syscalls::{SyscallError, SYS_GRAPHICS},
};

pub struct BlitCommand<'a> {
    pub memory: &'a [u8],
    pub src_framebuffer_info: FrameBufferInfo,
    pub src: (usize, usize),
    pub dst: (usize, usize),
    pub size: (usize, usize),
}

impl BlitCommand<'_> {
    fn is_buffer_valid(&self) -> bool {
        // check the memory buffer
        let buf_expected_len = self.src_framebuffer_info.memory_size();
        if self.memory.len() != buf_expected_len {
            return false;
        }

        // check the `src`
        if self.src.0 >= self.src_framebuffer_info.width
            || self.src.1 >= self.src_framebuffer_info.height
            || self.src.0 + self.size.0 > self.src_framebuffer_info.width
            || self.src.1 + self.size.1 > self.src_framebuffer_info.height
        {
            return false;
        }

        // the `dst` relies on the framebuffer info of the kernel
        // we don't have that info here, so we can't check it
        true
    }
}

/// # Safety
/// This function assumes that `command` and `extra` are valid.
/// based on what's expected by the kernel.
unsafe fn graphics(command: GraphicsCommand, extra: u64) -> Result<(), SyscallError> {
    unsafe {
        call_syscall!(
            SYS_GRAPHICS,
            command as u64, // command
            extra,          // extra
        )
        .map(|e| assert!(e == 0))
    }
}

pub fn take_ownership() -> Result<(), SyscallError> {
    // Safety: `TakeOwnership` is a valid command, and doesn't require any extra data.
    unsafe { graphics(GraphicsCommand::TakeOwnership, 0) }
}

pub fn release_ownership() -> Result<(), SyscallError> {
    // Safety: `ReleaseOwnership` is a valid command, and doesn't require any extra data.
    unsafe { graphics(GraphicsCommand::ReleaseOwnership, 0) }
}

pub fn get_framebuffer_info() -> Result<FrameBufferInfo, SyscallError> {
    let mut info = MaybeUninit::<FrameBufferInfo>::uninit();

    // Safety: `GetFrameBufferInfo` is a valid command, and requires a valid `FrameBufferInfo` pointer.
    //         which rust guarantees here.
    unsafe {
        graphics(
            GraphicsCommand::GetFrameBufferInfo,
            info.as_mut_ptr() as *const _ as u64,
        )?;
    }

    // Safety: `info` is now initialized.
    unsafe { Ok(info.assume_init()) }
}

pub fn blit(command: &BlitCommand<'_>) -> Result<(), SyscallError> {
    if !command.is_buffer_valid() {
        return Err(SyscallError::InvalidGraphicsBuffer);
    }

    let converted_command = kernel_user_link::graphics::BlitCommand {
        memory: command.memory.as_ptr(),
        src_framebuffer_info: command.src_framebuffer_info,
        src: command.src,
        dst: command.dst,
        size: command.size,
    };

    // Safety: `Blit` is a valid command, and requires a valid `BlitCommand` pointer.
    //         we just created one right now, so its valid.
    unsafe { graphics(GraphicsCommand::Blit, &converted_command as *const _ as u64) }
}
