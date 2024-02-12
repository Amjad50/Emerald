use core::{
    hint,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::sync::cache_padded::CachePadded;

/// A raw spin lock, only provides `lock` and `unlock` and waiting for the lock
///
/// This is an unsafe lock, it doesn't have any protection against deadlocks, or multiple locking
/// A safe wrappers are implemented with `Mutex` and `ReMutex`
pub(super) struct Lock {
    locked: CachePadded<AtomicBool>,
}

impl Lock {
    pub const fn new() -> Self {
        Self {
            locked: CachePadded::new(AtomicBool::new(false)),
        }
    }

    pub fn lock(&self) {
        // only try to lock once, then loop until we can, then try again
        // this reduces `cache exclusion` and improve performance
        while !self.try_lock() {
            while self.locked.load(Ordering::Relaxed) {
                hint::spin_loop();
            }
        }
    }

    #[must_use]
    #[inline(always)]
    /// Try to lock the lock, returns true if successful
    pub fn try_lock(&self) -> bool {
        self.locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// SAFETY: the caller must assure that there is only one accessor for this lock
    ///         we don't want multiple unlocks, it doesn't make sense for this Lock (check `super::remutex::ReMutex`)
    pub unsafe fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}
