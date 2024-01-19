use core::{
    cell::RefCell,
    fmt::{self, Write},
};

use alloc::sync::Arc;

use crate::{
    devices::{self, Device},
    fs::FileSystemError,
    sync::spin::{mutex::Mutex, remutex::ReMutex},
};

use super::{
    keyboard::{self, Keyboard},
    uart::{Uart, UartPort},
    video_memory::{VgaBuffer, DEFAULT_ATTRIB},
};

// SAFETY: the console is only used inside a lock or mutex
static mut CONSOLE: Console = Console::empty_early();

/// # SAFETY
/// the caller must assure that this is not called while not being initialized
/// at the same time
pub(super) fn run_with_console<F, U>(f: F) -> U
where
    F: FnMut(&mut dyn core::fmt::Write) -> U,
{
    // SAFETY: printing is done after initialization steps and not at the same time
    //  look at `io::_print`
    unsafe { CONSOLE.run_with(f) }
}

/// Create an early console, this is used before the kernel heap is initialized
pub fn early_init() {
    // SAFETY: we are running this initialization at the very startup,
    // without printing anything at the same time since we are only
    // running 1 CPU at the  time
    unsafe { CONSOLE.init_early() };
}

/// Create a late console, this is used after the kernel heap is initialized
/// And also assign a console device
pub fn init_late_device() {
    // SAFETY: we are running this initialization at `kernel_main` and its done alone
    //  without printing anything at the same time since we are only
    //  running 1 CPU at the  time
    //  We are also sure that no one is printing at this time
    let device = unsafe {
        CONSOLE.init_late();
        // Must have a device
        CONSOLE.late_device().unwrap()
    };

    devices::register_device(device);
}

pub(super) enum Console {
    Early(ReMutex<RefCell<EarlyConsole>>),
    Late(Arc<ReMutex<RefCell<LateConsole>>>),
}

impl Console {
    const fn empty_early() -> Self {
        // SAFETY: this is only called once on static context so nothing is running
        Self::Early(ReMutex::new(RefCell::new(unsafe { EarlyConsole::empty() })))
    }

    fn init_early(&self) {
        match self {
            Self::Early(console) => {
                let console = console.lock();
                console.borrow_mut().init();
            }
            Self::Late(_) => {
                panic!("Unexpected late console");
            }
        }
    }

    /// # SAFETY
    /// Must ensure that there is no console is being printed to/running at the same time
    unsafe fn init_late(&mut self) {
        match self {
            Self::Early(console) => {
                // SAFETY: we are relying on the caller calling this function alone
                //  since we are taking ownership of the early console, and we are sure that
                //  its not being used anywhere, this is fine
                let late_console = LateConsole::migrate_from_early(&console.lock().borrow());
                *self = Self::Late(Arc::new(ReMutex::new(RefCell::new(late_console))));
            }
            Self::Late(_) => {
                panic!("Unexpected late console");
            }
        }
    }

    fn late_device(&self) -> Option<Arc<ReMutex<RefCell<LateConsole>>>> {
        match self {
            Self::Early(_) => None,
            Self::Late(console) => Some(console.clone()),
        }
    }

    pub fn run_with<F, U>(&self, mut f: F) -> U
    where
        F: FnMut(&mut dyn core::fmt::Write) -> U,
    {
        let ret = match self {
            Console::Early(console) => {
                let console = console.lock();
                let x = if let Ok(mut c) = console.try_borrow_mut() {
                    Some(f(&mut *c))
                } else {
                    None
                };
                x
            }
            // we have to use another branch because the types are different
            // even though we use same function calls
            Console::Late(console) => {
                let console = console.lock();
                let x = if let Ok(mut c) = console.try_borrow_mut() {
                    Some(f(&mut *c))
                } else {
                    None
                };
                x
            }
        };

        if let Some(ret) = ret {
            ret
        } else {
            // if we can't get the lock, we are inside `panic`
            //  create a new early console and print to it
            let mut console = unsafe { EarlyConsole::empty() };
            console.init();
            f(&mut console)
        }
    }
}

