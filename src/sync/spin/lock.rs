use core::{
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

/// A spin lock
///
/// This is an unsafe lock, it doesn't have any protection against deadlocks, or multiple locking
/// A safe wrappers are implemented with `Mutex` and `ReMutex`
pub(super) struct Lock {
    locked: AtomicBool,
}

impl Lock {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    /// SAFETY: the caller must assure that there is only one accessor for this lock
    pub unsafe fn lock(&self) {
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            hint::spin_loop();
        }
    }

    /// SAFETY: the caller must assure that there is only one accessor for this lock
    pub unsafe fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}
