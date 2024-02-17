use core::{
    cell::Cell,
    fmt,
    marker::PhantomData,
    ops::Deref,
    sync::atomic::{AtomicI64, Ordering},
};

use crate::cpu;

use super::lock;

/// A mutex that can be intered more than once by the same CPU
///
/// Only provides `Deref`, and not `DerefMut`, because the data
/// would then be mutated with inconsistent data.
/// Use `Cell` or `RefCell` to allow mutation.
pub struct ReMutex<T> {
    lock: lock::Lock,
    owner_cpu: AtomicI64,
    lock_count: Cell<usize>,
    data: T,
}

unsafe impl<T: Send> Send for ReMutex<T> {}
unsafe impl<T: Send> Sync for ReMutex<T> {}

impl<T> fmt::Debug for ReMutex<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("ReMutex");
        s.field("owner_cpu", &self.owner_cpu)
            .field("lock_count", &self.lock_count);
        if let Some(data) = self.try_lock() {
            s.field("data", &data);
        } else {
            s.field("data", &"[locked]");
        }
        s.finish()
    }
}

#[must_use]
pub struct ReMutexGuard<'a, T: 'a> {
    lock: &'a ReMutex<T>,
    marker: PhantomData<*const ()>, // !Send
}

impl<T> fmt::Debug for ReMutexGuard<'_, T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T> ReMutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: lock::Lock::new(),
            owner_cpu: AtomicI64::new(-1),
            lock_count: Cell::new(0),
            data,
        }
    }

    pub fn lock(&self) -> ReMutexGuard<T> {
        let cpu = cpu::cpu();
        cpu.push_cli(); // disable interrupts to avoid deadlock
        let cpu_id = cpu.id as i64;

        if self.owner_cpu.load(Ordering::Relaxed) == cpu_id {
            assert!(self.lock_count.get() > 0);
            assert!(cpu.n_cli() > 0 && cpu.interrupts_disabled());
            self.lock_count.set(
                self.lock_count
                    .get()
                    .checked_add(1)
                    .expect("ReMutex lock count overflow"),
            );
            ReMutexGuard {
                lock: self,
                marker: PhantomData,
            }
        } else {
            self.lock.write_lock();
            self.owner_cpu.store(cpu_id, Ordering::Relaxed);
            self.lock_count.set(1);
            ReMutexGuard {
                lock: self,
                marker: PhantomData,
            }
        }
    }

    pub fn try_lock(&self) -> Option<ReMutexGuard<T>> {
        let cpu = cpu::cpu();
        cpu.push_cli(); // disable interrupts to avoid deadlock
        let cpu_id = cpu.id as i64;

        if self.owner_cpu.load(Ordering::Relaxed) == cpu_id {
            assert!(self.lock_count.get() > 0);
            self.lock_count.set(
                self.lock_count
                    .get()
                    .checked_add(1)
                    .expect("ReMutex lock count overflow"),
            );
            Some(ReMutexGuard {
                lock: self,
                marker: PhantomData,
            })
        } else if self.lock.try_write_lock() {
            // already locked here
            self.owner_cpu.store(cpu_id, Ordering::Relaxed);
            self.lock_count.set(1);
            Some(ReMutexGuard {
                lock: self,
                marker: PhantomData,
            })
        } else {
            None
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<T> core::ops::Deref for ReMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.lock.data
    }
}

impl<T> Drop for ReMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.lock_count.set(
            self.lock
                .lock_count
                .get()
                .checked_sub(1)
                .expect("ReMutex lock count underflow"),
        );
        if self.lock.lock_count.get() == 0 {
            self.lock.owner_cpu.store(-1, Ordering::Relaxed);
            // SAFETY: the mutex is locked, we are the only accessor
            unsafe { self.lock.lock.write_unlock() };
        }
        // re-enable interrupts
        // (we have to do this for every drop, even if we are the same owner, as we are pushing cli in each lock())
        cpu::cpu().pop_cli();
    }
}
