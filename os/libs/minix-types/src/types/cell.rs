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
        unsafe { *self.0.get() }
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

// ── D-41 (S-6.2): boot-frozen cell — the type-level "write-once, then
// read-only on every CPU" primitive ──

/// A cell written exactly once, then read-only forever — the type-level
/// expression of "boot-frozen" state (S-6.2 D-41): the BSP freezes the value
/// during single-threaded boot; every CPU (BSP and all APs) reads it
/// afterwards, concurrently.
///
/// # Why not [`AssumeSyncCell`](struct.AssumeSyncCell.html)?
///
/// `AssumeSyncCell`'s `Sync` is unconditional (`unsafe impl`) and relies on
/// the caller proving single-threadedness at *every* access site. For
/// boot-frozen globals that proof shape is different and stronger: one
/// write strictly before any reader can exist, then immutable sharing. This
/// type encodes that protocol in its API — `freeze` is the only write,
/// `get` only hands out shared references.
///
/// # Why not `OnceLock`?
///
/// `std::sync::OnceLock` needs `std` (or an allocator for `Once`'s futex);
/// `no_std` boot code has neither.
///
/// # Safety contract
///
/// - `freeze` must be called at most once (debug builds assert).
/// - `freeze` happens-before any `get`: the writer stores `frozen` with
///   `Release` after writing the value; readers `Acquire`-load `frozen`
///   before touching the value. The first reader that observes `frozen ==
///   true` therefore sees the fully initialized `T`, and all later readers
///   share the same immutable `&T`.
pub struct Frozen<T> {
    value: core::cell::UnsafeCell<core::mem::MaybeUninit<T>>,
    frozen: core::sync::atomic::AtomicBool,
}

// SAFETY: the freeze protocol makes concurrent access race-free —
// exactly one write (Release-published), readers acquire before every
// access, and a frozen `T` is only ever shared as `&T` (immutable).
// `Send` is required on `T` so the value may be written on the BSP and
// read on an AP (the value crosses a thread boundary at freeze time).
unsafe impl<T: Send> Sync for Frozen<T> {}

impl<T> Frozen<T> {
    /// Creates an empty (not yet frozen) cell — `const`, usable in statics.
    pub const fn new() -> Self {
        Self {
            value: core::cell::UnsafeCell::new(core::mem::MaybeUninit::uninit()),
            frozen: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Freeze `value` into the cell. Must be called at most once, before
    /// any `get` (debug builds assert; the boot order enforces it: the
    /// freezing phase is strictly before the readers exist).
    pub fn freeze(&self, value: T) {
        debug_assert!(
            !self.frozen.load(core::sync::atomic::Ordering::Acquire),
            "Frozen::freeze called twice",
        );
        // SAFETY: no `get` can observe the value until `frozen` is set
        // below (Acquire read of `false` makes the reader bail), and
        // freeze is unique by the contract above — no concurrent write.
        unsafe {
            (*self.value.get()).write(value);
        }
        self.frozen
            .store(true, core::sync::atomic::Ordering::Release);
    }

    /// Shared reference to the frozen value.
    ///
    /// # Panics
    ///
    /// Panics if `freeze` has not run yet (`"frozen before freeze"`).
    pub fn get(&self) -> &T {
        assert!(
            self.frozen.load(core::sync::atomic::Ordering::Acquire),
            "Frozen::get before freeze",
        );
        // SAFETY: `frozen` is observed `true` (Acquire) — the value was
        // written before the Release store and is now immutable shared
        // state; the pointer is valid for the life of the static.
        unsafe { (*self.value.get()).assume_init_ref() }
    }
}

impl<T> Default for Frozen<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for Frozen<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.frozen.load(core::sync::atomic::Ordering::Acquire) {
            // SAFETY: frozen — shared read is race-free (see type docs).
            unsafe { (*self.value.get()).assume_init_ref() }.fmt(f)
        } else {
            f.write_str("<unfrozen>")
        }
    }
}
