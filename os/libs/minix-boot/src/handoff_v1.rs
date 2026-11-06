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

use crate::memmap_source::{PhysicalRange, SourceKind, SourceOrigin, SourceRecord};
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

/// Wire form of one [`crate::memmap_source::SourceRecord`] — 48 bytes.
///
/// The record layer keeps a 128-bit exclusive end so the last page of the
/// address space stays representable; the wire carries that value as two
/// 64-bit halves (`end_hi` is therefore 0 or 1, and the decoder rejects
/// anything else). `a`/`b`/`cut` are the variant payload of the origin:
/// `(node, reg)` for node-derived origins, `(memory_id, 0)` plus `cut` for
/// the derived firmware prefix, and `(entry, 0)` for an MRB entry. The
/// trailing word is reserved and written as zero, so a future field can take
/// it without changing the record size.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireSourceRecord {
    pub start: u64,
    pub end_lo: u64,
    pub end_hi: u64,
    pub attribute: u64,
    pub virtual_start: u64,
    pub snapshot: u64,
    pub cut: u64,
    pub id: u16,
    pub local: u16,
    pub a: u16,
    pub b: u16,
    pub kind: u8,
    pub origin_tag: u8,
    pub input_object: u8,
    pub reserved0: u8,
    pub reserved: u32,
}

impl WireSourceRecord {
    /// `SourceKind` tags. Values are protocol constants: never renumber.
    pub const KIND_MEMORY: u8 = 0;
    pub const KIND_FIRMWARE_PREFIX: u8 = 1;
    pub const KIND_MRB_ENTRY: u8 = 2;
    pub const KIND_RESERVED_NO_MAP: u8 = 3;
    pub const KIND_RESERVED_ORDINARY: u8 = 4;
    pub const KIND_RESERVED_REUSABLE: u8 = 5;

    /// `SourceOrigin` tags. Values are protocol constants: never renumber.
    pub const ORIGIN_MEMORY_NODE: u8 = 0;
    pub const ORIGIN_FIRMWARE_PREFIX: u8 = 1;
    pub const ORIGIN_MRB_ENTRY: u8 = 2;
    pub const ORIGIN_RESERVED_MEMORY_CHILD: u8 = 3;
    /// The final `GetMemoryMap` snapshot; `a` is the descriptor index.
    pub const ORIGIN_UEFI_DESCRIPTOR: u8 = 4;

    /// Encode one record. Total: every `PhysicalRange` is representable
    /// (`start` is 64-bit, `end_exclusive` splits into two halves).
    pub fn encode(record: &SourceRecord) -> Self {
        let kind = match record.kind {
            SourceKind::Memory => Self::KIND_MEMORY,
            SourceKind::FirmwarePrefix => Self::KIND_FIRMWARE_PREFIX,
            SourceKind::MrbEntry => Self::KIND_MRB_ENTRY,
            SourceKind::ReservedNoMap => Self::KIND_RESERVED_NO_MAP,
            SourceKind::ReservedOrdinary => Self::KIND_RESERVED_ORDINARY,
            SourceKind::ReservedReusable => Self::KIND_RESERVED_REUSABLE,
        };
        let (origin_tag, a, b, cut) = match record.origin {
            SourceOrigin::MemoryNode { node, reg } => (Self::ORIGIN_MEMORY_NODE, node, reg, 0),
            SourceOrigin::FirmwarePrefix { memory_id, cut } => {
                (Self::ORIGIN_FIRMWARE_PREFIX, memory_id, 0, cut)
            }
            SourceOrigin::MrbEntry(entry) => (Self::ORIGIN_MRB_ENTRY, entry, 0, 0),
            SourceOrigin::ReservedMemoryChild { node, reg } => {
                (Self::ORIGIN_RESERVED_MEMORY_CHILD, node, reg, 0)
            }
            SourceOrigin::UefiDescriptor { index } => (Self::ORIGIN_UEFI_DESCRIPTOR, index, 0, 0),
        };
        Self {
            start: record.range.start,
            end_lo: record.range.end_exclusive as u64,
            end_hi: (record.range.end_exclusive >> 64) as u64,
            attribute: record.attribute,
            virtual_start: record.virtual_start,
            snapshot: record.snapshot_id,
            cut,
            id: record.id,
            local: record.local_source,
            a,
            b,
            kind,
            origin_tag,
            input_object: record.input_object.tag(),
            reserved0: 0,
            reserved: 0,
        }
    }

