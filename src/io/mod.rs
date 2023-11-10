use core::sync::atomic::AtomicBool;

pub mod console;
mod keyboard;
mod uart;
mod video_memory;

static PRINT_ERR: AtomicBool = AtomicBool::new(false);

pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;

    let con = console::CONSOLE.lock();

    // if we failed to borrow, it means we are inside panic, and we have paniced inside the lock/console
    // create a new raw console and print to it
    // no one else should be able to access the console, since we are holding the lock
    match con.try_borrow_mut() {
        Ok(mut con) => {
            con.write_fmt(args).unwrap();
        }
        Err(_) => {
            // SAFETY: we are creating a new console, and we are holding the lock
            //         so this acts as a form of locking
            let mut console = unsafe { console::Console::empty() };
            console.init();
            console.write_fmt(args).unwrap();
        }
    }
    drop(con);
}

pub fn read_chars(buf: &mut [u8]) -> usize {
    let con = console::CONSOLE.lock();
    let mut con = con.borrow_mut();
    con.read(buf)
}

pub fn write_chars(buf: &[u8]) {
    let con = console::CONSOLE.lock();
    let mut con = con.borrow_mut();
    con.write(buf);
}

// Enable `eprint!` and `eprintln!` macros
// sort of toggleable logging
#[allow(dead_code)]
pub fn set_err_enable(enable: bool) {
    PRINT_ERR.store(enable, core::sync::atomic::Ordering::Release);
}

pub fn _eprint(args: ::core::fmt::Arguments) {
    if PRINT_ERR.load(core::sync::atomic::Ordering::Acquire) {
        _print(args);
    }
}
