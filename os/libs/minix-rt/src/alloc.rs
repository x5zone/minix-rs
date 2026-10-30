//! Memory allocator: break management plus a slab allocator over pages.
//!
//! A C program's heap starts right after the program image: the linker
//! provides the symbol `_end`, an assembly stub publishes it as the initial
//! `_brksize` (`minix3/minix/lib/libc/arch/i386/sys/brksize.S`), and two
//! functions manage the boundary — `brk` moves it to an absolute address
//! through the virtual memory server, `sbrk` moves it relative to the current
//! position (`minix3/minix/lib/libc/sys/brk.c` and `sbrk.c`). The NetBSD
//! allocator on top mixes both strategies: small objects come from a
//! page-described arena grown with `sbrk`, and the memory mapping path
//! serves the allocator's page directory
//! (`minix3/lib/libc/stdlib/malloc.c:317-388` is the `sbrk` growth loop;
//! the `MMAP` requests live at `malloc.c:448` and `malloc.c:559`).
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

pub(crate) use minix_sys::ipc::{DirectTrapTransport, IpcTransport};
use minix_types::{Errno, VirBytes};

/// One page holds this many bytes.
///
/// The NetBSD allocator rounds its arena growth to page multiples (see
/// `malloc.c:383`), and the virtual memory server maps whole pages. Four
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
///
/// Must cover every pool page: a slab dedicates one page, so if this is
/// smaller than the pool's page count the fixed `slabs` record table fills
/// while pool bytes are still free, and a request fails with "out of memory"
/// despite an 87%-empty pool. Real-machine measurement (`OOM-RT
/// slabs=64/64 … px=65/512`): `MAX_SLABS=64` capped slab objects at ~256 KiB of
/// a 2 MiB pool because the pool had been grown to 512 pages without raising
/// this table. Tying it to the pool page count keeps tracking and byte capacity
/// consistent.
pub const MAX_SLABS: usize = GLOBAL_POOL_BYTES / PAGE_BYTES;

/// Maximum simultaneous whole-page allocations one allocator tracks.
///
/// A premature "out of memory" was confirmed on the real machine: after
/// `/bin/sh` was allowed to run, a cross-space `kdst copy` of a 4 KiB run hit
/// `OOM-RT size=001000 … big=20/20 px=094/400` (hex: `big=0x20/0x20` =
/// 32/32 record table full while `px=0x94/0x400` = 148/1024 pages were still
/// free)—the same defect class as the big-block-heavy server case noted
/// earlier, and the same kind as the `MAX_SLABS` one above: every big block is
/// at least one page, so the principled ceiling is [`GLOBAL_POOL_PAGES`].
///
/// It was not tied to `GLOBAL_POOL_PAGES` at first: that enlarges the
/// allocator's static instance (the `big_blocks` array in `GlobalAllocator`) —
/// each `Option<BigBlock>` slot is 24 B on x86_64 (`*mut u8` has no niche, so
/// `Option` needs a separate discriminant), and going from 32 to 1024 slots
/// costs ≈ +24 KB, shifting every user server's .bss/user-stack base. Boot
/// maps service images demand-fill-on-first-touch (no eager materialization),
/// and measurement showed VM deterministically panicking on a self-fault during
/// early boot (crashing back from line 30090 to line 124). That is a structural
/// debt repeatedly deferred (boot eager materialization / VM-backed heap
/// supplier). So a conservative capacity round of the same kind as the earlier
/// ones raised the table to 64—covering the measured real-machine peak (big
/// peak 47): (64−32)×24 B = +768 B < one page, never crossing an unmaterialized
/// page. The principled fix is to bind `GLOBAL_POOL_PAGES` once boot eager
/// materialization lands.
///
/// A later single-core rebuild with the same image twice pinned the exact gap in
/// "boot eager materialization": raising this constant to
/// `GLOBAL_POOL_PAGES`=1024 made `OOM-RT`=0 (the premature OOM disappeared),
/// but boot deterministically faulted at ~line 116 (`pagefault in VM`,
/// rip=0x23581b, cr2=rsp=0x7ffffffeef88, err=0x6 not-present); raising it to
/// 128 crashed the same way, and going back to 64 with a fresh rebuild
/// reproduced line 29685 down to the premature OOM (`OOM-RT`=1)—ruling out
/// build-artifact sensitivity and establishing the constant itself as the
/// regression cause. Root cause: `install_boot_stack` eagerly maps only **one**
/// frame page per boot service, and the remaining pages of the 4 MiB stack area
/// fault on demand, while VM cannot serve its own boot faults; `exec_bootproc`'s
/// eager `memsz` covers only PT_LOAD/.bss and **not the initial stack runway**
/// (so the earlier guess that eager `memsz` had resolved it was overturned).
/// ⇒ The real precondition for the principled fix is "materialize the boot
/// initial stack's eager runway" (the counterpart of C
/// `handle_memory_once`/`MAP_PREALLOC`, a known divergence), not merely raising
/// this constant. So 64 stayed (not a rollback—the only known non-crashing
/// value), bundling the constant raise and the eager-runway fix as one unit.
///
/// The eager-runway fix has landed: at the right spot,
/// `os/arch/src/arch/boot.rs` raises VM's own initial-stack eager runway
/// `VM_STACK_SIZE` from 64 KiB to 256 KiB (covering the measured ~65 KiB boot
/// stack descent). A later correction: the real spot is the kernel's
/// `load_vm_elf` (`install_boot_stack` is never called for VM), not memtype
/// `ev_new`. With the precondition gone, this constant binds
/// `GLOBAL_POOL_PAGES` directly and root-fixes the premature OOM (the table
/// below preserves the earlier conservative round's history).
pub const MAX_BIG_BLOCKS: usize = GLOBAL_POOL_PAGES;

