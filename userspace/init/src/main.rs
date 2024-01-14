//! `Init`
//!
//! This is the initialization user process
//!
//! For now it only keeps a shell running
//!
//! It acts as a wrapper, where it takes stdin stream as bytes,
//! and forwards it to the shell making sure to render them on each button press

use std::{
    fs::File,
    io::{Read, Write},
    os::amjad_os::io::{FromRawFd, OwnedFd},
    process::{Command, Stdio},
};

fn main() {
    let owned_stdin = unsafe { OwnedFd::from_raw_fd(0) };
    owned_stdin.set_nonblocking(true).unwrap();
    let mut stdin_file = File::from(owned_stdin);

    loop {
        let mut child = Command::new("/shell")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let child_pid = child.id();

        let mut child_stdin = child.stdin.take().unwrap();

        // running busy loop
        let mut line_buffer = Vec::new();
        let res = 'outer: loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            let mut buf = [0u8; 1];
            let mut counter = 0;
            while stdin_file.read(&mut buf).unwrap() == 0 {
                counter += 1;
                if counter > 20 {
                    // break outer, so that we can check the status of the child
                    continue 'outer;
                }
                core::hint::spin_loop();
            }
            // also output to our stdout
            std::io::stdout().write_all(&buf).unwrap();
            std::io::stdout().flush().unwrap();
            line_buffer.push(buf[0]);

            if buf[0] == b'\n' {
                if child_stdin.write_all(&line_buffer).is_err() {
                    // pipe closed, the process must have stopped, go back and check
                    // so that we can get the exit status
                    continue;
                }
                line_buffer.clear();
            }
        };

        println!("\n[init] child {} exited with {}", child_pid, res);
    }
}
