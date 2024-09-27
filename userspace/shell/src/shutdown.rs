use std::process::{exit, ExitCode};

fn main() -> ExitCode {
    emerald_runtime::power::shutdown().unwrap_or_else(|e| {
        eprintln!("[!] error: {}", e);
        exit(1); // TODO: replace with ExitCode::FAILURE
    });

    println!("[*] system is shutting down?");

    ExitCode::SUCCESS
}
