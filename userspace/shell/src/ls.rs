#![feature(io_error_more)]

use std::process::ExitCode;

/// Ls shell program
///
/// Usage: ls [paths...]

// helper function
// `print_parent`: indent more, this will be true if we are printing the parent directory
fn ls(path: &str, print_parent: bool) {
    let indent_space = if print_parent { "  " } else { "" };
    let mut dir = match std::fs::read_dir(path) {
        Ok(d) => d,
        Err(e) => {
            match e.kind() {
                std::io::ErrorKind::NotFound => {
                    println!("{indent_space}[!] path not found: {}", path);
                    return;
                }
                std::io::ErrorKind::NotADirectory => {
                    println!("  {}", path);
                    return;
                }
                _ => {}
            }

            println!("{indent_space}[!] error: {}", e);
            return;
        }
    };

    if print_parent {
        println!("  {}:", path);
    }

    loop {
        match dir.next() {
            Some(Ok(entry)) => {
                let dir_slash = if entry.file_type().unwrap().is_dir() {
                    "/"
                } else {
                    ""
                };
                println!(
                    "  {indent_space}{}{dir_slash}",
                    entry.file_name().to_string_lossy()
                );
            }
            Some(Err(e)) => {
                println!("{indent_space}[!] error: {}", e);
            }
            None => break,
        }
    }
}

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        ls(".", false);
        return ExitCode::SUCCESS;
    }

    let print_parent = args.len() > 2;

    for path in args.iter().skip(1) {
        ls(path, print_parent);
    }

    ExitCode::SUCCESS
}
