use std::{
    borrow::Cow,
    fs,
    io::{self, Write},
    path::Path,
    process::{Command, Stdio},
    string::String,
};

use colored::Colorize;

/// This was generated with `jp2a logo.png --width=50 --color-depth=4`, and modified later with `moebius`.
const ANSI_LOGO: &str = include_str!("../logo.ans");
const MAX_WIDTH: usize = 50;
/// Banner to show under the logo
const BANNER: &str = "Emerald OS";
const PADDING: usize = (MAX_WIDTH - BANNER.len()) / 2;

/// Print the shell logo
fn print_logo_with_name() {
    println!("{}", ANSI_LOGO);
    // would love to get this at compile time, so we don't allocate, but its not called a lot, so should be fine.
    let padding = " ".repeat(PADDING);
    println!("{}{}\n", padding, BANNER.bright_green());
}

/// Return `true` if we are the one handling this command, otherwise return `false`
/// so that the command is executed as a normal process.
fn handle_internal_cmds(cmd: &str, args: &[&str]) -> bool {
    match cmd {
        "exit" => {
            println!("Goodbye!");
            std::process::exit(0);
        }
        "cd" => {
            if args.is_empty() {
                eprintln!("cd: missing argument");
            } else {
                let path = args[0];
                match std::env::set_current_dir(path) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("cd: {e}");
                    }
                }
            }
        }
        "pwd" => {
            println!("{}", std::env::current_dir().unwrap().display());
        }
        "sleep" => {
            if args.is_empty() {
                eprintln!("sleep: missing operand");
            } else {
                let seconds = args[0].parse::<u64>();
                match seconds {
                    Ok(seconds) => std::thread::sleep(std::time::Duration::from_secs(seconds)),
                    Err(e) => {
                        eprintln!("sleep: invalid time interval `{}`, e: {e}", args[0])
                    }
                }
            }
        }
        "touch" => {
            if args.is_empty() {
                eprintln!("touch: missing operand");
            } else {
                let path = args[0];
                let file = fs::OpenOptions::new()
                    .truncate(true)
                    .create(true)
                    .write(true)
                    .open(path);
                match file {
                    Ok(_) => {}
                    Err(e) => {
                        println!("touch: {e}");
                    }
                }
            }
        }
        _ => return false,
    }

    true
}

fn main() {
    let mut old_result = None;

    print_logo_with_name();

    loop {
        if let Some(result) = old_result.take() {
            let result_str = format!("({})", result);
            if result == 0 {
                print!("{} ", result_str.green());
            } else {
                print!("{} ", result_str.red());
            }
        }
        print!("{}", "$ ".bright_blue());
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let input = input.trim();

        // try to see if there is file redirection
        let redirect_pos = input.find('>');

        let (input, out_file) = match redirect_pos {
            Some(pos) => {
                let (input, mut out_file) = input.split_at(pos);
                let mut is_append = false;

                if out_file.starts_with(">>") {
                    // this is >>, so we need to append
                    is_append = true;
                    out_file = &out_file[2..];

                    if out_file.starts_with('>') {
                        eprintln!("invalid operator >>>, use > or >>");
                        continue;
                    }
                } else {
                    out_file = &out_file[1..];
                }

                out_file = out_file.trim();

                // make sure `out_file` is a path and not empty
                if out_file.is_empty() {
                    eprintln!("missing output file");
                    continue;
                }

                if out_file.starts_with('"') {
                    // take until end quote
                    if let Some(end_quote) = out_file.find('"') {
                        if &out_file[end_quote - 1..end_quote] == "\\" {
                            eprintln!("can't have file path with quote escaped");
                            continue;
                        }
                        out_file = &out_file[1..end_quote];
                    } else {
                        eprintln!("missing end quote");
                        continue;
                    }
                } else {
                    // must not contain any whitespace
                    if out_file.contains(char::is_whitespace) {
                        eprintln!("invalid output file, can't have whitespace");
                        continue;
                    }
                }

                let mut open_options = fs::OpenOptions::new();
                open_options.write(true).create(true);
                if is_append {
                    open_options.append(true);
                } else {
                    open_options.truncate(true);
                }
                let file = match open_options.open(out_file) {
                    Ok(file) => file,
                    Err(e) => {
                        eprintln!("error creating out file: {e}");
                        continue;
                    }
                };

                (input.trim(), Some(file))
            }
            None => (input, None),
        };

        let args = input.split_whitespace().collect::<Vec<_>>();

        if args.is_empty() {
            continue;
        }

        let cmd = args[0];
        let remaining_args = &args[1..];

        // handle internal commands
        if handle_internal_cmds(cmd, remaining_args) {
            continue;
        }

        // if this cmd exist in the current directory, use it
        // otherwise, use the root
        let cmd_path: Cow<'_, str> = if Path::new(cmd).exists() {
            cmd.into()
        } else {
            format!("/{}", cmd).into()
        };

        let stdout = if let Some(file) = out_file {
            Stdio::from(file)
        } else {
            Stdio::inherit()
        };

        let result = match Command::new(cmd_path.as_ref())
            .stdout(stdout)
            .args(remaining_args)
            .spawn()
        {
            Ok(mut proc) => proc.wait().unwrap(),
            Err(e) => match e.kind() {
                io::ErrorKind::NotFound => {
                    eprintln!("[!] command not found: {cmd}");
                    old_result = Some(0x7F);
                    continue;
                }
                _ => {
                    eprintln!("[!] error: {e}");
                    old_result = Some(0x7F);
                    continue;
                }
            },
        };

        old_result = Some(result.code().unwrap());
    }
}
