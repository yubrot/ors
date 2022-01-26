use super::spin::Spin;
use crate::task;
use core::cell::UnsafeCell;
use core::fmt;
use core::ops::{Deref, DerefMut};

/// A mutex implementation based on `spin::Spin` and `task::scheduler`.
#[derive(Debug)]
pub struct Mutex<T: ?Sized> {
    locked: Spin<bool>,
    data: UnsafeCell<T>,
}

impl<T: ?Sized> Mutex<T> {
    fn chan(&self) -> task::WaitChannel {
        task::WaitChannel::from_ptr(self)
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }

    pub fn lock(&self) -> MutexGuard<T> {
        MutexGuard::new(self)
    }
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: Spin::new(false),
            data: UnsafeCell::new(value),
        }
    }

    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

pub struct MutexGuard<'a, T: 'a + ?Sized> {
    mutex: &'a Mutex<T>,
}

impl<'a, T: 'a + ?Sized> MutexGuard<'a, T> {
    fn new(mutex: &'a Mutex<T>) -> Self {
        loop {
            let mut locked = mutex.locked.lock();
            if !*locked {
                *locked = true; // acquire lock
                break;
            }
            task::scheduler().block(mutex.chan(), None, locked);
        }
        Self { mutex }
    }
}

impl<'a, T: 'a + ?Sized> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        *self.mutex.locked.lock() = false;
        task::scheduler().release(self.mutex.chan());
    }
}

impl<'a, T: 'a + ?Sized> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<'a, T: 'a + ?Sized> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<'a, T: 'a + fmt::Debug + ?Sized> fmt::Debug for MutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: 'a + fmt::Display + ?Sized> fmt::Display for MutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
