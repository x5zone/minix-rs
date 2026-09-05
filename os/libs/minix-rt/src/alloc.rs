//! Memory allocator: break management plus a slab allocator over pages.
//!
//! A C program's heap starts right after the program image: the linker
//! provides the symbol `_end`, an assembly stub publishes it as the initial
//! `_brksize` (`minix3/minix/lib/libc/arch/i386/sys/brksize.S`), and two
//! functions manage the boundary — `brk` moves it to an absolute address
//! through the virtual memory server, `sbrk` moves it relative to the current
//! position (`minix3/minix/lib/libc/sys/brk.c` and `sbrk.c`). The NetBSD
//! allocator on top mixes both strategies: small objects come from a
//! page-described arena grown with `sbrk`, large objects come straight from
//! memory mapping (`minix3/lib/libc/stdlib/malloc.c:317-388`).
//!
//! This module keeps the two-level shape but replaces both levels with owned
//! Rust types:
//!
//! 1. Break management becomes pure arithmetic ([`request_new_break`]): the
//!    overflow rule and the "only call the kernel when the address actually
//!    changed" rule, testable without any server.
//! 2. The arena becomes [`SlabAllocator`]: fixed size classes served from
//!    single pages with intrusive free lists, large objects served as whole
//!    page runs. Pages arrive through the [`PageSupplier`] trait, so tests
//!    serve them from plain buffers while real binaries chain a static pool
//!    and, later, virtual memory mapping.
//!
//! # Execution model
//!
//! The owned [`SlabAllocator`] needs no synchronization: tests and callers
//! own their instance. The single global instance behind [`global_alloc`]
//! and [`global_free`] assumes a single-threaded user process — the same
//! assumption the rest of this crate's startup path makes. Thread support
//! will revisit this contract when it lands.

use minix_types::Errno;

/// One page holds this many bytes.
///
/// The NetBSD allocator rounds its arena growth to page multiples (see
/// `malloc.c:381`), and the virtual memory server maps whole pages. Four
/// kibibytes is the page granularity on 64-bit Intel hardware.
pub const PAGE_BYTES: usize = 4096;

/// Small-object size classes in bytes.
///
/// Every class is a multiple of eight, so every slot is eight-byte aligned.
/// Objects up to the largest class come from slabs; anything bigger becomes
/// a whole page run. Nine classes keep internal waste below a factor of two
/// for every size while keeping the class search trivial.
pub const SIZE_CLASSES: [usize; 9] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

/// Largest object still served from a slab.
pub const MAX_SLAB_OBJECT_BYTES: usize = 2048;

/// Maximum slabs one allocator tracks (one page per slab).
pub const MAX_SLABS: usize = 64;

/// Maximum simultaneous whole-page allocations one allocator tracks.
pub const MAX_BIG_BLOCKS: usize = 32;

/// Initial heap pool for the global allocator, in bytes (sixteen pages).
pub const GLOBAL_POOL_BYTES: usize = 16 * PAGE_BYTES;

/// Why an allocation or break adjustment failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    /// No memory left: the supplier has no more pages, or every tracking
    /// slot is in use.
    OutOfMemory,
    /// The requested adjustment is malformed: a negative break move that
    /// wraps around, or a zero-sized allocation.
    InvalidRequest,
}

impl AllocError {
    /// Maps the failure to the closest Minix3 error number.
    ///
    /// Exhaustion is `ENOMEM` (value 12), the same error the C library
    /// reports when the virtual memory server refuses to grow the heap. A
    /// malformed request is `EINVAL` (value 22), the error the C library
    /// uses for malformed argument vectors. Both come from
    /// `minix3/sys/sys/errno.h`, so no new error code is invented.
    pub const fn to_errno(self) -> Errno {
        match self {
            AllocError::OutOfMemory => Errno::from_i32(minix_types::ENOMEM),
            AllocError::InvalidRequest => Errno::from_i32(minix_types::EINVAL),
        }
    }
}

