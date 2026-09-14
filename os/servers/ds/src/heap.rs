//! DS heap: the fixed pool behind STR/MEM byte-range buffers (A-3).
//!
//! C allocates with `malloc`/`free` (`minix3/minix/servers/ds/store.c:
//! 336-350, 635`): publish grabs `length` bytes, a longer overwrite
//! frees and re-allocates, delete releases. This crate has no allocator
//! in production builds, so the pool IS the design decision ([ARCH A-3],
//! decided 2026-09-15 alongside the transport campaign —
//! 07-stage-ds/todo.md P2-2): a fixed number of fixed-width slots,
//! handed out first-fit, returned by [`DsPool::release`].
//!
//! The bound is the one honest deviation, and it is C-parity in code:
//! an exhausted pool refuses publish with `ENOMEM`, the same errno C
//! returns when `malloc` fails (`store.c:338-340`). What differs is
//! *when* it trips — C only runs out of real memory, this pool at 32
//! live buffers. Every real subscriber state (driver labels, mount
//! flags, small status strings) sits far below the slot width.
//!
//! Zero-width note kept from the delete side: buffers handed back
//! through `DeleteEffect`/`heap_out` are *descriptors* (`MemBody`); the
//! bytes live here until [`DsPool::release`] retires the slot.

use crate::store::MemBody;

/// Live buffers the pool admits. C's ceiling is physical memory; 32
/// concurrent STR/MEM entries cover the registry's real workload with
/// room to spare (128 entry seats, most of them U32/LABEL).
pub const POOL_SLOTS: usize = 32;

/// Bytes per slot. Published states are small (driver labels, status
/// strings); a publish asking for more refuses with `ENOMEM` — the same
/// verdict C's `malloc(0)`-failure road would take, stated up front.
pub const POOL_SLOT: usize = 256;

/// The STR/MEM buffer pool (A-3).
///
/// First-fit free-list over equal slots: `alloc` takes the lowest free
/// slot, `release` returns it. Slots are recycled in table order, like
/// every other seat in this crate (04's first-fit discipline).
pub struct DsPool {
    /// Backing bytes: `POOL_SLOTS` slots of `POOL_SLOT`.
    buf: [u8; POOL_SLOTS * POOL_SLOT],
    /// Free-slot stack (`free[0..free_len]` are vacant slot numbers).
    free: [u8; POOL_SLOTS],
    free_len: usize,
    /// Live length per slot (the pool's own bookkeeping for `release`).
    live: [bool; POOL_SLOTS],
}

impl Default for DsPool {
    fn default() -> Self {
        Self::new()
    }
}

impl DsPool {
    /// An empty pool: every slot free, highest on top of the stack.
    pub const fn new() -> Self {
        let mut free = [0u8; POOL_SLOTS];
        let mut i = 0usize;
        while i < POOL_SLOTS {
            free[i] = (POOL_SLOTS - 1 - i) as u8;
            i += 1;
        }
        Self {
            buf: [0u8; POOL_SLOTS * POOL_SLOT],
            free,
            free_len: POOL_SLOTS,
            live: [false; POOL_SLOTS],
        }
    }

    /// Take a slot for `len` bytes (`len` over [`POOL_SLOT`] refuses —
    /// `ENOMEM`, C's `malloc` failure in the caller's own errno).
    ///
    /// Returns `(slot, MemBody)` — the body's pointer lanes point into
    /// this pool; the caller stamps it into the entry (`publish` commit)
    /// and later hands the descriptor to [`DsPool::release`].
    pub fn alloc(&mut self, len: usize) -> Result<(usize, MemBody), i32> {
        if len > POOL_SLOT {
            return Err(minix_types::ENOMEM);
        }
        if self.free_len == 0 {
            return Err(minix_types::ENOMEM);
        }
        self.free_len -= 1;
        let slot = self.free[self.free_len] as usize;
        self.live[slot] = true;
        let base = self.slot_base(slot);
        Ok((
            slot,
            MemBody {
                data: unsafe { self.buf.as_mut_ptr().add(base) },
                length: len,
                reallen: POOL_SLOT,
            },
        ))
    }

    /// Retire a buffer descriptor (delete/overwrite teardown). The slot
    /// must be live: releasing through a stale descriptor is a caller
    /// ordering bug — `assert`, mirroring `free_sub_slot`'s discipline.
    pub fn release(&mut self, body: &MemBody) {
        let slot = self.slot_of(body);
        assert!(self.live[slot], "DsPool::release: slot not live");
        self.live[slot] = false;
        self.free[self.free_len] = slot as u8;
        self.free_len += 1;
    }

    /// Read the live bytes of a descriptor (retrieve's copy source).
    pub fn slice(&self, body: &MemBody) -> &[u8] {
        let slot = self.slot_of(body);
        let base = self.slot_base(slot);
        &self.buf[base..base + body.length]
    }

    /// Writable view of a descriptor's slot (publish's copy target).
    pub fn slice_mut(&mut self, body: &MemBody) -> &mut [u8] {
        let slot = self.slot_of(body);
        let base = self.slot_base(slot);
        &mut self.buf[base..base + body.reallen]
    }

    fn slot_base(&self, slot: usize) -> usize {
        slot * POOL_SLOT
    }

    /// Which slot a descriptor points at (pointer distance to the pool
    /// base, in slots).
    fn slot_of(&self, body: &MemBody) -> usize {
        // SAFETY: `data` was stamped by `alloc` as `buf.as_mut_ptr() +
        // slot * POOL_SLOT` and is only handed back to this pool; the
        // offset math recovers the slot without dereferencing.
        let base = self.buf.as_ptr() as usize;
        let addr = body.data as usize;
        debug_assert!(addr >= base && addr < base + self.buf.len());
        (addr - base) / POOL_SLOT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc_release_roundtrip() {
        let mut pool = DsPool::new();
        let (slot, body) = pool.alloc(5).expect("empty pool must seat");
        assert_eq!(body.length, 5);
        assert_eq!(body.reallen, POOL_SLOT);
        pool.slice_mut(&body)[..5].copy_from_slice(b"hello");
        assert_eq!(pool.slice(&body), b"hello");
        pool.release(&body);
        assert_eq!(slot, 0); // the lowest free slot wins (first-fit, 04)
    }

    #[test]
    fn test_exhaustion_is_enomem() {
        let mut pool = DsPool::new();
        let mut bodies = Vec::new();
        for _ in 0..POOL_SLOTS {
            bodies.push(pool.alloc(8).expect("each slot seats once"));
        }
        assert_eq!(pool.alloc(8).map(|_| ()), Err(minix_types::ENOMEM));
        // Release one: the seat re-opens (first-fit on the free stack).
        pool.release(&bodies[0].1);
        assert!(pool.alloc(8).is_ok());
    }

    #[test]
    fn test_oversized_refuses_enomem() {
        let mut pool = DsPool::new();
        assert_eq!(
            pool.alloc(POOL_SLOT + 1).map(|_| ()),
            Err(minix_types::ENOMEM)
        );
        assert!(pool.alloc(POOL_SLOT).is_ok());
    }
}
