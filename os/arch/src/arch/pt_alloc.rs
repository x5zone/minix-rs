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
//! The arch crate serves the kernel (SMP + BKL), so the safety argument
//! is **write-once then read-only**, not single-threaded: the write
//! happens only during boot (single-threaded) or user-space VM init,
//! before any concurrent access; after registration the function pointer
//! is only read. `AtomicBool` is used for `PT_REGISTERED` to avoid
//! `static mut` (UB in Rust 2024 edition); the `Sync` impl is sound
//! because the `UnsafeCell` content is immutable during the concurrent
//! phase. The Release store in `register()` pairs with the Acquire load
//! in `alloc_pt_page()` to publish the fn pointer write.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use minix_types::{PhysBytes, VirBytes};
use crate::paging::PageTableError;

type PtAllocFn = fn() -> Result<(PhysBytes, VirBytes), PageTableError>;

/// Wrapper for a function pointer stored in a static.
/// SAFETY: write-once-then-read-only. The slot is written exactly once
/// (`register()`), during boot (single-threaded) or user-space VM init
/// before any concurrent access; afterwards it is only read. The
/// `PT_REGISTERED` flag's Release/Acquire pairing publishes the write to
/// any thread that observes `is_registered() == true`. `Sync` is sound
/// because the `UnsafeCell` content is immutable during the concurrent
/// phase — this is the kernel (SMP + BKL) execution model, not the
/// single-threaded server model.
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
    // Double registration within one domain is a boot bug — two allocators
    // would fight over the singleton. (The kernel guards its own call with
    // `is_registered()` so a test kernel may register first; the VM server
    // registers in its own address space.)
    debug_assert!(
        !PT_REGISTERED.load(Ordering::Relaxed),
        "pt_alloc::register called twice"
    );
    // SAFETY: write-once contract — callers register during boot
    // (single-threaded) or user-space VM init before any concurrent
    // access. The debug_assert above catches accidental re-registration.
    unsafe {
        core::ptr::write(PT_ALLOC.0.get(), alloc_fn);
    }
    // Release: publish the fn pointer write to any thread that Acquire-loads
    // `is_registered() == true` before calling `alloc_pt_page()`.
    PT_REGISTERED.store(true, Ordering::Release);
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
    // Acquire: pairs with `register()`'s Release store — guarantees the
    // fn pointer write is visible to this thread once registration is
    // observed (write-once-then-read-only; see module docs).
    let _ = PT_REGISTERED.load(Ordering::Acquire);
    // SAFETY: write-once-then-read-only — set via `register()` before any
    // concurrent access, then only read.
    let alloc_fn = unsafe { core::ptr::read(PT_ALLOC.0.get()) };
    alloc_fn()
}
