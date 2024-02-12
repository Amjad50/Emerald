use core::{
    cell::UnsafeCell,
    fmt,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::atomic::{AtomicI64, Ordering},
};

use crate::cpu;

use super::lock;

pub struct RwLock<T: ?Sized> {
    lock: lock::Lock,
    owner_cpu: AtomicI64,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> fmt::Debug for RwLock<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("RwLock");
        s.field("owner_cpu", &self.owner_cpu);
        if let Some(data) = self.try_read() {
            s.field("data", &data);
        } else {
            s.field("data", &"[write locked]");
        }
        s.finish()
    }
}

pub struct RwLockReadGuard<'a, T: ?Sized + 'a> {
    // NB: we use a pointer instead of `&'a T` to avoid `noalias` violations, because a
    // `Ref` argument doesn't hold immutability for its whole scope, only until it drops.
    // `NonNull` is also covariant over `T`, just like we would have with `&T`. `NonNull`
    // is preferable over `const* T` to allow for niche optimization.
    data: NonNull<T>,
    inner_lock: &'a lock::Lock,
    marker: PhantomData<*const ()>, // !Send
}

unsafe impl<T: ?Sized + Sync> Sync for RwLockReadGuard<'_, T> {}

pub struct RwLockWriteGuard<'a, T: ?Sized + 'a> {
    lock: &'a RwLock<T>,
    marker: PhantomData<*const ()>, // !Send
}

unsafe impl<T: ?Sized + Sync> Sync for RwLockWriteGuard<'_, T> {}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for RwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLockWriteGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for RwLockWriteGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

#[allow(dead_code)]
impl<T> RwLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: lock::Lock::new(),
            owner_cpu: AtomicI64::new(-1),
            data: UnsafeCell::new(data),
        }
    }
}

#[allow(dead_code)]
impl<T: ?Sized> RwLock<T> {
    pub fn read(&self) -> RwLockReadGuard<T> {
        self.lock.read_lock();
        // must be -1, i.e. no owner
        self.owner_cpu.store(-1, Ordering::Relaxed);
        RwLockReadGuard {
            data: unsafe { NonNull::new_unchecked(self.data.get()) },
            inner_lock: &self.lock,
            marker: PhantomData,
        }
    }

    pub fn try_read(&self) -> Option<RwLockReadGuard<T>> {
        if self.lock.try_read_lock() {
            // must be -1, i.e. no owner
            self.owner_cpu.store(-1, Ordering::Relaxed);
            Some(RwLockReadGuard {
                data: unsafe { NonNull::new_unchecked(self.data.get()) },
                inner_lock: &self.lock,
                marker: PhantomData,
            })
        } else {
            None
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<T> {
        let cpu = cpu::cpu();
        cpu.push_cli(); // disable interrupts to avoid deadlock
        let cpu_id = cpu.id as i64;

        if self.owner_cpu.load(Ordering::Relaxed) == cpu_id {
            panic!("Mutex already locked by this CPU");
        } else {
            self.lock.write_lock();
            self.owner_cpu.store(cpu_id, Ordering::Relaxed);
            RwLockWriteGuard {
                lock: self,
                marker: PhantomData,
            }
        }
    }

    pub fn try_write(&self) -> Option<RwLockWriteGuard<T>> {
        let cpu = cpu::cpu();
        cpu.push_cli(); // disable interrupts to avoid deadlock
        let cpu_id = cpu.id as i64;

        if self.owner_cpu.load(Ordering::Relaxed) == cpu_id {
            // we will not throw here, since the CPU might want to try to lock it again, at least its not a deadlock
            cpu.pop_cli();
            None
        } else if self.lock.try_write_lock() {
            self.owner_cpu.store(cpu_id, Ordering::Relaxed);
            Some(RwLockWriteGuard {
                lock: self,
                marker: PhantomData,
            })
        } else {
            cpu.pop_cli();
            None
        }
    }

    /// We know statically that no one else is accessing the lock, so we can
    /// just return a reference to the data without acquiring the lock.
    #[allow(dead_code)]
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the mutex is locked, we may not be the only accessors, but we know,
        //         that no one will change the value, thus we can get multiple references at the same time
        unsafe { self.data.as_ref() }
    }
}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the mutex is locked, we are the only accessors,
        //         and the pointer is valid, since it was generated for a valid T
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the mutex is locked, we are the only accessors,
        //         and the pointer is valid, since it was generated for a valid T
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        // SAFETY: the mutex is locked, we are the only accessor
        unsafe { self.inner_lock.read_unlock() };
    }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        assert_ne!(self.lock.owner_cpu.load(Ordering::Relaxed), -1);
        self.lock.owner_cpu.store(-1, Ordering::Relaxed);
        // SAFETY: the mutex is locked, we are the only accessor
        unsafe { self.lock.lock.write_unlock() };
        cpu::cpu().pop_cli(); // re-enable interrupts
    }
}
