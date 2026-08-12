//! ARM64 (aarch64) TLB flush and address-space switch implementation.
//!
//! C: `arch_do_vmctl.c:38-65` (ARM equivalent) — `tlbi alle1is` for
//! flush-all, `tlbi vaae1is` for single-page invalidation. C:
//! `arch_do_vmctl.c:19-33` — `setcr3()` (ARM equivalent writes TTBR0_EL1
//! + `tlbi alle1is` to install a new root).
//!
//! # Assembly
//!
//! - `flush_all`: `tlbi alle1is` — invalidate all TLB entries for EL1,
//!   inner-shareable (affects all CPUs in the inner shareability domain).
//! - `flush_addr`: `tlbi vaae1is, <va>` — invalidate TLB entry for the
//!   given virtual address, all ASIDs, EL1, inner-shareable.
//! - `set_active_root`: `msr TTBR0_EL1, root` + `tlbi alle1is` + `isb` —
//!   install new root and flush stale TLB entries for the old root.
//!
//! # ARM64 vs x86-64
//!
//! ARM64 does not reload TTBR0/TTBR1 to flush TLB (unlike x86-64's CR3
//! reload). Instead, dedicated `tlbi` instructions invalidate specific
//! TLB entries. `alle1is` is inner-shareable, meaning it broadcasts to
//! other CPUs in the same inner-shareability domain — this is the correct
//! behavior for SMP TLB shootdown.
//!
//! Writing TTBR0_EL1 alone does *not* invalidate TLB entries (ARM ARM
//! D5.4.5 — TLB entries remain valid until explicitly invalidated). The
//! `set_active_root` implementation therefore follows the TTBR0 write
//! with `tlbi alle1is` to flush stale entries from the old root.

use core::arch::asm;
use minix_types::{PhysBytes, VirBytes};
use crate::tlb_arch::TlbArch;

/// AArch64 TLB flush + address-space switch implementation.
pub struct AArch64TlbArch;

impl TlbArch for AArch64TlbArch {
    unsafe fn flush_all() {
        // tlbi alle1is: invalidate all TLB entries for EL1, inner-shareable.
        // ARM ARM D5.4.5 — TLBI ALLE1IS.
        // The `isb` afterward ensures the invalidation completes before
        // subsequent memory accesses.
        unsafe {
            asm!("tlbi alle1is", options(preserves_flags));
            asm!("isb", options(preserves_flags));
        }
    }

    unsafe fn flush_addr(vaddr: VirBytes) {
        // tlbi vaae1is, <va>: invalidate TLB entry for the given virtual
        // address, all ASIDs, EL1, inner-shareable.
        // The address is shifted right by 12 (page-aligned) as required
        // by the TLBI VAAE1IS encoding (ARM ARM D5.4.5).
        unsafe {
            let va = (vaddr.0 >> 12) & 0xFFFFFFFFFF; // bits [48:12]
            asm!("tlbi vaae1is, {}", in(reg) va, options(preserves_flags));
            asm!("isb", options(preserves_flags));
        }
    }

    unsafe fn set_active_root(phys_root: PhysBytes) {
        // C: arch_do_vmctl.c:31 — setcr3() ARM equivalent:
        //   msr TTBR0_EL1, phys_root
        //   tlbi alle1is       // flush stale TLB entries
        //   isb                // synchronize context
        //
        // ARM64 writes to TTBR0_EL1 do NOT implicitly flush the TLB
        // (ARM ARM D5.4.5). We must explicitly invalidate all EL1 TLB
        // entries to prevent stale translations from the old root.
        // `alle1is` is inner-shareable so it broadcasts to other CPUs
        // in the inner shareability domain (correct for SMP).
        //
        // SAFETY: Caller guarantees phys_root is a valid 4KB-aligned
        // L0 translation table physical address and paging is enabled.
        // The `isb` after `tlbi` ensures the invalidation completes
        // before any subsequent memory access uses the new root.
        unsafe {
            asm!("msr TTBR0_EL1, {}", in(reg) phys_root.0, options(preserves_flags));
            asm!("tlbi alle1is", options(preserves_flags));
            asm!("isb", options(preserves_flags));
        }
    }
}
