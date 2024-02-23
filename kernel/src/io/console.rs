mod vga_graphics;
mod vga_text;

use core::{
    cell::RefCell,
    fmt::{self, Write},
};

use alloc::{boxed::Box, string::String, sync::Arc};

use crate::{
    devices::{self, Device},
    fs::FileSystemError,
    multiboot2::{self, FramebufferColorInfo},
    sync::spin::remutex::ReMutex,
};

use self::{vga_graphics::VgaGraphics, vga_text::VgaText};

use super::{
    keyboard::{self, KeyboardReader},
    uart::{Uart, UartPort},
};

// SAFETY: the console is only used inside a lock or mutex
static mut CONSOLE: ConsoleController = ConsoleController::empty_early();

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
pub fn init_late_device(framebuffer: Option<multiboot2::Framebuffer>) {
    // SAFETY: we are running this initialization at `kernel_main` and its done alone
    //  without printing anything at the same time since we are only
    //  running 1 CPU at the  time
    //  We are also sure that no one is printing at this time
    let device = unsafe {
        CONSOLE.init_late(framebuffer);
        // Must have a device
        CONSOLE.late_device().unwrap()
    };

    devices::register_device(device);
}

fn create_video_console(framebuffer: Option<multiboot2::Framebuffer>) -> Box<dyn VideoConsole> {
    match framebuffer {
        Some(framebuffer) => match framebuffer.color_info {
            FramebufferColorInfo::Indexed { .. } => todo!(),
            FramebufferColorInfo::Rgb { .. } => {
                // assumes we have already initialized the vga display
                Box::new(VgaGraphics::new())
            }
            FramebufferColorInfo::EgaText => Box::new(VgaText::new(framebuffer)),
        },
        None => panic!("No framebuffer provided"),
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum AnsiColor {
    Black = 0,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl AnsiColor {
    fn from_u8(color: u8) -> Self {
        match color {
            0 => Self::Black,
            1 => Self::Red,
            2 => Self::Green,
            3 => Self::Yellow,
            4 => Self::Blue,
            5 => Self::Magenta,
            6 => Self::Cyan,
            7 => Self::White,
            8 => Self::BrightBlack,
            9 => Self::BrightRed,
            10 => Self::BrightGreen,
            11 => Self::BrightYellow,
            12 => Self::BrightBlue,
            13 => Self::BrightMagenta,
            14 => Self::BrightCyan,
            15 => Self::BrightWhite,
            _ => panic!("Invalid color"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct VideoConsoleAttribute {
    foreground: AnsiColor,
    background: AnsiColor,
    bold: bool,
    faint: bool,
}

impl Default for VideoConsoleAttribute {
    fn default() -> Self {
        Self {
            foreground: AnsiColor::White,
            background: AnsiColor::Black,
            bold: false,
            faint: false,
        }
    }
}

trait VideoConsole: Send + Sync {
    fn write_byte(&mut self, c: u8);
    fn init(&mut self);
    fn set_attrib(&mut self, attrib: VideoConsoleAttribute);
}

trait Console: Write {
    fn write(&mut self, src: &[u8]) -> usize;
    fn read(&mut self, dst: &mut [u8]) -> usize;
}

pub(super) enum ConsoleController {
    Early(ReMutex<RefCell<EarlyConsole>>),
    Late(Arc<ReMutex<RefCell<LateConsole>>>),
}

impl ConsoleController {
    const fn empty_early() -> Self {
        // SAFETY: this is only called once on static context so nothing is running
        Self::Early(ReMutex::new(RefCell::new(EarlyConsole::empty())))
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
    unsafe fn init_late(&mut self, framebuffer: Option<multiboot2::Framebuffer>) {
        match self {
            Self::Early(console) => {
                let video_console = create_video_console(framebuffer);

                // take the uart, replace the old one with dummy uart
                let uart = core::mem::replace(
                    &mut console.get_mut().get_mut().uart,
                    Uart::new(UartPort::COM1),
                );
                // SAFETY: we are relying on the caller calling this function alone
                //  since we are taking ownership of the early console, and we are sure that
                //  its not being used anywhere, this is fine
                let late_console = LateConsole::new(uart, video_console);
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
            ConsoleController::Early(console) => {
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
            ConsoleController::Late(console) => {
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
            let mut console = EarlyConsole::empty();
            console.init();
            f(&mut console)
        }
    }
}

pub(super) struct EarlyConsole {
    uart: Uart,
}

impl EarlyConsole {
    pub const fn empty() -> Self {
        Self {
            uart: Uart::new(UartPort::COM1),
        }
    }

    pub fn init(&mut self) {
        self.uart.init();
    }

    fn write_byte(&mut self, byte: u8) {
        // Safety: we are sure that the uart is initialized
        unsafe { self.uart.write_byte(byte) };
    }
}

impl Write for EarlyConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write(s.as_bytes());
        Ok(())
    }
}

impl Console for EarlyConsole {
    fn write(&mut self, src: &[u8]) -> usize {
        for &c in src {
            self.write_byte(c);
        }
        src.len()
    }

    fn read(&mut self, _dst: &mut [u8]) -> usize {
        // we can't read from early console
        0
    }
}

pub(super) struct LateConsole {
    uart: Uart,
    video_console: Box<dyn VideoConsole>,
    keyboard: KeyboardReader,
    console_cmd_buffer: Option<String>,
    current_attrib: VideoConsoleAttribute,
}

impl LateConsole {
    /// SAFETY: must ensure that there is no console running at the same time
    unsafe fn new(uart: Uart, video_console: Box<dyn VideoConsole>) -> Self {
        let mut s = Self {
            uart,
            video_console,
            keyboard: keyboard::get_keyboard_reader(),
            console_cmd_buffer: None,
            current_attrib: Default::default(),
        };

        // split inputs
        s.write_byte(b'\n');
        s
    }

    fn write_byte(&mut self, byte: u8) {
        let mut write_byte_inner = |byte: u8| {
            // backspace, ignore
            if byte != 8 {
                self.video_console.write_byte(byte);
                // Safety: we are sure that the uart is initialized
                unsafe { self.uart.write_byte(byte) };
            }
        };

        if let Some(buf) = &mut self.console_cmd_buffer {
            // is this the end of the command
            match byte {
                b'0'..=b'9' | b';' | b'[' => {
                    // part of the command
                    buf.push(byte as char);
                }
                b'm' => {
                    // end of the color command
                    if let Some(inner_cmd) = buf.strip_prefix('[') {
                        inner_cmd.split(';').for_each(|cmd| {
                            if let Ok(cmd) = cmd.parse::<u8>() {
                                match cmd {
                                    0 => {
                                        self.current_attrib = Default::default();
                                    }
                                    1 => {
                                        self.current_attrib.bold = true;
                                        self.current_attrib.faint = false;
                                    }
                                    2 => {
                                        self.current_attrib.bold = false;
                                        self.current_attrib.faint = true;
                                    }
                                    30..=37 => {
                                        let color = cmd - 30;
                                        self.current_attrib.foreground = AnsiColor::from_u8(color);
                                    }
                                    90..=97 => {
                                        let color = (cmd - 90) + 8;
                                        self.current_attrib.foreground = AnsiColor::from_u8(color);
                                    }
                                    40..=47 => {
                                        let color = cmd - 40;
                                        self.current_attrib.background = AnsiColor::from_u8(color);
                                    }
                                    100..=107 => {
                                        let color = (cmd - 100) + 8;
                                        self.current_attrib.background = AnsiColor::from_u8(color);
                                    }
                                    _ => {}
                                }
                                self.video_console.set_attrib(self.current_attrib);
                            }
                        });

                        // output all saved into the uart as well
                        // Safety: we are sure that the uart is initialized
                        unsafe {
                            self.uart.write_byte(0x1b);
                            self.uart.write_byte(b'[');
                            for &c in inner_cmd.as_bytes() {
                                self.uart.write_byte(c);
                            }
                            self.uart.write_byte(b'm');
                        }
                        self.console_cmd_buffer = None;
                    } else {
                        // not a valid command
                        // abort and write the char
                        self.console_cmd_buffer = None;
                        write_byte_inner(byte);
                    }
                }
                _ => {
                    // unsupported command or character of a command
                    // abort and write char, probably we lost some characters
                    // if this was not intended to be a command
                    self.console_cmd_buffer = None;
                    write_byte_inner(byte);
                }
            }
        } else {
            // start of a new command
            // 0x1b = ESC
            if byte == 0x1b {
                self.console_cmd_buffer = Some(String::new());
                return;
            }
            // otherwise, just write to the screen
            write_byte_inner(byte);
        }
    }
}

impl Write for LateConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write(s.as_bytes());
        Ok(())
    }
}

impl Console for LateConsole {
    fn write(&mut self, src: &[u8]) -> usize {
        for &c in src {
            self.write_byte(c);
        }
        src.len()
    }

    fn read(&mut self, dst: &mut [u8]) -> usize {
        let mut i = 0;

        // for some reason, uart returns \r instead of \n when pressing <enter>
        // so we have to convert it to \n
        // Safety: we are sure that the uart is initialized
        let read_uart = || unsafe {
            self.uart
                .try_read_byte()
                .map(|c| if c == b'\r' { b'\n' } else { c })
        };

        while i < dst.len() {
            // try to read from keyboard
            // if we can't read from keyboard, try to read from uart
            if let Some(c) = self
                .keyboard
                .recv()
                .and_then(|c| if c.pressed { c.virtual_char } else { None })
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
            c.read(buf)
        } else {
            // cannot read from console if its taken
            0
        };
        Ok(x as u64)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> Result<u64, FileSystemError> {
        let console = self.lock();
        let x = if let Ok(mut c) = console.try_borrow_mut() {
            c.write(buf)
        } else {
            // this should not be reached at all, but just in case
            //
            // if we can't get the lock, we are inside `panic`
            //  create a new early console and print to it
            let mut console = EarlyConsole::empty();
            console.init();
            console.write(buf)
        };

        Ok(x as u64)
    }
}