    /// Decode one record. `None` = a tag or an `end_hi` value the encoder
    /// cannot produce, i.e. a corrupted or foreign wire image; the caller
    /// must fail closed rather than guess.
    pub fn decode(self) -> Option<SourceRecord> {
        if self.end_hi > 1 || self.reserved != 0 || self.reserved0 != 0 {
            return None;
        }
        let input_object = crate::memmap_source::InputObjectId::from_tag(self.input_object)?;
        let kind = match self.kind {
            Self::KIND_MEMORY => SourceKind::Memory,
            Self::KIND_FIRMWARE_PREFIX => SourceKind::FirmwarePrefix,
            Self::KIND_MRB_ENTRY => SourceKind::MrbEntry,
            Self::KIND_RESERVED_NO_MAP => SourceKind::ReservedNoMap,
            Self::KIND_RESERVED_ORDINARY => SourceKind::ReservedOrdinary,
            Self::KIND_RESERVED_REUSABLE => SourceKind::ReservedReusable,
            _ => return None,
        };
        let origin = match self.origin_tag {
            Self::ORIGIN_MEMORY_NODE => SourceOrigin::MemoryNode {
                node: self.a,
                reg: self.b,
            },
            Self::ORIGIN_FIRMWARE_PREFIX => SourceOrigin::FirmwarePrefix {
                memory_id: self.a,
                cut: self.cut,
            },
            Self::ORIGIN_MRB_ENTRY => SourceOrigin::MrbEntry(self.a),
            Self::ORIGIN_RESERVED_MEMORY_CHILD => SourceOrigin::ReservedMemoryChild {
                node: self.a,
                reg: self.b,
            },
            Self::ORIGIN_UEFI_DESCRIPTOR => SourceOrigin::UefiDescriptor { index: self.a },
            _ => return None,
        };
        // Reject payload words the encoder leaves at zero, so one record has
        // exactly one wire image (a receiver can compare images).
        let payload_ok = match origin {
            SourceOrigin::FirmwarePrefix { .. } => self.b == 0,
            SourceOrigin::MrbEntry(_) | SourceOrigin::UefiDescriptor { .. } => {
                self.b == 0 && self.cut == 0
            }
            SourceOrigin::MemoryNode { .. } | SourceOrigin::ReservedMemoryChild { .. } => {
                self.cut == 0
            }
        };
        if !payload_ok {
            return None;
        }
        // Canonical wire image: fields the other input class leaves at zero
        // must stay zero, so one record has exactly one encoding.
        match input_object {
            crate::memmap_source::InputObjectId::DtbBootInput => {
                if self.snapshot != 0 || self.attribute != 0 || self.virtual_start != 0 {
                    return None;
                }
            }
            crate::memmap_source::InputObjectId::UefiMemoryMap => {
                // `a` carries the descriptor index (origin payload); the
                // explicit local source id must agree with it, and only the
                // DTB-only words may stay zero.
                if self.cut != 0 || self.b != 0 || self.local != self.a {
                    return None;
                }
            }
        }
        Some(SourceRecord {
            id: self.id,
            kind,
            origin,
            range: PhysicalRange::new(
                self.start,
                self.end_lo as u128 | ((self.end_hi as u128) << 64),
            ),
            input_object,
            local_source: self.local,
            snapshot_id: self.snapshot,
            attribute: self.attribute,
            virtual_start: self.virtual_start,
        })
    }
}

/// Protocol magic for the v1 envelope ("MNXH" in little-endian bytes).
pub const BOOT_HANDOFF_V1_MAGIC: u32 = 0x4D4E_5848;

/// Protocol version of the envelope defined in this module.
///
/// 2 = the source-record partition (`memmap_sources`) joined the wire image:
/// the per-descriptor provenance of the memory map now crosses the first hop
/// instead of being dropped on the sending side. A v1 receiver would ignore
/// the new partition and a v1 sender would leave it empty, so the version is
/// compared exactly, not as a minimum.
pub const BOOT_HANDOFF_V1_VERSION: u32 = 3;

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
    pub sources_byte_len: u64,
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
/// Source records carried across the hop.
///
/// 256 is the descriptor bound: the UEFI leg emits one record per descriptor
/// that produces an ordinary table entry, and the map buffer accepts at most
/// `MAX_MAP_DESCRIPTORS = 256` descriptors. The DTB collector's worst case is
/// 40 (`/memory` 8 + declared reservations 8 + MRB 16 + firmware prefixes 8),
/// so the same partition serves both legs.
pub const WIRE_SOURCES_CAP: usize = 256;
/// Wire size of one [`WireSourceRecord`].
pub const WIRE_SOURCE_RECORD_SIZE: usize = 72;

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

