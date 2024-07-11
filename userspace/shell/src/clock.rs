//! Clock shell program
//!
//! Just print the clock incrementing every second until it is killed
//! or a specific time is specified in the argument
//!
//! Usage: clock [seconds_to_run]

use std::{
    process::{exit, ExitCode},
    time::{Instant, SystemTime},
};

use chrono::{DateTime, Utc};

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();

    let seconds_to_run = if args.len() > 1 {
        if args[1] == "help" || args[1] == "-h" || args[1] == "--help" {
            println!("Usage: clock [-w seconds_to_run]");
            return ExitCode::SUCCESS;
        }

        if args[1] != "-w" {
            eprintln!("[!] error: unknown argument: {}", args[1]);
            println!("Usage: clock [-w seconds_to_run]");

            return ExitCode::FAILURE;
        }

        if args.len() < 3 {
            eprintln!("[!] error: missing argument");
            println!("Usage: clock [-w seconds_to_run]");
            return ExitCode::FAILURE;
        }

        args[2].parse::<u64>().map(Some).unwrap_or_else(|e| {
            eprintln!("[!] error: {}", e);
            exit(1); // TODO: replace with ExitCode::FAILURE
        })
    } else {
        None
    };

    let mut start = Instant::now();
    let mut seconds_passed = 0;
    println!();

    loop {
        let now = Instant::now();
        let elapsed = now - start;
        if elapsed.as_secs() >= 1 {
            seconds_passed += 1;

            start = now;

            let system_time = SystemTime::now();
            let datetime: DateTime<Utc> = system_time.into();
            println!("{}", datetime.format("%d/%m/%Y %T.%f (%s)"));

            if let Some(seconds_to_run) = seconds_to_run {
                if seconds_passed >= seconds_to_run {
                    break;
                }
            } else {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    ExitCode::SUCCESS
}
