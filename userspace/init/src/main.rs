#![feature(restricted_std)]

use std::process::Stdio;

fn main() {
    // we are in `init` now
    println!("[init] Hello!\n\n");

    let result = std::process::Command::new("/shell")
        .stderr(Stdio::null())
        .output()
        .unwrap();
    println!(
        "[init] shell output: {}",
        String::from_utf8_lossy(&result.stdout)
    );
}
