//! The memory map of the device-tree boot chains: one concrete collector plus
//! the two-table shape every boot chain hands the OS semantic layer.
//!
//! Semantic contract (the positive use of axiom 3): every boot chain exposes
//! the **same** shape to the OS semantic layer — two `'static` tables,
//! conventional (the PMM may allocate from it directly) and reserved (touched
//! through the Direct Map at run time but never reallocated). Each chain's own
//! firmware shape (UEFI `GetMemoryMap`, DTB `/memory`) stays inside its own
//! collector and does not leak into consumers (previously the opensbi leg handed
//! the kernel a "hard-coded single QEMU virt DRAM region" directly — exactly the
//! leak this contract closes).
//!
//! The contract is carried by the **result shape** that [`DtbMemoryMap::collect`]
//! returns, not by a trait. A trait here would advertise source substitutability
//! that no call site uses: no consumer ever took the collector as a trait object
//! or under a generic bound, the production UEFI leg cannot satisfy the
//! "two `'static` tables" signature while boot services are still alive (its
//! classification writes into the handoff arena instead), and a placeholder for
//! a boot form this repository does not have can only implement it by returning
//! two empty tables — an empty table that looks like a valid answer is worse
//! than no answer, because it fails deep in the kernel instead of at the edge.
//! A caller names the concrete collector it uses. An interface becomes worth
//! introducing when a second source gains a consumer that genuinely wants to be
//! source-agnostic.
//!
//! The boot-form layer is **not** merged here (axiom 3: the standalone `BootShim`
//! executable and the `bootface` self-boot face correspond to different machine
//! states; this layer only unifies the memmap source underneath).
//!
//! Implementation placement: this DTB collector lives in the library (DTB
//! parsing is isomorphic for every bare-metal boot face, and the `fdt`
//! dependency is already on this library's list). The UEFI leg lives in
//! boot-shim, because `GetMemoryMap` needs the `uefi` crate, which must not
//! point back into this library.

use crate::kernel_info::MemoryRegion;
use minix_types::PhysBytes;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Capacity of the DTB memory map. Symmetric with kernel-image bootface's old
/// `MEMMAP_MAX = 8` (the capacity moved with the migration; consumer semantics
/// are unchanged).
const DTB_MEMMAP_MAX: usize = 8;

// `static mut`: the same form as bootface's MEMMAP_STORAGE — the single-hart
// boot face writes once and the storage is read-only afterwards; the read side
// never hands out `&mut` (addr_of! base plus from_raw_parts).
static mut DTB_FREE: [MemoryRegion; DTB_MEMMAP_MAX] =
    [MemoryRegion { base: PhysBytes(0), len: 0 }; DTB_MEMMAP_MAX];
static mut DTB_RESERVED: [MemoryRegion; DTB_MEMMAP_MAX] =
    [MemoryRegion { base: PhysBytes(0), len: 0 }; DTB_MEMMAP_MAX];
static DTB_FREE_N: AtomicUsize = AtomicUsize::new(0);
static DTB_RESERVED_N: AtomicUsize = AtomicUsize::new(0);

/// Clip the part of `[start, start+len)` that falls inside the resident
/// firmware area (`[0, fw_end)`, a lower-bound semantic measured in PAs from
/// zero) into reserved, and return the rest as conventional. Returns
/// `(free_start, free_len)`; `None` = the whole span is firmware area (no free
/// remainder).
///
/// A pure function — the compile-time guards at the bottom of this module pin
/// down its single-character mutations (the arithmetic was migrated from the
/// original bootface `clip_firmware`; with `fw_end` parameterized, the riscv leg
/// passes `KERN_PHYS_BASE` and the opensbi boot-shim leg passes 0 = no
/// clipping).
const fn clip_reserved(
    start: u64,
    len: u64,
    fw_end: u64,
) -> Option<(u64, u64)> {
    if start >= fw_end {
        return Some((start, len));
    }
    let drop = fw_end - start;
    if drop >= len {
        return None; // the whole span sits below the firmware area
                     // (including the exact len==drop case)
    }
    Some((fw_end, len - drop))
}

// Compile-time guards for the clipping arithmetic (migrated from the original
// bootface `clip_firmware` assertion group; `fw_end` is passed explicitly as
// 0x8020_0000 = the true value of the riscv leg's `KERN_PHYS_BASE`):
//   • a pure firmware span `[DRAM, FW_END)` → None
const _FW_PURE: () = assert!(clip_reserved(0x8000_0000, 0x20_0000, 0x8020_0000).is_none());
//   • a span entirely above the image → kept as is
const _FW_ABOVE: () = assert!(
    matches!(clip_reserved(0x8020_0000, 0x1000, 0x8020_0000), Some((0x8020_0000, 0x1000)))
);
//   • a straddling span → clip off the firmware area, keep the rest
const _FW_CROSS: () = assert!(
    matches!(
        clip_reserved(0x8000_0000, 0x20_1000, 0x8020_0000),
        Some((0x8020_0000, 0x1000))
    )
);

