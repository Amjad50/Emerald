//! from `vcell` crate
//! Copyright (c) 2017 Jorge Aparicio
//!
//! Permission is hereby granted, free of charge, to any
//! person obtaining a copy of this software and associated
//! documentation files (the "Software"), to deal in the
//! Software without restriction, including without
//! limitation the rights to use, copy, modify, merge,
//! publish, distribute, sublicense, and/or sell copies of
//! the Software, and to permit persons to whom the Software
//! is furnished to do so, subject to the following
//! conditions:
//!
//! The above copyright notice and this permission notice
//! shall be included in all copies or substantial portions
//! of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
//! ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
//! TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
//! PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
//! SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
//! CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
//! OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
//! IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
//! DEALINGS IN THE SOFTWARE.
//!
//! ---
//!
//! AND from `volatile-register` crate
//! Copyright (c) 2016 Jorge Aparicio
//!
//! Permission is hereby granted, free of charge, to any
//! person obtaining a copy of this software and associated
//! documentation files (the "Software"), to deal in the
//! Software without restriction, including without
//! limitation the rights to use, copy, modify, merge,
//! publish, distribute, sublicense, and/or sell copies of
//! the Software, and to permit persons to whom the Software
//! is furnished to do so, subject to the following
//! conditions:
//!
//! The above copyright notice and this permission notice
//! shall be included in all copies or substantial portions
//! of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
//! ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
//! TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
//! PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
//! SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
//! CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
//! OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
//! IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
//! DEALINGS IN THE SOFTWARE.

#![allow(dead_code)]

use core::cell::UnsafeCell;
use core::ptr;

/// Just like [`Cell`] but with [volatile] read / write operations
///
/// [`Cell`]: https://doc.rust-lang.org/std/cell/struct.Cell.html
/// [volatile]: https://doc.rust-lang.org/std/ptr/fn.read_volatile.html
#[repr(transparent)]
struct VCell<T> {
    value: UnsafeCell<T>,
}

impl<T> VCell<T> {
    /// Creates a new `VolatileCell` containing the given value
    pub const fn new(value: T) -> Self {
        VCell {
            value: UnsafeCell::new(value),
        }
    }

    /// Returns a copy of the contained value
    #[inline(always)]
    pub fn get(&self) -> T
    where
        T: Copy,
    {
        unsafe { ptr::read_volatile(self.value.get()) }
    }

    /// Sets the contained value
    #[inline(always)]
    pub fn set(&self, value: T)
    where
        T: Copy,
    {
        unsafe { ptr::write_volatile(self.value.get(), value) }
    }

    /// Returns a raw pointer to the underlying data in the cell
    #[inline(always)]
    pub fn as_ptr(&self) -> *mut T {
        self.value.get()
    }
}

// NOTE implicit because of `UnsafeCell`
// unsafe impl<T> !Sync for VolatileCell<T> {}

/// Read-Only register
#[repr(transparent)]
pub struct RO<T>
where
    T: Copy,
{
    register: VCell<T>,
}

impl<T> RO<T>
where
    T: Copy,
{
    /// Reads the value of the register
    #[inline(always)]
    pub fn read(&self) -> T {
        self.register.get()
    }
}

/// Read-Write register
#[repr(transparent)]
pub struct RW<T>
where
    T: Copy,
{
    register: VCell<T>,
}

impl<T> RW<T>
where
    T: Copy,
{
    /// Performs a read-modify-write operation
    ///
    /// NOTE: `unsafe` because writes to a register are side effectful
    #[inline(always)]
    pub unsafe fn modify<F>(&self, f: F)
    where
        F: FnOnce(T) -> T,
    {
        self.register.set(f(self.register.get()));
    }

    /// Reads the value of the register
    #[inline(always)]
    pub fn read(&self) -> T {
        self.register.get()
    }

    /// Writes a `value` into the register
    ///
    /// NOTE: `unsafe` because writes to a register are side effectful
    #[inline(always)]
    pub unsafe fn write(&self, value: T) {
        self.register.set(value)
    }
}

/// Write-Only register
#[repr(transparent)]
pub struct WO<T>
where
    T: Copy,
{
    register: VCell<T>,
}

impl<T> WO<T>
where
    T: Copy,
{
    /// Writes `value` into the register
    ///
    /// NOTE: `unsafe` because writes to a register are side effectful
    #[inline(always)]
    pub unsafe fn write(&self, value: T) {
        self.register.set(value)
    }
}
