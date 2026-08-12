//! TLB (Translation Lookaside Buffer) flush and address-space switch
//! architecture abstraction.
//!
//! Defines the trait interface for CPU-wide TLB invalidation and page-table
//! root installation. Unlike `Paging::flush_tlb` (which is an instance method
//! tied to a specific page table), `TlbArch` provides **static** methods that
//! operate on the *current* CPU's TLB regardless of which page table is active.
//!
//! # When to use TlbArch vs Paging::flush_tlb
//!
//! - **`Paging::flush_tlb(&self)`**: Use when you have a `Paging` instance
//!   and want to flush TLB entries for *that* page table. The impl typically
//!   reloads CR3/satp with the instance's root, which flushes TLB entries
//!   associated with the old root (when PCID/ASID is not in use).
//!
//! - **`TlbArch::flush_all()`**: Use when you need to flush the *current*
//!   CPU's entire TLB without reference to a specific page table instance.
//!   This is the kernel's `SYS_VMCTL_FLUSHTLB` path: the caller passes a
//!   target process, but the TLB flush must happen on the *current* CPU
//!   (which may or may not be running the target process).
//!
//! - **`TlbArch::set_active_root(phys_root)`**: Use when switching the
//!   currently-loaded page table to a *different* root (e.g., VM's
//!   `VMCTL_SETADDRSPACE` when the target is the current ptproc). This
//!   installs the new root register (CR3/TTBR0/satp) which both changes
//!   the active address space *and* flushes non-global TLB entries.
//!
//! # C source mapping
//!
//! ```c
//! // arch_do_vmctl.c:38-65 — arch_do_vmctl()
//! case SVMCTL_FLUSHTLB:
//!     write_cr3(p->p_seg.p_cr3);   // reload CR3 → flush TLB
//!     break;
//! case SVMCTL_INVLPG:
//!     invlpg(m_ptr->SVMCTL_WHERE); // invalidate single page
//!     break;
//! ```
//!
//! ```c
//! // arch_do_vmctl.c:19-33 — setcr3() (called by SVMCTL_SETADDRSPACE)
//! static void setcr3(struct proc *p, u32_t cr3, u32_t *v) {
//!     p->p_seg.p_cr3 = cr3;
//!     p->p_seg.p_cr3_v = v;
//!     if (p == get_cpulocal_var(ptproc)) {
//!         write_cr3(p->p_seg.p_cr3);   // ← TlbArch::set_active_root
//!     }
//!     ...
//! }
//! ```
//!
//! On x86-64, `write_cr3(cr3)` reloads CR3 which flushes all non-global TLB
//! entries. On aarch64, the equivalent is `msr TTBR0_EL1, root` + `tlbi alle1is`
//! (TLB flush is not implicit on TTBR0 write). On riscv64, `csrw satp, value`
//! + `sfence.vma zero, zero`.
//!
//! # Three-architecture coverage
//!
//! - **x86_64**: `mov cr3, rax` (flush all / set root) / `invlpg [addr]` (single page)
//! - **aarch64**: `tlbi alle1is` (flush all) / `msr TTBR0_EL1` + `tlbi alle1is` (set root) / `tlbi vaae1is, <va>` (single page)
//! - **riscv64**: `sfence.vma zero, zero` (flush all) / `csrw satp` + `sfence.vma` (set root) / `sfence.vma <va>, zero` (single page)

use minix_types::{PhysBytes, VirBytes};

/// Architecture abstraction for CPU-wide TLB invalidation and address-space
/// root installation.
///
/// All methods are associated functions (no `&self`) because TLB flush and
/// root installation operate on the *current* CPU's MMU state, which is a
/// global resource not tied to any particular `Paging` instance.
///
/// # Safety
///
/// All methods are `unsafe` because:
/// - They must be called with a valid MMU context (paging enabled)
/// - On SMP, the caller must ensure proper cross-CPU TLB shootdown if
///   the flush needs to be global (current impl is CPU-local only)
pub trait TlbArch {
    /// Flush all non-global TLB entries on the current CPU.
    ///
    /// C: `write_cr3(read_cr3())` (x86) / `tlbi alle1is` (aarch64) /
    ///    `sfence.vma zero, zero` (riscv64)
    ///
    /// # Safety
    ///
    /// Caller must ensure paging is enabled on the current CPU.
    /// On SMP systems, this only flushes the *current* CPU's TLB;
    /// cross-CPU shootdown is the caller's responsibility.
    unsafe fn flush_all();

