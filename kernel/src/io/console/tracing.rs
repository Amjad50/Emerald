//! Implement `tracing` subscriber that outputs logs to the console.

use core::fmt::{self, Write};

use alloc::vec::Vec;
use kernel_user_link::file::{BlockingMode, OpenOptions};
use tracing::{span, Level};

use crate::{
    io::console,
    sync::{once::OnceLock, spin::mutex::Mutex},
};

static LOG_FILE: OnceLock<Mutex<LogFile>> = OnceLock::new();
const LOG_FILE_PATH: &str = "/kernel.log";

const fn level_str(level: &Level, color: bool) -> &'static str {
    if color {
        match *level {
            Level::TRACE => "\x1b[37mTRACE\x1b[0m",
            Level::DEBUG => "\x1b[34mDEBUG\x1b[0m",
            Level::INFO => "\x1b[32mINFO\x1b[0m",
            Level::WARN => "\x1b[33mWARN\x1b[0m",
            Level::ERROR => "\x1b[31mERROR\x1b[0m",
        }
    } else {
        match *level {
            Level::TRACE => "TRACE",
            Level::DEBUG => "DEBUG",
            Level::INFO => "INFO",
            Level::WARN => "WARN",
            Level::ERROR => "ERROR",
        }
    }
}

fn log_file() -> &'static Mutex<LogFile> {
    LOG_FILE.get_or_init(|| Mutex::new(LogFile::default()))
}

pub fn flush_log_file() {
    if let Some(log_file) = LOG_FILE.try_get() {
        log_file.lock().flush()
    }
}

/// This require heap allocation sadly, as it uses `Arc` internally, even though it could be done
/// without it
pub fn init() {
    tracing::dispatch::set_global_default(tracing::Dispatch::from_static(&CONSOLE_SUBSCRIBER))
        .unwrap();
}

/// Move the log buffer into the heap, and we can store more data there
pub fn move_to_dynamic_buffer() {
    log_file().lock().move_to_dynamic_buffer()
}

static mut INITIAL_BUFFER: [u8; 0x1000] = [0; 0x1000];

enum Buffer {
    Static {
        buffer: &'static mut [u8],
        len: usize,
    },
    Dynamic(Vec<u8>),
}

impl Buffer {
    fn static_buf() -> Self {
        Buffer::Static {
            // SAFETY: We are the only place where this buffer is used, and the `LogFile` is behind a mutex
            buffer: unsafe { core::ptr::addr_of_mut!(INITIAL_BUFFER).as_mut().unwrap() },
            len: 0,
        }
    }

    fn move_to_dynamic(&mut self) {
        match self {
            Buffer::Static { buffer, len } => {
                *self = Buffer::Dynamic(buffer[..*len].to_vec());
            }
            Buffer::Dynamic(_) => {}
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Buffer::Static { len, .. } => *len == 0,
            Buffer::Dynamic(v) => v.is_empty(),
        }
    }

    fn as_bytes(&self) -> &[u8] {
        match self {
            Buffer::Static { buffer, len } => &buffer[..*len],
            Buffer::Dynamic(v) => v,
        }
    }

    fn clear(&mut self) {
        match self {
            Buffer::Static { len, .. } => *len = 0,
            Buffer::Dynamic(v) => v.clear(),
        }
    }

    fn push_str(&mut self, s: &str) {
        match self {
            Buffer::Static { buffer, len } => {
                if len == &buffer.len() {
                    return;
                }

                let mut i = *len;
                let new_buf = s.bytes();
                let needed_end = i + new_buf.len();
                let end = if needed_end > buffer.len() {
                    buffer.len()
                } else {
                    needed_end
                };

                if end == buffer.len() {
                    println!("\n\nWARN: Buffer is full, cannot write more data");
                }

                for b in new_buf {
                    buffer[i] = b;
                    i += 1;
                    if i == end {
                        break;
                    }
                }
                *len = i;
            }
            Buffer::Dynamic(v) => {
                v.extend_from_slice(s.as_bytes());
            }
        }
    }
}

struct LogFile {
    file: Option<crate::fs::File>,
    buffer: Buffer,
}

