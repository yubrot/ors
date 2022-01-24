use crate::interrupts::Cli;
use core::fmt;
use core::mem;
use core::ops::{Deref, DerefMut};

/// `spin::Mutex` with `crate::interrupts::Cli` to avoid deadlocks.
#[derive(Debug)]
pub struct Mutex<T: ?Sized> {
    inner: spin::Mutex<T>,
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<T> {
        let cli = Cli::new();
        let inner = self.inner.lock();
        MutexGuard { inner, cli }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        let cli = Cli::new();
        let inner = self.inner.try_lock()?;
        Some(MutexGuard { inner, cli })
    }

    pub fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            inner: spin::Mutex::new(value),
        }
    }

    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }
}

pub struct MutexGuard<'a, T: 'a + ?Sized> {
    inner: spin::MutexGuard<'a, T>,
    cli: Cli,
}

impl<'a, T: 'a + ?Sized> MutexGuard<'a, T> {
    pub fn leak(this: Self) -> &'a mut T {
        let inner = spin::MutexGuard::leak(this.inner);
        mem::forget(this.cli);
        inner
    }
}

impl<'a, T: 'a + ?Sized> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl<'a, T: 'a + ?Sized> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.inner
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
