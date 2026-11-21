//! Page table page allocator — registration mechanism.
//!
//! Paging implementations (x86-64, aarch64, riscv64) need to allocate
//! intermediate page table pages during `map_huge` / `map`. This module
//! provides a function-pointer-based registration mechanism — callers
//! (hello-boot, boot-uefi, VM server) register their own allocator
//! implementation via `register()`, and Paging implementations call
//! the generic `alloc_pt_page()`.
//!
//! # Why a function pointer?
//!
//! Boot and VM use different allocation strategies with the same interface:
//!
//! | Stage | Strategy       | VA↔PA                   |
//! |-------|----------------|-------------------------|
//! | Boot  | Bump (identity)| VA = PA                 |
//! | VM    | VmPageAllocator| VA = DM_BASE + PA (TBD) |
//!
//! A function pointer keeps the type signature clean — `fn()` never leaks
//! whether it's Boot or VM. No enum tag, no dead code path at runtime.
//! The implementation lives at the call site (hello-boot, boot-uefi, VM),
//! not in this module. This module is just the thin glue layer.
//!
//! # Concurrency safety
//!
//! Each slot is one `AtomicUsize` holding the function pointer; 0 means
//! unregistered. `register()` admits exactly one writer with a
//! compare-exchange and publishes the pointer with Release ordering;
//! readers Acquire-load the slot. The pointer is never replaced after the
//! exchange, so the concurrent phase is read-only. The arch crate serves
//! the kernel (SMP + BKL), so this is the write-once-then-read-only
//! argument, not single-threaded access: the write happens during boot
//! (single-threaded) or user-space VM init, before any concurrent read.

use core::sync::atomic::{AtomicUsize, Ordering};
use minix_types::{PhysBytes, VirBytes};
use crate::paging::PageTableError;

type PtAllocFn = fn() -> Result<(PhysBytes, VirBytes), PageTableError>;

/// Free function for page-table pages: takes the physical address
/// of the page and returns it to the source allocator.
type PtFreeFn = fn(PhysBytes);

/// Registered allocator, or 0 when unregistered.
static PT_ALLOC: AtomicUsize = AtomicUsize::new(0);
/// Registered free hook, or 0 when unregistered.
static PT_FREE: AtomicUsize = AtomicUsize::new(0);

/// Register a page table page allocator.
///
/// Must be called once before any Paging::map_huge / map operation.
/// The provided function must return (phys, virt) — in boot both equal;
/// under VM virt = DM_BASE + phys.
pub fn register(alloc_fn: fn() -> Result<(PhysBytes, VirBytes), PageTableError>) {
    // Double registration within one domain is a boot bug — two allocators
    // would fight over the singleton. (The kernel guards its own call with
    // `is_registered()` so a test kernel may register first; the VM server
    // registers in its own address space.)
    //
    // Write-once is enforced in both profiles. A second registration is a
    // boot bug: silently keeping the first allocator hides a wiring error,
    // so fail closed in release too.
    PT_ALLOC
        .compare_exchange(0, alloc_fn as usize, Ordering::AcqRel, Ordering::Acquire)
        .unwrap_or_else(|_| panic!("pt_alloc::register called twice"));
}

/// Returns true if a page table page allocator has been registered.
pub fn is_registered() -> bool {
    PT_ALLOC.load(Ordering::Acquire) != 0
}

/// Allocate a zero-filled physical page for an intermediate page table.
///
/// Returns `(phys, virt)` — in boot stage both are equal (identity mapping);
/// under the VM allocator (future) `virt = DM_BASE + phys`.
///
/// Called by `map_huge` / `map` inside Paging implementations.
#[inline]
pub fn alloc_pt_page() -> Result<(PhysBytes, VirBytes), PageTableError> {
    // Program invariant: `register()` must precede any map operation. Keep the
    // failure distinct from a genuine allocation failure so diagnostics point
    // at the boot wiring, not at memory exhaustion.
    let raw = PT_ALLOC.load(Ordering::Acquire);
    assert!(
        raw != 0,
        "pt_alloc::alloc_pt_page called before register"
    );
    // SAFETY: `register()` stored a `PtAllocFn` value in this slot and the
    // slot is never replaced; `usize` and the function pointer have the same
    // width on every supported 64-bit target.
    let alloc_fn: PtAllocFn = unsafe { core::mem::transmute(raw) };
    alloc_fn()
}

/// Register a page-table page free function.
///
/// Pair with [`register`]: the same allocator domain that hands out
/// page-table pages takes them back here. Same write-once contract as
/// [`register`] — callers register during init before concurrent access.
pub fn register_free(free_fn: PtFreeFn) {
    // Same write-once contract as `register()`: a second hook is a wiring
    // bug and fails closed in both profiles.
    PT_FREE
        .compare_exchange(0, free_fn as usize, Ordering::AcqRel, Ordering::Acquire)
        .unwrap_or_else(|_| panic!("pt_alloc::register_free called twice"));
}

/// Returns true if a page-table page free function has been registered.
pub fn is_free_registered() -> bool {
    PT_FREE.load(Ordering::Acquire) != 0
}

/// Return a page-table page to its source allocator.
///
/// Panics before `register_free()` — destroy paths gate the full-reclaim
/// walk on [`is_free_registered()`] and fall back to zero-root-only when
/// absent (boot-stage tables with no allocator domain).
#[inline]
pub fn free_pt_page(phys: PhysBytes) {
    let raw = PT_FREE.load(Ordering::Acquire);
    assert!(
        raw != 0,
        "pt_alloc::free_pt_page called before register_free \
         (destroy path must gate on is_free_registered)"
    );
    // SAFETY: `register_free()` stored a `PtFreeFn` value in this slot and
    // the slot is never replaced; pointer width matches `usize`.
    let free_fn: PtFreeFn = unsafe { core::mem::transmute(raw) };
    free_fn(phys)
}
