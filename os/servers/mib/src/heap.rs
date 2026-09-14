//! MIB heap: the byte budget behind every dynamic allocation (A-3).
//!
//! C allocates with `malloc`/`free`: one dynode block carries header +
//! name + data (`tree.c:679-685`), descriptions are `strdup`ed
//! (`:1043-1048`), oversized writes take a temporary heap buffer
//! (`:1235-1238`), and temp mount points claim head + name + desc
//! (`:1739`). Two C rules shape this module:
//!
//! 1. **Exhaustion is `EINVAL`, never `ENOMEM`** (`tree.c:589,1043,
//!    1207`) — an allocation failure means the request is inadmissible,
//!    not that the system is out of memory. The single exception is the
//!    temp mount point (`:1741-1744`), and that mapping is the *caller's*
//!    policy, not the pool's.
//! 2. **SEF restart loses everything** (`main.c:415-431`) — dynamic
//!    nodes, their buffers, and the accounting all reset together.
//!
//! The pool is an *accounted budget* over `alloc`: every claim reserves
//! bytes up front, every buffer remembers its length, and release
//! returns exactly what was claimed. There is no slot width to pad to —
//! MIB's allocation sizes span three orders of magnitude (a 12-byte
//! dynamic node next to a multi-KB create payload), which is why the
//! DS-style fixed slot pool (`os/servers/ds/src/heap.rs`) does not fit
//! here. `[ARCH: ...]` C's heap-bound exhaustion becomes a self-imposed
//! budget: the trip point differs (C runs out of real memory, this
//! budget at [`MIB_HEAP_BUDGET`]), the errno at the trip is C-parity.

use alloc::boxed::Box;

/// Total dynamic bytes the server admits. C's ceiling is the heap; this
/// budget is the same ceiling made explicit — generous against the real
/// workload (a handful of dynamic nodes, descriptions ≤
/// [`MAX_DESC_LEN`](crate::tree::node::MAX_DESC_LEN), staging buffers a
/// page at a time) yet bounded well before a runaway create loop starves
/// the process.
pub const MIB_HEAP_BUDGET: usize = 256 * 1024;

/// Why a claim was refused: the budget is spent. Callers map this to
/// `EINVAL` (everywhere) or `ENOMEM` (temp mount points only) per the C
/// rules above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetExhausted;

/// The accounting pool (A-3).
///
/// Not an allocator — a ledger. Buffers are ordinary `Box<[u8]>`
/// allocations; the budget decides *whether* they may exist and keeps
/// the running total honest. Claim-before-allocate means an allocation
/// failure after a successful claim cannot strand bytes: the claim is
/// the commit point.
#[derive(Debug)]
pub struct MibBudget {
    /// Bytes currently lent out.
    used: usize,
    /// Bytes the server admits in total.
    cap: usize,
}

impl MibBudget {
    /// The server's pool: full budget available. C: an empty heap.
    pub const fn new() -> Self {
        Self { used: 0, cap: MIB_HEAP_BUDGET }
    }

    /// A pool with an explicit ceiling (tests, restart sizing).
    pub const fn with_cap(cap: usize) -> Self {
        Self { used: 0, cap }
    }

    /// Reserve `n` bytes; refuse what does not fit.
    ///
    /// The subtraction runs cap-first, so a huge `n` cannot wrap the
    /// check into a false pass.
    pub fn claim(&mut self, n: usize) -> Result<(), BudgetExhausted> {
        if n > self.cap - self.used {
            return Err(BudgetExhausted);
        }
        self.used += n;
        Ok(())
    }

    /// Return `n` bytes. `n` must match a prior claim — the arena's
    /// remove paths are the only callers, and each pairs one release
    /// with one claim (the C `free` discipline, tree.c:757-761).
    pub fn release(&mut self, n: usize) {
        self.used -= n;
    }

    /// Claim `n` bytes and hand back a zeroed buffer of exactly `n`.
    ///
    /// One call, one commit point: a buffer exists only if its bytes
    /// were budgeted, and its length *is* the claim (free_buf needs no
    /// remembered size).
    pub fn alloc_buf(&mut self, n: usize) -> Result<Box<[u8]>, BudgetExhausted> {
        self.claim(n)?;
        // `vec![0u8; n]` so the slice's length *is* the claim — a
        // with_capacity/into_boxed_slice pairing would hand back an
        // empty box (Vec's len starts at zero) and silently uncharge.
        let buf = alloc::vec![0u8; n].into_boxed_slice();
        Ok(buf)
    }

    /// Retire a buffer and return its bytes to the budget. Consuming
    /// the buffer is what makes double-release unrepresentable.
    pub fn free_buf(&mut self, buf: Box<[u8]>) {
        let n = buf.len();
        drop(buf);
        self.release(n);
    }

    /// Bytes currently lent out.
    pub const fn used(&self) -> usize {
        self.used
    }

    /// Bytes still claimable.
    pub const fn available(&self) -> usize {
        self.cap - self.used
    }

    /// Wipe the accounting on a SEF restart loss. The callers drop the
    /// buffers they still hold first; this only zeroes the ledger.
    /// C: the restart path frees nothing either — the address space is
    /// replaced wholesale (`main.c:415-431`).
    pub fn reset(&mut self) {
        self.used = 0;
    }
}

impl Default for MibBudget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claim_release_roundtrip() {
        let mut b = MibBudget::with_cap(100);
        b.claim(40).unwrap();
        assert_eq!(b.used(), 40);
        assert_eq!(b.available(), 60);
        b.claim(60).unwrap();
        assert_eq!(b.available(), 0);
        // The boundary is exact: one more byte does not fit.
        assert_eq!(b.claim(1), Err(BudgetExhausted));
        b.release(60);
        assert_eq!(b.available(), 60);
        b.claim(1).unwrap();
    }

    #[test]
    fn test_huge_claim_cannot_wrap() {
        let mut b = MibBudget::with_cap(100);
        // usize-wide claims refuse, never pass by wraparound.
        assert_eq!(b.claim(usize::MAX), Err(BudgetExhausted));
        assert_eq!(b.used(), 0);
    }

    #[test]
    fn test_alloc_buf_zeroed_and_freed() {
        let mut b = MibBudget::with_cap(64);
        let buf = b.alloc_buf(16).unwrap();
        assert_eq!(buf.len(), 16);
        assert!(buf.iter().all(|&byte| byte == 0));
        assert_eq!(b.used(), 16);
        // Freeing returns exactly the buffer's length.
        b.free_buf(buf);
        assert_eq!(b.used(), 0);
    }

    #[test]
    fn test_alloc_refusal_claims_nothing() {
        let mut b = MibBudget::with_cap(16);
        assert_eq!(b.alloc_buf(17), Err(BudgetExhausted));
        // A refused allocation left no partial charge behind.
        assert_eq!(b.available(), 16);
    }

    #[test]
    fn test_reset_loses_the_ledger() {
        let mut b = MibBudget::with_cap(64);
        b.claim(32).unwrap();
        b.reset();
        assert_eq!(b.used(), 0);
        assert_eq!(b.available(), 64);
    }
}
