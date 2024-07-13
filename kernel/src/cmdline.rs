use parser::{CmdlineParse, ParseError, ParseErrorKind, Result};
use tokenizer::Tokenizer;
use tracing::{error, info};

use crate::{multiboot2::MultiBoot2Info, sync::once::OnceLock};

mod macros;
mod parser;
mod tokenizer;

static CMDLINE: OnceLock<Cmd> = OnceLock::new();

fn parse_cmdline(inp: &str) -> Result<Cmd> {
    let mut tokenizer = Tokenizer::new(inp);
    Cmd::parse_cmdline(&mut tokenizer)
}

const fn default_cmdline() -> Cmd<'static> {
    Cmd {
        uart: true,
        uart_baud: 115200,
        max_log_level: LogLevel::Info,
        log_file: "/kernel.log",
        allow_hpet: true,
        log_aml: LogAml::Off,
    }
}

pub fn init(multiboot_info: &'static MultiBoot2Info) {
    let cmdline = multiboot_info
        .cmdline()
        .and_then(|cmdline| {
            // we will print the result later if there is an error
            parse_cmdline(cmdline).ok()
        })
        .unwrap_or(default_cmdline());

    CMDLINE.set(cmdline).expect("Should only be called once");
}

/// This is extra work, but it's done purely for debugging purposes
pub fn print_cmdline_parse(multiboot_info: &MultiBoot2Info) {
    if let Some(cmdline) = multiboot_info.cmdline() {
        let parsed = parse_cmdline(cmdline);
        info!("Command line: {cmdline:?}");
        match parsed {
            Ok(parsed) => info!("Parsed command line: {parsed:?}"),
            Err(e) => error!("Failed to parse command line: {e:?}"),
        }
    };
}

pub fn cmdline() -> &'static Cmd<'static> {
    // if we didn't initialize, we will use the default (applies for `test`)
    CMDLINE.get_or_init(default_cmdline)
}

macros::cmdline_struct! {
    #[derive(Debug)]
    pub struct Cmd<'a> {
        /// Enable the UART
        #[default = true]
        pub uart: bool,
        /// UART baudrate
        #[default = 115200]
        pub uart_baud: u32,
        /// Log level
        #[default = LogLevel::Info]
        pub max_log_level: LogLevel,
        /// Log file
        #[default = "/kernel.log"]
        pub log_file: &'a str,
        /// Allow `HPET` (if present), otherwise always use `PIT`
        #[default = true]
        pub allow_hpet: bool,
        /// Log the AML content as ASL code on boot from ACPI tables
        #[default = LogAml::Off]
        pub log_aml: LogAml,
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for tracing::Level {
    fn from(val: LogLevel) -> Self {
        match val {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

impl<'a> CmdlineParse<'a> for LogLevel {
    fn parse_cmdline(tokenizer: &mut Tokenizer<'a>) -> Result<'a, Self> {
        let (loc, value) = tokenizer.next_value().ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::Unexpected {
                    need: "trace/debug/info/warn/error",
                    got: None,
                },
                tokenizer.current_index(),
            )
        })?;

        match value {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err(ParseError::new(
                ParseErrorKind::Unexpected {
                    need: "trace/debug/info/warn/error",
                    got: Some(value),
                },
                loc,
            )),
        }
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub enum LogAml {
    /// Do not print the ASL content
    #[default]
    Off,
    /// Print the ASL content as parsed, without moving anything
    Normal,
    /// Reorgnize the content of the ASL code to be in an easier structure
    /// to work with and treverse
    Structured,
}

impl<'a> CmdlineParse<'a> for LogAml {
    fn parse_cmdline(tokenizer: &mut Tokenizer<'a>) -> Result<'a, Self> {
        let (loc, value) = tokenizer.next_value().ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::Unexpected {
                    need: "off/normal/structured",
                    got: None,
                },
                tokenizer.current_index(),
            )
        })?;

        match value {
            "off" => Ok(Self::Off),
            "normal" => Ok(Self::Normal),
            "structured" => Ok(Self::Structured),
            _ => Err(ParseError::new(
                ParseErrorKind::Unexpected {
                    need: "off/normal/structured",
                    got: Some(value),
                },
                loc,
            )),
        }
    }
}
