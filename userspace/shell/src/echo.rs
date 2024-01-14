use std::process::ExitCode;

/// Echo shell program
///
/// Usage: echo [string]

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        println!("Usage: {} [string]", args[0]);
        return ExitCode::FAILURE;
    }

    // output directly to stdout all args
    for arg in &args[1..] {
        print!("{} ", arg);
    }
    println!();

    ExitCode::SUCCESS
}
