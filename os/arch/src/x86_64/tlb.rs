//! x86-64 TLB flush and address-space switch implementation.
//!
//! C: `arch_do_vmctl.c:38-65` — `write_cr3()` for flush-all, `invlpg` for
//! single-page invalidation. C: `arch_do_vmctl.c:19-33` — `setcr3()` calls
//! `write_cr3()` to install a new CR3 when target is ptproc.
//!
//! # Assembly
//!
//! - `flush_all`: `mov cr3, rax` where rax = current CR3 — reloading CR3
//!   flushes all non-global TLB entries (Intel SDM Vol 3 §4.10.4).
//! - `flush_addr`: `invlpg [addr]` — invalidate single page TLB entry.
//! - `set_active_root`: `mov cr3, rax` where rax = new CR3 — installing a
//!   new root flushes all non-global TLB entries for the old root.
//!
//! # PCID consideration
//!
//! x86-64 PCID (Process Context ID) allows tagging TLB entries by context.
//! When PCID is enabled, `mov cr3` with bit 63 set preserves TLB entries
//! for the old PCID. This implementation does NOT use PCID (matching Minix3
//! C which never enables PCID), so `mov cr3` always flushes all non-global
//! entries.

use core::arch::asm;
use minix_types::{PhysBytes, VirBytes};
use crate::tlb_arch::TlbArch;

/// x86-64 TLB flush + address-space switch implementation.
pub struct X86_64TlbArch;

impl TlbArch for X86_64TlbArch {
    unsafe fn flush_all() {
        // Read current CR3 and write it back. Reloading CR3 flushes all
        // non-global TLB entries (Intel SDM Vol 3 §4.10.4.1).
        // C: arch_do_vmctl.c:48 — write_cr3(p->p_seg.p_cr3)
        unsafe {
            let cr3: u64;
            asm!("mov {}, cr3", out(reg) cr3, options(preserves_flags));
            asm!("mov cr3, {}", in(reg) cr3, options(preserves_flags));
        }
    }

    unsafe fn flush_addr(vaddr: VirBytes) {
        // C: arch_do_vmctl.c:55 — invlpg(m_ptr->SVMCTL_WHERE)
        // INVTLG invalidates the TLB entry for the given virtual address.
        // If the address is not in the TLB, this is a no-op.
        unsafe {
            asm!("invlpg [{}]", in(reg) vaddr.0, options(nostack, preserves_flags));
        }
    }

    unsafe fn set_active_root(phys_root: PhysBytes) {
        // C: arch_do_vmctl.c:31 — write_cr3(p->p_seg.p_cr3) inside setcr3().
        //
        // Load a new CR3 value. On x86-64, writing CR3:
        //   1. Switches the active PML4 root (address space changes)
        //   2. Flushes all non-global TLB entries for the old root
        //      (Intel SDM Vol 3 §4.10.4.1 — "MOV to CR3" invalidation)
        //
        // The low 12 bits of CR3 are available for PCID / flags; we pass
        // phys_root unchanged because PCID is not used (see module docs).
        // Caller is responsible for ensuring phys_root is 4KB-aligned and
        // points to a valid PML4 table mapped in the current address space.
        //
        // SAFETY: Caller guarantees phys_root is a valid PML4 physical
        // address and paging is enabled. The serializing semantics of
        // MOV-to-CR3 (Intel SDM Vol 3 §8.1.3) ensure no speculative
        // memory accesses use the old root after this instruction.
        unsafe {
            asm!("mov cr3, {}", in(reg) phys_root.0, options(preserves_flags));
        }
    }
}
