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
//! # Safety
//!
//! The allocator is a process-wide singleton set via `register()`.
//! Minix-RS user-space servers are single-threaded event loops, so no
//! concurrent mutation occurs.

use minix_types::{PhysBytes, VirBytes};
use crate::paging::PageTableError;

type PtAllocFn = fn() -> Result<(PhysBytes, VirBytes), PageTableError>;

static mut PT_ALLOC: PtAllocFn = uninit_alloc;
static mut PT_REGISTERED: bool = false;

fn uninit_alloc() -> Result<(PhysBytes, VirBytes), PageTableError> {
    Err(PageTableError::AllocationFailed)
}

/// Register a page table page allocator.
///
/// Must be called once before any Paging::map_huge / map operation.
/// The provided function must return (phys, virt) — in boot both equal;
/// under VM virt = DM_BASE + phys.
pub fn register(alloc_fn: fn() -> Result<(PhysBytes, VirBytes), PageTableError>) {
    unsafe {
        PT_ALLOC = alloc_fn;
        PT_REGISTERED = true;
    }
}

/// Returns true if a page table page allocator has been registered.
pub fn is_registered() -> bool {
    unsafe { PT_REGISTERED }
}

/// Allocate a zero-filled physical page for an intermediate page table.
///
/// Returns `(phys, virt)` — in boot stage both are equal (identity mapping);
/// under the VM allocator (future) `virt = DM_BASE + phys`.
///
/// Called by `map_huge` / `map` inside Paging implementations.
#[inline]
pub fn alloc_pt_page() -> Result<(PhysBytes, VirBytes), PageTableError> {
    unsafe { PT_ALLOC() }
}