// The old compile-time guard is retired: it required the
// `(MAX_BIG_BLOCKS - 32)` array growth to stay under one page to avoid a boot
// panic. Measurements showed that panic came from VM's own initial-stack
// runway being too short (fixed in `boot.rs` `VM_STACK_SIZE`), **not** from
// this array growth crossing an unmaterialized page—VM's `.bss` is eagerly
// materialized whole-segment by `load_vm_elf`'s loop per memsz. Binding
// `GLOBAL_POOL_PAGES` (one record per pool page) is the principled ceiling,
// so the one-page guard was deleted.

/// Initial heap pool for the global allocator, in bytes (2048 pages).
///
/// The pool is `.bss` storage, so its size charges each binary's image
/// segment. The bootproc leg materializes every segment page eagerly
/// (VM `exec_bootproc` copies the ELF image through freshly allocated
/// PFNs), and C additionally preallocates at mmap time for anything
/// carrying `MAP_PREALLOC` (C's image memmap leg is
/// `MAP_ANON|MAP_PREALLOC|MAP_UNINITIALIZED|MAP_FIXED`,
/// `minix3/minix/lib/libexec/exec_general.c:25`; `region.c:492-499`
/// takes the frames right there) — although this rewrite's VM currently
/// only records the PREALLOC bit and demand-fills on first touch
/// (`mmap.rs` `PREALLOC_MAP`, `memtype.rs` `AnonymousMemory` has no
/// `ev_new`), a known divergence. Either way the
/// bootproc leg prices the pool in real pages at exec time, which caps
/// how far it may grow until the VM-backed supplier lands (C's RS asks
/// for an 8 MiB prealloc map outright — `rs/const.h:83
/// RS_VM_DEFAULT_MAP_PREALLOC_LEN`).
/// The former 16-page pool OOM'd real servers —
/// RS's boot allocates a 256512-byte table and aborted before its main
/// loop (measured: "memory allocation of 256512 bytes failed"). C servers
/// grow a real heap through VM `brk`; a
/// VM-backed supplier is the follow-up (alloc.rs §module docs, "later,
/// virtual memory mapping").
///
/// 512 pages died on the real machine (VFS panic
/// `memory allocation of 40960 bytes failed`, raw hex diag
/// `size=00a000 slabs=009/200 big=06/20 px=200/200 fp=0aa/200`): VFS's
/// own tables alone consume ~327 pages — `FProc` is
/// 4368 B (`filps: [Option<usize>; 255]` costs 16 B per slot where C's
/// `file_desc` pointer costs 8 B) × 256 slots, plus `filp[1024]` and
/// `vnode[1024]` — and the freed exec buffers cannot be reused for the
/// next multi-page `hdr_buf` run because the free stack only serves
/// single pages (see `supply_pages`). A legal working set, not a leak:
/// C keeps those tables in BSS and never caps its heap. 1024 pages
/// doubles the runway; the faithful fix is the VM-backed supplier.
///
/// A later real-machine ATF-suite run showed the 1024-page pool
/// hitting the same wall again from a new consumer — MFS's block cache pool
/// (`DEFAULT_POOL_BUFFERS` 1024 × 4 KiB lazily-filled `Buffer` pages, the C
/// `DEFAULT_NR_BUFS` faithful value) legally fills the whole pool while the
/// 16 MiB imgrd (ATF suite seeding) grows the working set, leaving
/// zero runway for transient allocations: `oomrt caller=0xa` (MFS_PROC_NR),
/// `big=3b6/400 px=400/400 fp=0`. Budget identity for alloc-global servers
/// with a C-sized cache: cache pages + transient headroom ≤ pool pages —
/// 2048 doubles the runway again (the same capacity-round stance as above; VM
/// is unaffected: it does not take `alloc-global`, `os/servers/vm/Cargo.toml:
/// 47`, so the .bss-self-paging landmine does not apply here). The
/// faithful fix stays the VM-backed supplier (module docs).
#[cfg(not(target_arch = "x86_64"))]
pub const GLOBAL_POOL_BYTES: usize = 2048 * PAGE_BYTES; // MFS cache budget round

