use core::sync::atomic::{AtomicU64, Ordering};

use crate::sync::cache_padded::CachePadded;

// this is taken from `rust`, `futex_rwlock` in `unix` without the `futex`
// implementation, since we don't have that.
//
// Bits 0..30:
//   0: Unlocked
//   1..=0x3FFF_FFFE: Locked by N readers
//   0x3FFF_FFFF: Write locked
const UNLOCKED: u64 = 0;
const READ_LOCKED: u64 = 1;
const MASK: u64 = (1 << 30) - 1;
const WRITE_LOCKED: u64 = MASK;
const MAX_READERS: u64 = MASK - 1;

#[inline]
fn is_read_lockable(state: u64) -> bool {
    // This also returns false if the counter could overflow if we tried to read lock it.
    //
    // We don't allow read-locking if there's readers waiting, even if the lock is unlocked
    // and there's no writers waiting. The only situation when this happens is after unlocking,
    // at which point the unlocking thread might be waking up writers, which have priority over readers.
    // The unlocking thread will clear the readers waiting bit and wake up readers, if necessary.
    state & MASK < MAX_READERS
}

/// A raw spin lock, provides `read_lock`, `read_unlock`, `write_lock`, and `write_unlock`
///
/// This raw is designed for the broader case of `RwLock`, but with `write_lock`, `write_unlock` it
/// acts as a simple boolean look, that is used in Mutexes
///
/// This is an unsafe lock, it doesn't have any protection against deadlocks, or multiple locking
/// A safe wrappers are implemented with `Mutex`, `ReMutex`, and `RwLock`
pub(super) struct Lock {
    state: CachePadded<AtomicU64>,
}

impl Lock {
    pub const fn new() -> Self {
        Self {
            state: CachePadded::new(AtomicU64::new(UNLOCKED)),
        }
    }

    pub fn read_lock(&self) {
        while !self.try_read_lock() {
            loop {
                let state = self.state.load(Ordering::Relaxed);
                if is_read_lockable(state) {
                    break;
                }
                core::hint::spin_loop();
            }
        }
    }

    #[must_use]
    #[inline(always)]
    /// Try to lock the lock, returns true if successful
    pub fn try_read_lock(&self) -> bool {
        self.state
            .fetch_update(Ordering::Acquire, Ordering::Relaxed, |s| {
                is_read_lockable(s).then(|| s + READ_LOCKED)
            })
            .is_ok()
    }

    pub unsafe fn read_unlock(&self) {
        self.state.fetch_sub(READ_LOCKED, Ordering::Release);
    }

    pub fn write_lock(&self) {
        // only try to lock once, then loop until we can, then try again
        // this reduces `cache exclusion` and improve performance
        while !self.try_write_lock() {
            while self.state.load(Ordering::Relaxed) != UNLOCKED {
                core::hint::spin_loop();
            }
        }
    }

    #[must_use]
    #[inline(always)]
    /// Try to lock the lock, returns true if successful
    pub fn try_write_lock(&self) -> bool {
        self.state
            .compare_exchange_weak(UNLOCKED, WRITE_LOCKED, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// SAFETY: the caller must assure that there is only one accessor for this lock
    ///         we don't want multiple unlocks, it doesn't make sense for this Lock (check `super::remutex::ReMutex`)
    pub unsafe fn write_unlock(&self) {
        let state = self.state.fetch_sub(WRITE_LOCKED, Ordering::Release) - WRITE_LOCKED;
        assert_eq!(state, UNLOCKED);
    }
}
