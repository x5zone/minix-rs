//! RISC-V 64-bit TLB flush and address-space switch implementation.
//!
//! C: `arch_do_vmctl.c:38-65` (RISC-V equivalent) — `sfence.vma zero, zero`
//! for flush-all, `sfence.vma <va>, zero` for single-page invalidation. C:
//! `arch_do_vmctl.c:19-33` — `setcr3()` (RISC-V equivalent writes satp +
//! `sfence.vma` to install a new root).
//!
//! # Assembly
//!
//! - `flush_all`: `sfence.vma zero, zero` — fence all stores and invalidate
//!   all TLB entries for all ASIDs and all virtual addresses (RISC-V Priv
//!   ISA §4.2.1).
//! - `flush_addr`: `sfence.vma <va>, zero` — invalidate TLB entry for the
//!   given virtual address, all ASIDs.
//! - `set_active_root`: `csrw satp, value` + `sfence.vma zero, zero` —
//!   install new root and flush stale TLB entries for the old root.
//!
//! # RISC-V vs x86-64/ARM64
//!
//! RISC-V `sfence.vma` is a supervisor-level instruction that takes two
//! operands: virtual address and ASID. `zero` for either means "all".
//! Unlike x86-64 (CR3 reload) or ARM64 (tlbi with inner-shareable domain),
//! `sfence.vma` on a single hart only affects that hart's TLB. For SMP
//! TLB shootdown, the caller must send IPIs to other harts.
//!
//! Writing satp alone does *not* invalidate TLB entries (RISC-V Priv ISA
//! §4.2.1 — sfence.vma is required to invalidate stale entries after
//! satp change). The `set_active_root` implementation therefore follows
//! the satp write with `sfence.vma zero, zero`.
//!
//! # satp encoding (Sv39)
//!
//! In RISC-V Sv39, satp is a 64-bit CSR with format:
//! `[MODE(1)] [ASID(16)] [PPN(44)]`
//!
//! - MODE=8: Sv39 (3-level paging, 39-bit VA)
//! - MODE=9: Sv48 (4-level paging, 48-bit VA) — not used here
//! - PPN: physical page number of the root table = phys_root >> 12
//!
//! ASID is left as 0 (no PCID-like tagging in this implementation).

use core::arch::asm;
use minix_types::{PhysBytes, VirBytes};
use crate::tlb_arch::TlbArch;

/// RISC-V Sv39 satp MODE field value (3-level paging).
const SV39_MODE: u64 = 8;

/// RISC-V 64-bit TLB flush + address-space switch implementation.
pub struct Riscv64TlbArch;

impl TlbArch for Riscv64TlbArch {
    unsafe fn flush_all() {
        // sfence.vma zero, zero: invalidate all TLB entries for all ASIDs
        // and all virtual addresses on the current hart.
        // RISC-V Privileged ISA §4.2.1.
        unsafe {
            asm!("sfence.vma zero, zero", options(preserves_flags));
        }
    }

    unsafe fn flush_addr(vaddr: VirBytes) {
        // sfence.vma <va>, zero: invalidate TLB entry for the given virtual
        // address, all ASIDs, on the current hart.
        unsafe {
            asm!("sfence.vma {}, zero", in(reg) vaddr.0, options(preserves_flags));
        }
    }

    unsafe fn set_active_root(phys_root: PhysBytes) {
        // C: arch_do_vmctl.c:31 — setcr3() RISC-V equivalent:
        //   csrw satp, (SV39_MODE << 60) | (phys_root >> 12)
        //   sfence.vma zero, zero    // flush stale TLB entries
        //
        // RISC-V writes to satp do NOT implicitly flush the TLB
        // (RISC-V Priv ISA §4.2.1). We must explicitly issue sfence.vma
        // to invalidate stale entries from the old root.
        //
        // satp encoding (Sv39): [MODE(1)=8] [ASID(16)=0] [PPN(44)]
        // PPN = phys_root >> 12 (drop the low 12 bits — page offset).
        //
        // SAFETY: Caller guarantees phys_root is a valid 4KB-aligned
        // root page table physical address and paging is enabled.
        // The sfence.vma after the satp write ensures no stale
        // translations from the old root remain in the TLB.
        let satp_value = (SV39_MODE << 60) | (phys_root.0 >> 12);
        unsafe {
            asm!("csrw satp, {}", in(reg) satp_value, options(preserves_flags));
            asm!("sfence.vma zero, zero", options(preserves_flags));
        }
    }
}
