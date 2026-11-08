//! Boot-stage bump-region arithmetic.
//!
//! Two independent allocators hand out physical pages during boot: the
//! boot-shim's fixed pool (root page, page-table subrange, module pages,
//! arena) and the kernel image's resumed page-table allocator (the
//! page-table subrange only). They are deliberately *separate instances over
//! disjoint fixed partitions* — merging their cursors was tried and breaks
//! the partition semantics: the shim's cursor moves past the subrange end
//! (modules and the arena follow it), so resuming the kernel allocator from
//! that cursor makes `BootAlloc::resume` see `next > end`.
//!
//! What the two instances do share is this arithmetic core, so the monotonic
//! cursor and its bounds check cannot drift between them. An instance is a
//! plain value: each owner stores the three numbers in whatever form its
//! concurrency model needs (`AtomicU64` on the kernel side and on the
//! boot-shim pool, both single-threaded at this stage) and rebuilds the
//! value for each operation.

/// Base page size of all boot-stage mappings (4 KiB on every target).
pub const BUMP_PAGE_SIZE: u64 = 0x1000;

/// A monotonic bump region `[start, end)` with cursor `next`.
///
/// Construction does not validate: an uninitialized allocator is the legal
/// `(0, 0, 0)` state where every allocation fails, and a zero-length region
/// `start == end` is a legal "exhausted from the beginning" state. Values
/// arriving from outside the image (the cross-image wire payload) must be
/// checked with [`Self::is_consistent`] before use; values built from
/// compile-time constants are checked by the owner's `const` assertions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BumpRegion {
    start: u64,
    next: u64,
    end: u64,
}

impl BumpRegion {
    /// A fresh region with the cursor at `start`.
    pub const fn new(start: u64, end: u64) -> Self {
        Self {
            start,
            next: start,
            end,
        }
    }

    /// A region continuing from a cursor that a previous stage left behind.
    ///
    /// The caller owns validation (`is_consistent`); this constructor only
    /// carries the three numbers.
    pub const fn resumed(start: u64, next: u64, end: u64) -> Self {
        Self { start, next, end }
    }

    /// Region start (first page ever handed out).
    pub const fn start(&self) -> u64 {
        self.start
    }

    /// Cursor: address of the next page to hand out.
    pub const fn next(&self) -> u64 {
        self.next
    }

    /// Region end (exclusive).
    pub const fn end(&self) -> u64 {
        self.end
    }

    /// Bytes handed out so far.
    pub const fn used_bytes(&self) -> u64 {
        self.next.saturating_sub(self.start)
    }

    /// `start < end && start <= next <= end`.
    ///
    /// This is the predicate for values that crossed an image boundary: an
    /// inverted or out-of-range triple would let [`Self::alloc_pages`] hand
    /// out pages outside the declared region (and the DM-coverage
    /// registration would receive an inverted range).
    pub const fn is_consistent(&self) -> bool {
        self.start < self.end && self.start <= self.next && self.next <= self.end
    }

    /// Allocate `num_pages` contiguous pages and return the range's start.
    ///
    /// Returns `None` when the request does not fit. A zero-page request is
    /// rejected with a panic instead: a "successful" zero-page allocation
    /// would return the current cursor without advancing it, so two callers
    /// would receive the same address and the no-overlap invariant would
    /// silently break.
    pub fn alloc_pages(&mut self, num_pages: usize) -> Option<u64> {
        assert!(
            num_pages > 0,
            "BumpRegion::alloc_pages: num_pages must be > 0"
        );
        let need = (num_pages as u64).checked_mul(BUMP_PAGE_SIZE)?;
        let new_next = self.next.checked_add(need)?;
        if new_next > self.end {
            return None;
        }
        let addr = self.next;
        self.next = new_next;
        Some(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocations_ascend_by_page_and_report_usage() {
        let mut region = BumpRegion::new(0x1000_0000, 0x1000_3000);
        assert_eq!(region.alloc_pages(1), Some(0x1000_0000));
        assert_eq!(region.alloc_pages(2), Some(0x1000_1000));
        assert_eq!(region.next(), 0x1000_3000);
        assert_eq!(region.used_bytes(), 0x3000);
        assert_eq!(region.alloc_pages(1), None, "region is exhausted");
        assert_eq!(
            region.next(),
            0x1000_3000,
            "a failed allocation must not move the cursor"
        );
    }

    #[test]
    #[should_panic(expected = "num_pages must be > 0")]
    fn zero_page_request_panics_instead_of_returning_a_duplicate_address() {
        let mut region = BumpRegion::new(0x2000_0000, 0x2000_1000);
        let _ = region.alloc_pages(0);
    }

    #[test]
    fn overflow_in_page_count_or_address_is_a_failed_allocation() {
        let mut region = BumpRegion::new(0x3000_0000, 0x3000_1000);
        assert_eq!(region.alloc_pages(usize::MAX), None);
        let mut top = BumpRegion::new(u64::MAX - 0x800, u64::MAX);
        assert_eq!(top.alloc_pages(1), None);
    }

    #[test]
    fn consistency_predicate_rejects_inverted_and_out_of_range_triples() {
        assert!(BumpRegion::new(0x4000_0000, 0x4000_1000).is_consistent());
        assert!(BumpRegion::resumed(0x4000_0000, 0x4000_1000, 0x4000_2000).is_consistent());
        // Inverted region.
        assert!(!BumpRegion::resumed(0x5000_0000, 0x5000_1000, 0x4000_0000).is_consistent());
        // Cursor below the start.
        assert!(!BumpRegion::resumed(0x5000_0000, 0x4fff_f000, 0x5000_1000).is_consistent());
        // Cursor past the end.
        assert!(!BumpRegion::resumed(0x5000_0000, 0x5000_2000, 0x5000_1000).is_consistent());
    }

    #[test]
    fn zero_length_region_is_legal_and_exhausted() {
        let mut region = BumpRegion::new(0x6000_0000, 0x6000_0000);
        assert_eq!(region.alloc_pages(1), None);
        assert_eq!(region.used_bytes(), 0);
    }
}
