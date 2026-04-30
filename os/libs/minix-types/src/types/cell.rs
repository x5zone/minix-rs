//! Single-threaded interior mutability primitives.
//!
//! This module provides cell types for single-threaded contexts where
//! the standard library's thread-safe types (like `Mutex` or `RwLock`)
//! are not available or would be unnecessary overhead.
//!
//! # Safety Note
//!
//! All types in this module are **NOT thread-safe** and should only be
//! used in single-threaded contexts (like Minix3 kernel or servers).

/// A cell type that unsafely implements `Sync`.
///
/// `AssumeSyncCell` wraps `UnsafeCell` and implements `Sync`, allowing
/// it to be used in static variables. This is only safe in single-threaded
/// contexts where the caller manually ensures exclusive access.
///
/// # Use Cases
///
/// - Static arrays that need interior mutability (process tables, etc.)
/// - Global state that is accessed from a single thread
/// - Performance-critical code where `Mutex` overhead is unacceptable
///
/// # Safety
///
/// The `Sync` implementation is a *promise* that the caller will:
/// 1. Only use this in single-threaded contexts
/// 2. Ensure exclusive access to each element (typically via typestate)
/// 3. Never create aliasing mutable references
///
/// Violating these rules causes undefined behavior.
///
/// # Examples
///
/// ```
/// use minix_types::AssumeSyncCell;
///
/// static DATA: [AssumeSyncCell<u32>; 4] = [
///     AssumeSyncCell::new(0),
///     AssumeSyncCell::new(1),
///     AssumeSyncCell::new(2),
///     AssumeSyncCell::new(3),
/// ];
///
/// // Safe because we're in a single-threaded context
/// // and we know no other code is accessing DATA[0]
/// let ptr = unsafe { DATA[0].get() };
/// unsafe { *ptr = 42; }
/// ```
#[repr(transparent)]
pub struct AssumeSyncCell<T>(core::cell::UnsafeCell<T>);

// SAFETY: This is only safe in single-threaded contexts.
// The caller must ensure exclusive access to each cell.
unsafe impl<T> Sync for AssumeSyncCell<T> {}

impl<T> AssumeSyncCell<T> {
    /// Creates a new `AssumeSyncCell` containing the given value.
    ///
    /// This is a `const fn`, allowing it to be used in static initializers.
    #[inline]
    pub const fn new(value: T) -> Self {
        Self(core::cell::UnsafeCell::new(value))
    }

    /// Returns a raw pointer to the inner value.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - No mutable references to this cell are active
    /// - This thread has exclusive access to the cell
    ///
    /// Violating these rules causes undefined behavior.
    #[inline]
    pub unsafe fn get(&self) -> *mut T {
        self.0.get()
    }

    /// Returns a raw pointer to the inner value (immutable access).
    ///
    /// This is safe because it only returns a raw pointer, not a reference.
    /// The caller must still ensure proper synchronization when dereferencing.
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.0.get()
    }
}

impl<T: Copy> AssumeSyncCell<T> {
    /// Returns a copy of the contained value.
    ///
    /// # Safety
    ///
    /// Caller must ensure no mutable references are active.
    #[inline]
    pub unsafe fn get_copy(&self) -> T {
        *self.0.get()
    }
}

impl<T: Default> Default for AssumeSyncCell<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for AssumeSyncCell<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_usage() {
        let cell = AssumeSyncCell::new(42);
        unsafe {
            let ptr = cell.get();
            assert_eq!(*ptr, 42);
            *ptr = 100;
            assert_eq!(*ptr, 100);
        }
    }

    #[test]
    fn test_static_usage() {
        static CELL: AssumeSyncCell<u32> = AssumeSyncCell::new(0);
        unsafe {
            let ptr = CELL.get();
            *ptr = 42;
            assert_eq!(*ptr, 42);
        }
    }

    #[test]
    fn test_array_usage() {
        static ARRAY: [AssumeSyncCell<u32>; 4] = [
            AssumeSyncCell::new(0),
            AssumeSyncCell::new(1),
            AssumeSyncCell::new(2),
            AssumeSyncCell::new(3),
        ];

        unsafe {
            let ptr = ARRAY[2].get();
            assert_eq!(*ptr, 2);
            *ptr = 20;
            assert_eq!(*ptr, 20);
        }
    }

    #[test]
    fn test_default() {
        let cell: AssumeSyncCell<u32> = Default::default();
        unsafe {
            assert_eq!(*cell.get(), 0);
        }
    }

    #[test]
    fn test_from() {
        let cell: AssumeSyncCell<u32> = 42.into();
        unsafe {
            assert_eq!(*cell.get(), 42);
        }
    }
}
