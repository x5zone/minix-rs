//! Kernel device-window description and high-half virtual mapping strategy
//! (PD-22 step 2, ruling `pd22-step2-brief.md` §15).
//!
//! Two layers, deliberately separated (review §4):
//!
//! - **Physical device description** ([`device_window_regions`]): where the
//!   MMIO regions live in the physical address space. The values are
//!   platform facts (QEMU virt layout verified against the machine DTB —
//!   see the brief §14.1), *not* RISC-V/x86 ISA constants; real boards
//!   re-derive them from DTB/ACPI and the boot-time checks must confirm the
//!   discovered ranges fall inside these windows.
//! - **Virtual mapping strategy** ([`DEVICE_VA_BASE`] + [`device_va_for`]):
//!   each region i is mapped, supervisor-only RW/NX/G, at a private 2 MiB
//!   slot `DEVICE_VA_BASE + i * WINDOW_STRIDE`; the VA-reserved-but-absent
//!   case (ACLINT SSWI without an `aclint=on` DTB node) is a *virtual
//!   address reservation*, not a claim that the device exists.
//!
//! This module is pure description: no page-table writes, no consumers.
//! `map_kernel` (process roots) and the bootstrap-root bring-up install the
//! mappings; platform drivers receive the kernel VA from
//! [`device_va_for`] — they never synthesize addresses from physical ones.
//!
//! Historical note: this replaces the former `kernel_identity_mmio_regions`
//! identity replay (PD-22 step 2 — identity mappings must not exist in
//! process roots).

use crate::paging::PageFlags;

/// One MMIO region to be mapped into the kernel device window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceWindowRegion {
    /// Physical base address of the MMIO region.
    pub phys_base: u64,
    /// Page-aligned mapping length (≥ the device's reg length).
    pub size: u64,
    /// Stable slot index (VA base = cumulative span of lower slots).
    /// Never renumber — drivers bake the resulting VA.
    pub slot: usize,
    /// Whether this region has bootstrap-phase consumers (controller/timer
    /// init running under the bootstrap root). Only bootstrap-true regions
    /// are installed into the bootstrap root (huge leaves only — the
    /// 4 KiB map() path hangs pre-DM-establishment); ALL regions are
    /// installed into process roots by `map_kernel` (4 KiB path, live
    /// allocator). Regions whose consumers arm at runtime (UART0 selftest,
    /// ACLINT SSWI scheduler IPI) are bootstrap=false.
    pub bootstrap: bool,
}

/// Page-round a region size for VA-allocation purposes (PTE coverage is
/// page-granular even when the device `reg` length is not).
const fn page_round(v: u64) -> u64 {
    (v + 0xFFF) & !0xFFF
}

/// 2 MiB-align a span: every slot base is 2 MiB-aligned so bootstrap-root
/// installs can use huge leaves (the proven pre-paging mapping path).
const fn huge_round(v: u64) -> u64 {
    (v + 0x1F_FFFF) & !0x1F_FFFF
}

/// VA base of a region's slot: the cumulative 2 MiB-aligned span of all
/// regions with smaller slots. Sizing by region length (not a fixed
/// stride) makes overlapping slots impossible by construction — the first
/// attempt used a fixed 2 MiB stride and the PLIC region (6 MiB) swallowed
/// the following two slots (caught by the riscv ATF gate as `AlreadyMapped`
/// in VM's first `init_page_table`). Slots are 2 MiB-aligned so boot-root
/// installs map them as single huge leaves.
pub const fn slot_va_base(regions: &[DeviceWindowRegion], slot: usize) -> u64 {
    let mut base = DEVICE_VA_BASE;
    let mut i = 0usize;
    while i < slot && i < regions.len() {
        base += huge_round(regions[i].size);
        i += 1;
    }
    base
}

/// Total VA span reserved by the table (for collision audits against the
/// rest of the high-half address map).
pub const fn device_window_span(regions: &[DeviceWindowRegion]) -> u64 {
    let mut end = DEVICE_VA_BASE;
    let mut i = 0usize;
    while i < regions.len() {
        end += huge_round(regions[i].size);
        i += 1;
    }
    end - DEVICE_VA_BASE
}

// ── x86-64 ──────────────────────────────────────────────────────────────

/// x86-64 device-window VA base: inside the free span between the kernel
/// image (ends ≈ `0xFFFF8000_00600000`) and the kernel Direct Map
/// (`0xFFFF8080_0000_0000`).
#[cfg(target_arch = "x86_64")]
pub const DEVICE_VA_BASE: u64 = 0xFFFF_8040_0000_0000;

#[cfg(target_arch = "x86_64")]
const _: () = assert!(
    0x2000 <= 0xFFFF_8080_0000_0000 - DEVICE_VA_BASE,
    "x86-64 device window span must fit before the kernel DM window"
);

#[cfg(target_arch = "x86_64")]
const _: () = assert!(
    DEVICE_VA_BASE >= 0xFFFF_8000_0060_0000,
    "x86-64 device window must start above the kernel image end"
);

