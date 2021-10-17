use super::once::Once;
use core::cell::Cell;
use core::fmt;
use core::ops::Deref;

/// Almost same as `spin::Lazy`, except it uses `ors_kernel::sync::once::Once`.
pub struct Lazy<T, F = fn() -> T> {
    cell: Once<T>,
    init: Cell<Option<F>>,
}

impl<T, F> Lazy<T, F> {
    pub const fn new(f: F) -> Self {
        Self {
            cell: Once::new(),
            init: Cell::new(Some(f)),
        }
    }

    pub fn as_mut_ptr(&self) -> *mut T {
        self.cell.as_mut_ptr()
    }
}

impl<T, F: FnOnce() -> T> Lazy<T, F> {
    pub fn force(this: &Self) -> &T {
        this.cell.call_once(|| match this.init.take() {
            Some(f) => f(),
            None => panic!("Lazy instance has previously been poisoned"),
        })
    }
}

unsafe impl<T, F: Send> Sync for Lazy<T, F> where Once<T>: Sync {}

impl<T: fmt::Debug, F> fmt::Debug for Lazy<T, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lazy")
            .field("cell", &self.cell)
            .field("init", &"..")
            .finish()
    }
}

impl<T, F: FnOnce() -> T> Deref for Lazy<T, F> {
    type Target = T;

    fn deref(&self) -> &T {
        Self::force(self)
    }
}

impl<T: Default> Default for Lazy<T, fn() -> T> {
    fn default() -> Self {
        Self::new(T::default)
    }
}
