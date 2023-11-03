pub mod console;
mod uart;
mod video_memory;

pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;
    unsafe { console::CONSOLE.write_fmt(args).unwrap() };
}