/// x86_64 tiering—the x86 regression fix for the 2048-page capacity round (the
/// real-machine follow-up after that round): the same full boot falls back from
/// init runcom to SingleUser on x86 (aarch64 with the same pool is healthy, with
/// real-machine gate readings), because x86's load leg has an independent budget
/// constraint against a 4→8 MiB .bss expansion—**the exact limit point is not
/// established** (candidate sites: the bootface identity/DM window, the
/// `copy_in_bytes` sibling window; note that demand-fill is VM `exec_bootproc`'s
/// architecture-independent leg and x86 goes through it too, so this cannot be
/// blamed on a demand-fill difference and must not be offered as the root
/// cause). x86 has no ATF suite requirement yet (xtask seeds only under
/// `ATF_BOOT_LEG_READY=[aarch64]`), so 1024 pages—the baseline value before the
/// 2048-page round—puts x86 artifacts back byte-identically to the old state,
/// with zero regression risk. riscv takes the 2048 tier below but its boot is
/// gated by (A) and has **never started with a 2048-page pool**: the effect of
/// the .bss expansion on riscv's load leg is unverified, and once (A) unlocks
/// it needs the same regression as x86. The principled fix (a demand-zero
/// runtime segment or a VM-backed heap, unifying all three architectures at
/// 2048+) is recorded in the module docs; delete this tiering when it lands.
#[cfg(target_arch = "x86_64")]
pub const GLOBAL_POOL_BYTES: usize = 1024 * PAGE_BYTES;

/// Number of whole pages the global pool holds (`GLOBAL_POOL_BYTES` /
/// [`PAGE_BYTES`]). Used to size the free-page stack and slab record table
/// so neither caps before the pool bytes are exhausted.
pub const GLOBAL_POOL_PAGES: usize = GLOBAL_POOL_BYTES / PAGE_BYTES;

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
    /// Pool cursor snapshot: `(pages bumped, pages in the pool, reusable
    /// pages on the free stack)`. Lets a caller tell "every round really grew
    /// until the pool ran out" apart from "fragmented / tracking-array capped".
    /// The default implementation serves suppliers that keep no such cursors
    /// (it returns all zeroes, which callers ignore).
    fn pool_diag(&self) -> (usize, usize, usize) {
        (0, 0, 0)
    }
    /// Whether this supplier is in the VM-backed form (used for install
    /// idempotence; defaults to `false`, and the VM form overrides it).
    fn is_vm_form(&self) -> bool {
        false
    }
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
    free_pages: [*mut u8; GLOBAL_POOL_PAGES],
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
            free_pages: [core::ptr::null_mut(); GLOBAL_POOL_PAGES],
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
        // A one-page run carries no adjacency requirement, so it is exactly
        // the "single-page request" the comment below reserves the free
        // stack for: delegate to `supply_page`, which pops a recycled page
        // first and only then advances the bump cursor. Without this a
        // single-page big-block request fails the moment the bump cursor
        // reaches the pool end even though freed pages sit on the free stack
        // (measured on the real machine: `size=0x1000` refused while
        // `px=400/400` bump exhausted and `fp=39c/400`=924 pages stranded).
        if page_count == 1 {
            return self.supply_page();
        }
        // Multi-page runs still need contiguity the scattered free stack
        // cannot promise, so they come only from the fresh bump region:
        // recycled single pages on the free stack are not necessarily
        // adjacent. Serving runs from a coalesced free list (or a
        // VM-backed heap) is the deferred structural fix.
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
        // reusable page. The stack holds one slot per pool page (see
        // [`GLOBAL_POOL_PAGES`]), so it can never overflow before every page
        // is returned; this branch is defensive only.
    }

    fn pool_diag(&self) -> (usize, usize, usize) {
        (self.next_page, self.total_pages(), self.free_count)
    }
}