/// Computes the new break address for a relative adjustment.
///
/// This is the pure half of `sbrk` (`minix3/minix/lib/libc/sys/sbrk.c:13-26`):
/// add the signed increment to the current break and reject wrap-around in
/// both directions (growing past the top or shrinking below the bottom, see
/// `sbrk.c:20-21`). Pointer arithmetic cannot be unit tested without mapped
/// memory; integer arithmetic over addresses can.
pub const fn request_new_break(current_break: u64, increment: i64) -> Result<u64, AllocError> {
    let candidate = current_break.wrapping_add(increment as u64);
    if increment > 0 && candidate < current_break {
        return Err(AllocError::InvalidRequest);
    }
    if increment < 0 && candidate > current_break {
        return Err(AllocError::InvalidRequest);
    }
    Ok(candidate)
}

/// Reports whether the kernel must be told about a new break address.
///
/// C: `brk` skips the server call entirely when the requested address equals
/// the cached `_brksize` (`minix3/minix/lib/libc/sys/brk.c:27-32`). The
/// server round trip is the expensive part of moving the break; this
/// predicate keeps the "did anything change" decision in one tested place.
pub const fn needs_kernel_update(cached_break: u64, requested_break: u64) -> bool {
    cached_break != requested_break
}

/// Source of whole pages for an allocator.
///
/// The trait separates page supply (a mechanism owned by the platform: a test
/// buffer, a static pool, or virtual memory mapping) from page management
/// (the slab policy in [`SlabAllocator`]). It has two behaviorally different
/// implementations today (always-refuse vs. fixed pool, plus a virtual
/// memory mapping supplier planned for later) and is used as a generic
/// bound, which keeps the abstraction justified under the project rule
/// for traits.
///
/// Contiguity contract: [`supply_pages`] must hand out one contiguous run;
/// single pages from [`supply_page`] carry no adjacency promise.
pub trait PageSupplier {
    /// Hands out one zeroed page, or `None` when exhausted.
    fn supply_page(&mut self) -> Option<*mut u8>;
    /// Hands out `page_count` zeroed contiguous pages, or `None`.
    fn supply_pages(&mut self, page_count: usize) -> Option<*mut u8>;
    /// Returns a previously supplied page.
    fn release_page(&mut self, page: *mut u8);
    /// Returns a previously supplied contiguous run.
    ///
    /// # Safety
    ///
    /// `first` must be the start of a run of exactly `page_count` pages
    /// previously handed out by [`supply_pages`] on this same supplier.
    unsafe fn release_pages(&mut self, first: *mut u8, page_count: usize) {
        let mut index = 0;
        while index < page_count {
            // SAFETY: upheld by the caller contract above.
            self.release_page(unsafe { first.add(index * PAGE_BYTES) });
            index += 1;
        }
    }
}

/// Page supplier that always refuses.
///
/// Models the state before any memory source is wired: every allocation
/// fails fast with a null pointer instead of faulting. Tests use it to cover
/// the exhaustion paths deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FailingSupplier;

impl PageSupplier for FailingSupplier {
    fn supply_page(&mut self) -> Option<*mut u8> {
        None
    }
    fn supply_pages(&mut self, _page_count: usize) -> Option<*mut u8> {
        None
    }
    fn release_page(&mut self, _page: *mut u8) {}
}

/// Page supplier carving pages out of a fixed buffer.
///
/// The buffer is split into [`PAGE_BYTES`]-byte pages handed out in order;
/// released pages go onto a free stack for reuse. The global allocator feeds
/// this supplier from its static pool; tests feed it from local arrays, which
/// keeps every test hermetic.
///
/// Alignment: the usable region starts at the first eight-byte boundary
/// inside the buffer (at most seven leading bytes are skipped), so every
/// page start — and therefore every slot — is eight-byte aligned no matter
/// how the caller aligned the buffer.
#[derive(Debug)]
pub struct FixedPoolSupplier<'a> {
    // Holds the exclusive borrow of the backing buffer for the whole
    // supplier lifetime; addressing goes through base/usable_len below.
    // The field is never read directly, which is intentional.
    #[allow(dead_code)]
    pool: &'a mut [u8],
    base: *mut u8,
    usable_len: usize,
    next_page: usize,
    free_pages: [*mut u8; 64],
    free_count: usize,
}