#[cfg(target_arch = "x86_64")]
const _: () = assert!(
    (DEVICE_VA_BASE >> 47) & 1 == 1 && (DEVICE_VA_BASE >> 63) == 1,
    "x86-64 device window must be canonical high-half"
);

/// x86-64 regions: IO-APIC and local-APIC pages (QEMU defaults; the LAPIC
/// runtime base is MSR-verified against the constant at APIC init —
/// check-and-reject, see `plat/src/x86_64/interrupt.rs`).
#[cfg(target_arch = "x86_64")]
pub const fn device_window_regions() -> &'static [DeviceWindowRegion] {
    &[
        DeviceWindowRegion { phys_base: 0x0FEC0_0000, size: 0x1000, slot: 0, bootstrap: true },
        DeviceWindowRegion { phys_base: 0x0FEE0_0000, size: 0x1000, slot: 1, bootstrap: true },
    ]
}

// ── AArch64 ─────────────────────────────────────────────────────────────

/// aarch64: the interrupt controller is reached through system registers
/// and devices through the TTBR1-pinned high MMIO window — no device window
/// and no identity replay (PD-22 step 2 already satisfied on this arch).
#[cfg(target_arch = "aarch64")]
pub const DEVICE_VA_BASE: u64 = 0;

#[cfg(target_arch = "aarch64")]
pub const fn device_window_regions() -> &'static [DeviceWindowRegion] {
    &[]
}

// ── RISC-V 64 ───────────────────────────────────────────────────────────

/// riscv64 device-window VA base: in the free span between the kernel image
/// (`0xFFFFFFC0_0000_0000`, ≤ 8 MiB span) and the kernel Direct Map
/// (`0xFFFFFFC0_4000_0000`).
#[cfg(target_arch = "riscv64")]
pub const DEVICE_VA_BASE: u64 = 0xFFFF_FFC0_2000_0000;

#[cfg(target_arch = "riscv64")]
const _: () = assert!(
    0x60_0000 <= 0xFFFF_FFC0_4000_0000 - DEVICE_VA_BASE,
    "riscv64 device window span (3 huge-aligned slots) must fit before \
     the kernel DM window"
);

#[cfg(target_arch = "riscv64")]
const _: () = assert!(
    DEVICE_VA_BASE >= 0xFFFF_FFC0_0080_0000,
    "riscv64 device window must start above the kernel image end"
);

// Compile-time pins for the riscv64 values that the brief §14.1 verified
// against the machine DTB (host tests cannot reach the riscv64 table —
// these asserts run on every riscv64 build instead).
#[cfg(target_arch = "riscv64")]
const _: () = {
    let regions = device_window_regions();
    assert!(regions.len() == 3);
    assert!(regions[0].phys_base == 0x0C00_0000 && regions[0].size == 0x60_0000);
    assert!(regions[1].phys_base == 0x1000_0000 && regions[1].size == 0x1000);
    assert!(regions[2].phys_base == 0x02F0_0000 && regions[2].size == 0x4000);
};

/// riscv64 regions (QEMU virt, verified against the machine DTB —
/// `pd22-step2-brief.md` §14.1). Region 2 (ACLINT SSWI) is a *conditional*
/// device: the VA slot is always reserved, but the mapping is installed and
/// the IPI path enabled only when the DTB carries the
/// `riscv,aclint-sswi` node (existence is checked against this window at
/// boot; default QEMU has no node and nothing may access the slot).
#[cfg(target_arch = "riscv64")]
pub const fn device_window_regions() -> &'static [DeviceWindowRegion] {
    &[
        // PLIC — full DTB reg length 0x600000 (the former 2 MiB window was
        // proven insufficient: claim(context 1) at offset 0x201004 fell
        // outside it; brief §14.2).
        DeviceWindowRegion {
            phys_base: 0x0C00_0000,
            size: 0x60_0000,
            slot: 0,
            bootstrap: true,
        },
        // UART0 page (NS16550A; consumer kernel/src/uart_irq.rs) — the RX
        // selftest arms at BOOT on the timer carrier (under the bootstrap
        // root), so this page IS bootstrap-phase. Physical base is 2 MiB
        // aligned, so the bootstrap install maps it as one huge leaf.
        DeviceWindowRegion { phys_base: 0x1000_0000, size: 0x1000, slot: 1, bootstrap: true },
        // ACLINT SSWI (conditional — DTB-gated at install time); the
        // scheduler-IPI write happens at runtime, under process roots only.
        DeviceWindowRegion { phys_base: 0x02F0_0000, size: 0x4000, slot: 2, bootstrap: false },
    ]
}

/// The kernel virtual address for MMIO address `pa` inside region `slot`
/// of table `regions`. Install-time and driver code both go through this
/// function — drivers never compute `DEVICE_VA_BASE + pa` themselves.
///
/// Returns `None` when `pa` falls outside the region's mapped extent.
pub const fn device_va_for(
    regions: &[DeviceWindowRegion],
    slot: usize,
    pa: u64,
) -> Option<u64> {
    if slot >= regions.len() {
        return None;
    }
    let region = &regions[slot];
    if pa < region.phys_base || pa >= region.phys_base + region.size {
        return None;
    }
    Some(slot_va_base(regions, slot) + (pa - region.phys_base))
}