pub(super) struct EarlyConsole {
    uart: Uart,
    video_buffer: VgaBuffer,
}

impl EarlyConsole {
    /// SAFETY: the console must be used inside a lock or mutex
    ///  as the Video buffer position is global
    pub const unsafe fn empty() -> Self {
        Self {
            uart: Uart::new(UartPort::COM1),
            video_buffer: VgaBuffer::new(),
        }
    }

    pub fn init(&mut self) {
        self.video_buffer.init();
        self.uart.init();
    }

    /// SAFETY: the caller must assure that this is called from once place at a time
    ///         and should handle synchronization
    unsafe fn write_byte(&mut self, byte: u8) {
        self.video_buffer.write_byte(byte, DEFAULT_ATTRIB);
        self.uart.write_byte(byte);
    }

    /// SAFETY: the caller must assure that this is called from once place at a time
    ///        and should handle synchronization
    pub unsafe fn write(&mut self, src: &[u8]) -> usize {
        for &c in src {
            self.write_byte(c);
        }
        src.len()
    }
}

impl Write for EarlyConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        unsafe { self.write(s.as_bytes()) };
        Ok(())
    }
}

pub(super) struct LateConsole {
    uart: Uart,
    video_buffer: VgaBuffer,
    keyboard: Arc<Mutex<Keyboard>>,
}

impl LateConsole {
    /// SAFETY: must ensure that there is no console running at the same time
    unsafe fn migrate_from_early(early: &EarlyConsole) -> Self {
        let mut s = Self {
            uart: early.uart.clone(),
            video_buffer: early.video_buffer.clone(),
            keyboard: keyboard::get_keyboard(),
        };

        // split inputs
        s.write_byte(b'\n');
        s
    }

    /// SAFETY: the caller must assure that this is called from once place at a time
    ///         and should handle synchronization
    unsafe fn write_byte(&mut self, byte: u8) {
        self.video_buffer.write_byte(byte, DEFAULT_ATTRIB);
        self.uart.write_byte(byte);
    }

    /// SAFETY: the caller must assure that this is called from once place at a time
    ///        and should handle synchronization
    pub unsafe fn write(&mut self, src: &[u8]) -> usize {
        for &c in src {
            self.write_byte(c);
        }
        src.len()
    }

    pub unsafe fn read(&mut self, dst: &mut [u8]) -> usize {
        let mut i = 0;
        let mut keyboard = self.keyboard.lock();

        // for some reason, uart returns \r instead of \n when pressing <enter>
        // so we have to convert it to \n
        let read_uart = || {
            self.uart
                .try_read_byte()
                .map(|c| if c == b'\r' { b'\n' } else { c })
        };

        while i < dst.len() {
            // try to read from keyboard
            // if we can't read from keyboard, try to read from uart
            if let Some(c) = keyboard
                .get_next_char()
                .and_then(|c| c.virtual_char)
                .or_else(read_uart)
            {
                dst[i] = c;
                i += 1;
                // ignore if its not a valid char
            } else {
                break;
            }
        }
        i
    }
}

impl Write for LateConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        unsafe { self.write(s.as_bytes()) };
        Ok(())
    }
}

impl fmt::Debug for LateConsole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LateConsole").finish()
    }
}

impl Device for ReMutex<RefCell<LateConsole>> {
    fn name(&self) -> &str {
        "console"
    }

    fn read(&self, _offset: u64, buf: &mut [u8]) -> Result<u64, FileSystemError> {
        let console = self.lock();
        let x = if let Ok(mut c) = console.try_borrow_mut() {
            unsafe { c.read(buf) }
        } else {
            // cannot read from console if its taken
            0
        };
        Ok(x as u64)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> Result<u64, FileSystemError> {
        let console = self.lock();
        let x = if let Ok(mut c) = console.try_borrow_mut() {
            unsafe { c.write(buf) }
        } else {
            // this should not be reached at all, but just in case
            //
            // if we can't get the lock, we are inside `panic`
            //  create a new early console and print to it
            let mut console = unsafe { EarlyConsole::empty() };
            console.init();
            unsafe { console.write(buf) }
        };

        Ok(x as u64)
    }
}