/// Heap page supplier backed by VM_BRK (the final landing point for VM-backed
/// heap supply).
///
/// C ground truth: `brk` in `minix3/minix/lib/libc/sys/brk.c`—it pushes the
/// break to an absolute address and forwards the request to the virtual memory
/// server; this supplier uses it as a whole-page supply mechanism:
/// [`PageSupplier::supply_pages`] advances the break by n pages at once and
/// returns the start of the range (C `map_region_extend_upto_v` extends the
/// existing region, so the VA is naturally contiguous; the growth leg on the VM
/// side is an adjacent ANON region). Zero-fill of new pages comes from ANON's
/// demand-zero semantics (the first touch faults in a zero page), which is
/// observably equivalent to [`FixedPoolSupplier`]'s explicit `write_bytes`
/// clearing.
///
/// # Release semantics (the key design point behind swapping suppliers)
///
/// Before the install point the global allocator has already taken pages from
/// the fixed pool (the allocations of the birth process itself); the release
/// records for those old pages still live in `SlabAllocator`, so returning them
/// lands here—but pool pages are not inside this supplier's heap range and must
/// not be mistaken for VM pages. Therefore [`release_page`] is an honest no-op
/// for out-of-range pointers: a pool page's lifetime equals the process
/// lifetime, so a final release means nothing; out-of-range releases are counted
/// in [`Self::foreign_releases`] for real-machine diagnosis, preventing a future
/// range-computation defect from being swallowed silently. In-range releases do
/// not lower the break either—C shrinking brk never unmaps (`brk.rs`
/// `extend_upto_v`'s "a lower break never unmaps"), the page stays covered by
/// the region, and the supplier simply stops reusing it.
pub struct VmBreakSupplier<T: IpcTransport> {
    transport: T,
    /// The VM-side data region's initial break (provided by the caller at
    /// install time; the counterpart of C's initial `_brksize` = the end of the
    /// data segment established by exec, with the source wired to the linker's
    /// `__bss_end` at the install point).
    heap_base: u64,
    /// Current break cache (the same shape as C libc's `_brksize` cache;
    /// [`break_via`]'s "same address means no round trip" short-circuit reuses
    /// it directly).
    top: u64,
    foreign_releases: usize,
}

impl<T: IpcTransport> VmBreakSupplier<T> {
    /// Builds a supplier around the VM-side initial break. An `initial_break` of
    /// 0 is meaningless and is rejected by [`install_vm_supplier`] (the global
    /// install door); direct construction (tests) takes that responsibility
    /// itself.
    pub fn new(transport: T, initial_break: u64) -> Self {
        VmBreakSupplier {
            transport,
            heap_base: initial_break,
            top: initial_break,
            foreign_releases: 0,
        }
    }

    /// Whole pages already supplied on the VM side (diagnostic: how many pages
    /// of the break range have been consumed).
    pub fn pages_supplied(&self) -> usize {
        ((self.top - self.heap_base) / PAGE_BYTES as u64) as usize
    }

    /// Out-of-range release count (releases of pages not owned here, such as
    /// old pool pages).
    pub fn foreign_releases(&self) -> usize {
        self.foreign_releases
    }

    fn owns(&self, page: *mut u8) -> bool {
        let address = page as u64;
        address >= self.heap_base && address < self.top
    }
}

impl<T: IpcTransport> PageSupplier for VmBreakSupplier<T> {
    fn supply_page(&mut self) -> Option<*mut u8> {
        self.supply_pages(1)
    }

    fn supply_pages(&mut self, page_count: usize) -> Option<*mut u8> {
        if page_count == 0 {
            return None;
        }
        let old_top = self.top;
        let new_top = old_top + page_count as u64 * PAGE_BYTES as u64;
        if new_top < old_top {
            // Address space exhausted (overflow): fail fast with None, the same
            // shape as pool exhaustion.
            return None;
        }
        // C: brk.c — any change of the break must issue a request; `break_via`
        // itself only handles the "unchanged" short-circuit.
        // An Errno (VM ENOMEM etc.) → None, the same shape as
        // [`FixedPoolSupplier`] exhaustion.
        match minix_sys::vm::break_via(&self.transport, VirBytes(old_top), VirBytes(new_top)) {
            Ok(_) => {
                self.top = new_top;
                // SAFETY: the range [old_top, new_top) was just extended and
                // covered by VM (ANON demand-zero, so the first touch yields a
                // zero page); page alignment is guaranteed by `extend_upto_v`'s
                // roundup.
                Some(old_top as *mut u8)
            }
            Err(_) => None,
        }
    }

