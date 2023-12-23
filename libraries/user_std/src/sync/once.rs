use core::{
    cell::UnsafeCell,
    fmt,
    marker::PhantomData,
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
};

const ONCE_STATE_INIT: usize = 0;
const ONCE_STATE_RUNNING: usize = 1;
const ONCE_STATE_DONE: usize = 2;

struct Once {
    state: AtomicUsize,
}

impl Once {
    pub const fn new() -> Self {
        Once {
            state: AtomicUsize::new(ONCE_STATE_INIT),
        }
    }

    fn is_completed(&self) -> bool {
        self.state.load(Ordering::Relaxed) == ONCE_STATE_DONE
    }

    pub fn call(&self, f: impl FnOnce()) {
        let mut state = self.state.load(Ordering::Acquire);
        loop {
            match state {
                ONCE_STATE_INIT => {
                    // Try to transition to RUNNING state.
                    match self.state.compare_exchange_weak(
                        ONCE_STATE_INIT,
                        ONCE_STATE_RUNNING,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => {
                            // We've successfully transitioned to RUNNING state.
                            // Run the closure.
                            f();

                            // Transition to DONE state.
                            self.state.store(ONCE_STATE_DONE, Ordering::Release);
                            return;
                        }
                        Err(new) => {
                            // We failed to transition to RUNNING state.
                            state = new;
                            continue;
                        }
                    }
                }
                ONCE_STATE_RUNNING => {
                    panic!("Once::call already running");
                }
                ONCE_STATE_DONE => return,
                _ => unreachable!("state is never set to invalid values"),
            }
        }
    }
}

pub struct OnceLock<T> {
    once: Once,
    // Whether or not the value is initialized is tracked by `once.is_completed()`.
    value: UnsafeCell<MaybeUninit<T>>,
    /// `PhantomData` to make sure dropck understands we're dropping T in our Drop impl.
    ///
    /// ```compile_fail,E0597
    /// use std::sync::OnceLock;
    ///
    /// struct A<'a>(&'a str);
    ///
    /// impl<'a> Drop for A<'a> {
    ///     fn drop(&mut self) {}
    /// }
    ///
    /// let cell = OnceLock::new();
    /// {
    ///     let s = String::new();
    ///     let _ = cell.set(A(&s));
    /// }
    /// ```
    _marker: PhantomData<T>,
}

unsafe impl<T: Sync + Send> Sync for OnceLock<T> {}
unsafe impl<T: Send> Send for OnceLock<T> {}

impl Default for Once {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: fmt::Debug> fmt::Debug for OnceLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_tuple("OnceLock");
        match self.try_get() {
            Some(v) => d.field(v),
            None => d.field(&format_args!("<uninit>")),
        };
        d.finish()
    }
}

impl<T: Clone> Clone for OnceLock<T> {
    #[inline]
    fn clone(&self) -> OnceLock<T> {
        let cell = Self::new();
        if let Some(value) = self.try_get() {
            match cell.set(value.clone()) {
                Ok(()) => (),
                Err(_) => unreachable!(),
            }
        }
        cell
    }
}

#[allow(dead_code)]
impl<T> OnceLock<T> {
    pub const fn new() -> Self {
        OnceLock {
            once: Once::new(),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            _marker: PhantomData,
        }
    }

    pub fn set(&self, value: T) -> Result<(), T> {
        if self.is_completed() {
            return Err(value);
        }
        self.init(|| Ok(value))
    }

    pub fn get(&self) -> &T {
        if self.once.is_completed() {
            unsafe { self.get_unchecked() }
        } else {
            panic!("OnceLock::get called before OnceLock::set");
        }
    }

    pub fn try_get(&self) -> Option<&T> {
        if self.once.is_completed() {
            Some(unsafe { self.get_unchecked() })
        } else {
            None
        }
    }

    pub fn get_or_init<F>(&self, f: F) -> &T
    where
        F: FnOnce() -> T,
    {
        match self.get_or_try_init(|| Ok::<T, ()>(f())) {
            Ok(val) => val,
            Err(_) => panic!(),
        }
    }

    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        // Fast path check
        // NOTE: We need to perform an acquire on the state in this method
        // in order to correctly synchronize `LazyLock::force`. This is
        // currently done by calling `self.get()`, which in turn calls
        // `self.is_initialized()`, which in turn performs the acquire.
        if let Some(value) = self.try_get() {
            return Ok(value);
        }
        self.init(f)?;

        debug_assert!(self.is_completed());

        // SAFETY: The inner value has been initialized
        Ok(unsafe { self.get_unchecked() })
    }

    #[cold]
    fn init<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let mut res: Result<(), E> = Ok(());
        let slot = &self.value;

        // Ignore poisoning from other threads
        // If another thread panics, then we'll be able to run our closure
        self.once.call(|| match f() {
            Ok(value) => {
                unsafe { (*slot.get()).write(value) };
            }
            Err(e) => {
                res = Err(e);
            }
        });
        res
    }

    unsafe fn get_unchecked(&self) -> &T {
        debug_assert!(self.once.is_completed());
        (*self.value.get()).assume_init_ref()
    }

    fn is_completed(&self) -> bool {
        self.once.is_completed()
    }
}

impl<T> Drop for OnceLock<T> {
    fn drop(&mut self) {
        if self.once.is_completed() {
            unsafe {
                (*self.value.get()).assume_init_drop();
            }
        }
    }
}
