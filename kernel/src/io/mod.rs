use core::{fmt, sync::atomic::AtomicBool};

pub mod console;
mod uart;

static PRINT_ERR: AtomicBool = AtomicBool::new(false);

macro_rules! impl_copy_clone_deref_wrapper {
    ($new_type:tt <$generic:tt>) => {
        impl<$generic> ::core::clone::Clone for $new_type<$generic>
        where
            $generic: ::core::clone::Clone,
        {
            fn clone(&self) -> Self {
                $new_type(self.0.clone())
            }
        }

        impl<$generic> ::core::marker::Copy for $new_type<$generic> where
            $generic: ::core::marker::Copy
        {
        }

        impl<$generic> ::core::ops::Deref for $new_type<$generic> {
            type Target = $generic;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<$generic> ::core::ops::DerefMut for $new_type<$generic> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }
    };
}

// This is a wrapper around a type that implements Debug, but we don't want to print it
// its kinda like using some libraries that allow you to disable debug for some fields
#[repr(transparent)]
pub struct NoDebug<T>(pub T);

impl<T> fmt::Debug for NoDebug<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[no_debug]")
    }
}

impl_copy_clone_deref_wrapper!(NoDebug<T>);

// This is a wrapper around a arrays/vectors to make them display proper hex in 1 line
#[repr(transparent)]
pub struct HexArray<T>(pub T);

// a private trait to make the compiler happy about usage of `U` constriant
trait ArrayTrait {
    type Item;
    fn data(&self) -> &[Self::Item];
}

impl<T> ArrayTrait for T
where
    T: AsRef<[u8]>,
{
    type Item = u8;
    fn data(&self) -> &[Self::Item] {
        self.as_ref()
    }
}

impl<T, U> fmt::Debug for HexArray<T>
where
    T: ArrayTrait<Item = U>,
    U: fmt::UpperHex,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // write!(f, "[no_debug]")
        write!(f, "[")?;
        for (index, data) in self.0.data().iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            // for now we use 04 width, which won't work as expected for u16/u32/u64
            // but we don't use them for now
            write!(f, "{data:#04X}")?;
        }
        write!(f, "]")
    }
}

impl_copy_clone_deref_wrapper!(HexArray<T>);

// This is a wrapper around a byte array that allows us to print it as a string
#[repr(transparent)]
pub struct ByteStr<T>(pub T);

impl<T> fmt::Debug for ByteStr<T>
where
    T: AsRef<[u8]>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b\"")?;
        // display each char if printable, otherwise replace with \xXX
        for &c in self.0.as_ref().iter() {
            if c.is_ascii_graphic() || c == b' ' {
                write!(f, "{}", c as char)?;
            } else {
                write!(f, "\\x{:02X}", c)?;
            }
        }
        write!(f, "\"")
    }
}

impl_copy_clone_deref_wrapper!(ByteStr<T>);

#[allow(dead_code)]
pub fn hexdump(buf: &[u8]) {
    // lock first so that none else can access the console
    // its ReMutex so we can aquire the lock
    console::run_with_console(|inner| {
        // print hex dump
        for i in 0..buf.len() / 16 {
            write!(inner, "{:08X}:  ", i * 16)?;
            for j in 0..16 {
                write!(inner, "{:02X} ", buf[i * 16 + j])?;
            }
            // print ascii
            write!(inner, "  ")?;
            for j in 0..16 {
                let c = buf[i * 16 + j];
                if (32..127).contains(&c) {
                    write!(inner, "{}", c as char)?;
                } else {
                    write!(inner, ".")?;
                }
            }
            writeln!(inner)?;
        }
        // print remaining if any
        let remaining = buf.len() % 16;
        if remaining != 0 {
            let remaining_start = (buf.len() / 16) * 16;

            write!(inner, "{:08X}:  ", remaining_start)?;
            for c in buf[remaining_start..].iter() {
                write!(inner, "{:02X} ", c)?;
            }
            for _ in 0..(16 - remaining) {
                write!(inner, "   ")?;
            }
            // print ascii
            write!(inner, "  ")?;
            for &c in buf[remaining_start..].iter() {
                if (32..127).contains(&c) {
                    write!(inner, "{}", c as char)?;
                } else {
                    write!(inner, ".")?;
                }
            }
            writeln!(inner)?;
        }
        Ok::<(), fmt::Error>(())
    })
    .unwrap();
}

pub fn _print(args: ::core::fmt::Arguments) {
    console::run_with_console(|inner| inner.write_fmt(args)).unwrap();
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