/// DTB leg: parse `reg` of `/memory` and `/memory@<unit>` (the unit-address
/// variant), clip off the resident firmware area through [`clip_reserved`], and
/// produce the two tables. Lookup walks the whole tree through `all_nodes`
/// rather than `find_node` (upstream fdt 0.1.5's `find_node` misses nodes in
/// some firmware-rebuilt blobs — demonstrated in practice, so both consuming
/// bootfaces take the same detour).
///
/// `firmware_reserve_end`: upper bound (PA) of the resident firmware area. The
/// riscv bootface leg passes `KERN_PHYS_BASE` (OpenSBI resides in
/// `[DRAM_BASE, KERN_PHYS_BASE)` and the VM bitmap metadata memsets that whole
/// span — confirmed on real hardware, so clipping is functionally required); the
/// opensbi boot-shim leg passes 0 = no clipping (that leg's kernel side cuts the
/// map itself through `cut_memmap`).
pub struct DtbMemoryMap<'a> {
    fdt: &'a fdt::Fdt<'a>,
    firmware_reserve_end: u64,
}

impl<'a> DtbMemoryMap<'a> {
    pub fn new(fdt: &'a fdt::Fdt<'a>, firmware_reserve_end: u64) -> Self {
        Self {
            fdt,
            firmware_reserve_end,
        }
    }
}

impl DtbMemoryMap<'_> {
    /// Return the two `'static` tables `(conventional, reserved)`.
    ///
    /// Borrowing `self` reads the DTB a second time is not required: the tables
    /// live in this library's static storage, so the returned slices outlive
    /// the borrow of the blob.
    pub fn collect(&self) -> (&'static [MemoryRegion], &'static [MemoryRegion]) {
        // Single-hart boot face: written once, then read-only; a re-entry that
        // zeroes the counters (a second collect on the same face) merely
        // overwrites the same cells.
        DTB_FREE_N.store(0, Ordering::Relaxed);
        DTB_RESERVED_N.store(0, Ordering::Relaxed);
        let mut free_n = 0usize;
        let mut reserved_n = 0usize;
        for node in self.fdt.all_nodes() {
                // `/memory` or `/memory@80000000` (the unit-address variant).
                if node.name.split('@').next() != Some("memory") {
                    continue;
                }
                let Some(regs) = node.reg() else { continue };
                for region in regs {
                    let start = region.starting_address as u64;
                    let len = match region.size {
                        Some(s) if s != 0 => s,
                        _ => continue,
                    };
                    if let Some((free_start, free_len)) =
                        clip_reserved(start, len as u64, self.firmware_reserve_end)
                    {
                        if free_n < DTB_MEMMAP_MAX {
                            // SAFETY: free_n < the array length; single-hart boot.
                            unsafe {
                                DTB_FREE[free_n] =
                                    MemoryRegion { base: PhysBytes(free_start), len: free_len as usize };
                            }
                            free_n += 1;
                        }
                    }
                    if start < self.firmware_reserve_end && reserved_n < DTB_MEMMAP_MAX {
                        let r_len = (self.firmware_reserve_end - start).min(len as u64) as usize;
                        // SAFETY: reserved_n < the array length; single-hart boot.
                        unsafe {
                            DTB_RESERVED[reserved_n] =
                                MemoryRegion { base: PhysBytes(start), len: r_len };
                        }
                        reserved_n += 1;
                    }
                }
            }
        DTB_FREE_N.store(free_n, Ordering::Release);
        DTB_RESERVED_N.store(reserved_n, Ordering::Release);
        // SAFETY: both static arrays have length DTB_MEMMAP_MAX; n ≤
        // DTB_MEMMAP_MAX (clamped above before enqueueing); on the single-hart
        // boot face the read only happens after the write.
        unsafe {
            (
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(DTB_FREE) as *const MemoryRegion,
                    free_n,
                ),
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(DTB_RESERVED) as *const MemoryRegion,
                    reserved_n,
                ),
            )
        }
    }
}

