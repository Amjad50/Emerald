#![feature(restricted_std)]

/// Echo shell program
///
/// Usage: echo [string]

fn main() {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        println!("Usage: {} [string]", args[0]);
        return;
    }

    // output directly to stdout all args
    for arg in &args[1..] {
        print!("{} ", arg);
    }
    println!();
}
