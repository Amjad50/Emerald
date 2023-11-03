use core::{
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

/// A spin lock
pub(super) struct Lock {
    locked: AtomicBool,
}

impl Lock {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) {
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            hint::spin_loop();
        }
    }

    pub fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}
