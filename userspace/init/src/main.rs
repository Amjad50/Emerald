#![feature(restricted_std)]

use std::{
    io::{Read, Write},
    process::Stdio,
};

fn main() {
    // we are in `init` now
    println!("[init] Hello!\n\n");

    let result = std::process::Command::new("/shell")
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    println!("[init] spawned shell with pid {}\n", result.id());

    result
        .stdin
        .unwrap()
        .write_all(b"echo Hello from init!")
        .unwrap();

    let mut stdout = Vec::new();
    result.stdout.unwrap().read_to_end(&mut stdout).unwrap();

    let output = String::from_utf8(stdout).unwrap();
    println!("[init] shell output: {}", output);
}
