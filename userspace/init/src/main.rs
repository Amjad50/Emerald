#![feature(restricted_std)]

fn main() {
    // we are in `init` now
    println!("[init] Hello!\n\n");

    let result = std::process::Command::new("/shell").spawn().unwrap();

    println!("[init] spawned shell with pid {}\n", result.id());
}
