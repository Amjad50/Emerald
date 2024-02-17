// implement print! and println! macros
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::io::_print(format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

// implement eprint! and eprintln! macros
#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {
        $crate::io::_eprint(format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! eprintln {
    () => ($crate::eprint!("\n"));
    ($($arg:tt)*) => ($crate::eprint!("{}\n", format_args!($($arg)*)));
}

// copied from `std`
#[macro_export]
macro_rules! dbg {
    // NOTE: We cannot use `concat!` to make a static string as a format argument
    // of `eprintln!` because `file!` could contain a `{` or
    // `$val` expression could be a block (`{ .. }`), in which case the `eprintln!`
    // will be malformed.
    () => {
        $crate::println!("[{}:{}:{}]", ::core::file!(), ::core::line!(), ::core::column!())
    };
    ($val:expr $(,)?) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                $crate::println!("[{}:{}:{}] {} = {:#?}",
                    ::core::file!(), ::core::line!(), ::core::column!(), ::core::stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}

// same as above, but for eprintln!
#[macro_export]
macro_rules! edbg {
    () => {
        $crate::eprintln!("[{}:{}:{}]", ::core::file!(), ::core::line!(), ::core::column!())
    };
    ($val:expr $(,)?) => {
        match $val {
            tmp => {
                $crate::eprintln!("[{}:{}:{}] {} = {:#?}",
                    ::core::file!(), ::core::line!(), ::core::column!(), ::core::stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}