    /// Flush the TLB entry for a single virtual address on the current CPU.
    ///
    /// C: `invlpg(addr)` (x86) / `tlbi vaae1is, <va>` (aarch64) /
    ///    `sfence.vma <va>, zero` (riscv64)
    ///
    /// # Safety
    ///
    /// Caller must ensure paging is enabled and `vaddr` is a valid
    /// virtual address in the current address space.
    unsafe fn flush_addr(vaddr: VirBytes);

    /// Install a new page-table root on the current CPU.
    ///
    /// Unlike `flush_all()` (which reloads the *current* root to flush TLB),
    /// this method loads a *new* root address into the MMU's root register
    /// (CR3 on x86-64, TTBR0_EL1 on aarch64, satp on riscv64).
    ///
    /// C: `write_cr3(p->p_seg.p_cr3)` inside `setcr3()` — arch_do_vmctl.c:31
    ///
    /// On x86-64, writing CR3 implicitly flushes all non-global TLB entries.
    /// On aarch64, writing TTBR0_EL1 does *not* flush TLB; the implementation
    /// follows with `tlbi alle1is` to invalidate stale entries. On riscv64,
    /// writing satp also does not flush; the implementation follows with
    /// `sfence.vma zero, zero`.
    ///
    /// # Architecture-specific encoding
    ///
    /// - **x86-64**: `phys_root` is the CR3 value (physical address of PML4,
    ///   4KB-aligned, low 12 bits available for PCID/flags — currently unused).
    /// - **aarch64**: `phys_root` is the TTBR0_EL1 value (physical address of
    ///   the L0 translation table, 4KB-aligned).
    /// - **riscv64**: `phys_root` is the *physical address* of the root page
    ///   table (Sv39/Sv48). The implementation encodes it as
    ///   `(MODE << 60) | (phys_root >> 12)` before writing to satp. MODE=8
    ///   (Sv39) for 3-level paging; MODE=9 (Sv48) for 4-level paging.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - Paging is enabled on the current CPU.
    /// - `phys_root` points to a valid, 4KB-aligned page-table root that is
    ///   currently mapped in the active address space (or via Direct Map).
    /// - On SMP, cross-CPU shootdown is the caller's responsibility if other
    ///   CPUs were executing under the old root.
    unsafe fn set_active_root(phys_root: PhysBytes);
}

// ── Mock implementation for host tests ──

/// Mock `TlbArch` that does nothing (for `#[cfg(test)]` / `mock` feature).
///
/// All operations are no-ops. Safe because there's no real hardware to
/// touch; the mock exists purely to let kernel code compile and run
/// unit tests on a host architecture.
pub struct MockTlbArch;

impl TlbArch for MockTlbArch {
    unsafe fn flush_all() {
        // no-op: mock has no real TLB
    }

    unsafe fn flush_addr(_vaddr: VirBytes) {
        // no-op: mock has no real TLB
    }

    unsafe fn set_active_root(_phys_root: PhysBytes) {
        // no-op: mock has no real MMU root register
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify MockTlbArch compiles and its methods are callable.
    /// The mock is a no-op, so we only check that calls don't panic.
    #[test]
    fn test_mock_tlb_arch_flush_all_compiles() {
        // SAFETY: Mock implementation — no real hardware touched.
        unsafe { MockTlbArch::flush_all(); }
    }

    #[test]
    fn test_mock_tlb_arch_flush_addr_compiles() {
        // SAFETY: Mock implementation — no real hardware touched.
        unsafe { MockTlbArch::flush_addr(VirBytes(0xDEAD_BEEF)); }
    }

    /// Verify the new `set_active_root` method is callable on the mock.
    /// This is the host-test path used by `dispatch_vmctl(SetAddrSpace)`.
    #[test]
    fn test_mock_tlb_arch_set_active_root_compiles() {
        // SAFETY: Mock implementation — no real hardware touched.
        unsafe { MockTlbArch::set_active_root(PhysBytes(0x1000)); }
    }

    /// Verify the TlbArch trait can be used as a generic constraint.
    /// This ensures the trait is object-safe enough for generic dispatch.
    #[test]
    fn test_tlb_arch_as_generic_constraint() {
        fn do_flush_all<T: TlbArch>() {
            // SAFETY: Mock implementation — no real hardware touched.
            unsafe { T::flush_all(); }
        }
        do_flush_all::<MockTlbArch>();
    }

    /// Verify `set_active_root` is also reachable through a generic bound.
    /// This guards against accidental removal of the method from the trait.
    #[test]
    fn test_tlb_arch_set_active_root_as_generic_constraint() {
        fn do_set_root<T: TlbArch>(root: PhysBytes) {
            // SAFETY: Mock implementation — no real hardware touched.
            unsafe { T::set_active_root(root); }
        }
        do_set_root::<MockTlbArch>(PhysBytes(0x2000));
    }
}
