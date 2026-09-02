//! Test-only fixtures for B-class kernel state objects.
//!
//! `KProcess` and `KPriv` implement defensive `Drop` (design:
//! `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/panic-in-drop.md` §1
//! three-way classification): destroying an *occupied* slot via Rust's
//! implicit destruction semantics is a kernel-invariant violation, so
//! dropping an occupied slot panics.
//!
//! Unit tests routinely build slot-shaped scratch values and mutate slots
//! into occupied shapes to exercise state-transition logic — without ever
//! performing a real OS lifecycle operation (there is no OS in a unit
//! test, no caller queue, no timer chain, no scheduler linkage). If such a
//! fixture were dropped through the normal path, the defensive alarm would
//! fire even though no kernel invariant was violated.
//!
//! The fixture types below are the *labeled* exemption for that class of
//! test: the inner table is held in `ManuallyDrop`, so scope-exit does not
//! run the inner slots' defensive `Drop`. The exemption is visible in the
//! type name — nothing is hidden (cf. `panic-in-drop.md` §3.3: `Box::leak`
//! is the *wrong* tool because it hides the bypass; a fixture type
//! declares it).
//!
//! Tests that intend to exercise the defensive `Drop` itself must construct
//! `KProcess` / `KPriv` / tables directly — never through these fixtures.

use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};

use crate::kpriv::PrivTable;
use crate::proc_table::ProcessTable;

/// Test-local `ProcessTable` whose scope-exit does not run its slots'
/// defensive `Drop`.
///
/// Unit tests shape slots into "occupied" form to drive state-transition
/// logic (e.g. `rts_set`/`rts_unset`, scheduler enqueue, IPC queues).
/// Releasing such a fixture must not trip the slot-ownership alarm,
/// because the test never expressed an OS lifecycle operation.
#[cfg(test)]
pub(crate) struct TestProcTable {
    inner: ManuallyDrop<ProcessTable>,
}

#[cfg(test)]
impl TestProcTable {
    /// Wrap a freshly-built test table (see [`test_proc_table`]).
    pub(crate) fn new(table: ProcessTable) -> Self {
        Self {
            inner: ManuallyDrop::new(table),
        }
    }
}

#[cfg(test)]
impl Deref for TestProcTable {
    type Target = ProcessTable;
    fn deref(&self) -> &ProcessTable {
        &self.inner
    }
}

#[cfg(test)]
impl DerefMut for TestProcTable {
    fn deref_mut(&mut self) -> &mut ProcessTable {
        &mut self.inner
    }
}

/// Test-local `PrivTable` whose scope-exit does not run its slots'
/// defensive `Drop`. Same rationale as [`TestProcTable`].
#[cfg(test)]
pub(crate) struct TestPrivTable {
    inner: ManuallyDrop<PrivTable>,
}

#[cfg(test)]
impl TestPrivTable {
    /// Wrap a freshly-built test table (see [`test_priv_table`]).
    pub(crate) fn new(table: PrivTable) -> Self {
        Self {
            inner: ManuallyDrop::new(table),
        }
    }
}

#[cfg(test)]
impl Deref for TestPrivTable {
    type Target = PrivTable;
    fn deref(&self) -> &PrivTable {
        &self.inner
    }
}

#[cfg(test)]
impl DerefMut for TestPrivTable {
    fn deref_mut(&mut self) -> &mut PrivTable {
        &mut self.inner
    }
}

/// Fresh test `ProcessTable` fixture.
///
/// # Semantics
///
/// The returned value is *not* an OS lifecycle owner: dropping it is a
/// no-op. Do **not** use it to verify `Drop` behavior — construct
/// `ProcessTable::new()` directly for that.
#[cfg(test)]
pub(crate) fn test_proc_table() -> TestProcTable {
    TestProcTable::new(ProcessTable::new())
}

/// Fresh test `PrivTable` fixture. Same exemption semantics as
/// [`test_proc_table`].
#[cfg(test)]
pub(crate) fn test_priv_table() -> TestPrivTable {
    TestPrivTable::new(PrivTable::new())
}

// ── Single-process scratch values ──
//
// Some unit tests (IPC core, page-fault handler, dispatch tests) shape a
// *standalone* `KProcess` into an "occupied-looking" form (e.g. by
// re-assigning `p_rts_flags`, which clears `SLOT_FREE`) without a
// surrounding table. Such a value is a slot-shaped scratch value again,
// so the defensive `Drop` must not fire when it goes out of scope.

use crate::proc::KProcess;

/// Test-local `KProcess` whose scope-exit does not run the defensive
/// `Drop`. Same rationale as [`TestProcTable`] but for standalone scratch
/// processes (no surrounding table).
#[cfg(test)]
pub(crate) struct TestKProc {
    inner: ManuallyDrop<KProcess>,
}

#[cfg(test)]
impl TestKProc {
    /// Wrap an existing test process (see [`scratch_kproc`]).
    pub(crate) fn new(proc: KProcess) -> Self {
        Self {
            inner: ManuallyDrop::new(proc),
        }
    }
}

#[cfg(test)]
impl Deref for TestKProc {
    type Target = KProcess;
    fn deref(&self) -> &KProcess {
        &self.inner
    }
}

#[cfg(test)]
impl DerefMut for TestKProc {
    fn deref_mut(&mut self) -> &mut KProcess {
        &mut self.inner
    }
}

/// Wrap a standalone test `KProcess` into a scope-exit-no-drop fixture.
///
/// Used when a test re-assigns `p_rts_flags` (thereby clearing
/// `SLOT_FREE`) on a process that lives outside any `ProcessTable`.
#[cfg(test)]
pub(crate) fn scratch_kproc(proc: KProcess) -> TestKProc {
    TestKProc::new(proc)
}

/// Test-local `[KProcess; N]` whose scope-exit does not run element Drops.
///
/// IPC-engine tests build a *mini process table* as a plain array and
/// shape some elements into occupied-looking forms (re-assigned
/// `p_rts_flags`, caller queues). Wrapping the array declares the same
/// fixture exemption as [`TestProcTable`] — the array is the test's
/// table.
#[cfg(test)]
pub(crate) struct TestProcArray<const N: usize> {
    inner: ManuallyDrop<[KProcess; N]>,
}

#[cfg(test)]
impl<const N: usize> TestProcArray<N> {
    /// Wrap an existing test array (see [`scratch_procs`]).
    pub(crate) fn new(procs: [KProcess; N]) -> Self {
        Self {
            inner: ManuallyDrop::new(procs),
        }
    }
}

#[cfg(test)]
impl<const N: usize> Deref for TestProcArray<N> {
    type Target = [KProcess];
    fn deref(&self) -> &[KProcess] {
        &self.inner[..]
    }
}

#[cfg(test)]
impl<const N: usize> DerefMut for TestProcArray<N> {
    fn deref_mut(&mut self) -> &mut [KProcess] {
        &mut self.inner[..]
    }
}

/// Wrap a standalone test `[KProcess; N]` into a scope-exit-no-drop
/// fixture. The array is the test's mini process table.
#[cfg(test)]
pub(crate) fn scratch_procs<const N: usize>(procs: [KProcess; N]) -> TestProcArray<N> {
    TestProcArray::new(procs)
}
