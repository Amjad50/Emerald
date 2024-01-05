#![feature(restricted_std)]

use std::{
    io::{self, Write},
    process::Command,
    string::String,
};

fn main() {
    let mut old_result = None;

    loop {
        if let Some(result) = old_result.take() {
            print!("{result} ");
        }
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let input = input.trim();
        let args = input.split_whitespace().collect::<Vec<_>>();

        if args.len() == 0 {
            continue;
        }

        let cmd = args[0];
        // TODO: add support for relative paths and other stuff, for now we must specify the full path
        let cmd_path = format!("/{cmd}");
        let remaining_args = &args[1..];

        let result = match Command::new(cmd_path).args(remaining_args).spawn() {
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
