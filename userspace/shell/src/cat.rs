use std::{io::Read, process::ExitCode};

/// Cat shell program
///
/// Usage: cat [file]

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        println!("Usage: {} [file]", args[0]);
        return ExitCode::FAILURE;
    }

    let file = &args[1];

    let mut file = match std::fs::File::open(file) {
        Ok(f) => f,
        Err(e) => {
            println!("[!] error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let mut buf = [0u8; 1024];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let s = std::str::from_utf8(&buf[..n]).unwrap();
                print!("{}", s);
            }
            Err(e) => {
                println!("[!] error: {}", e);
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}
