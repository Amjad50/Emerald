//! Implement `tracing` subscriber that outputs logs to the console.

use core::fmt::{self, Write};

use alloc::string::String;
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
    LOG_FILE.get_or_init(|| {
        Mutex::new(LogFile {
            file: None,
            buffer: String::new(),
        })
    })
}

pub fn flush_log_file() {
    if let Some(log_file) = LOG_FILE.try_get() {
        log_file.lock().flush()
    }
}

/// This require heap allocation sadly, as it uses `Arc` internally, even though it could be done
/// without it
pub fn init() {
    tracing::subscriber::set_global_default(ConsoleSubscriber).unwrap();
}

struct LogFile {
    file: Option<crate::fs::File>,
    buffer: String,
}

impl LogFile {
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

pub struct ConsoleSubscriber;

impl tracing::Subscriber for ConsoleSubscriber {
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