    fn release_page(&mut self, page: *mut u8) {
        if !self.owns(page) {
            self.foreign_releases += 1;
        }
        // In-range and out-of-range pages are both never really reclaimed (see
        // the release semantics in the type docs).
    }

    fn pool_diag(&self) -> (usize, usize, usize) {
        // (pages consumed, pages total, pages free): the VM form has no fixed
        // total, so total/free are 0—an OOM diagnostic printing total 0 is how
        // one reads "switched to the VM form".
        (self.pages_supplied(), 0, 0)
    }

    /// Whether this is the VM-backed form (used for install idempotence; the
    /// fixed-pool form keeps the trait default `false`, and [`HeapSupplier`]
    /// forwards by its current arm).
    fn is_vm_form(&self) -> bool {
        true
    }
}

/// Runtime form of the global allocator's page supplier (VM-backed heap supply).
///
/// From birth until the VM handshake it is [`FixedPoolSupplier`] (behavior
/// exactly as before); after the handshake [`install_vm_supplier`] switches it
/// to [`VmBreakSupplier`]. An enum rather than a trait object: the global
/// instance avoids a vtable, and the switch point is a finite set known at
/// compile time (fixed pool / VM_BRK, with the `mmap` channel as later
/// evolution). Its lifetime follows the fixed-pool arm (the global instance uses
/// a `'static` `.bss` pool; tests may use a local buffer). The transport is
/// parameterized and defaults to [`DirectTrapTransport`]: the global instance
/// uses the default (real traps), and host tests inject a mock to pin the
/// closed loop.
pub enum HeapSupplier<'a, T = DirectTrapTransport>
where
    T: IpcTransport,
{
    /// `.bss` fixed pool (the birth default form).
    Fixed(FixedPoolSupplier<'a>),
    /// VM_BRK growing heap (installed after the handshake).
    Vm(VmBreakSupplier<T>),
}

impl<T: IpcTransport> PageSupplier for HeapSupplier<'_, T> {
    fn supply_page(&mut self) -> Option<*mut u8> {
        match self {
            HeapSupplier::Fixed(s) => s.supply_page(),
            HeapSupplier::Vm(s) => s.supply_page(),
        }
    }

    fn supply_pages(&mut self, page_count: usize) -> Option<*mut u8> {
        match self {
            HeapSupplier::Fixed(s) => s.supply_pages(page_count),
            HeapSupplier::Vm(s) => s.supply_pages(page_count),
        }
    }

    fn release_page(&mut self, page: *mut u8) {
        match self {
            HeapSupplier::Fixed(s) => s.release_page(page),
            HeapSupplier::Vm(s) => s.release_page(page),
        }
    }

    fn pool_diag(&self) -> (usize, usize, usize) {
        match self {
            HeapSupplier::Fixed(s) => s.pool_diag(),
            HeapSupplier::Vm(s) => s.pool_diag(),
        }
    }

    fn is_vm_form(&self) -> bool {
        matches!(self, HeapSupplier::Vm(_))
    }
}

// The match arms above need to know T at the `HeapSupplier::Fixed`
// construction site: the assignment/annotation context supplies it (the global
// instance uses the default DirectTrapTransport, tests use a mock).

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

    /// Swaps the page supplier: the switch point from the birth form (fixed
    /// pool) to the VM form.
    ///
    /// Caller contract: the new supplier must honestly no-op on releasing
    /// pointers outside its own range (old pool pages)—[`VmBreakSupplier`]'s
    /// release semantics satisfy this (out-of-range releases are only counted;
    /// see its type docs). The old pool pages' slab/free-list records stay in
    /// this structure unchanged: the swap does not affect the validity of any
    /// allocated block, only where subsequent [`supply_pages`] calls get new
    /// pages from.
    pub fn replace_supplier(&mut self, supplier: S) {
        self.supplier = supplier;
    }

    /// Whether the current supplier is in the VM-backed form (the idempotence
    /// test of [`install_vm_supplier`]).
    pub fn is_vm_form(&self) -> bool {
        self.supplier.is_vm_form()
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
        if size >= 0x10000 {
            let ptr = self.alloc_big(size);
            return if ptr.is_null() {
                core::ptr::null_mut()
            } else {
                ptr
            };
        }
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

    /// Allocator state snapshot: slabs in use, big blocks in use, and the pool
    /// cursor triple. Reading it tells whether `slabs_in_use` climbs toward
    /// `MAX_SLABS` (tracking-array / fragmentation exhaustion) or
    /// `pages_consumed` toward the pool size (byte exhaustion).
    pub fn diag(&self) -> AllocDiag {
        let mut slabs_in_use = 0;
        let mut index = 0;
        while index < MAX_SLABS {
            if self.slabs[index].is_some() {
                slabs_in_use += 1;
            }
            index += 1;
        }
        let mut big_in_use = 0;
        index = 0;
        while index < MAX_BIG_BLOCKS {
            if self.big_blocks[index].is_some() {
                big_in_use += 1;
            }
            index += 1;
        }
        let (pages_consumed, total_pages, free_pages) = self.supplier.pool_diag();
        AllocDiag {
            slabs_in_use,
            big_in_use,
            pages_consumed,
            total_pages,
            free_pages,
        }
    }
}

