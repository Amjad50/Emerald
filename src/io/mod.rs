pub mod console;
mod uart;
mod video_memory;

pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;

    let con = unsafe { console::CONSOLE.lock() };

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
