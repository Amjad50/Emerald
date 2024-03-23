//! Ls shell program
//!
//! Usage: ls [paths...]

#![feature(io_error_more)]

use std::{fmt, process::ExitCode};

use colored::{control::SHOULD_COLORIZE, ColoredString, Colorize};

#[repr(transparent)]
pub struct FileSize(pub u64);

impl fmt::Display for FileSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // find the best unit
        let mut size = self.0;
        let mut remaining = 0;
        let mut unit = "B";
        if size >= 1024 {
            remaining = size % 1024;
            size /= 1024;
            unit = "K";
        }
        if size >= 1024 {
            remaining = size % 1024;
            size /= 1024;
            unit = "M";
        }
        if size >= 1024 {
            remaining = size % 1024;
            size /= 1024;
            unit = "G";
        }
        if size >= 1024 {
            remaining = size % 1024;
            size /= 1024;
            unit = "T";
        }
        if size >= 1024 {
            remaining = size % 1024;
            size /= 1024;
            unit = "P";
        }

        size.fmt(f).and_then(|_| {
            let remaining = remaining * 10 / 1024;
            write!(f, ".{remaining:01}")?;
            write!(f, "{unit}")
        })
    }
}

impl FileSize {
    pub fn colored(&self) -> ColoredString {
        let s = format!("{}", self);
        if self.0 < 1024 {
            s.green()
        } else if self.0 < 1024 * 1024 {
            s.yellow()
        } else if self.0 < 1024 * 1024 * 1024 {
            s.bright_red()
        } else {
            s.red()
        }
    }
}

// helper function
// `print_parent`: indent more, this will be true if we are printing the parent directory
fn ls(path: &str, print_parent: bool) -> bool {
    let indent_space = if print_parent { "  " } else { "" };
    let dir = match std::fs::read_dir(path) {
        Ok(d) => d,
        Err(e) => {
            match e.kind() {
                std::io::ErrorKind::NotFound => {
                    println!("{indent_space}[!] path not found: {}", path);
                    return false;
                }
                std::io::ErrorKind::NotADirectory => {
                    let meta = match std::fs::metadata(path) {
                        Ok(s) => s,
                        Err(e) => {
                            println!("{indent_space}[!] error: {}", e);
                            return false;
                        }
                    };
                    let filesize_str = format!("{:>6} ", FileSize(meta.len()).colored());
                    println!("{filesize_str}{}", path);
                    return true;
                }
                _ => {}
            }

            println!("{indent_space}[!] error: {}", e);
            return false;
        }
    };

    if print_parent {
        println!("{}:", path);
    }

    let entries = dir.collect::<Result<Vec<_>, _>>();

    match entries {
        Ok(mut entries) => {
            if entries.is_empty() {
                println!("{indent_space}[!] empty directory",);
                return true;
            }

            entries.sort_unstable_by(|a, b| {
                let a_is_dir = a.file_type().unwrap().is_dir();
                let b_is_dir = b.file_type().unwrap().is_dir();
                if a_is_dir && !b_is_dir {
                    return std::cmp::Ordering::Less;
                }
                if !a_is_dir && b_is_dir {
                    return std::cmp::Ordering::Greater;
                }
                a.file_name().cmp(&b.file_name())
            });

            for entry in entries {
                let filesize = FileSize(entry.metadata().unwrap().len());
                let is_dir = entry.file_type().unwrap().is_dir();
                // the empty color doesn't matter
                let dir_slash = if is_dir {
                    "/".bright_blue()
                } else {
                    "".white()
                };
                let filename = if is_dir {
                    entry.file_name().to_string_lossy().bright_blue()
                } else {
                    entry.file_name().to_string_lossy().white()
                };
                let filesize_str = if is_dir {
                    format!("{:>6}", "-".bright_blue())
                } else {
                    format!("{:>6}", filesize.colored())
                };
                println!("{indent_space}{filesize_str} {filename}{dir_slash}",);
            }
        }
        Err(e) => {
            println!("{indent_space}[!] error: {}", e);
        }
    }

    true
}

fn main() -> ExitCode {
    SHOULD_COLORIZE.set_override(true);
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        ls(".", false);
        return ExitCode::SUCCESS;
    }

    let print_parent = args.len() > 2;

    let mut res = true;
    for path in args.iter().skip(1) {
        res &= ls(path, print_parent);
    }

    if res {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
