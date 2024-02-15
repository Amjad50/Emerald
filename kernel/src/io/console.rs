mod vga_graphics;
mod vga_text;

use core::fmt::{self, Write};

use alloc::{boxed::Box, string::String, sync::Arc};

use crate::{
    devices::{self, Device},
    fs::FileSystemError,
    multiboot2::{self, FramebufferColorInfo},
    sync::spin::mutex::Mutex,
};

use self::{
    vga_graphics::VgaGraphics,
    vga_text::{VgaText, DEFAULT_ATTRIB},
};

use super::{
    keyboard::{self, Keyboard},
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
            FramebufferColorInfo::Rgb { .. } => Box::new(VgaGraphics::new(framebuffer)),
            FramebufferColorInfo::EgaText => Box::new(VgaText::new(framebuffer)),
        },
        None => panic!("No framebuffer provided"),
    }
}

trait VideoConsole: Send + Sync {
    fn write_byte(&mut self, c: u8);
    fn init(&mut self);
    fn set_attrib(&mut self, attrib: u8);
    fn get_attrib(&self) -> u8;
}

trait Console: Write {
    fn write(&mut self, src: &[u8]) -> usize;
    fn read(&mut self, dst: &mut [u8]) -> usize;
}

#[allow(clippy::large_enum_variant)]
pub(super) enum ConsoleController {
    Early(Mutex<EarlyConsole>),
    Late(Arc<Mutex<LateConsole>>),
}

impl ConsoleController {
    const fn empty_early() -> Self {
        // SAFETY: this is only called once on static context so nothing is running
        Self::Early(Mutex::new(EarlyConsole::empty()))
    }

    fn init_early(&self) {
        match self {
            Self::Early(console) => {
                console.lock().init();
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
                let uart =
                    core::mem::replace(&mut console.get_mut().uart, Uart::new(UartPort::COM1));
                // SAFETY: we are relying on the caller calling this function alone
                //  since we are taking ownership of the early console, and we are sure that
                //  its not being used anywhere, this is fine
                let late_console = LateConsole::new(uart, video_console);
                *self = Self::Late(Arc::new(Mutex::new(late_console)));
            }
            Self::Late(_) => {
                panic!("Unexpected late console");
            }
        }
    }

    fn late_device(&self) -> Option<Arc<Mutex<LateConsole>>> {
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
            ConsoleController::Early(console) => f(&mut *console.lock()),
            // we have to use another branch because the types are different
            // even though we use same function calls
            ConsoleController::Late(console) => f(&mut *console.lock()),
        };
        ret
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
    keyboard: Arc<Mutex<Keyboard>>,
    console_cmd_buffer: Option<String>,
    /// 0..=7 is normal, 8..=15 is bright
    /// this is saved state, so we can use it to update the `VGA` attribute when `bold` or `faint` is changed
    current_foreground: u8,
    /// if the current color is bold, i.e. brighter
    is_bold: bool,
    /// if the current color is faint, i.e. darker
    is_faint: bool,
}

impl LateConsole {
    /// SAFETY: must ensure that there is no console running at the same time
    unsafe fn new(uart: Uart, video_console: Box<dyn VideoConsole>) -> Self {
        let mut s = Self {
            uart,
            video_console,
            keyboard: keyboard::get_keyboard(),
            console_cmd_buffer: None,
            current_foreground: 0xF, // white
            is_bold: false,
            is_faint: false,
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

        let terminal_to_vga_color = |color: u8| {
            let mappings = &[
                0,  // black
                4,  // red
                2,  // green
                6,  // brown
                1,  // blue
                5,  // magenta
                3,  // cyan
                7,  // light gray
                8,  // dark gray
                12, // light red
                10, // light green
                14, // yellow
                9,  // light blue
                13, // light magenta
                11, // light cyan
                15, // white
            ];
            mappings[color as usize]
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
                                let mut current_attrib = self.video_console.get_attrib();
                                match cmd {
                                    0 => {
                                        current_attrib = DEFAULT_ATTRIB;
                                        self.is_bold = false;
                                        self.is_faint = false;
                                    }
                                    1 => {
                                        self.is_bold = true;
                                        self.is_faint = false;
                                        // recalculate the color
                                        let color = if self.current_foreground <= 7 {
                                            self.current_foreground + 8
                                        } else {
                                            self.current_foreground
                                        };
                                        current_attrib &= 0b1111_0000;
                                        current_attrib |= terminal_to_vga_color(color);
                                    }
                                    2 => {
                                        self.is_bold = false;
                                        self.is_faint = true;
                                        // recalculate the color
                                        let color = if self.current_foreground > 7 {
                                            self.current_foreground - 8
                                        } else {
                                            self.current_foreground
                                        };
                                        current_attrib &= 0b1111_0000;
                                        current_attrib |= terminal_to_vga_color(color);
                                    }
                                    30..=37 => {
                                        current_attrib &= 0b1111_0000;
                                        let mut color = cmd - 30;
                                        self.current_foreground = color;
                                        if self.is_bold {
                                            color += 8;
                                        }
                                        current_attrib |= terminal_to_vga_color(color);
                                    }
                                    90..=97 => {
                                        current_attrib &= 0b1111_0000;
                                        let mut color = (cmd - 90) + 8;
                                        self.current_foreground = color;
                                        if self.is_faint {
                                            color -= 8;
                                        }
                                        current_attrib |= terminal_to_vga_color(color);
                                    }
                                    40..=47 => {
                                        current_attrib &= 0b1000_1111;
                                        current_attrib |=
                                            (terminal_to_vga_color(cmd - 40) & 7) << 4;
                                    }
                                    100..=107 => {
                                        current_attrib &= 0b1000_1111;
                                        current_attrib |=
                                            (terminal_to_vga_color((cmd - 100) + 8) & 7) << 4;
                                    }
                                    _ => {}
                                }
                                self.video_console.set_attrib(current_attrib);
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
        let mut keyboard = self.keyboard.lock();

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

impl fmt::Debug for LateConsole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LateConsole").finish()
    }
}

impl Device for Mutex<LateConsole> {
    fn name(&self) -> &str {
        "console"
    }

    fn read(&self, _offset: u64, buf: &mut [u8]) -> Result<u64, FileSystemError> {
        Ok(self.lock().read(buf) as _)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> Result<u64, FileSystemError> {
        Ok(self.lock().write(buf) as _)
    }
}