/// Page flags for device-window mappings: supervisor RW, NX, global —
/// never user-accessible, never executable (W^X; brief §15).
pub const fn device_window_flags() -> PageFlags {
    PageFlags::kernel_read_write()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every region is page-aligned and non-empty; slots are unique and
    /// dense (slot i exists for i in 0..count) so VA reservations are
    /// deterministic.
    #[test]
    fn regions_are_page_aligned_with_dense_slots() {
        let regions = device_window_regions();
        for (i, r) in regions.iter().enumerate() {
            assert_eq!(r.slot, i, "slots must be dense");
            assert!(r.size > 0 && r.size % 0x1000 == 0, "page-aligned");
            assert_eq!(r.phys_base % 0x1000, 0, "page-aligned base");
        }
    }

    /// The PLIC window covers every register offset the driver can emit:
    /// claim/complete/threshold for the highest S-mode context and the
    /// priority/enable arrays for `ndev` sources (brief §14.2 — the former
    /// 2 MiB window failed exactly this check at offset 0x201004).
    #[cfg(target_arch = "riscv64")]
    #[test]
    fn plic_window_covers_all_context_registers() {
        let regions = device_window_regions();
        let plic = &regions[0];
        assert_eq!(plic.phys_base, 0x0C00_0000);
        assert_eq!(plic.size, 0x60_0000, "full DTB reg length");
        // context = 2*hart + 1 (S-mode); QEMU virt max config used by the
        // gates is 4 harts → highest S context 7.
        let max_context = 7usize;
        let claim = 0x200004 + max_context * 0x1000;
        let threshold = 0x200000 + max_context * 0x1000;
        let enable_tail = 0x2000 + max_context * 0x80 + 96 * 4;
        for off in [claim, threshold, enable_tail] {
            assert!(
                device_va_for(regions, 0, 0x0C00_0000 + off as u64).is_some(),
                "PLIC offset {off:#x} outside the device window"
            );
        }
        // The historical fault address must be inside the new window.
        assert!(device_va_for(regions, 0, 0x0C00_0000 + 0x201004).is_some());
    }

    /// The SSWI window covers `setip` for the gate hart count, and the
    /// historical VirtIO/UART adjacency is not confused with it
    /// (0x02F00000, not 0x10001000 — brief §13/§14).
    #[cfg(target_arch = "riscv64")]
    #[test]
    fn sswi_window_covers_setip_for_gated_harts() {
        let regions = device_window_regions();
        let sswi = &regions[2];
        assert_eq!(sswi.phys_base, 0x02F0_0000);
        assert_eq!(sswi.size, 0x4000);
        for hart in 0..4usize {
            let setip = sswi.phys_base + 4 * hart as u64;
            assert!(
                device_va_for(regions, 2, setip).is_some(),
                "hart {hart} setip"
            );
        }
        // Adjacent-platform addresses must NOT resolve into the SSWI window.
        assert!(device_va_for(regions, 2, 0x1000_1000).is_none()); // VirtIO
        assert!(device_va_for(regions, 2, 0x1000_0000).is_none()); // UART0
        // Slots never overlap by construction: each slot base is at or
        // above the previous slot's end.
        let b0 = slot_va_base(regions, 0);
        let b1 = slot_va_base(regions, 1);
        let b2 = slot_va_base(regions, 2);
        assert!(b1 >= b0 + huge_round(regions[0].size));
        assert!(b2 >= b1 + huge_round(regions[1].size));
    }

    /// Device VAs land in the reserved span, inside the region's own slot,
    /// with the region offset preserved.
    #[test]
    fn device_va_preserves_region_offset() {
        let regions = device_window_regions();
        for slot in 0..regions.len() {
            let r = &regions[slot];
            let va = device_va_for(regions, slot, r.phys_base).expect("base must map");
            assert_eq!(
                va,
                slot_va_base(regions, slot),
                "base VA = cumulative slot base"
            );
            let last = device_va_for(regions, slot, r.phys_base + r.size - 1)
                .expect("last byte must map");
            assert!(
                last < slot_va_base(regions, slot) + huge_round(r.size),
                "region stays inside its own slot"
            );
        }
        // Out-of-window physical addresses resolve to None.
        assert_eq!(device_va_for(regions, 0, 0), None);
        // Slot beyond the table resolves to None.
        assert_eq!(device_va_for(regions, regions.len(), 0), None);
    }

    /// Device-window flags: supervisor RW, NX, global — the W^X contract
    /// for the new mappings (no user bit, no execute bit).
    #[test]
    fn device_window_flags_are_rw_nx_global_supervisor() {
        let f = device_window_flags();
        assert!(f.contains(PageFlags::WRITABLE));
        assert!(f.contains(PageFlags::GLOBAL));
        assert!(!f.contains(PageFlags::EXECUTABLE));
        assert!(!f.contains(PageFlags::USER_ACCESSIBLE));
    }
}
