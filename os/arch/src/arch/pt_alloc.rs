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
//! The allocator is a process-wide singleton set via `register()`.
//! Minix-RS user-space servers are single-threaded event loops, so no
//! concurrent mutation occurs. `AtomicBool` is used for `PT_REGISTERED`
//! to avoid `static mut` (UB in Rust 2024 edition). The function pointer
//! is wrapped in `UnsafeCell` with a `Sync` impl documented as safe under
//! single-threaded access.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use minix_types::{PhysBytes, VirBytes};
use crate::paging::PageTableError;

type PtAllocFn = fn() -> Result<(PhysBytes, VirBytes), PageTableError>;

/// Wrapper for a function pointer stored in a static.
/// SAFETY: `PtAllocSlot` is only written once during boot (single-threaded),
/// and only read afterwards. `Sync` is safe because there is no concurrent
/// mutation in the single-threaded event loop model.
struct PtAllocSlot(UnsafeCell<PtAllocFn>);

unsafe impl Sync for PtAllocSlot {}

static PT_ALLOC: PtAllocSlot = PtAllocSlot(UnsafeCell::new(uninit_alloc));
static PT_REGISTERED: AtomicBool = AtomicBool::new(false);

fn uninit_alloc() -> Result<(PhysBytes, VirBytes), PageTableError> {
    Err(PageTableError::AllocationFailed)
}

/// Register a page table page allocator.
///
/// Must be called once before any Paging::map_huge / map operation.
/// The provided function must return (phys, virt) — in boot both equal;
/// under VM virt = DM_BASE + phys.
pub fn register(alloc_fn: fn() -> Result<(PhysBytes, VirBytes), PageTableError>) {
    // SAFETY: Single-threaded boot context; no concurrent access.
    unsafe {
        core::ptr::write(PT_ALLOC.0.get(), alloc_fn);
    }
    PT_REGISTERED.store(true, Ordering::Relaxed);
}

/// Returns true if a page table page allocator has been registered.
pub fn is_registered() -> bool {
    PT_REGISTERED.load(Ordering::Relaxed)
}

/// Allocate a zero-filled physical page for an intermediate page table.
///
/// Returns `(phys, virt)` — in boot stage both are equal (identity mapping);
/// under the VM allocator (future) `virt = DM_BASE + phys`.
///
/// Called by `map_huge` / `map` inside Paging implementations.
#[inline]
pub fn alloc_pt_page() -> Result<(PhysBytes, VirBytes), PageTableError> {
    // SAFETY: Single-threaded access; the function pointer is set once
    // via `register()` and then only read.
    let alloc_fn = unsafe { core::ptr::read(PT_ALLOC.0.get()) };
    alloc_fn()
}
