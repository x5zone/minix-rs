//! VM process table module.
//!
//! Provides:
//! - `VmFlags`: Process state flags
//! - `VmProcTable`: Process table (AssumeSyncCell-backed)
//! - `EmptySlot<'a>`: Free slot typestate view
//! - `ActiveProc<'a>`: Active process typestate view
//! - `ExitingProc<'a>`: Exiting process typestate view
//!
//! # Visibility Design
//!
//! VM is an independent user-space process. All types are `pub(crate)` —
//! no external crate should depend on VM internals. External processes
//! (PM, VFS, RS) interact with VM via IPC messages.
//!
//! Within the vmproc module tree:
//! - `VmProc` is NOT exported — external code must use typestate views
//! - `get_slot_mut()` is `pub(super)` — only vmproc module tree can access raw `&mut VmProc`
//! - Typestate views (`EmptySlot`, `ActiveProc`, `ExitingProc`) are `pub(crate)`
//! - Query methods on `VmProcTable` are `pub(crate)`
//!
//! State transitions:
//! ```text
//! EmptySlot ──[activate]──> ActiveProc ──[mark_exiting]──> ExitingProc ──[reap]──> EmptySlot
//!                           ActiveProc ──[force_clear]──────────────────────────> EmptySlot
//! ```
//!
//! # API safety contract — `activate` vs `activate_relaxed` (2026-06-13)
//!
//! `EmptySlot` exposes two entry methods:
//!
//! - [`EmptySlot::activate`] — **strict mode**. Verifies in debug builds
//!   that `endpoint.slot() == self.slot()`. Use this in 99% of cases.
//! - [`EmptySlot::activate_relaxed`] — **relaxed mode**. Skips the
//!   strict pairing check. Use ONLY for these three cases:
//!
//!   1. **Fork** (`fork.rs::do_fork`): the child endpoint is
//!      `Endpoint::NONE` initially and is overwritten with the real
//!      endpoint after `sys_fork()` returns.
//!   2. **Exec temporary slot** (`VM_EXEC_TMP_SLOT`): exec writes
//!      through a temporary slot whose index differs from the endpoint
//!      it is about to install.
//!   3. **Unit tests**: tests use arbitrary endpoint/slot combinations
//!      because the IPC routing path is not exercised.
//!
//!   Even in relaxed mode, `activate_relaxed` performs a debug-only
//!   `debug_assert!` (hardening, 2026-06-13) to catch obvious
//!   misuses: a non-NONE endpoint with slot `>= VM_PROC_COUNT` is
//!   rejected, and a non-NONE endpoint whose slot neither matches
//!   `self.slot()` nor equals `VM_EXEC_TMP_SLOT` is rejected. These
//!   checks are compiled out of release builds and are a typestate
//!   hardening — they do not provide concurrency safety. Concurrency
//!   safety is provided by the single-threaded VM event loop model
//!   (see `lib.rs` module header for the full "VM is single-threaded"
//!   discussion).
//!
//! Callers that want **release-build safety** beyond the typestate
//! should migrate the call site to `activate()` (strict) once the
//! underlying logic supports it. The fork path in `fork.rs` cannot
//! use `activate()` because the child endpoint is genuinely NONE
//! during the slot transition.

mod flags;
// clippy::module_inception: `VmProc` lives in `vmproc::vmproc` (mirrors the
// C struct name); the nested name is intentional.
#[allow(clippy::module_inception)]
mod vmproc;
mod vmproc_handle;
mod table;

pub(crate) use flags::VmFlags;
pub(crate) use vmproc_handle::{EmptySlot, ActiveProc, ExitingProc};
pub(crate) use table::{VmProcTable, EndpointError};

#[cfg(test)]
pub(super) mod test_utils {
    use minix_types::{Endpoint, UserSlot};
    use crate::vmproc::VmProcTable;
    use super::ActiveProc;

    /// Creates an ActiveProc with page table and regions initialized.
    /// Use this for tests that need page table access (fork, mmap, etc.).
    pub fn get_active_vmproc(slot: UserSlot) -> ActiveProc<'static> {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_page_table().unwrap();
        active.init_regions();
        // SAFETY: See extend_to_static_lifetime() documentation below.
        unsafe { extend_to_static_lifetime(active) }
    }

    /// Creates an ActiveProc with only regions initialized (no page table).
    /// Use this for tests that only need process metadata (ACL, flags, etc.)
    /// and don't require page table operations.
    /// Avoids SIGSEGV from accessing mock physical memory in init_page_table().
    pub fn get_active_vmproc_no_pt(slot: UserSlot) -> ActiveProc<'static> {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_regions();
        // SAFETY: See extend_to_static_lifetime() documentation below.
        unsafe { extend_to_static_lifetime(active) }
    }

    /// Extend an `ActiveProc<'_>` lifetime to `'static` for test use.
    ///
    /// # Safety
    ///
    /// This is safe ONLY when all of the following hold:
    ///
    /// 1. The `ActiveProc` borrows a slot from the global `VmProcTable`,
    ///    which has `'static` lifetime (`VmProcTable::get_global()` returns
    ///    `&'static VmProcTable`).
    /// 2. The slot's memory is never moved or deallocated during the test.
    /// 3. Single-threaded test execution ensures no concurrent access.
    ///
    /// This is only used in test code — production code uses the typestate
    /// API without lifetime extension. The transmute is needed because Rust
    /// infers a shorter lifetime from the local scope, even though the
    /// underlying data is `'static`.
    unsafe fn extend_to_static_lifetime(proc: ActiveProc<'_>) -> ActiveProc<'static> {
        core::mem::transmute::<ActiveProc<'_>, ActiveProc<'static>>(proc)
    }
}
