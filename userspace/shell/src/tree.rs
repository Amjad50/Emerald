//! Tree shell program
//!
//! Usage: tree [paths...]

#![feature(io_error_more)]

use std::{
    path::Path,
    process::{exit, ExitCode},
};

use colored::{control::SHOULD_COLORIZE, Colorize};

struct Counter {
    files: usize,
    dirs: usize,
}

impl Counter {
    fn new() -> Self {
        Self { files: 0, dirs: 0 }
    }

    fn inc_file(&mut self) {
        self.files += 1;
    }

    fn inc_dir(&mut self) {
        self.dirs += 1;
    }
}

fn tree<P: AsRef<Path>>(path: P, depth: Option<usize>, counter: &mut Counter) {
    fn inner_tree(path: &Path, depth: usize, padding: &str, counter: &mut Counter) {
        if depth == 0 {
            return;
        }

        let entries = match path.read_dir() {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("[!] error: {}", e);
                return;
            }
        };

        let entries = entries.filter_map(|e| e.ok()).collect::<Vec<_>>();

        for (i, entry) in entries.iter().enumerate() {
            let last = i == entries.len() - 1;
            let is_dir = entry.metadata().unwrap().is_dir();
            let name = entry
                .file_name()
                .into_string()
                .unwrap_or_else(|_| String::from("<invalid>"));
            if name == "." || name == ".." {
                continue;
            }
            let name = if is_dir {
                name.bold().blue()
            } else {
                name.normal()
            };

            println!("{}{}{}", padding, if last { "+-- " } else { "|-- " }, name);
            if is_dir {
                counter.inc_dir();
                if last {
                    inner_tree(
                        &entry.path(),
                        depth - 1,
                        &format!("{}    ", padding),
                        counter,
                    );
                } else {
                    inner_tree(
                        &entry.path(),
                        depth - 1,
                        &format!("{}|   ", padding),
                        counter,
                    );
                }
            } else {
                counter.inc_file();
            }
        }
    }

    let path = path.as_ref();
    let depth = depth.unwrap_or(usize::MAX);

    if path.is_dir() {
        counter.inc_dir();
        println!("{}", path.display().to_string().bold().blue());
        inner_tree(path, depth, "", counter);
    } else {
        counter.inc_file();
        println!("{}", path.display().to_string().normal());
    }
}

fn main() -> ExitCode {
    SHOULD_COLORIZE.set_override(true);
    let args = std::env::args().collect::<Vec<_>>();

    let mut counter = Counter::new();

    let mut paths = Vec::new();
    let mut depth = None;

    if args.len() < 2 {
        paths.push(".");
    }

    let mut iter = args.iter().skip(1);

    while let Some(arg) = iter.next() {
        if arg == "-d" {
            let arg = iter.next().unwrap_or_else(|| {
                eprintln!("missing argument for -d");
                exit(1); // TODO: replace with ExitCode::FAILURE
            });
            depth = Some(arg.parse::<usize>().unwrap_or_else(|_| {
                eprintln!("invalid argument for -d");
                exit(1); // TODO: replace with ExitCode::FAILURE
            }));
        } else if arg == "-h" {
            eprintln!("Usage: {} [-d <n>] [paths...]", args[0]);
            return ExitCode::SUCCESS;
        } else {
            paths.push(arg);
        }
    }

    for path in paths {
        tree(path, depth, &mut counter);
    }

    println!("\n{} directories, {} files", counter.dirs, counter.files);

    ExitCode::SUCCESS
}
