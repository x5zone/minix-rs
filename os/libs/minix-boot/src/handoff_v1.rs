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

use crate::memmap_source::{SourceKind, SourceOrigin, SourceRange, SourceRecord};
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

/// Wire form of one platform descriptor source (16 B: kind tag + blob length
/// + physical address). The kind is a bare `u32` so no Rust enum
/// representation participates in the protocol. `len = 0` is the legacy
/// producer's "unknown"; readers must then fall back to one 2 MiB piece.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WirePlatformDescSource {
    pub kind: u32,
    pub len: u32,
    pub addr: u64,
}

/// Wire form of one [`crate::memmap_source::SourceRecord`] — 72 bytes.
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
    pub address_domain: u8,
    /// The firmware's own `EFI_MEMORY_DESCRIPTOR.Type`; 0 for non-UEFI input.
    pub raw_type: u32,
}

impl WireSourceRecord {
    /// `SourceKind` tags. Values are protocol constants: never renumber.
    pub const KIND_MEMORY: u8 = 0;
    pub const KIND_FIRMWARE_PREFIX: u8 = 1;
    pub const KIND_MRB_ENTRY: u8 = 2;
    pub const KIND_RESERVED_NO_MAP: u8 = 3;
    pub const KIND_RESERVED_ORDINARY: u8 = 4;
    pub const KIND_RESERVED_REUSABLE: u8 = 5;
    /// A structurally valid descriptor outside both ordinary tables.
    pub const KIND_UEFI_EXCLUDED: u8 = 7;
    // Kind 6 (startup object) is retired with the v10 startup-object set.

