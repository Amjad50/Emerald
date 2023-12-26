#![feature(restricted_std)]

use std::{io::Read, string::String};

fn main() {
    // we are in `init` now
    // create some delay
    println!("[shell] Hello!\n\n");

    // open `/message.txt` and print the result

    println!("[shell] content of `/message.txt`:\n");
    let mut f = std::fs::File::open("/message.txt").unwrap();
    let mut buf = [0; 100];
    f.read(&mut buf).unwrap();
    println!("{}", String::from_utf8_lossy(&buf));
}