/// The snapshot shape `diag` reports; see [`SlabAllocator::diag`].
#[derive(Debug, Clone, Copy)]
pub struct AllocDiag {
    pub slabs_in_use: usize,
    pub big_in_use: usize,
    pub pages_consumed: usize,
    pub total_pages: usize,
    pub free_pages: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_sys::ipc::{AsyncSlot, IpcStatus, TrapStatus};

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

    /// Regression pin: the `big_blocks` record table was once hardcoded to 32,
    /// which caused a premature OOM while pool bytes were still plentiful (the
    /// record table full: measured `OOM-RT size=001000 big=20/20
    /// px=094/400`, hex = 32/32 table full while 148/1024 pages remained free).
    /// The conservative capacity round described in the constant's docs raised
    /// the table to 64 (covering the measured peak of 32/32), with the
    /// principled fix (binding [`GLOBAL_POOL_PAGES`]) pending boot eager
    /// materialization. This test allocates 40 single-page big blocks in a row
    /// from a 48-page pool (the old 32 cap returned null on the 33rd); every
    /// one must succeed, and after freeing them all a record slot must be
    /// reusable.
    #[test]
    fn test_big_block_table_not_capped_at_32() {
        let mut pool = [0u8; 48 * PAGE_BYTES];
        let mut allocator = test_allocator(&mut pool);
        let mut ptrs = [core::ptr::null_mut::<u8>(); 40];
        for slot in ptrs.iter_mut() {
            let p = allocator.alloc(PAGE_BYTES); // 4 KiB → a single big block
            assert!(!p.is_null(), "big block >32 must not premature-OOM");
            *slot = p;
        }
        for p in ptrs {
            allocator.free(p);
        }
        // After freeing everything one more big block must still be allocatable
        // (record-table slots are reclaimed).
        assert!(!allocator.alloc(PAGE_BYTES).is_null());
    }

    /// Regression pin: a single-page big block must be able to reuse the
    /// free-stack. The old `supply_pages` only walked the bump cursor, so once
    /// the cursor hit the pool end it refused to serve a 1-page request even
    /// with free pages on the free-stack (measured on the real machine:
    /// `size=0x1000` while `fp=924`, stranded). This test uses a 4-page pool:
    /// fill the bump first (4 single-page big blocks), release one back to the
    /// free-stack, then the next allocation must succeed from the reclaimed
    /// page.
    #[test]
    fn test_single_page_big_block_reuses_free_stack() {
        let mut pool = [0u8; 4 * PAGE_BYTES];
        let mut allocator = test_allocator(&mut pool);
        // Fill the bump area: 4 single-page big blocks, cursor at the pool end.
        let mut held = [core::ptr::null_mut::<u8>(); 4];
        for slot in held.iter_mut() {
            let p = allocator.alloc(PAGE_BYTES); // 4 KiB → one 1-page big block
            assert!(!p.is_null(), "fill must succeed while bump has room");
            *slot = p;
        }
        // Release one → back to the free-stack; the bump is now exhausted, so a
        // new request can only come from the reclaimed page (the old code's
        // `supply_pages(1)` necessarily returned null here).
        allocator.free(held[0]);
        held[0] = core::ptr::null_mut();
        let reused = allocator.alloc(PAGE_BYTES);
        assert!(
            !reused.is_null(),
            "1-page big block must reuse a page from the free stack"
        );
        allocator.free(reused);
        for p in held {
            if !p.is_null() {
                allocator.free(p);
            }
        }
    }

    #[test]
    fn test_exhausted_supplier_fails_fast() {
        let mut allocator = SlabAllocator::new(FailingSupplier);
        assert!(allocator.alloc(8).is_null());
        assert!(allocator.alloc(8192).is_null());
    }