impl<'a> FixedPoolSupplier<'a> {
    /// Borrows the buffer as a page pool. A trailing partial page is ignored,
    /// as are up to seven leading bytes before the first eight-byte boundary.
    pub fn new(pool: &'a mut [u8]) -> Self {
        let raw = pool.as_mut_ptr() as usize;
        let aligned = raw.next_multiple_of(8);
        let skip = aligned - raw;
        let usable_len = pool.len().saturating_sub(skip);
        FixedPoolSupplier {
            pool,
            base: aligned as *mut u8,
            usable_len,
            next_page: 0,
            free_pages: [core::ptr::null_mut(); 64],
            free_count: 0,
        }
    }

    /// How many whole pages the buffer holds.
    pub fn total_pages(&self) -> usize {
        self.usable_len / PAGE_BYTES
    }

    fn page_at(&self, index: usize) -> *mut u8 {
        // SAFETY: index is always below total_pages, so the offset stays
        // inside the aligned usable region, which sits inside the borrowed
        // buffer that outlives the supplier.
        unsafe { self.base.add(index * PAGE_BYTES) }
    }
}

impl PageSupplier for FixedPoolSupplier<'_> {
    fn supply_page(&mut self) -> Option<*mut u8> {
        if self.free_count > 0 {
            self.free_count -= 1;
            let page = self.free_pages[self.free_count];
            self.free_pages[self.free_count] = core::ptr::null_mut();
            // Zero the reused page so callers always observe clean memory.
            // SAFETY: the page came from this pool and is page-sized.
            unsafe { core::ptr::write_bytes(page, 0, PAGE_BYTES) };
            return Some(page);
        }
        if self.next_page < self.total_pages() {
            let page = self.page_at(self.next_page);
            self.next_page += 1;
            // SAFETY: fresh pool memory, page-sized by construction.
            unsafe { core::ptr::write_bytes(page, 0, PAGE_BYTES) };
            return Some(page);
        }
        None
    }

    fn supply_pages(&mut self, page_count: usize) -> Option<*mut u8> {
        if page_count == 0 {
            return None;
        }
        // Only the bump region hands out contiguous runs: recycled single
        // pages on the free stack are not necessarily adjacent, so they are
        // reserved for single-page requests.
        if self.next_page + page_count <= self.total_pages() {
            let first = self.page_at(self.next_page);
            self.next_page += page_count;
            // SAFETY: fresh pool memory, sized by construction.
            unsafe { core::ptr::write_bytes(first, 0, page_count * PAGE_BYTES) };
            return Some(first);
        }
        None
    }

    fn release_page(&mut self, page: *mut u8) {
        if self.free_count < self.free_pages.len() {
            self.free_pages[self.free_count] = page;
            self.free_count += 1;
        }
        // When the free stack is full the page is dropped: the pool owns it,
        // so nothing leaks outside the pool; the supplier simply forgets one
        // reusable page. The stack holds 64 entries, far above any
        // realistic pool, so this branch is defensive only.
    }
}

/// One slab: a single page cut into equal slots of one size class.
#[derive(Debug, Clone, Copy)]
struct Slab {
    page: *mut u8,
    class_index: u8,
    slot_bytes: u16,
    slot_count: u16,
    free_head: u16,
    free_count: u16,
}

/// Sentinel meaning "no free slot" in a free list head.
const NO_FREE_SLOT: u16 = u16::MAX;

