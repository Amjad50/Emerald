#![feature(restricted_std)]

use std::process::Command;

fn main() {
    // we are in `init` now
    println!("[init] Hello!\n\n");

    loop {
        let mut child = Command::new("/shell").spawn().unwrap();
        let child_pid = child.id();

        let res = child.wait().unwrap();

        println!("\n[init] child {} exited with {}", child_pid, res);
    }
}
