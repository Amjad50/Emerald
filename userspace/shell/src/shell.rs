use std::{
    borrow::Cow,
    io::{self, Write},
    path::Path,
    process::Command,
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
                println!("cd: missing argument");
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
                println!("sleep: missing operand");
            } else {
                let seconds = args[0].parse::<u64>();
                match seconds {
                    Ok(seconds) => std::thread::sleep(std::time::Duration::from_secs(seconds)),
                    Err(e) => {
                        println!("sleep: invalid time interval `{}`, e: {e}", args[0])
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

        // if this cmd exsist in the current directory, use it
        // otherwise, use the root
        let cmd_path: Cow<'_, str> = if Path::new(cmd).exists() {
            cmd.into()
        } else {
            format!("/{}", cmd).into()
        };

        let result = match Command::new(cmd_path.as_ref()).args(remaining_args).spawn() {
            Ok(mut proc) => proc.wait().unwrap(),
            Err(e) => match e.kind() {
                io::ErrorKind::NotFound => {
                    println!("[!] command not found: {cmd}");
                    old_result = Some(0x7F);
                    continue;
                }
                _ => {
                    println!("[!] error: {e}");
                    old_result = Some(0x7F);
                    continue;
                }
            },
        };

        old_result = Some(result.code().unwrap());
    }
}