/// Out-of-bounds gate for the aarch64 UEFI chain's DTB descriptor source (a
/// pure function, host-testable): every conventional span of the firmware
/// `GetMemoryMap` snapshot must be fully covered by the DTB `/memory` panorama
/// (produced by `DtbMemoryMap` with `firmware_reserve_end=0` = no clipping).
/// Out of bounds = the firmware snapshot contradicts platform truth (missing
/// accounting for self-occupied areas, or firmware lying); the caller then
/// fails fast (while the shim can still print). Returns `Err(base, len)` of the
/// offending span.
///
/// The v1 semantics are gate plus source printing, not source replacement — the
/// DTB `/memory` is the DRAM panorama (including the LOADER_DATA pages the shim
/// occupies itself), so using it directly as the conventional source would hand
/// those self-occupied pages to the kernel PMM (the self-allocation accounting
/// risk already on record); replacing the source awaits a separate accounting
/// decision.
pub fn dtb_covers_conventional(
    dtb_free: &[MemoryRegion],
    conventional: &[MemoryRegion],
) -> Result<(), (u64, usize)> {
    for r in conventional {
        let r_start = r.base.0;
        let r_end = r.base.0 + r.len as u64;
        let covered = dtb_free.iter().any(|d| {
            let d_start = d.base.0;
            let d_end = d.base.0 + d.len as u64;
            d_start <= r_start && r_end <= d_end
        });
        if !covered {
            return Err((r.base.0, r.len));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{format, vec::Vec};

    /// Hand-build a minimal FDT blob in canonical form: header(40) + memrsv(16)
    /// + structure block + strings. The root node carries
    /// `#address-cells=2`/`#size-cells=2`, and each `/memory` child node (with or
    /// without the unit-address variant) hangs a 128-bit `reg`.
    fn build_fdt(memory_regs: &[(u64, u64)], unit_address: bool) -> Vec<u8> {
        let mut strings: Vec<u8> = Vec::new();
        strings.extend_from_slice(b"#address-cells\0");
        strings.extend_from_slice(b"#size-cells\0");
        let nameoff_cells = 0usize;
        let nameoff_size = 15usize; // "#address-cells\0" is exactly 15 bytes
        strings.extend_from_slice(b"reg\0");
        let nameoff_reg = 27usize; // 15 + 12 ("#size-cells\0" is exactly 12 bytes)
        let mut struct_blk: Vec<u8> = Vec::new();
        let push_u32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_be_bytes());
        let push_prop = |out: &mut Vec<u8>, nameoff: u32, val: &[u8]| {
            push_u32(out, 3); // FDT_PROP
            push_u32(out, val.len() as u32);
            push_u32(out, nameoff as u32);
            out.extend_from_slice(val);
            while out.len() % 4 != 0 { out.push(0); }
        };
        push_u32(&mut struct_blk, 1); // FDT_BEGIN_NODE (root, empty name)
        struct_blk.push(0);
        while struct_blk.len() % 4 != 0 { struct_blk.push(0); }
        push_prop(&mut struct_blk, nameoff_cells as u32, &2u32.to_be_bytes());
        push_prop(&mut struct_blk, nameoff_size as u32, &2u32.to_be_bytes());
        for (base, size) in memory_regs {
            push_u32(&mut struct_blk, 1); // FDT_BEGIN_NODE
            let name: &str = if unit_address {
                &format!("memory@{base:x}\u{0}")
            } else {
                "memory\u{0}"
            };
            struct_blk.extend_from_slice(name.as_bytes());
            while struct_blk.len() % 4 != 0 { struct_blk.push(0); }
            // The root declares cells 2/2, so reg is fully encoded for that
            // semantic (128 bits: [hi(base)=0, base, hi(size)=0, size]). A
            // lesson from real hardware: a short 1/1 encoding is truncated under
            // 2/2 semantics (size=None) → the region is silently skipped.
            let mut reg: Vec<u8> = Vec::new();
            reg.extend_from_slice(&0u32.to_be_bytes());
            reg.extend_from_slice(&(*base as u32).to_be_bytes());
            reg.extend_from_slice(&0u32.to_be_bytes());
            reg.extend_from_slice(&(*size as u32).to_be_bytes());
            push_prop(&mut struct_blk, nameoff_reg as u32, &reg);
            push_u32(&mut struct_blk, 2); // FDT_END_NODE
        }
        push_u32(&mut struct_blk, 2); // FDT_END_NODE (root)
        push_u32(&mut struct_blk, 9); // FDT_END

        let struct_off = 40u32 + 16u32;
        let strings_off = struct_off + struct_blk.len() as u32;
        let mut out: Vec<u8> = Vec::new();
        let push = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_be_bytes());
        push(&mut out, 0xd00d_feed);          // magic
        push(&mut out, 0);                    // totalsize (patched in later)
        push(&mut out, struct_off);
        push(&mut out, strings_off);
        push(&mut out, 40);                   // off_memrsv
        push(&mut out, 17);                   // version
        push(&mut out, 16);                   // last_comp_version
        push(&mut out, 0);                    // boot_cpuid
        push(&mut out, strings.len() as u32); // size_strings
        push(&mut out, struct_blk.len() as u32); // size_struct
        out.extend_from_slice(&[0u8; 16]);    // memrsv
        out.extend_from_slice(&struct_blk);
        out.extend_from_slice(&strings);
        while out.len() % 4 != 0 { out.push(0); }
        let total = out.len() as u32;
        out[4..8].copy_from_slice(&total.to_be_bytes());
        out
    }

    #[test]
    fn dtb_single_memory_region_fw_end_zero() {
        let blob = build_fdt(&[(0x8000_0000, 0x4000_0000)], false);
        let fdt = fdt::Fdt::new(&blob).expect("规范形 stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0).collect();
        assert_eq!(conv.len(), 1);
        assert_eq!(conv[0].base.0, 0x8000_0000);
        assert_eq!(conv[0].len, 0x4000_0000 as usize);
        assert!(res.is_empty(), "fw_end=0 不产生 reserved");
    }

    #[test]
    fn dtb_multi_region_and_unit_address_variant() {
        let blob = build_fdt(&[(0x8000_0000, 0x4000_0000), (0x8800_0000, 0x1000_0000)], true);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, _res) = DtbMemoryMap::new(&fdt, 0).collect();
        assert_eq!(conv.len(), 2, "两段 /memory → 2 条（含 unit-address 变体）");
        assert_eq!(conv[0].base.0, 0x8000_0000);
        assert_eq!(conv[1].base.0, 0x8800_0000);
    }

    #[test]
    fn dtb_fw_clip_moves_tail_to_reserved() {
        // /memory = [0x8000_0000, +64MiB), fw_end = 0x8020_0000 (2MiB OpenSBI)
        // → free = [0x8020_0000, +62MiB), reserved = [0x8000_0000, 2MiB)
        let blob = build_fdt(&[(0x8000_0000, 0x400_0000)], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0x8020_0000).collect();
        assert_eq!(conv.len(), 1);
        assert_eq!(conv[0].base.0, 0x8020_0000, "削界后 free 起点抬到 fw_end");
        assert_eq!(conv[0].len, (0x400_0000 - 0x20_0000) as usize);
        assert_eq!(res.len(), 1, "削掉的固件段进 reserved");
        assert_eq!(res[0].base.0, 0x8000_0000);
        assert_eq!(res[0].len, 0x20_0000 as usize);
    }

    #[test]
    fn dtb_fully_reserved_region_drops_from_free() {
        // /memory = [0x8000_0000, +1MiB) lies entirely below fw_end=0x8020_0000
        // → free is empty and reserved holds the whole span (a bootface-side
        // consumer seeing an empty table halts honestly).
        let blob = build_fdt(&[(0x8000_0000, 0x10_0000)], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0x8020_0000).collect();
        assert!(conv.is_empty(), "整段固件区 → free 空（诚实停机由消费方判）");
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn dtb_no_memory_node_yields_empty() {
        let blob = build_fdt(&[], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0).collect();
        assert!(conv.is_empty(), "无 /memory → 空（诚实停机由消费方判）");
        assert!(res.is_empty());
    }

    #[test]
    fn dtb_gate_covers_contained_and_multi_region() {
        // both conventional spans fall inside /memory [0x4000_0000, +512MiB) → Ok
        let dtb = [MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x2000_0000 }];
        let conv = [
            MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x1000_0000 },
            MemoryRegion { base: PhysBytes(0x5000_0000), len: 0x800_0000 },
        ];
        assert_eq!(dtb_covers_conventional(&dtb, &conv), Ok(()));
    }

    #[test]
    fn dtb_gate_reports_out_of_bounds_region() {
        // an out-of-bounds span (conventional above the /memory upper bound) →
        // Err(base, len)
        let dtb = [MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x1000_0000 }];
        let conv = [
            MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x800_0000 },
            MemoryRegion { base: PhysBytes(0x5000_0000), len: 0x1000 },
        ];
        assert_eq!(
            dtb_covers_conventional(&dtb, &conv),
            Err((0x5000_0000, 0x1000))
        );
    }

    #[test]
    fn dtb_gate_rejects_partial_overlap() {
        // a straddling span (lower half inside /memory, upper half outside) does
        // not count as covered
        let dtb = [MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x800_0000 }];
        let conv = [MemoryRegion { base: PhysBytes(0x4040_0000), len: 0x800_0000 }];
        assert_eq!(
            dtb_covers_conventional(&dtb, &conv),
            Err((0x4040_0000, 0x800_0000))
        );
    }

    #[test]
    fn dtb_gate_empty_conventional_is_ok() {
        let dtb = [MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x1000 }];
        assert_eq!(dtb_covers_conventional(&dtb, &[]), Ok(()));
    }
}