    #[test]
    fn test_big_allocation_spans_whole_pages() {
        // The pool must be page-aligned for this test: the global pool is
        // `#[repr(align(4096))]`, and a large block starts at the first
        // page the supplier hands out — so with an aligned pool the block
        // start is page-aligned too. A plain array would not mirror that
        // contract.
        #[repr(align(4096))]
        struct AlignedPool([u8; 16384]);
        let mut pool = AlignedPool([0u8; 16384]);
        let mut allocator = SlabAllocator::new(FixedPoolSupplier::new(&mut pool.0));
        let pointer = allocator.alloc(5000);
        assert!(!pointer.is_null());
        // The block starts a whole two-page run: page-aligned by the
        // supplier's page-boundary arithmetic, writable across both pages.
        assert_eq!(
            pointer as usize % PAGE_BYTES,
            0,
            "large allocation must start on a supplier page boundary"
        );
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

    // ===== VmBreakSupplier closed loop (host mock transport) =====

    /// Shared state of the mock VM transport: `sendrec` always succeeds and
    /// records the brk request payload, while the other channels are never
    /// reached by this supplier and uniformly refuse. The host test pins the
    /// "break advance → page take → out-of-range release → ENOMEM → None" closed
    /// loop; the real trap channel is verified by the real-machine gate (the
    /// same shape as `FakeTimesTransport` in minix-sys `time.rs`).
    struct MockVmState {
        /// The requested address of each `VM_CALL_BREAK` (`break_via` writes it
        /// into the first 8 bytes of the raw payload).
        breaks: core::cell::RefCell<Vec<u64>>,
        /// When set, simulates VM refusing growth (the ENOMEM path).
        refuse_breaks: core::cell::Cell<bool>,
    }

    /// The transport is moved into [`VmBreakSupplier`] by value, so the test
    /// keeps a `Rc` clone of the state to observe/flip it after the move (the
    /// std test build allows this).
    struct MockVmTransport {
        state: std::rc::Rc<MockVmState>,
    }

    impl MockVmTransport {
        fn new(state: std::rc::Rc<MockVmState>) -> Self {
            MockVmTransport { state }
        }
    }

    impl minix_sys::ipc::IpcTransport for MockVmTransport {
        fn send(
            &self,
            _: minix_types::Endpoint,
            _: &minix_types::Message,
        ) -> Result<(), TrapStatus> {
            Err(TrapStatus(-1))
        }
        fn receive(
            &self,
            _: minix_types::Endpoint,
            _: &mut minix_types::Message,
        ) -> Result<IpcStatus, TrapStatus> {
            Err(TrapStatus(-1))
        }
        fn sendrec(
            &self,
            _: minix_types::Endpoint,
            message: &mut minix_types::Message,
        ) -> Result<(), TrapStatus> {
            if self.state.refuse_breaks.get() {
                return Err(TrapStatus(-1));
            }
            // SAFETY: the raw payload is 56 bytes, and `break_via` has just
            // written the first 8.
            let requested =
                unsafe { u64::from_ne_bytes(message.m_u.raw[..8].try_into().unwrap()) };
            self.state.breaks.borrow_mut().push(requested);
            Ok(())
        }
        fn notify(&self, _: minix_types::Endpoint) -> Result<(), TrapStatus> {
            Err(TrapStatus(-1))
        }
        fn sendnb(
            &self,
            _: minix_types::Endpoint,
            _: &minix_types::Message,
        ) -> Result<(), TrapStatus> {
            Err(TrapStatus(-1))
        }
        fn senda(&self, _: &[AsyncSlot]) -> Result<(), TrapStatus> {
            Err(TrapStatus(-1))
        }
        fn query_kerninfo_page(&self) -> Result<u64, TrapStatus> {
            Err(TrapStatus(-1))
        }
    }

    /// Closed loop: initial break → supply 2 pages → break advances and the old
    /// break is returned → supply 1 more page → in-range/out-of-range release
    /// semantics (out-of-range counted, in-range no-op) → None when VM refuses
    /// with no state rollback → the diagnostic triple (consumed 3 / total 0 /
    /// free 0).
    #[test]
    fn test_vm_break_supplier_growth_page_and_release_semantics() {
        let state = std::rc::Rc::new(MockVmState {
            breaks: core::cell::RefCell::new(Vec::new()),
            refuse_breaks: core::cell::Cell::new(false),
        });
        let mut supplier = VmBreakSupplier::new(MockVmTransport::new(state.clone()), 0x10_0000);
        assert_eq!(supplier.pages_supplied(), 0);
        assert!(supplier.is_vm_form());

        // Growth: supply 2 pages at once and return the old break
        // (SlabAllocator treats it as the big block's start address).
        let first = supplier.supply_pages(2).expect("VM 堆首次供给");
        assert_eq!(first as u64, 0x10_0000);
        assert_eq!(supplier.pages_supplied(), 2);
        assert_eq!(
            *state.breaks.borrow(),
            vec![0x10_0000 + 2 * PAGE_BYTES as u64]
        );

        // Supply 1 more page: the break continues from the cache instead of
        // repeating heap_base.
        let second = supplier.supply_page().expect("第二次供给");
        assert_eq!(second as u64, 0x10_0000 + 2 * PAGE_BYTES as u64);

        // In-range release: honest no-op, not counted as foreign (C shrinking
        // brk never unmaps).
        supplier.release_page(second);
        assert_eq!(supplier.foreign_releases(), 0);

        // Out-of-range release (old pool pages etc.): counted for real-machine
        // diagnosis, not reclaimed (the stack array address is only compared,
        // never touched).
        let foreign_buffer = [0u8; 16];
        supplier.release_page(foreign_buffer.as_ptr() as *mut u8);
        assert_eq!(supplier.foreign_releases(), 1);

        // VM refusal (the ENOMEM path): None, with neither the break cache nor
        // the request count rolling back.
        state.refuse_breaks.set(true);
        assert!(supplier.supply_pages(1).is_none());
        assert_eq!(supplier.pages_supplied(), 3);
        assert_eq!(state.breaks.borrow().len(), 2);

        // Diagnostics: the VM form has no fixed total, so total/free are 0.
        assert_eq!(supplier.pool_diag(), (3, 0, 0));
    }

    /// HeapSupplier's Fixed arm forwarding: diagnostics and allocation behavior
    /// match a bare FixedPoolSupplier (the regression pin that the birth form
    /// behaves unchanged).
    #[test]
    fn test_heap_supplier_fixed_form_serves_from_pool() {
        let mut pool = pool_buffer();
        let mut allocator: SlabAllocator<HeapSupplier<'_, DirectTrapTransport>> =
            SlabAllocator::new(HeapSupplier::Fixed(FixedPoolSupplier::new(&mut pool)));
        assert!(!allocator.is_vm_form());
        let diag = allocator.diag();
        assert_eq!(diag.total_pages, 4); // 16 KiB / 4 KiB
        let pointer = allocator.alloc(8);
        assert!(!pointer.is_null());
        allocator.free(pointer);
    }

    /// Switch closed loop: the birth form allocates from the fixed pool
    /// (unchanged behavior) → `replace_supplier` switches to the VM form →
    /// releasing the birth-time big block lands in the foreign count (honest
    /// no-op) → new allocations grow from the VM break → diagnostics switch the
    /// VM flag (total/free 0).
    #[test]
    fn test_replace_supplier_switches_to_vm_growth() {
        let mut pool = pool_buffer();
        let transport = MockVmTransport::new(std::rc::Rc::new(MockVmState {
            breaks: core::cell::RefCell::new(Vec::new()),
            refuse_breaks: core::cell::Cell::new(false),
        }));
        let mut allocator: SlabAllocator<HeapSupplier<'_, MockVmTransport>> =
            SlabAllocator::new(HeapSupplier::Fixed(FixedPoolSupplier::new(&mut pool)));
        // Birth phase: the fixed pool hands out one 1-page big block (a release
        // only returns to the supplier; small-slot returns go through the slab
        // free list and never touch the supplier).
        let birth = allocator.alloc(PAGE_BYTES);
        assert!(!birth.is_null());

        allocator.replace_supplier(HeapSupplier::Vm(VmBreakSupplier::new(
            transport,
            0x10_0000,
        )));
        assert!(allocator.is_vm_form());

        // The birth block is still releasable after the switch: a fixed-pool
        // page is not in the VM range → foreign count.
        allocator.free(birth);

        // A new allocation grows from the VM break (1-page big block →
        // supply_pages(1)).
        let vm_page = allocator.alloc(PAGE_BYTES);
        assert!(!vm_page.is_null());
        assert_eq!(vm_page as u64, 0x10_0000);

        let diag = allocator.diag();
        assert_eq!((diag.total_pages, diag.free_pages), (0, 0));
        assert_eq!(diag.pages_consumed, 1);
        if let HeapSupplier::Vm(vm) = &allocator.supplier {
            assert_eq!(vm.foreign_releases(), 1);
            assert_eq!(vm.pages_supplied(), 1);
        } else {
            panic!("replace_supplier 后必须是 Vm 臂");
        }
    }
}