/// One whole-page allocation record.
#[derive(Debug, Clone, Copy)]
struct BigBlock {
    page: *mut u8,
    page_count: usize,
}

/// Slab allocator over a page supplier.
///
/// Small objects (up to [`MAX_SLAB_OBJECT_BYTES`] bytes) come from slabs:
/// each slab dedicates one page to one size class and threads its free slots
/// into an intrusive list, storing the next-slot index in the first two
/// bytes of each free slot. Large objects become whole page runs tracked in
/// a fixed record table. A zero-sized request returns null: unlike some C
/// libraries, this allocator never hands out an ambiguous zero-byte block,
/// and the contract is documented on [`SlabAllocator::alloc`] so callers can
/// rely on it.
#[derive(Debug)]
pub struct SlabAllocator<S: PageSupplier> {
    supplier: S,
    slabs: [Option<Slab>; MAX_SLABS],
    big_blocks: [Option<BigBlock>; MAX_BIG_BLOCKS],
}

impl<S: PageSupplier> SlabAllocator<S> {
    /// Creates an allocator that draws pages from `supplier`.
    pub const fn new(supplier: S) -> Self {
        SlabAllocator {
            supplier,
            slabs: [None; MAX_SLABS],
            big_blocks: [None; MAX_BIG_BLOCKS],
        }
    }

