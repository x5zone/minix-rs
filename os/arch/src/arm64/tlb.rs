//! ARM64 (aarch64) TLB flush and address-space switch implementation.
//!
//! C: `arch_do_vmctl.c:38-65` (ARM equivalent) — `tlbi vmalle1is` for
//! flush-all, `tlbi vaae1is` for single-page invalidation. C:
//! `arch_do_vmctl.c:19-33` — `setcr3()` (ARM equivalent writes TTBR0_EL1
//! + a full stage-1 broadcast TLBI to install a new root).
//!
//! # Assembly
//!
//! - `flush_all`: `tlbi vmalle1is` — invalidate all EL1&0 **stage-1** TLB
//!   entries (all ASIDs), inner-shareable (broadcasts to all CPUs in the
//!   inner-shareability domain).
//! - `flush_addr`: `tlbi vaae1is, <va>` — invalidate the EL1&0 stage-1 TLB
//!   entry for the given virtual address, all ASIDs, inner-shareable.
//! - `set_active_root`: `msr TTBR0_EL1, root` + `isb` + `tlbi vmalle1is` +
//!   `dsb ish` + `isb` — install the new root and flush stale stage-1
//!   entries left from the old root.
//!
//! # Why `vmalle1is`, not `alle1is` (§1.115 real-machine fix)
//!
//! `tlbi alle1is`/`alle1` flush **both stage-1 and stage-2** translations for
//! the EL1&0 regime. minix-rs runs a **stage-1-only** EL1 regime (EDK2 hands
//! off at EL1; there is no EL2 guest/stage-2), where that wider-scope TLBI is
//! not a defined EL1 operation — QEMU raises it as a synchronous exception
//! with `ESR.EC = 0b000000` ("unknown reason", i.e. UNDEFINED). The very
//! first `switch_address_space` therefore faulted on its `tlbi` (never
//! observed before §1.115: the inline low-half boot died earlier, at the
//! post-switch instruction fetch). `vmalle1is` is the EL1&0 **stage-1**
//! "invalidate all" op — always defined at EL1 and exactly the flush a root
//! switch needs here; the boot page-table helpers already use it
//! (`split_huge`/`grant_user_walk`) without incident.
//!
//! # ARM64 vs x86-64
//!
//! ARM64 does not reload TTBR0/TTBR1 to flush TLB (unlike x86-64's CR3
//! reload). Instead, dedicated `tlbi` instructions invalidate specific
//! TLB entries. The `*is` (inner-shareable) forms broadcast to other CPUs in
//! the same inner-shareability domain — the correct behavior for SMP TLB
//! shootdown.
//!
//! Writing TTBR0_EL1 alone does *not* invalidate TLB entries (ARM ARM
//! D5.4.5 — TLB entries remain valid until explicitly invalidated). The
//! `set_active_root` implementation therefore follows the TTBR0 write
//! with a broadcast stage-1 `tlbi` to flush stale entries from the old root.

use core::arch::asm;
use minix_types::{PhysBytes, VirBytes};
use crate::tlb_arch::TlbArch;

/// AArch64 TLB flush + address-space switch implementation.
pub struct AArch64TlbArch;

impl TlbArch for AArch64TlbArch {
    unsafe fn flush_all() {
        // tlbi vmalle1is: invalidate all EL1&0 stage-1 TLB entries (all
        // ASIDs), inner-shareable. ARM ARM D5.4.5 — TLBI VMALLE1IS. We use
        // the stage-1-only form, not `alle1is` (which also targets stage 2 and
        // is UNDEFINED at EL1 in our stage-1-only regime — see module doc).
        // The `dsb ish` + `isb` afterward ensure the broadcast invalidation
        // completes before subsequent memory accesses (mirrors
        // `set_active_root`; a bare `isb` does not wait for the `*is`
        // broadcast to finish on other cores).
        unsafe {
            asm!("tlbi vmalle1is", options(preserves_flags));
            asm!("dsb ish", options(preserves_flags));
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
        //   isb                // TTBR0 write must be context-synchronized
        //                      // before any dependent op (ARM ARM D5.4.5
        //                      // break-before-make: a bare TLBI right after
        //                      // the register write races the update).
        //   tlbi vmalle1is     // flush stale stage-1 TLB entries for the old
        //                      // root (stage-1-only regime; see module doc)
        //   dsb ish            // ensure the broadcast invalidation completes
        //   isb                // synchronize context for subsequent fetch
        //
        // ARM64 writes to TTBR0_EL1 do NOT implicitly flush the TLB
        // (ARM ARM D5.4.5). We must explicitly invalidate all EL1&0 stage-1
        // TLB entries to prevent stale translations from the old root.
        // The `*is` (inner-shareable) form broadcasts to other CPUs in the
        // inner-shareability domain (correct for SMP).
        //
        // SAFETY: Caller guarantees phys_root is a valid 4KB-aligned
        // L0 translation table physical address and paging is enabled.
        // The `isb`/`dsb`/`isb` sequence guarantees the invalidation
        // completes before any subsequent memory access uses the new root.
        unsafe {
            asm!("msr TTBR0_EL1, {}", in(reg) phys_root.0, options(preserves_flags));
            asm!("isb", options(preserves_flags));
            asm!("tlbi vmalle1is", options(preserves_flags));
            asm!("dsb ish", options(preserves_flags));
            asm!("isb", options(preserves_flags));
        }
    }

    unsafe fn get_active_root() -> PhysBytes {
        // C: klib.S:618 — `mov %cr3, %ecx` ARM equivalent: mrs x0, ttbr0_el1
        // reads the live translation table base. Since we write the raw
        // physical address (no ASID, A1=0 in TCR → TTBR0 bits[63:48] are
        // RES0), the value read back equals what was written.
        let val: u64;
        unsafe {
            asm!("mrs {}, ttbr0_el1", out(reg) val, options(preserves_flags, nomem));
        }
        // Mask to the 48-bit physical address field (clear potential
        // implementation-defined upper bits in the BADDR encoding).
        PhysBytes(val & 0x0000_FFFF_FFFF_FFFF)
    }
}
