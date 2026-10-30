//! Instruction-cache synchronization across the data→instruction
//! transition ("cache maintenance on data→instruction transitions").
//!
//! # Why this contract exists
//!
//! Loading a program means *writing bytes that the CPU will later fetch as
//! instructions*. Every architecture in this port keeps (or may keep) the
//! instruction stream out of sync with those writes unless software says
//! otherwise:
//!
//! - **aarch64**: the I-cache is not coherent with the D-cache; the writes
//!   the loader makes through the kernel's boot channel sit in the
//!   D-cache/point-of-coherency, and a fetch of the same physical lines can
//!   miss stale data. The ARM ARM prescribes the by-VA maintenance sequence
//!   (clean D to PoU → `dsb` → invalidate I to PoU → `dsb` → `isb`).
//! - **riscv64**: `FENCE.I` is required between stores to instruction
//!   memory and subsequent fetches on the same hart.
//! - **x86_64**: I and D caches are coherent; no cache-maintenance
//!   instruction exists. The architectural obligation is only a
//!   serializing instruction on the executing processor before the new
//!   code is fetched (see `x86_64::cache_sync`).
//!
//! C has no counterpart to call: Minix3's kernel C has no aarch64 port and
//! no cache-maintenance call anywhere in its loader path (checked:
//! `minix3/minix/kernel/` has no `icache`/`cache_sync`/`cvau` call site).
//! This contract is therefore an `[ARCH]` addition, recorded in the
//! structural-debt register
//! (`rewrite-notes/coordination/STRUCTURAL-DEBT-REGISTER-20261008.md`):
//! "a per-arch `sync_icache(range)` contract in the arch-trait family;
//! called at the ELF-loader tail".
//!
//! # What is *not* this contract's job
//!
//! Cross-hart synchronization for secondary CPUs. On aarch64 the by-VA
//! operations broadcast to the inner shareable domain (`IC IVAU` is a
//! domain operation), so one hart's sequence covers the others. On riscv64
//! `FENCE.I` is hart-local; the remote-fence arm (SBI `remote_fence_i`)
//! belongs to the SMP milestone and is bound to the main-core clamp removal
//! — see `riscv64::cache_sync` for the binding note.

use core::ops::Range;

/// Architecture abstraction for instruction-cache synchronization.
///
/// # Address space of `range`
///
/// The cache-maintenance instructions this contract compiles to take a
/// *virtual* address and translate it through the currently-active
/// translation regime. The caller must therefore pass addresses that are
/// mapped **in the active root**, pointing at the frames that hold the
/// newly written bytes — typically the kernel-accessible virtual address of
/// the frame (the loader's `PhysAccess` channel), not the user-space VA the
/// image will eventually run under (that root may not be active, or even
/// installed, at call time).
///
/// The operations are idempotent and over-invalidation is harmless, so the
/// range may be widened to whole frames/lines.
pub trait CacheSyncArch {
    /// Synchronize the instruction stream with data writes already made
    /// through the kernel-accessible virtual addresses in `range`.
    ///
    /// Callers must have finished *all* data writes to the range before
    /// calling. An empty range (`start >= end`) is a defined no-op.
    ///
    /// C: none — no counterpart exists in the Minix3 C tree (see module
    /// docs); this is the `[ARCH: cache-sync]` addition described above.
    fn sync_icache_range(range: Range<u64>);
}

// ── Mock implementation for host tests ──

/// Mock `CacheSyncArch` that records calls (for the host test form).
///
/// The host form compiles the x86_64 loader against this mock so tests can
/// assert that the loader *did* invoke the contract, with which range,
/// without executing any cache-maintenance instruction (the host x86_64
/// build has no such instruction to execute, and the masked range belongs
/// to a synthetic address space anyway).
pub struct MockCacheSyncArch;

/// Number of `sync_icache_range` calls observed since the last
/// [`mock_reset_cache_sync`].
static MOCK_SYNC_CALLS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Low/high (exclusive) ends of the most recent observed range.
static MOCK_SYNC_LO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static MOCK_SYNC_HI: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Reset the mock's call counter and recorded range. Call from test setup
/// so one test's observations cannot leak into another's assertions.
pub fn mock_reset_cache_sync() {
    use core::sync::atomic::Ordering;
    MOCK_SYNC_CALLS.store(0, Ordering::Relaxed);
    MOCK_SYNC_LO.store(0, Ordering::Relaxed);
    MOCK_SYNC_HI.store(0, Ordering::Relaxed);
}

/// Number of calls observed since [`mock_reset_cache_sync`].
pub fn mock_sync_calls() -> usize {
    MOCK_SYNC_CALLS.load(core::sync::atomic::Ordering::Relaxed)
}

/// The most recent observed range as `(lo, hi)` (exclusive end), or `None`
/// when no call has been observed since the last reset.
pub fn mock_last_range() -> Option<(u64, u64)> {
    use core::sync::atomic::Ordering;
    if MOCK_SYNC_CALLS.load(Ordering::Relaxed) == 0 {
        return None;
    }
    Some((
        MOCK_SYNC_LO.load(Ordering::Relaxed),
        MOCK_SYNC_HI.load(Ordering::Relaxed),
    ))
}

impl CacheSyncArch for MockCacheSyncArch {
    fn sync_icache_range(range: Range<u64>) {
        use core::sync::atomic::Ordering;
        MOCK_SYNC_LO.store(range.start, Ordering::Relaxed);
        MOCK_SYNC_HI.store(range.end, Ordering::Relaxed);
        MOCK_SYNC_CALLS.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_records_call_and_range() {
        mock_reset_cache_sync();
        assert_eq!(mock_sync_calls(), 0);
        assert_eq!(mock_last_range(), None);

        MockCacheSyncArch::sync_icache_range(0x1000..0x3000);

        assert_eq!(mock_sync_calls(), 1);
        assert_eq!(mock_last_range(), Some((0x1000, 0x3000)));

        mock_reset_cache_sync();
        assert_eq!(mock_sync_calls(), 0);
        assert_eq!(mock_last_range(), None);
    }

    #[test]
    fn mock_accepts_empty_range() {
        mock_reset_cache_sync();
        MockCacheSyncArch::sync_icache_range(0x2000..0x2000);
        assert_eq!(mock_sync_calls(), 1);
        assert_eq!(mock_last_range(), Some((0x2000, 0x2000)));
    }
}
