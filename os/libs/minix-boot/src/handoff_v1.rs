//! Cross-image handoff wire protocol v1 (`BootHandoffV1`) — layout authority.
//!
//! This module defines the **only** authoritative binary shape for data that
//! crosses from a boot surface into a separately linked kernel image. The
//! design rationale lives in `rewrite-notes/01-stage-kernel/01-code-design.md`
//! (T1, v9): the envelope carries protocol header fields (`magic`/`version`/
//! `size`), `arena_len`, the kernel scalars as fixed-width wire types, the
//! runtime handoff fields patched in after the root switch, and one
//! `byte_len` descriptor per payload partition. Payload addresses are *not*
//! stored: each partition lives at a fixed offset from the arena base, so
//! "points at the wrong partition / overlaps / points into the envelope"
//! are unrepresentable rather than merely rejected.
//!
//! Every type here is `#[repr(C)]` with fixed-width fields; the element sizes
//! and the arena offsets are pinned by compile-time assertions below. The
//! assertions run on every build of every supported target (the design's
//! layout gate: a cross-compile check per target, not a host-only test).
//!
//! Wire types are deliberately separate from the internal semantic types
//! (`KernelInfo`/`MemoryRegion`/`BootModule`/`PlatformDescSource`): the
//! domain model keeps its idiomatic Rust types, and conversion happens only
//! at the controlled-copy boundary.

use core::mem::{align_of, offset_of, size_of};

/// Physical-address wire field (semantics declared per field).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WirePhysAddr(pub u64);

/// Virtual-address wire field (semantics declared per field).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireVirtAddr(pub u64);

/// Wire form of one memory-map entry (16 B: base + byte length).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireMemoryRegion {
    pub base: u64,
    pub len: u64,
}

/// Wire form of one boot module (32 B: fixed 16-byte name slot + start + len).
///
/// The 16-byte slot is a **protocol bound**, not a mirror of any kernel
/// storage width: names are copied strlcpy-style and a longer name is a
/// protocol violation on the producing side (same 15+NUL semantics as C).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WireBootModule {
    pub name: [u8; 16],
    pub start: u64,
    pub len: u64,
}

/// Wire form of one platform descriptor source (16 B: kind tag + padding +
/// physical address). The kind is a bare `u32` so no Rust enum representation
/// participates in the protocol.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WirePlatformDescSource {
    pub kind: u32,
    pub reserved: u32,
    pub addr: u64,
}

/// Protocol magic for the v1 envelope ("MNXH" in little-endian bytes).
pub const BOOT_HANDOFF_V1_MAGIC: u32 = 0x4D4E_5848;

/// Protocol version of the envelope defined in this module.
pub const BOOT_HANDOFF_V1_VERSION: u32 = 1;

/// `free_upper_idx` sentinel for "no value" (a valid index can never be
/// `u64::MAX`; decode checks the sentinel first, then the range).
pub const FREE_UPPER_IDX_NONE: u64 = u64::MAX;

/// The handoff envelope. Lives at the start of the handoff arena; the
/// runtime fields (`root_page`, bump triple) are patched in place after the
/// root switch, everything else is filled before `ExitBootServices`.
///
/// `size` is the size of *this struct* (the design pins that meaning);
/// `arena_len` is the whole arena capacity (`ARENA_CAP`), validated by the
/// receiver.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootHandoffV1 {
    // ── protocol header ──
    pub magic: u32,
    pub version: u32,
    pub size: u32,
    pub reserved0: u32,
    pub arena_len: u64,
    // ── kernel scalars (fixed-width wire forms) ──
    pub kern_virt_base: WireVirtAddr,
    pub kern_phys_base: WirePhysAddr,
    pub kern_size: u64,
    pub free_upper_idx: u64,
    pub user_sp: WireVirtAddr,
    pub kern_stack_top: WireVirtAddr,
    pub syscall_entry: WireVirtAddr,
    pub bootstrap_start: WirePhysAddr,
    pub bootstrap_len: u64,
    // ── runtime handoff fields (patched after the root switch) ──
    pub root_page: u64,
    pub bump_base: u64,
    pub bump_next: u64,
    pub bump_end: u64,
    // ── payload partition descriptors (byte lengths only) ──
    pub memmap_byte_len: u64,
    pub reserved_regions_byte_len: u64,
    pub boot_modules_byte_len: u64,
    pub platform_sources_byte_len: u64,
    pub param_buf_byte_len: u64,
}

// ── arena layout (fixed offsets, compile-time arithmetic) ────────────────

/// Envelope budget (the envelope itself must fit; 256 B is the design budget).
pub const ENVELOPE_CAP: usize = 256;

/// Wire partition capacities, in elements (`128×16 + 256×16 + 12×32 + 4×16
/// + 1024 = 7616` bytes).
pub const WIRE_MEMMAP_CAP: usize = 128;
pub const WIRE_RESERVED_CAP: usize = 256;
pub const WIRE_MODULE_CAP: usize = 12;
pub const WIRE_PLATFORM_CAP: usize = 4;
pub const WIRE_PARAM_CAP: usize = 1024;

/// Semantic work-partition capacities (`128×16 + 256×16 = 6144` bytes).
pub const SEMANTIC_MEMMAP_CAP: usize = 128;
pub const SEMANTIC_RESERVED_CAP: usize = 256;