    /// Selects the size class for `size`, or `None` for the page-run path.
    fn class_for(size: usize) -> Option<usize> {
        if size == 0 || size > MAX_SLAB_OBJECT_BYTES {
            return None;
        }
        let mut index = 0;
        while index < SIZE_CLASSES.len() {
            if SIZE_CLASSES[index] >= size {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    fn read_next(slot: *mut u8) -> u16 {
        // SAFETY: the slot belongs to a live slab page and stores a u16 at
        // offset zero by construction; slots are two-byte aligned because
        // every class size is a multiple of eight.
        unsafe { (slot as *const u16).read() }
    }

    fn write_next(slot: *mut u8, next: u16) {
        // SAFETY: same bounds as read_next; the write stays inside the slot.
        unsafe { (slot as *mut u16).write(next) };
    }

    fn slot_at(page: *mut u8, slot_bytes: usize, index: u16) -> *mut u8 {
        // SAFETY: index is always below the slab slot count, so the slot
        // stays inside the page.
        unsafe { page.add(index as usize * slot_bytes) }
    }

    fn find_slab_with_room(&self, class_index: usize) -> Option<usize> {
        let mut index = 0;
        while index < MAX_SLABS {
            if let Some(slab) = self.slabs[index]
                && slab.class_index as usize == class_index
                && slab.free_count > 0
            {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    fn add_slab(&mut self, class_index: usize) -> Option<usize> {
        let page = self.supplier.supply_page()?;
        let slot_bytes = SIZE_CLASSES[class_index];
        let slot_count = (PAGE_BYTES / slot_bytes) as u16;
        let mut slot = 0;
        while slot < slot_count {
            let next = if slot + 1 < slot_count {
                slot + 1
            } else {
                NO_FREE_SLOT
            };
            Self::write_next(Self::slot_at(page, slot_bytes, slot), next);
            slot += 1;
        }
        let mut index = 0;
        while index < MAX_SLABS {
            if self.slabs[index].is_none() {
                self.slabs[index] = Some(Slab {
                    page,
                    class_index: class_index as u8,
                    slot_bytes: slot_bytes as u16,
                    slot_count,
                    free_head: 0,
                    free_count: slot_count,
                });
                return Some(index);
            }
            index += 1;
        }
        // No slab record free: hand the page back so the supplier can reuse
        // it instead of stranding it inside the allocator.
        self.supplier.release_page(page);
        None
    }

    fn alloc_from_slab(&mut self, slab_index: usize) -> *mut u8 {
        let slab = self.slabs[slab_index].as_mut().expect("slab exists");
        let slot_index = slab.free_head;
        let slot = Self::slot_at(slab.page, slab.slot_bytes as usize, slot_index);
        slab.free_head = Self::read_next(slot);
        slab.free_count -= 1;
        slot
    }

    fn alloc_big(&mut self, size: usize) -> *mut u8 {
        let pages = size.div_ceil(PAGE_BYTES);
        let first = match self.supplier.supply_pages(pages) {
            Some(first) => first,
            None => return core::ptr::null_mut(),
        };
        let mut index = 0;
        while index < MAX_BIG_BLOCKS {
            if self.big_blocks[index].is_none() {
                self.big_blocks[index] = Some(BigBlock {
                    page: first,
                    page_count: pages,
                });
                return first;
            }
            index += 1;
        }
        // No record free: this allocation could never be freed later by
        // address lookup, so hand the run back and refuse it rather than
        // create an untracked block.
        // SAFETY: the run just came from supply_pages above.
        unsafe { self.supplier.release_pages(first, pages) };
        core::ptr::null_mut()
    }

    /// Allocates `size` bytes, returning null on any failure.
    ///
    /// The returned pointer is eight-byte aligned and points at readable,
    /// writable memory. A null return means "no memory" (supplier exhausted
    /// or tracking full) or "zero-sized request"; both are fail-fast instead
    /// of faulting later. Callers must pass the pointer back to [`free`]
    /// exactly once, or leak it deliberately with a comment.
    pub fn alloc(&mut self, size: usize) -> *mut u8 {
        match Self::class_for(size) {
            Some(class_index) => {
                if let Some(slab_index) = self.find_slab_with_room(class_index) {
                    return self.alloc_from_slab(slab_index);
                }
                match self.add_slab(class_index) {
                    Some(slab_index) => self.alloc_from_slab(slab_index),
                    None => core::ptr::null_mut(),
                }
            }
            None => {
                if size == 0 {
                    return core::ptr::null_mut();
                }
                self.alloc_big(size)
            }
        }
    }

    /// Returns a block to the allocator.
    ///
    /// A null pointer is accepted and ignored, matching the C convention that
    /// freeing null is a no-op. Any other pointer must have come from
    /// [`alloc`] on this same allocator and must not have been freed before.
    /// Anything else stops the process with a panic instead of corrupting
    /// the heap silently: fail-fast beats silent corruption, and the C
    /// library answers the same situation by aborting with a heap-corruption
    /// diagnostic.
    pub fn free(&mut self, pointer: *mut u8) {
        if pointer.is_null() {
            return;
        }
        let mut index = 0;
        while index < MAX_SLABS {
            if let Some(slab) = self.slabs[index].as_mut() {
                let base = slab.page as usize;
                let address = pointer as usize;
                if address >= base && address < base + PAGE_BYTES {
                    let offset = address - base;
                    let slot_bytes = slab.slot_bytes as usize;
                    assert!(
                        offset.is_multiple_of(slot_bytes)
                            && offset / slot_bytes < slab.slot_count as usize,
                        "minix-rt: free of a pointer that is not a slot start"
                    );
                    Self::write_next(pointer, slab.free_head);
                    slab.free_head = (offset / slot_bytes) as u16;
                    slab.free_count += 1;
                    return;
                }
            }
            index += 1;
        }
        let mut big = 0;
        while big < MAX_BIG_BLOCKS {
            if let Some(block) = self.big_blocks[big]
                && block.page == pointer
            {
                // SAFETY: the record proves this run came from supply_pages.
                unsafe { self.supplier.release_pages(block.page, block.page_count) };
                self.big_blocks[big] = None;
                return;
            }
            big += 1;
        }
        panic!("minix-rt: free of a pointer this allocator never handed out");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool_buffer() -> [u8; 16384] {
        [0u8; 16384]
    }

    fn test_allocator(pool: &mut [u8]) -> SlabAllocator<FixedPoolSupplier<'_>> {
        SlabAllocator::new(FixedPoolSupplier::new(pool))
    }

    #[test]
    fn test_break_grows_and_shrinks() {
        assert_eq!(request_new_break(0x1000, 0x500), Ok(0x1500));
        assert_eq!(request_new_break(0x1500, -0x500), Ok(0x1000));
        assert_eq!(request_new_break(0x1000, 0), Ok(0x1000));
    }

    #[test]
    fn test_break_overflow_in_both_directions_is_rejected() {
        assert_eq!(
            request_new_break(u64::MAX, 1),
            Err(AllocError::InvalidRequest)
        );
        assert_eq!(request_new_break(0, -1), Err(AllocError::InvalidRequest));
    }

    #[test]
    fn test_unchanged_break_needs_no_kernel_call() {
        assert!(!needs_kernel_update(0x1000, 0x1000));
        assert!(needs_kernel_update(0x1000, 0x2000));
    }

    #[test]
    fn test_alloc_returns_writable_aligned_memory() {
        let mut pool = pool_buffer();
        let mut allocator = test_allocator(&mut pool);
        let pointer = allocator.alloc(13);
        assert!(!pointer.is_null());
        assert_eq!(pointer as usize % 8, 0);
        // SAFETY: the allocator just handed out 16 bytes here.
        unsafe {
            core::ptr::write_bytes(pointer, 0xAB, 13);
            assert_eq!(core::ptr::read(pointer), 0xAB);
        }
    }

    #[test]
    fn test_zero_sized_request_returns_null() {
        let mut pool = pool_buffer();
        let mut allocator = test_allocator(&mut pool);
        assert!(allocator.alloc(0).is_null());
    }

    #[test]
    fn test_free_null_is_a_no_op() {
        let mut pool = pool_buffer();
        let mut allocator = test_allocator(&mut pool);
        allocator.free(core::ptr::null_mut());
    }

    #[test]
    fn test_freed_slot_is_reused() {
        let mut pool = pool_buffer();
        let mut allocator = test_allocator(&mut pool);
        let first = allocator.alloc(8);
        allocator.free(first);
        let second = allocator.alloc(8);
        // Last-in-first-out reuse: the same slot comes back.
        assert_eq!(first, second);
    }

    #[test]
    fn test_exhausted_supplier_fails_fast() {
        let mut allocator = SlabAllocator::new(FailingSupplier);
        assert!(allocator.alloc(8).is_null());
        assert!(allocator.alloc(8192).is_null());
    }

    #[test]
    fn test_big_allocation_spans_whole_pages() {
        let mut pool = pool_buffer();
        let mut allocator = test_allocator(&mut pool);
        let pointer = allocator.alloc(5000);
        assert!(!pointer.is_null());
        // SAFETY: 5000 bytes across two pages were just handed out.
        unsafe {
            core::ptr::write_bytes(pointer, 0xCD, 5000);
            assert_eq!(core::ptr::read(pointer.add(4999)), 0xCD);
        }
        allocator.free(pointer);
    }

    #[test]
    fn test_size_classes_cover_every_small_size() {
        // Every size up to the slab limit maps to a class that fits it.
        let mut size = 1;
        while size <= MAX_SLAB_OBJECT_BYTES {
            let class = SlabAllocator::<FailingSupplier>::class_for(size).expect("class exists");
            assert!(SIZE_CLASSES[class] >= size);
            size += 1;
        }
        assert_eq!(
            SlabAllocator::<FailingSupplier>::class_for(MAX_SLAB_OBJECT_BYTES + 1),
            None
        );
    }

    #[test]
    fn test_alloc_errors_map_to_documented_errnos() {
        assert_eq!(
            AllocError::OutOfMemory.to_errno(),
            Errno::from_i32(minix_types::ENOMEM)
        );
        assert_eq!(
            AllocError::InvalidRequest.to_errno(),
            Errno::from_i32(minix_types::EINVAL)
        );
    }
}