/// Source-record partition: the first use of the arena's design spare after
/// the semantic work partition. Receiver-owned landing storage is the
/// kernel's, so this partition has no semantic twin.
pub const WIRE_SOURCES_OFFSET: usize = SEMANTIC_END;
pub const WIRE_SOURCES_END: usize =
    WIRE_SOURCES_OFFSET + WIRE_SOURCES_CAP * WIRE_SOURCE_RECORD_SIZE;

/// Whole-arena capacity: 7 pages (28 KiB) — the 16 KiB envelope + wire +
/// semantic partitions plus the 12 KiB source-record partition.
pub const ARENA_CAP: usize = 9 * 4096;

// ── layout gate ──────────────────────────────────────────────────────────

const _: () = {
    // Element shapes.
    assert!(size_of::<WireMemoryRegion>() == 16);
    assert!(align_of::<WireMemoryRegion>() == 8);
    assert!(size_of::<WireBootModule>() == 32);
    assert!(align_of::<WireBootModule>() == 8);
    assert!(size_of::<WirePlatformDescSource>() == 16);
    assert!(align_of::<WirePlatformDescSource>() == 8);
    assert!(size_of::<WireSourceRecord>() == WIRE_SOURCE_RECORD_SIZE);
    assert!(align_of::<WireSourceRecord>() == 8);
    assert!(size_of::<WirePhysAddr>() == 8);
    assert!(size_of::<WireVirtAddr>() == 8);

    // Envelope: fits the budget, 8-byte aligned, key offsets pinned.
    assert!(size_of::<BootHandoffV1>() == 176);
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
    assert!(offset_of!(BootHandoffV1, sources_byte_len) == 168);

    // Arena partitions: element-aligned, disjoint, exact, and fitting the cap.
    assert!(WIRE_MEMMAP_OFFSET.is_multiple_of(align_of::<WireMemoryRegion>()));
    assert!(WIRE_RESERVED_OFFSET.is_multiple_of(align_of::<WireMemoryRegion>()));
    assert!(WIRE_MODULES_OFFSET.is_multiple_of(align_of::<WireBootModule>()));
    assert!(WIRE_PLATFORM_OFFSET.is_multiple_of(align_of::<WirePlatformDescSource>()));
    assert!(WIRE_PARAM_OFFSET == 6848);
    assert!(WIRE_END == 7872);
    assert!(SEMANTIC_MEMMAP_OFFSET == WIRE_END);
    assert!(
        SEMANTIC_RESERVED_OFFSET.is_multiple_of(align_of::<crate::kernel_info::MemoryRegion>())
    );
    assert!(SEMANTIC_END == 14016);
    assert!(WIRE_SOURCES_OFFSET.is_multiple_of(align_of::<WireSourceRecord>()));
    assert!(WIRE_SOURCES_END == 32448);
    assert!(WIRE_SOURCES_END <= ARENA_CAP);
    assert!(SEMANTIC_END <= ARENA_CAP);
    assert!(ARENA_CAP.is_multiple_of(4096));
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_element_sizes_are_the_protocol_values() {
        assert_eq!(size_of::<WireMemoryRegion>(), 16);
        assert_eq!(size_of::<WireBootModule>(), 32);
        assert_eq!(size_of::<WirePlatformDescSource>(), 16);
        assert_eq!(size_of::<BootHandoffV1>(), 176);
        assert_eq!(size_of::<WireSourceRecord>(), 72);
    }

    #[test]
    fn arena_partition_arithmetic_matches_the_design() {
        // 128×16 + 256×16 + 12×32 + 4×16 + 1024 = 7616 wire bytes.
        assert_eq!(WIRE_END - WIRE_OFFSET, 7616);
        // 128×16 + 256×16 = 6144 semantic bytes.
        assert_eq!(SEMANTIC_END - SEMANTIC_OFFSET, 6144);
        // Envelope + wire + semantic ≈ 14 KiB; fitting the 16 KiB arena is
        // asserted at compile time by the layout gate above.
        assert_eq!(SEMANTIC_END, 14016);
    }

    #[test]
    fn free_upper_idx_sentinel_is_distinct_from_any_valid_index() {
        assert_eq!(FREE_UPPER_IDX_NONE, u64::MAX);
        assert!(FREE_UPPER_IDX_NONE > u32::MAX as u64);
    }
}
