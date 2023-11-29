use core::{fmt, ops, sync::atomic::AtomicBool};

pub mod console;
mod keyboard;
mod uart;
mod video_memory;

static PRINT_ERR: AtomicBool = AtomicBool::new(false);

// This is a wrapper around a type that implements Debug, but we don't want to print it
// its kinda like using some libraries that allow you to disable debug for some fields
#[repr(transparent)]
pub struct NoDebug<T>(pub T);

impl<T> fmt::Debug for NoDebug<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[no_debug]")
    }
}

impl<T> Clone for NoDebug<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        NoDebug(self.0.clone())
    }
}
impl<T> Copy for NoDebug<T> where T: Copy {}

impl<T> ops::Deref for NoDebug<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> ops::DerefMut for NoDebug<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[allow(dead_code)]
pub fn hexdump(buf: &[u8]) {
    // lock first so that none else can access the console
    // its ReMutex so we can aquire the lock
    let _con = console::CONSOLE.lock();

    // print hex dump
    for i in 0..buf.len() / 16 {
        print!("{:08X}:  ", i * 16);
        for j in 0..16 {
            print!("{:02X} ", buf[i * 16 + j]);
        }
        // print ascii
        print!("  ");
        for j in 0..16 {
            let c = buf[i * 16 + j];
            if (32..127).contains(&c) {
                print!("{}", c as char);
            } else {
                print!(".");
            }
        }
        println!();
    }
    // print remaining if any
    let remaining = buf.len() % 16;
    if remaining != 0 {
        let remaining_start = (buf.len() / 16) * 16;

        print!("{:08X}:  ", remaining_start);
        for c in buf[remaining_start..].iter() {
            print!("{:02X} ", c);
        }
        // print ascii
        print!("  ");
        for &c in buf[remaining_start..].iter() {
            if (32..127).contains(&c) {
                print!("{}", c as char);
            } else {
                print!(".");
            }
        }
        println!();
    }
}

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
