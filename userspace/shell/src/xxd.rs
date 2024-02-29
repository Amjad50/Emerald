use std::{
    io::{Read, Seek},
    process::{exit, ExitCode},
};

/// XXD shell program, prints the hexdump of a file
///
/// Usage: xxd [file]

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        println!("Usage: {} [-l <n>] [-s <n>] [file]", args[0]);
        return ExitCode::FAILURE;
    }

    let mut file = None;
    let mut limit = None;
    let mut skip = None;

    let mut iter = args.iter().skip(1);

    while let Some(arg) = iter.next() {
        if arg == "-l" {
            let arg = iter.next().unwrap_or_else(|| {
                println!("missing argument for -l");
                exit(1); // TODO: replace with ExitCode::FAILURE
            });
            limit = Some(arg.parse::<usize>().unwrap_or_else(|_| {
                println!("invalid argument for -l");
                exit(1); // TODO: replace with ExitCode::FAILURE
            }));
        } else if arg == "-s" {
            let arg = iter.next().unwrap_or_else(|| {
                println!("missing argument for -s");
                exit(1); // TODO: replace with ExitCode::FAILURE
            });
            skip = Some(arg.parse::<usize>().unwrap_or_else(|_| {
                println!("invalid argument for -s");
                exit(1); // TODO: replace with ExitCode::FAILURE
            }));
        } else {
            file = Some(arg);
        }
    }

    let file = file.unwrap_or_else(|| {
        println!("missing file argument");
        exit(1); // TODO: replace with ExitCode::FAILURE
    });

    let mut file = match std::fs::File::open(file) {
        Ok(f) => f,
        Err(e) => {
            println!("[!] error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    if let Some(skip) = skip {
        if let Err(e) = file.seek(std::io::SeekFrom::Start(skip as u64)) {
            println!("[!] error: {}", e);
            return ExitCode::FAILURE;
        }
    }

    let mut buf = [0u8; 1024];
    let mut last_16 = [0u8; 16];
    let mut offset = 0;
    'outer: loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for &c in buf.iter().take(n) {
                    last_16[offset % 16] = c;

                    if offset % 16 == 0 {
                        print!("{:08x}: ", offset);
                    }

                    print!("{:02x}", c);

                    if offset % 2 == 1 {
                        print!(" "); // space separating each 2 bytes
                    }

                    if offset % 16 == 15 {
                        print!(" "); // more space between the hex and ascii
                        for &c in last_16.iter() {
                            if (0x20..=0x7e).contains(&c) {
                                print!("{}", c as char);
                            } else {
                                print!(".");
                            }
                        }
                        println!();
                    }
                    offset += 1;

                    if let Some(limit) = limit {
                        if offset >= limit {
                            break 'outer;
                        }
                    }
                }
            }
            Err(e) => {
                println!("[!] error: {}", e);
                return ExitCode::FAILURE;
            }
        }
    }
    // print the last line
    println!();

    ExitCode::SUCCESS
}