impl LogFile {
    pub fn move_to_dynamic_buffer(&mut self) {
        self.buffer.move_to_dynamic();
    }

    pub fn flush(&mut self) {
        if self.buffer.is_empty() {
            return;
        }

        if self.file.is_none() {
            let file = crate::fs::File::open_blocking(
                LOG_FILE_PATH,
                BlockingMode::None,
                OpenOptions::CREATE | OpenOptions::TRUNCATE | OpenOptions::WRITE,
            );
            // cannot create the file yet
            match file {
                // not yet initialized the filesystem mapping, wait until we do
                Err(crate::fs::FileSystemError::FileNotFound) => return,
                Err(e) => {
                    println!("Failed to open log file: {:?}", e);
                    return;
                }
                Ok(f) => {
                    self.file = Some(f);
                }
            }
        }

        {
            let file = self.file.as_mut().unwrap();

            file.write(self.buffer.as_bytes()).unwrap();
            file.flush().unwrap();
        }

        self.buffer.clear();
    }
}

impl Default for LogFile {
    fn default() -> Self {
        LogFile {
            file: None,
            buffer: Buffer::static_buf(),
        }
    }
}

impl Write for LogFile {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.buffer.push_str(s);
        Ok(())
    }
}

struct MultiWriter<'a> {
    console: &'a mut dyn Write,
    file: &'a mut LogFile,
}

impl<'a> Write for MultiWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.console.write_str(s)?;
        self.file.write_str(s)?;
        Ok(())
    }
}

static CONSOLE_SUBSCRIBER: ConsoleSubscriber = ConsoleSubscriber;

pub struct ConsoleSubscriber;

impl tracing::Collect for ConsoleSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
        panic!("Spans are not supported")
    }

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {
        panic!("Spans are not supported")
    }

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {
        panic!("Spans are not supported")
    }

    fn event(&self, event: &tracing::Event<'_>) {
        // don't log debug and trace messages for now
        // TODO: use kernel cmdline flags
        if event.metadata().level() >= &Level::DEBUG {
            return;
        }

        // TODO: filter by level
        console::run_with_console(|console| {
            let log_file = &mut log_file().lock();

            // first write the level separatly in order to control the colors
            console.write_char('[')?;
            log_file.write_char('[')?;
            console.write_str(level_str(event.metadata().level(), true))?;
            log_file.write_str(level_str(event.metadata().level(), false))?;

            let mut writer = MultiWriter {
                console,
                file: log_file,
            };
            writer.write_str("  ")?;
            writer.write_str(event.metadata().module_path().unwrap_or("unknown"))?;
            writer.write_str("] ")?;

            let mut visitor = Visitor::new(&mut writer);
            event.record(&mut visitor);
            visitor.finish()?;

            writer.write_char('\n')?;

            Ok::<_, fmt::Error>(())
        })
        .unwrap();
    }

    fn enter(&self, _span: &span::Id) {
        panic!("Spans are not supported")
    }

    fn exit(&self, _span: &span::Id) {
        panic!("Spans are not supported")
    }

    fn current_span(&self) -> tracing_core::span::Current {
        tracing_core::span::Current::none()
    }
}

struct Visitor<'a> {
    console: &'a mut dyn Write,
    result: Option<fmt::Error>,
    is_first: bool,
}

impl<'a> Visitor<'a> {
    fn new(console: &'a mut dyn Write) -> Self {
        Self {
            console,
            result: None,
            is_first: true,
        }
    }

    fn finish(self) -> Result<(), fmt::Error> {
        self.result.map_or(Ok(()), Err)
    }
}

impl tracing::field::Visit for Visitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn alloc::fmt::Debug) {
        if self.result.is_some() {
            return;
        }

        if !self.is_first {
            self.result = self.console.write_str(", ").err();
        }

        self.is_first = false;

        if field.name() != "message" {
            self.result = self
                .console
                .write_str(field.name())
                .and_then(|_| self.console.write_str(" = "))
                .err();
        }

        self.result = self.console.write_fmt(format_args!("{:?}", value)).err();
    }
}
