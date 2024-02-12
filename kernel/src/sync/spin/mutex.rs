use core::{
    cell::UnsafeCell,
    fmt,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicI64, Ordering},
};

use crate::cpu;

use super::lock;

pub struct Mutex<T: ?Sized> {
    lock: lock::Lock,
    owner_cpu: AtomicI64,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

impl<T> fmt::Debug for Mutex<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Mutex");
        s.field("owner_cpu", &self.owner_cpu);
        if let Some(data) = self.try_lock() {
            s.field("data", &data);
        } else {
            s.field("data", &"[locked]");
        }
        s.finish()
    }
}

#[must_use]
pub struct MutexGuard<'a, T: ?Sized + 'a> {
    lock: &'a Mutex<T>,
    marker: PhantomData<*const ()>, // !Send
}

unsafe impl<T: ?Sized + Sync> Sync for MutexGuard<'_, T> {}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: lock::Lock::new(),
            owner_cpu: AtomicI64::new(-1),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<T> {
        let cpu = cpu::cpu();
        cpu.push_cli(); // disable interrupts to avoid deadlock
        let cpu_id = cpu.id as i64;

        if self.owner_cpu.load(Ordering::Relaxed) == cpu_id {
            panic!("Mutex already locked by this CPU");
        } else {
            self.lock.write_lock();
            self.owner_cpu.store(cpu_id, Ordering::Relaxed);
            MutexGuard {
                lock: self,
                marker: PhantomData,
            }
        }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        let cpu = cpu::cpu();
        cpu.push_cli(); // disable interrupts to avoid deadlock
        let cpu_id = cpu.id as i64;

        if self.owner_cpu.load(Ordering::Relaxed) == cpu_id {
            // we will not throw here, since the CPU might want to try to lock it again, at least its not a deadlock
            cpu.pop_cli();
            None
        } else if self.lock.try_write_lock() {
            self.owner_cpu.store(cpu_id, Ordering::Relaxed);
            Some(MutexGuard {
                lock: self,
                marker: PhantomData,
            })
        } else {
            cpu.pop_cli();
            None
        }
    }

    /// A special method to allow accessing the variable inside
    /// the lock after locking it.
    ///
    /// The difference between this and using `Deref` is that
    /// the lifetime of the returned reference is tied to main value of the lock.
    #[allow(dead_code)]
    pub fn run_with<'a, R>(&'a self, f: impl FnOnce(&'a T) -> R) -> R {
        let guard: MutexGuard<'a, T> = self.lock();
        let d = unsafe { guard.lock.data.get().as_ref().unwrap() };
        f(d)
    }

    /// A special method to allow accessing the variable inside
    /// the lock after locking it.
    ///
    /// The difference between this and using `DerefMut` is that
    /// the lifetime of the returned reference is tied to main value of the lock.
    #[allow(dead_code)]
    pub fn run_with_mut<'a, R>(&'a self, f: impl FnOnce(&'a mut T) -> R) -> R {
        let guard: MutexGuard<'a, T> = self.lock();
        let d = unsafe { guard.lock.data.get().as_mut().unwrap() };
        f(d)
    }

    /// We know statically that no one else is accessing the lock, so we can
    /// just return a reference to the data without acquiring the lock.
    #[allow(dead_code)]
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: the mutex is locked, we are the only accessors,
        //         and the pointer is valid, since it was generated for a valid T
        unsafe { self.lock.data.get().as_ref().unwrap() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: the mutex is locked, we are the only accessors,
        //         and the pointer is valid, since it was generated for a valid T
        unsafe { self.lock.data.get().as_mut().unwrap() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.owner_cpu.store(-1, Ordering::Relaxed);
        // SAFETY: the mutex is locked, we are the only accessor
        unsafe { self.lock.lock.write_unlock() };
        cpu::cpu().pop_cli(); // re-enable interrupts
    }
}
