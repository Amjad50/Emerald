use core::{
    cell::UnsafeCell,
    fmt,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use super::lock;

pub struct Mutex<T: ?Sized> {
    lock: lock::Lock,
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
        if let Some(data) = self.try_lock() {
            s.field("data", &data);
        } else {
            s.field("data", &"[locked]");
        }
        s.finish()
    }
}

#[must_use]
pub struct MutexGuard<'a, T: ?Sized> {
    lock: &'a Mutex<T>,
    marker: PhantomData<*const ()>, // !Send
}

unsafe impl<T: ?Sized + Sync> Sync for MutexGuard<'_, T> {}

impl<T> fmt::Debug for MutexGuard<'_, T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: lock::Lock::new(),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        // SAFETY: we are the only accessor, and we are locking it, its never locked again until unlocked
        self.lock.lock();
        MutexGuard {
            lock: self,
            marker: PhantomData,
        }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.lock.try_lock() {
            Some(MutexGuard {
                lock: self,
                marker: PhantomData,
            })
        } else {
            None
        }
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: the mutex is locked, we are the only accessors,
        //         and the pointer is valid, since it was generated for a valid T
        unsafe { self.lock.data.get().as_ref().unwrap() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: the mutex is locked, we are the only accessors,
        //         and the pointer is valid, since it was generated for a valid T
        unsafe { self.lock.data.get().as_mut().unwrap() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        // SAFETY: the mutex is locked, we are the only accessor
        unsafe { self.lock.lock.unlock() };
    }
}