    /// `SourceOrigin` tags. Values are protocol constants: never renumber.
    pub const ORIGIN_MEMORY_NODE: u8 = 0;
    pub const ORIGIN_FIRMWARE_PREFIX: u8 = 1;
    pub const ORIGIN_MRB_ENTRY: u8 = 2;
    pub const ORIGIN_RESERVED_MEMORY_CHILD: u8 = 3;
    /// The final `GetMemoryMap` snapshot; `a` is the descriptor index.
    pub const ORIGIN_UEFI_DESCRIPTOR: u8 = 4;
    // Origin 5 (startup object) is retired with the v10 startup-object set.

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
            SourceKind::UefiExcluded => Self::KIND_UEFI_EXCLUDED,
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
            address_domain: record.address_domain.tag(),
            raw_type: record.raw_type,
        }
    }

    /// Decode one record. `None` = a tag or an `end_hi` value the encoder
    /// cannot produce, i.e. a corrupted or foreign wire image; the caller
    /// must fail closed rather than guess.
    pub fn decode(self) -> Option<SourceRecord> {
        if self.end_hi > 1 {
            return None;
        }
        let input_object = crate::memmap_source::InputObjectId::from_tag(self.input_object)?;
        let address_domain = crate::memmap_source::AddressDomain::from_tag(self.address_domain)?;
        let kind = match self.kind {
            Self::KIND_MEMORY => SourceKind::Memory,
            Self::KIND_FIRMWARE_PREFIX => SourceKind::FirmwarePrefix,
            Self::KIND_MRB_ENTRY => SourceKind::MrbEntry,
            Self::KIND_RESERVED_NO_MAP => SourceKind::ReservedNoMap,
            Self::KIND_RESERVED_ORDINARY => SourceKind::ReservedOrdinary,
            Self::KIND_RESERVED_REUSABLE => SourceKind::ReservedReusable,
            Self::KIND_UEFI_EXCLUDED => SourceKind::UefiExcluded,
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
                // `a` carries the descriptor / object index (origin payload);
                // the explicit local source id must agree with it, and only the
                // DTB-only words may stay zero.
                if self.cut != 0 || self.b != 0 || self.local != self.a {
                    return None;
                }
            }
        }
        // A record in a non-physical address domain is provenance only; it
        // must not be constructible as an ordinary source that a physical
        // projection could lean on.
        if address_domain != crate::memmap_source::AddressDomain::Physical
            && kind != SourceKind::UefiExcluded
        {
            return None;
        }
        Some(SourceRecord {
            id: self.id,
            kind,
            origin,
            range: SourceRange::new(
                self.start,
                self.end_lo as u128 | ((self.end_hi as u128) << 64),
            ),
            input_object,
            local_source: self.local,
            snapshot_id: self.snapshot,
            attribute: self.attribute,
            virtual_start: self.virtual_start,
            address_domain,
            raw_type: self.raw_type,
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
///
/// 8 = the `free_upper_idx` / `bootstrap_start` / `bootstrap_len` scalars
/// left the envelope (D01-F03): all producers wrote the "none" forms
/// (sentinel / 0 / 0), the kernel-side consumer of `free_upper_idx` was
/// the GET_KINFO report which fills 0 anyway, and the bootstrap reclaim
/// does not exist in the Rust port. Envelope shrinks 184 → 160 bytes.
///
/// 9 = the arena's semantic work partition (two `MemoryRegion` tables, 6144
/// B) was deleted (R18 D-2): the sender keeps its semantic tables in its own
/// `BootStorage` and the wire is the only representation in the arena. The
/// source and startup partitions move down by 6144 B and the arena shrinks
/// from nine pages to eight. This is a layout change, so a v8 receiver must
/// reject a v9 arena and vice versa — the comparison stays exact.
///
/// 10 = the three derived scalars (`user_sp`, `kern_stack_top`,
/// `syscall_entry`) and the startup-object partition left the envelope
/// (D-062): every producer wrote `USER_STACK_TOP` / `kern_virt_base +
/// kern_size` / `kern_virt_base`, and the only startup reader was a strict
/// gate now answered by the memory-map tables. Envelope shrinks 160 → 128
/// bytes and the startup arena partition (20 × 72 = 1440 B) is retired, so
/// a v9 receiver must reject a v10 arena and vice versa.
pub const BOOT_HANDOFF_V1_VERSION: u32 = 10;

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
    /// The RAM authority of the sending leg: 0 = boot DTB, 1 = UEFI memory
    /// map. The receiver refuses any other value; a declared authority with
    /// no records fails closed (no fallback to the other input).
    pub ram_authority: u32,
    pub arena_len: u64,
    // ── kernel scalars (fixed-width wire forms) ──
    pub kern_virt_base: WireVirtAddr,
    pub kern_phys_base: WirePhysAddr,
    pub kern_size: u64,
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

/// Envelope field offsets, derived from the struct itself (DESIGN-06).
///
/// Writer and reader both use these constants; a field rename/reorder changes
/// the `offset_of!` result, so neither side can keep a stale bare number.
pub mod envelope_offsets {
    use super::BootHandoffV1;
    use core::mem::offset_of;

    pub const MAGIC: usize = offset_of!(BootHandoffV1, magic);
    pub const VERSION: usize = offset_of!(BootHandoffV1, version);
    pub const SIZE: usize = offset_of!(BootHandoffV1, size);
    pub const RAM_AUTHORITY: usize = offset_of!(BootHandoffV1, ram_authority);
    pub const ARENA_LEN: usize = offset_of!(BootHandoffV1, arena_len);
    pub const KERN_VIRT_BASE: usize = offset_of!(BootHandoffV1, kern_virt_base);
    pub const KERN_PHYS_BASE: usize = offset_of!(BootHandoffV1, kern_phys_base);
    pub const KERN_SIZE: usize = offset_of!(BootHandoffV1, kern_size);
    pub const ROOT_PAGE: usize = offset_of!(BootHandoffV1, root_page);
    pub const BUMP_BASE: usize = offset_of!(BootHandoffV1, bump_base);
    pub const BUMP_NEXT: usize = offset_of!(BootHandoffV1, bump_next);
    pub const BUMP_END: usize = offset_of!(BootHandoffV1, bump_end);
    pub const MEMMAP_BYTE_LEN: usize = offset_of!(BootHandoffV1, memmap_byte_len);
    pub const RESERVED_BYTE_LEN: usize = offset_of!(BootHandoffV1, reserved_regions_byte_len);
    pub const MODULES_BYTE_LEN: usize = offset_of!(BootHandoffV1, boot_modules_byte_len);
    pub const PLATFORM_BYTE_LEN: usize = offset_of!(BootHandoffV1, platform_sources_byte_len);
    pub const PARAM_BYTE_LEN: usize = offset_of!(BootHandoffV1, param_buf_byte_len);
    pub const SOURCES_BYTE_LEN: usize = offset_of!(BootHandoffV1, sources_byte_len);
}

// ── arena layout (fixed offsets, compile-time arithmetic) ────────────────

/// Envelope budget (the envelope itself must fit; 256 B is the design budget).
pub const ENVELOPE_CAP: usize = 256;

/// Wire partition capacities, in elements (`128×16 + 256×16 + 12×32 + 4×16
/// + 1024 = 7616` bytes).
/// 128 is a conservative bound derived from `MAX_MAP_DESCRIPTORS = 256`
/// (conventional spans are at most every other descriptor); QEMU runs
/// observed 12–13 conventional spans. Kept in lockstep with the kernel
/// landing store by a compile-time equality in `os/kernel/src/globals.rs`.
pub const WIRE_MEMMAP_CAP: usize = 128;
pub const WIRE_RESERVED_CAP: usize = 256;
pub const WIRE_MODULE_CAP: usize = 12;
pub const WIRE_PLATFORM_CAP: usize = 4;
pub const WIRE_PARAM_CAP: usize = 1024;
/// Source records carried across the hop.
///
/// The bound is additive, not either/or: an aarch64 UEFI boot has both the
/// final `GetMemoryMap` descriptors (at most `MAX_MAP_DESCRIPTORS = 256`) and
/// the configuration-table DTB's own records (at most 40 = `/memory` 8 +
/// declared reservations 8 + MRB 16 + firmware prefixes 8). 256 + 40 = 296.
pub const WIRE_SOURCES_CAP: usize = 296;
/// Wire size of one [`WireSourceRecord`].
pub const WIRE_SOURCE_RECORD_SIZE: usize = 72;

/// Wire partition offsets from the arena base.
pub const WIRE_OFFSET: usize = ENVELOPE_CAP;
pub const WIRE_MEMMAP_OFFSET: usize = WIRE_OFFSET;
pub const WIRE_RESERVED_OFFSET: usize = WIRE_MEMMAP_OFFSET + WIRE_MEMMAP_CAP * 16;
pub const WIRE_MODULES_OFFSET: usize = WIRE_RESERVED_OFFSET + WIRE_RESERVED_CAP * 16;
pub const WIRE_PLATFORM_OFFSET: usize = WIRE_MODULES_OFFSET + WIRE_MODULE_CAP * 32;
pub const WIRE_PARAM_OFFSET: usize = WIRE_PLATFORM_OFFSET + WIRE_PLATFORM_CAP * 16;
pub const WIRE_END: usize = WIRE_PARAM_OFFSET + WIRE_PARAM_CAP;

/// Source-record partition: follows the wire payload directly. The sender's
/// semantic tables live in its own `BootStorage` (R18 D-2), so the arena
/// carries no second representation of the map.
pub const WIRE_SOURCES_OFFSET: usize = WIRE_END;
pub const WIRE_SOURCES_END: usize =
    WIRE_SOURCES_OFFSET + WIRE_SOURCES_CAP * WIRE_SOURCE_RECORD_SIZE;

/// Whole-arena capacity: 8 pages (32 KiB) — envelope + wire payload up to
/// `WIRE_END` and the 296-slot memory-source partition; 3584 bytes of design
/// spare remain after `WIRE_SOURCES_END` (32768 − 29184). The ninth page the
/// v8 layout needed (for the deleted semantic partition) is gone, and v10
/// returned the 1440-byte startup partition to that spare.
pub const ARENA_CAP: usize = 8 * 4096;

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
    assert!(size_of::<BootHandoffV1>() == 128);
    assert!(align_of::<BootHandoffV1>() == 8);
    assert!(size_of::<BootHandoffV1>() <= ENVELOPE_CAP);
    assert!(offset_of!(BootHandoffV1, magic) == 0);
    assert!(offset_of!(BootHandoffV1, version) == 4);
    assert!(offset_of!(BootHandoffV1, ram_authority) == 12);
    assert!(offset_of!(BootHandoffV1, size) == 8);
    assert!(offset_of!(BootHandoffV1, arena_len) == 16);
    assert!(offset_of!(BootHandoffV1, kern_virt_base) == 24);
    assert!(offset_of!(BootHandoffV1, kern_phys_base) == 32);
    assert!(offset_of!(BootHandoffV1, root_page) == 48);
    assert!(offset_of!(BootHandoffV1, bump_end) == 72);
    assert!(offset_of!(BootHandoffV1, memmap_byte_len) == 80);
    assert!(offset_of!(BootHandoffV1, param_buf_byte_len) == 112);
    assert!(offset_of!(BootHandoffV1, sources_byte_len) == 120);

    // Arena partitions: element-aligned, disjoint, exact, and fitting the cap.
    assert!(WIRE_MEMMAP_OFFSET.is_multiple_of(align_of::<WireMemoryRegion>()));
    assert!(WIRE_RESERVED_OFFSET.is_multiple_of(align_of::<WireMemoryRegion>()));
    assert!(WIRE_MODULES_OFFSET.is_multiple_of(align_of::<WireBootModule>()));
    assert!(WIRE_PLATFORM_OFFSET.is_multiple_of(align_of::<WirePlatformDescSource>()));
    assert!(WIRE_PARAM_OFFSET == 6848);
    assert!(WIRE_END == 7872);
    assert!(WIRE_SOURCES_OFFSET.is_multiple_of(align_of::<WireSourceRecord>()));
    assert!(WIRE_SOURCES_OFFSET == WIRE_END);
    assert!(WIRE_SOURCES_END == 29184);
    assert!(WIRE_SOURCES_END <= ARENA_CAP);
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
        assert_eq!(size_of::<BootHandoffV1>(), 128);
        assert_eq!(size_of::<WireSourceRecord>(), 72);
    }

    #[test]
    fn arena_partition_arithmetic_matches_the_design() {
        // 128×16 + 256×16 + 12×32 + 4×16 + 1024 = 7616 wire bytes.
        assert_eq!(WIRE_END - WIRE_OFFSET, 7616);
        // The wires are the only map representation in the arena (v9); the
        // source partition starts immediately after the payload.
        assert_eq!(WIRE_SOURCES_OFFSET, WIRE_END);
        assert_eq!(WIRE_SOURCES_END, 29184);
        // Envelope + wire + sources = 29184; the eight-page arena leaves
        // 3584 bytes of design spare (v10 returned the 1440-byte startup
        // partition to it).
        assert_eq!(ARENA_CAP - WIRE_SOURCES_END, 3584);
    }
}