/// Wire partition offsets from the arena base.
pub const WIRE_OFFSET: usize = ENVELOPE_CAP;
pub const WIRE_MEMMAP_OFFSET: usize = WIRE_OFFSET;
pub const WIRE_RESERVED_OFFSET: usize = WIRE_MEMMAP_OFFSET + WIRE_MEMMAP_CAP * 16;
pub const WIRE_MODULES_OFFSET: usize = WIRE_RESERVED_OFFSET + WIRE_RESERVED_CAP * 16;
pub const WIRE_PLATFORM_OFFSET: usize = WIRE_MODULES_OFFSET + WIRE_MODULE_CAP * 32;
pub const WIRE_PARAM_OFFSET: usize = WIRE_PLATFORM_OFFSET + WIRE_PLATFORM_CAP * 16;
pub const WIRE_END: usize = WIRE_PARAM_OFFSET + WIRE_PARAM_CAP;

/// Semantic work-partition offsets (shim-side `MemoryRegion` supply; ignored
/// by the receiver, no wire descriptor).
pub const SEMANTIC_OFFSET: usize = WIRE_END;
pub const SEMANTIC_MEMMAP_OFFSET: usize = SEMANTIC_OFFSET;
pub const SEMANTIC_RESERVED_OFFSET: usize = SEMANTIC_MEMMAP_OFFSET + SEMANTIC_MEMMAP_CAP * 16;
pub const SEMANTIC_END: usize = SEMANTIC_RESERVED_OFFSET + SEMANTIC_RESERVED_CAP * 16;

/// Whole-arena capacity: 4 pages (16 KiB).
pub const ARENA_CAP: usize = 4 * 4096;

// ── layout gate ──────────────────────────────────────────────────────────

const _: () = {
    // Element shapes.
    assert!(size_of::<WireMemoryRegion>() == 16);
    assert!(align_of::<WireMemoryRegion>() == 8);
    assert!(size_of::<WireBootModule>() == 32);
    assert!(align_of::<WireBootModule>() == 8);
    assert!(size_of::<WirePlatformDescSource>() == 16);
    assert!(align_of::<WirePlatformDescSource>() == 8);
    assert!(size_of::<WirePhysAddr>() == 8);
    assert!(size_of::<WireVirtAddr>() == 8);

    // Envelope: fits the budget, 8-byte aligned, key offsets pinned.
    assert!(size_of::<BootHandoffV1>() == 168);
    assert!(align_of::<BootHandoffV1>() == 8);
    assert!(size_of::<BootHandoffV1>() <= ENVELOPE_CAP);
    assert!(offset_of!(BootHandoffV1, magic) == 0);
    assert!(offset_of!(BootHandoffV1, version) == 4);
    assert!(offset_of!(BootHandoffV1, size) == 8);
    assert!(offset_of!(BootHandoffV1, arena_len) == 16);
    assert!(offset_of!(BootHandoffV1, kern_virt_base) == 24);
    assert!(offset_of!(BootHandoffV1, kern_phys_base) == 32);
    assert!(offset_of!(BootHandoffV1, free_upper_idx) == 48);
    assert!(offset_of!(BootHandoffV1, root_page) == 96);
    assert!(offset_of!(BootHandoffV1, bump_end) == 120);
    assert!(offset_of!(BootHandoffV1, memmap_byte_len) == 128);
    assert!(offset_of!(BootHandoffV1, param_buf_byte_len) == 160);

    // Arena partitions: element-aligned, disjoint, exact, and fitting the cap.
    assert!(WIRE_MEMMAP_OFFSET % align_of::<WireMemoryRegion>() == 0);
    assert!(WIRE_RESERVED_OFFSET % align_of::<WireMemoryRegion>() == 0);
    assert!(WIRE_MODULES_OFFSET % align_of::<WireBootModule>() == 0);
    assert!(WIRE_PLATFORM_OFFSET % align_of::<WirePlatformDescSource>() == 0);
    assert!(WIRE_PARAM_OFFSET == 6848);
    assert!(WIRE_END == 7872);
    assert!(SEMANTIC_MEMMAP_OFFSET == WIRE_END);
    assert!(SEMANTIC_RESERVED_OFFSET % align_of::<crate::kernel_info::MemoryRegion>() == 0);
    assert!(SEMANTIC_END == 14016);
    assert!(SEMANTIC_END <= ARENA_CAP);
    assert!(ARENA_CAP % 4096 == 0);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_element_sizes_are_the_protocol_values() {
        assert_eq!(size_of::<WireMemoryRegion>(), 16);
        assert_eq!(size_of::<WireBootModule>(), 32);
        assert_eq!(size_of::<WirePlatformDescSource>(), 16);
        assert_eq!(size_of::<BootHandoffV1>(), 168);
    }

    #[test]
    fn arena_partition_arithmetic_matches_the_design() {
        // 128×16 + 256×16 + 12×32 + 4×16 + 1024 = 7616 wire bytes.
        assert_eq!(WIRE_END - WIRE_OFFSET, 7616);
        // 128×16 + 256×16 = 6144 semantic bytes.
        assert_eq!(SEMANTIC_END - SEMANTIC_OFFSET, 6144);
        // Envelope + wire + semantic ≈ 14 KiB, inside the 16 KiB arena.
        assert_eq!(SEMANTIC_END, 14016);
        assert!(SEMANTIC_END <= ARENA_CAP);
    }

    #[test]
    fn free_upper_idx_sentinel_is_distinct_from_any_valid_index() {
        assert_eq!(FREE_UPPER_IDX_NONE, u64::MAX);
        assert!(FREE_UPPER_IDX_NONE > u32::MAX as u64);
    }
}
