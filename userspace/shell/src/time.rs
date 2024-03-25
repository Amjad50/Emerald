//! Time shell program
//!
//! Usage: time <exe> [args...]

use std::{
    process::{exit, ExitCode},
    time::Instant,
};

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        println!("Usage: {} <exe> [args...]", args[0]);
        return ExitCode::FAILURE;
    }

    let exe = args[1].as_str();
    let args = &args[2..];

    let start = Instant::now();

    let status = std::process::Command::new(exe)
        .args(args)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("[!] error: {}", e);
            exit(1); // TODO: replace with ExitCode::FAILURE
        });

    let elapsed = start.elapsed();

    if status.success() {
        println!("Execution succeeded in {:?}", elapsed);
        ExitCode::SUCCESS
    } else {
        println!(
            "Execution failed in {:?} with code: {:?}",
            elapsed,
            status.code()
        );
        ExitCode::FAILURE
    }
}
