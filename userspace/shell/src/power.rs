use std::process::{exit, ExitCode};

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        eprintln!("Usage: {} [shutdown|reboot]", args[0]);
        return ExitCode::FAILURE;
    }

    let cmd = match args[1].as_str() {
        "shutdown" => emerald_runtime::power::PowerCommand::Shutdown,
        "reboot" => emerald_runtime::power::PowerCommand::Reboot,
        _ => {
            eprintln!("Invalid command: {}", args[1]);
            eprintln!("Usage: {} [shutdown|reboot]", args[0]);
            return ExitCode::FAILURE;
        }
    };

    cmd.run().unwrap_or_else(|e| {
        eprintln!("[!] error: {}", e);
        exit(1); // TODO: replace with ExitCode::FAILURE
    });

    println!("[*] system is shutting down?");

    ExitCode::SUCCESS
}
