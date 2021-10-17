use crate::interrupts::Cli;
use core::fmt;

/// `spin::Once` with `crate::interrupts::Cli` to avoid deadlocks.
pub struct Once<T> {
    inner: spin::Once<T>,
}

impl<T> Once<T> {
    pub const INIT: Self = Self::new();

    pub const fn new() -> Self {
        Self {
            inner: spin::Once::new(),
        }
    }

    pub const fn initialized(data: T) -> Self {
        Self {
            inner: spin::Once::initialized(data),
        }
    }

    pub fn as_mut_ptr(&self) -> *mut T {
        self.inner.as_mut_ptr()
    }

    pub fn get(&self) -> Option<&T> {
        self.inner.get()
    }

    pub unsafe fn get_unchecked(&self) -> &T {
        self.inner.get_unchecked()
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.inner.get_mut()
    }

    pub fn try_into_inner(self) -> Option<T> {
        self.inner.try_into_inner()
    }

    pub fn is_completed(&self) -> bool {
        self.inner.is_completed()
    }

    pub fn call_once<F: FnOnce() -> T>(&self, f: F) -> &T {
        // We try get() at first to avoid Cli overhead
        match self.inner.get() {
            Some(data) => data,
            None => {
                let cli = Cli::new();
                let data = self.inner.call_once(f);
                drop(cli);
                data
            }
        }
    }

    pub fn wait(&self) -> &T {
        self.inner.wait()
    }

    pub fn poll(&self) -> Option<&T> {
        self.inner.poll()
    }
}

impl<T: fmt::Debug> fmt::Debug for Once<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T> From<T> for Once<T> {
    fn from(data: T) -> Self {
        Self::initialized(data)
    }
}
