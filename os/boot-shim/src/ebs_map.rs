//! UEFI memory-map parser and the single classification policy.
//!
//! This module owns both halves of turning a firmware memory map into the OS
//! memory tables, and it is the **only** place those rules live. The pairing
//! loop that writes the handoff arena and the helper that builds the kernel
//! image's tables from the typed map view both end up in [`classify`] and
//! [`Plan::write_to`], so one firmware map can never be classified one way on
//! the way into the kernel image and another way on the way into the handoff.
//! Duplicating partition rules is a correctness defect rather than a style
//! problem: the copy nobody touches gains a check or a diagnostic and the two
//! silently disagree about the same machine.
//!
//! # Shape of the data
//!
//! [`MapStorage::parse_raw_map`] is the single parser. It bounds-checks every
//! field it reads and produces [`DescriptorRef`] values that carry the raw
//! `MemoryType` **and** the raw `Attribute` word: a policy that decides whether
//! a range may be mapped write-back cannot work against a parser that threw the
//! attribute away.
//!
//! Descriptors land in caller-provided storage, and one traversal feeds two
//! passes, which is why the parser and the policy take a slice rather than an
//! iterator: classification has to look at the whole map before it writes any of
//! it, so that a capacity mistake fails the boot at the edge instead of leaving
//! a half-written table behind.
//!
//! # The five-step adjudication order
//!
//! [`adjudicate`] answers one question per descriptor — where does this range
//! go? The step order is fixed, because which check a caller happened to write
//! first must not decide the outcome of a Type/Attribute combination.
//!
//! 1. **Unclassifiable input fails closed.** A `MemoryType` above the baseline's
//!    last known value (which covers the reserved numeric band, the OEM band and
//!    the OS-loader band), an ISA attribute present in any form, or an attribute
//!    bit the baseline does not define: each is a classification error. A range
//!    we cannot name has no later step to fall through to.
//! 2. **Types this boot path refuses to run with.** `UNACCEPTED` memory faults if
//!    it is read before being accepted, and `PAL_CODE` belongs to a firmware
//!    runtime model this kernel does not have. Either one means the machine is
//!    outside what this path supports: stop at the edge, while the shim can still
//!    print, rather than booting on a map that quietly omitted them.
//! 3. **Mandatory type dispositions beat every attribute.** `RESERVED` and
//!    `UNUSABLE` are excluded; `MMIO` and `MMIO_PORT_SPACE` take the device
//!    mapping path. A type states what firmware says a range *is*; an attribute
//!    only hints how it *could* be used, so no attribute can turn "the OS must
//!    not touch this" into "mapped but not allocated". They do not enter the
//!    side list either — filing a type-based exclusion under an attribute
//!    reason would misreport why the range is absent. Their `Attribute` words
//!    are **not** recorded today (the type alone sends them to the device
//!    path); a per-descriptor provenance layer must add that when it lands.
//! 4. **Candidate types meet the three independent attributes.** For the types
//!    that could otherwise reach an ordinary table, any of `RUNTIME`, `NV` or
//!    `SP` forfeits ordinary allocation *and* ordinary direct-map eligibility:
//!    this version calls no runtime services, does not use persistent memory, and
//!    has no consumer for specific-purpose memory. One uniform rule — any of the
//!    three ⇒ side list — is deliberate: no combination keeps an ordinary mapping
//!    anyway, so no row of the policy is left undecided.
//! 5. **Write-back admission, then the type table.** An ordinary write-back
//!    mapping requires the descriptor to assert `WRITE_BACK`. Without it the range
//!    goes to the side list whatever its type said; there is no back door from a
//!    failed admission into the ordinary reserved table. With it,
//!    `CONVENTIONAL` becomes allocatable, loader / boot-services / ACPI-reclaim
//!    data becomes reserved-but-mapped, and runtime-services, ACPI NVS and
//!    persistent memory keep their extent in the side list because nothing maps
//!    them yet.
//!
//! # Capacity, and why the second pass cannot fail
//!
//! [`classify`] is the pass that can fail: header shape, per-descriptor policy and
//! geometry, and the capacity of all three outputs are settled there before
//! anything is written. It returns a [`Plan`], whose only content is the count of
//! what fits where. The write pass consumes that plan and reports no error: the
//! dispositions it re-derives are a pure function of descriptors that have not
//! moved, and the counts were already proved against the capacities of the very
//! buffers the caller handed over. Infallibility is carried by the types — a plan
//! that does not fit cannot be constructed at all.
//!
//! # The three outputs
//!
//! Allocatable and reserved-but-mapped are the two tables the kernel is handed.
//! The third, the **side list**, is a *shim-internal diagnostic*: it records which
//! ranges policy kept out of both ordinary tables, with full extents, because a
//! count cannot later be turned back into ranges. It is not part of the handoff
//! protocol and never reaches the kernel; a kernel that eventually wants one of
//! those regions must be given it through an explicit `KernelInfo` extension, not
//! by reading this list.

use minix_boot::{
    HandoffV1WriteError, MemoryRegion, PhysicalRange, Serializer, SourceKind, SourceOrigin,
    SourceRecord, WIRE_MEMMAP_CAP, WIRE_RESERVED_CAP, WIRE_SOURCES_CAP,
};
use minix_types::PhysBytes;

// The record partition must hold the union of both inputs an aarch64 UEFI
// boot carries: the final memory-map descriptors and the configuration-table
// DTB's records. The bound is additive, not either/or.
const _: () = assert!(
    minix_boot::WIRE_SOURCES_CAP >= MAX_MAP_DESCRIPTORS + minix_boot::DTB_SOURCE_CAP,
    "source partition must fit UEFI descriptors plus DTB records"
);

/// The only supported `EFI_MEMORY_DESCRIPTOR` version.
pub const MEMORY_DESCRIPTOR_VERSION: u32 = 1;

/// `sizeof(EFI_MEMORY_DESCRIPTOR)` — the version-1 prefix; firmware may append a
/// tail, which is why the walker uses `descriptor_size` as the stride.
pub const MEMORY_DESCRIPTOR_SIZE: usize = 40;

/// UEFI page size.
pub const PAGE_SIZE: u64 = 4096;

/// Field offsets inside the version-1 descriptor prefix.
const OFF_TYPE: usize = 0;
const OFF_PHYSICAL_START: usize = 8;
const OFF_VIRTUAL_START: usize = 16;
const OFF_NUMBER_OF_PAGES: usize = 24;
const OFF_ATTRIBUTE: usize = 32;

/// Largest firmware map the shim's storage can hold. Real maps stay far below
/// this; a bigger one is refused at the edge rather than truncated.
pub const MAX_MAP_DESCRIPTORS: usize = 256;

/// Room for the ranges that policy keeps out of both ordinary tables.
pub const SIDE_CAP: usize = 64;

/// `EFI_MEMORY_DESCRIPTOR` types the policy knows, per the UEFI 2.10 baseline.
///
/// Values above [`ty::LAST_KNOWN`] are bands whose meaning firmware never
/// declared: the reserved numeric band, the OEM band `0x7000_0000..=0x7fff_ffff`,
/// and the OS-loader band `0x8000_0000..=0xffff_ffff`.
pub mod ty {
    pub const RESERVED: u32 = 0;
    pub const LOADER_CODE: u32 = 1;
    pub const LOADER_DATA: u32 = 2;
    pub const BOOT_SERVICES_CODE: u32 = 3;
    pub const BOOT_SERVICES_DATA: u32 = 4;
    pub const RUNTIME_SERVICES_CODE: u32 = 5;
    pub const RUNTIME_SERVICES_DATA: u32 = 6;
    /// `EfiConventionalMemory` — the only free RAM.
    pub const CONVENTIONAL: u32 = 7;
    pub const UNUSABLE: u32 = 8;
    pub const ACPI_RECLAIM: u32 = 9;
    pub const ACPI_NVS: u32 = 10;
    pub const MMIO: u32 = 11;
    pub const MMIO_PORT_SPACE: u32 = 12;
    pub const PAL_CODE: u32 = 13;
    pub const PERSISTENT: u32 = 14;
    pub const UNACCEPTED: u32 = 15;
    /// Highest type value the baseline defines.
    pub const LAST_KNOWN: u32 = UNACCEPTED;
    /// First value of the band firmware may use for its own purposes.
    pub const OEM_BAND: u32 = 0x7000_0000;
    /// First value of the band an OS loader may use.
    pub const OS_LOADER_BAND: u32 = 0x8000_0000;
}

/// `EFI_MEMORY_DESCRIPTOR.Attribute` bits, on the UEFI 2.10 baseline.
///
/// The baseline is an explicit commitment rather than an accident: this policy
/// handles exactly the bits 2.10 defines, so a bit added by a later revision
/// arrives as an unknown attribute and fails closed. Widening the baseline stays
/// a deliberate decision instead of a silent widening.
pub mod attr {
    pub const UNCACHEABLE: u64 = 0x1;
    pub const WRITE_COMBINE: u64 = 0x2;
    pub const WRITE_THROUGH: u64 = 0x4;
    /// The admission requirement for an ordinary write-back mapping.
    pub const WRITE_BACK: u64 = 0x8;
    pub const UNCACHABLE_EXPORTED: u64 = 0x10;
    /// Listed as a physical-protection bit in older revisions; in current usage
    /// it names a write-protected *cacheability* policy, so it is treated as a
    /// cache capability and never as a permission.
    pub const WRITE_PROTECT: u64 = 0x1000;
    pub const READ_PROTECT: u64 = 0x2000;
    pub const EXECUTE_PROTECT: u64 = 0x4000;
    pub const NON_VOLATILE: u64 = 0x8000;
    pub const MORE_RELIABLE: u64 = 0x1_0000;
    pub const READ_ONLY: u64 = 0x2_0000;
    pub const SPECIAL_PURPOSE: u64 = 0x4_0000;
    pub const CPU_CRYPTO: u64 = 0x8_0000;
    pub const RUNTIME: u64 = 0x8000_0000_0000_0000;
    pub const ISA_VALID: u64 = 0x4000_0000_0000_0000;
    pub const ISA_MASK: u64 = 0x0FFF_F000_0000_0000;

    /// Every bit the baseline defines, whether this policy acts on it, records
    /// it, or rejects it. Anything outside this set is unknown.
    pub const KNOWN: u64 = UNCACHEABLE
        | WRITE_COMBINE
        | WRITE_THROUGH
        | WRITE_BACK
        | UNCACHABLE_EXPORTED
        | WRITE_PROTECT
        | READ_PROTECT
        | EXECUTE_PROTECT
        | NON_VOLATILE
        | MORE_RELIABLE
        | READ_ONLY
        | SPECIAL_PURPOSE
        | CPU_CRYPTO
        | RUNTIME
        | ISA_VALID
        | ISA_MASK;

    /// The three attributes independent of `MemoryType`: any one of them takes a
    /// range out of ordinary allocation and ordinary mapping. `NON_VOLATILE`
    /// and `SPECIAL_PURPOSE` are unreachable through the classifier today (see
    /// `AMBIGUOUS` and `PROFILE_ONLY`); only `RUNTIME` remains live.
    pub const TYPE_INDEPENDENT: u64 = RUNTIME | NON_VOLATILE | SPECIAL_PURPOSE;

    /// Bits that exist only in one of the two firmware encodings. The published
    /// 2.10/2.11 tables do not carry `RO`/`SP`/`CPU_CRYPTO` at these positions;
    /// the specification repository and EDK2 do. Interpreting them without a
    /// pinned firmware identity would silently adopt one branch, so the bits
    /// fail closed until a profile names the firmware actually booting.
    pub const PROFILE_ONLY: u64 = READ_ONLY | SPECIAL_PURPOSE | CPU_CRYPTO;

    /// Bits whose meaning is not the same in every official text: the published
    /// 2.10/2.11 PDF tables and the specification repository/EDK2 branch assign
    /// different names across `0x1000..=0x1_0000` (for example `0x4000` is
    /// `EFI_MEMORY_SP` in one text and `EFI_MEMORY_XP` in the other; `0x8000` is
    /// `CPU_CRYPTO` vs `NV`; `0x1_0000` is `HOT_PLUGGABLE` vs `MORE_RELIABLE`).
    /// Every reading is unsafe for this policy: the PDF one can hide
    /// special-purpose or non-volatile memory behind a "protection" bit, and
    /// the repository one can turn a hot-pluggable report into a reliability
    /// hint. The project pins no firmware profile, so the whole contested field
    /// fails closed instead of being sorted under either name.
    pub const AMBIGUOUS: u64 =
        WRITE_PROTECT | READ_PROTECT | EXECUTE_PROTECT | NON_VOLATILE | MORE_RELIABLE;

    /// UEFI 2.11 defines this bit as `EFI_MEMORY_HOT_PLUGGABLE`. The baseline
    /// this project tracks does not include it, so it exercises the
    /// unknown-bit path: memory the kernel cannot unplug must not reach the
    /// PMM, and refusing the descriptor is the fail-closed outcome.
    pub const UNKNOWN_FOR_TESTING: u64 = 1 << 20;
}

/// One descriptor exactly as the policy needs it: raw type, raw geometry, raw
/// attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescriptorRef {
    pub raw_type: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

impl DescriptorRef {
    /// An empty slot, for fixed-size storage the parser fills in.
    pub const EMPTY: DescriptorRef = DescriptorRef {
        raw_type: 0,
        physical_start: 0,
        virtual_start: 0,
        number_of_pages: 0,
        attribute: 0,
    };

    /// Byte length of the range. Only reached for descriptors whose geometry
    /// [`checked_len`] already proved, so this multiply cannot overflow.
    pub fn byte_len(&self) -> u64 {
        self.number_of_pages * PAGE_SIZE
    }

    /// The same descriptor as an emitted region.
    pub fn region(&self, len: u64) -> MemoryRegion {
        MemoryRegion {
            base: PhysBytes(self.physical_start),
            len: len as usize,
        }
    }
}

/// Where a validated descriptor's range goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Free RAM, handed to the physical memory manager.
    Allocate,
    /// Reached through the ordinary direct map, never reallocated.
    ReservedMapped,
    /// Kept out of both ordinary tables; full extent recorded in the side list.
    Side,
    /// Firmware says the OS must not use it: in no table, side list included.
    Excluded,
    /// Device aperture, which arrives through the device-window path.
    DevicePath,
}

/// Why a descriptor was refused. Every variant stops the whole map: the boot path
/// never falls back to a partially classified memory map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyError {
    /// Step 1: a type the baseline does not define.
    UnknownType,
    /// Step 1: an attribute bit outside the baseline.
    UnknownAttribute,
    /// Step 1: a bit whose meaning depends on a firmware encoding profile this
    /// project has not pinned (`RO`/`SP`/`CPU_CRYPTO` in the repository branch).
    ProfileAttribute,
    /// Step 1: a bit whose meaning differs between official texts
    /// (`0x0001_0000` is `HOT_PLUGGABLE` in the UEFI 2.11 PDF and
    /// `MORE_RELIABLE` in the specification repository and EDK2). Either
    /// reading names something this policy cannot honour, so the descriptor is
    /// refused instead of being sorted under one of the two names.
    AmbiguousAttribute,
    /// Step 1: an ISA attribute present, whether or not its validity bit is set.
    IsaAttributes,
    /// Step 2: memory this boot path cannot honour.
    RefuseToBoot,
    /// A start address that is not page-aligned.
    UnalignedStart,
    /// A zero page count describes no range.
    ZeroPageCount,
    /// A page count whose byte length overflows 64 bits.
    LengthOverflow,
    /// A range whose end overflows the address space.
    RangeOverflow,
    /// A range ending above this architecture's page-table ceiling.
    OutOfPhysRange,
}

/// The descriptor that failed admission, captured for the halt report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescriptorRejection {
    pub index: u16,
    pub raw_type: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
    pub failed: PolicyError,
}

/// Why a whole map was refused, before or during classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    UnsupportedVersion,
    DescriptorTooSmall,
    DescriptorSizeNotAligned,
    MapSizeBeyondBuffer,
    MapSizeNotMultiple,
    /// A descriptor prefix running past the reported map size.
    DescriptorBeyondMap,
    /// More descriptors than the caller's storage holds.
    TooManyDescriptors,
    Policy(PolicyError),
    /// A descriptor failed admission; the record names the descriptor and the
    /// failed requirement, so the halt report is actionable.
    RejectedDescriptor(DescriptorRejection),
    /// A produced list needs more room than the caller gave it.
    Capacity(HandoffV1WriteError),
    /// More side-list entries than the diagnostic buffer holds.
    TooManySideEntries,
}

impl From<HandoffV1WriteError> for MapError {
    fn from(err: HandoffV1WriteError) -> Self {
        MapError::Capacity(err)
    }
}

impl From<PolicyError> for MapError {
    fn from(err: PolicyError) -> Self {
        MapError::Policy(err)
    }
}

fn read_u32(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap())
}

/// Validate the map header firmware reported, or explain why the map cannot be
/// walked at all. Returns the descriptor count.
fn validate_header(
    buffer_len: usize,
    map_size: usize,
    descriptor_size: usize,
    descriptor_version: u32,
) -> Result<usize, MapError> {
    if descriptor_version != MEMORY_DESCRIPTOR_VERSION {
        return Err(MapError::UnsupportedVersion);
    }
    if descriptor_size < MEMORY_DESCRIPTOR_SIZE {
        return Err(MapError::DescriptorTooSmall);
    }
    // Every later descriptor must start where the previous one ended; an odd
    // stride would put a 64-bit field at an unaligned offset.
    if !descriptor_size.is_multiple_of(8) {
        return Err(MapError::DescriptorSizeNotAligned);
    }
    if map_size > buffer_len {
        return Err(MapError::MapSizeBeyondBuffer);
    }
    if !map_size.is_multiple_of(descriptor_size) {
        return Err(MapError::MapSizeNotMultiple);
    }
    Ok(map_size / descriptor_size)
}

/// Caller-owned storage for one map: the parsed descriptors, and the side list
/// the shim keeps for itself.
///
/// The pairing loop must not allocate between reading a map and exiting boot
/// services — an allocation there is precisely what can change the map being
/// classified — so both buffers are reserved once and reused. They also carry
/// the capacities [`classify`] is proved against.
pub struct MapStorage {
    descs: [DescriptorRef; MAX_MAP_DESCRIPTORS],
    side: [MemoryRegion; SIDE_CAP],
    len: usize,
    side_len: usize,
    snapshot_id: u64,
}

impl MapStorage {
    pub const fn new() -> Self {
        Self {
            descs: [DescriptorRef::EMPTY; MAX_MAP_DESCRIPTORS],
            side: [MemoryRegion {
                base: PhysBytes(0),
                len: 0,
            }; SIDE_CAP],
            len: 0,
            side_len: 0,
            snapshot_id: 0,
        }
    }

    /// Parse a raw `GetMemoryMap` buffer — the only parser in the project.
    ///
    /// `descriptor_size` is firmware's stride and may exceed the version-1
    /// prefix, which is why the walker reads fields at fixed offsets instead of
    /// casting to a typed descriptor: a cast would be misaligned and would
    /// over-claim a tail whose layout was never negotiated.
    pub fn parse_raw_map(
        &mut self,
        map: &[u8],
        map_size: usize,
        descriptor_size: usize,
        descriptor_version: u32,
        snapshot_id: u64,
    ) -> Result<(), MapError> {
        let entries = validate_header(map.len(), map_size, descriptor_size, descriptor_version)?;
        if entries > self.descs.len() {
            return Err(MapError::TooManyDescriptors);
        }
        self.side_len = 0;
        self.snapshot_id = snapshot_id;
        for (i, slot) in self.descs.iter_mut().enumerate().take(entries) {
            let off = i * descriptor_size;
            let prefix = map
                .get(off..off + MEMORY_DESCRIPTOR_SIZE)
                .ok_or(MapError::DescriptorBeyondMap)?;
            *slot = DescriptorRef {
                raw_type: read_u32(prefix, OFF_TYPE),
                physical_start: read_u64(prefix, OFF_PHYSICAL_START),
                virtual_start: read_u64(prefix, OFF_VIRTUAL_START),
                number_of_pages: read_u64(prefix, OFF_NUMBER_OF_PAGES),
                attribute: read_u64(prefix, OFF_ATTRIBUTE),
            };
        }
        self.len = entries;
        Ok(())
    }

    /// The parsed map.
    pub fn descriptors(&self) -> &[DescriptorRef] {
        &self.descs[..self.len]
    }

    /// Capacity of this storage's side-list buffer.
    pub fn side_capacity(&self) -> usize {
        self.side.len()
    }

    /// The side list: the ranges classification kept out of both ordinary
    /// tables, in firmware order, as written by [`Plan::commit`].
    pub fn side(&self) -> &[MemoryRegion] {
        &self.side[..self.side_len]
    }

    /// Classify the parsed map into the handoff tables, keeping the side list
    /// here for the caller to read afterwards.
    ///
    /// `serializer.reset_map_partitions()` is the caller's job: the pairing loop
    /// reclassifies against a fresh MapKey on each attempt and must clear the
    /// previous attempt's entries first.
    pub fn classify_into_handoff(
        &mut self,
        pa_limit: u64,
        serializer: &mut Serializer<'_>,
    ) -> Result<Classification, MapError> {
        // Borrowing the descriptor list and the side list as separate fields is
        // what lets the read side and the write side coexist across the commit.
        let descs = &self.descs[..self.len];
        let side = &mut self.side[..];
        let plan = classify(descs, pa_limit, side.len())?;
        let counts = plan.commit(descs, pa_limit, serializer, side, self.snapshot_id);
        self.side_len = counts.side;
        Ok(counts)
    }
}

/// Prove a range is representable and return its byte length.
///
/// `pa_limit` is this architecture's *page-table representability* ceiling — how
/// far our page tables can reach, an implementation fact, and never a claim about
/// how much memory firmware installed. Whether the kernel actually built a direct
/// mapping across an accepted range is a separate contract, checked by the mapping
/// plan on the kernel side; nothing here can satisfy it.
///
/// The start address must be page-aligned because the `GetMemoryMap` contract
/// requires it, and an unaligned range cannot become page-table entries without
/// silently dropping the head of the region.
pub fn checked_len(desc: &DescriptorRef, pa_limit: u64) -> Result<u64, PolicyError> {
    if !desc.physical_start.is_multiple_of(PAGE_SIZE) {
        return Err(PolicyError::UnalignedStart);
    }
    if desc.number_of_pages == 0 {
        return Err(PolicyError::ZeroPageCount);
    }
    let len = desc
        .number_of_pages
        .checked_mul(PAGE_SIZE)
        .ok_or(PolicyError::LengthOverflow)?;
    let end = desc
        .physical_start
        .checked_add(len)
        .ok_or(PolicyError::RangeOverflow)?;
    if end > pa_limit {
        return Err(PolicyError::OutOfPhysRange);
    }
    Ok(len)
}

/// Run the five-step order on one descriptor.
///
/// Returns the disposition and, for the three dispositions that produce a range,
/// its byte length. Ranges that produce none (`Excluded`, `DevicePath`) skip the
/// geometry gate: firmware legitimately describes apertures and holes above what
/// this kernel can address, and failing the boot over the shape of a range we
/// deliberately do not emit would turn a harmless hole into an outage.
pub fn adjudicate(desc: &DescriptorRef, pa_limit: u64) -> Result<(Disposition, u64), PolicyError> {
    // Step 1 — an object we cannot name gets no later step.
    if desc.raw_type > ty::LAST_KNOWN {
        return Err(PolicyError::UnknownType);
    }
    let a = desc.attribute;
    if a & attr::AMBIGUOUS != 0 {
        return Err(PolicyError::AmbiguousAttribute);
    }
    if a & attr::PROFILE_ONLY != 0 {
        return Err(PolicyError::ProfileAttribute);
    }
    if a & !attr::KNOWN != 0 {
        return Err(PolicyError::UnknownAttribute);
    }
    // Present in any form: none of this project's architectures interprets an
    // ISA mask, and a mask whose validity bit is clear is not well-formed either.
    if a & (attr::ISA_VALID | attr::ISA_MASK) != 0 {
        return Err(PolicyError::IsaAttributes);
    }

    // Step 2 — stronger than every later step, and unmoved by attributes.
    if matches!(desc.raw_type, ty::UNACCEPTED | ty::PAL_CODE) {
        return Err(PolicyError::RefuseToBoot);
    }

    // Step 3 — the type's own mandatory disposition wins over any attribute.
    if matches!(desc.raw_type, ty::RESERVED | ty::UNUSABLE) {
        return Ok((Disposition::Excluded, 0));
    }
    if matches!(desc.raw_type, ty::MMIO | ty::MMIO_PORT_SPACE) {
        return Ok((Disposition::DevicePath, 0));
    }

    let len = checked_len(desc, pa_limit)?;

    // Step 4 — an independent attribute forfeits both ordinary roles.
    if a & attr::TYPE_INDEPENDENT != 0 {
        return Ok((Disposition::Side, len));
    }

    // Step 5 — write-back admission, then the type table.
    if a & attr::WRITE_BACK == 0 {
        return Ok((Disposition::Side, len));
    }
    match desc.raw_type {
        ty::CONVENTIONAL => Ok((Disposition::Allocate, len)),
        ty::LOADER_CODE
        | ty::LOADER_DATA
        | ty::BOOT_SERVICES_CODE
        | ty::BOOT_SERVICES_DATA
        | ty::ACPI_RECLAIM => Ok((Disposition::ReservedMapped, len)),
        _ => Ok((Disposition::Side, len)),
    }
}

/// The entry counts of one classification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Classification {
    pub conventional: usize,
    pub reserved: usize,
    pub side: usize,
}

/// A proved plan: how many entries each output will receive.
///
/// Only [`classify`] builds one, and only after every descriptor adjudicated and
/// every capacity checked — which is what lets the write pass have no error path.
#[derive(Debug, Clone, Copy)]
pub struct Plan {
    counts: Classification,
}

/// Counting pass: adjudicate every descriptor and prove all three outputs fit.
///
/// This is the pass that can fail. The two handoff capacities and `side_cap` are
/// checked here, so a map that cannot be expressed is rejected while nothing has
/// been written yet.
pub fn classify(descs: &[DescriptorRef], pa_limit: u64, side_cap: usize) -> Result<Plan, MapError> {
    let mut counts = Classification::default();
    for (index, desc) in descs.iter().enumerate() {
        let (disposition, _) = adjudicate(desc, pa_limit).map_err(|failed| {
            MapError::RejectedDescriptor(DescriptorRejection {
                index: index as u16,
                raw_type: desc.raw_type,
                physical_start: desc.physical_start,
                virtual_start: desc.virtual_start,
                number_of_pages: desc.number_of_pages,
                attribute: desc.attribute,
                failed,
            })
        })?;
        match disposition {
            Disposition::Allocate => counts.conventional += 1,
            Disposition::ReservedMapped => counts.reserved += 1,
            Disposition::Side => counts.side += 1,
            Disposition::Excluded | Disposition::DevicePath => {}
        }
    }
    if counts.conventional > WIRE_MEMMAP_CAP {
        return Err(MapError::Capacity(HandoffV1WriteError::TooManyMemmap));
    }
    if counts.reserved > WIRE_RESERVED_CAP {
        return Err(MapError::Capacity(
            HandoffV1WriteError::TooManyReservedRegions,
        ));
    }
    // One source record per structurally valid descriptor (every disposition
    // gets an identity, including the ones outside both ordinary tables). The
    // descriptor count is bounded by MAX_MAP_DESCRIPTORS, so this can only
    // trip on a contract change, never on a real map.
    if descs.len() > WIRE_SOURCES_CAP {
        return Err(MapError::Capacity(
            HandoffV1WriteError::TooManySourceRecords,
        ));
    }
    if counts.side > side_cap {
        return Err(MapError::TooManySideEntries);
    }
    Ok(Plan { counts })
}

impl Plan {
    /// The counts this plan will produce.
    pub fn counts(&self) -> Classification {
        self.counts
    }

    /// Hand every emitted range to `emit`, in firmware order, exactly once per
    /// descriptor that produces one.
    ///
    /// This is the single routing step shared by both sinks: the handoff writer
    /// and the region-table writer differ only in what they do with a range, never
    /// in which ranges they receive.
    pub fn write_to<F>(&self, descs: &[DescriptorRef], pa_limit: u64, mut emit: F) -> Classification
    where
        F: FnMut(usize, Disposition, &MemoryRegion),
    {
        let mut counts = Classification::default();
        for (index, desc) in descs.iter().enumerate() {
            // Re-deriving is a pure function of a descriptor that has not moved,
            // so these dispositions are the ones `classify` already counted.
            let Ok((disposition, len)) = adjudicate(desc, pa_limit) else {
                continue;
            };
            if len == 0 {
                continue;
            }
            let region = desc.region(len);
            match disposition {
                Disposition::Allocate => counts.conventional += 1,
                Disposition::ReservedMapped => counts.reserved += 1,
                Disposition::Side => counts.side += 1,
                Disposition::Excluded | Disposition::DevicePath => {}
            }
            emit(index, disposition, &region);
        }
        assert_eq!(counts, self.counts, "a plan must be written exactly once");
        counts
    }

    /// Write the planned map into the handoff tables and the side list.
    ///
    /// Infallible by construction: the counts were proved against the very
    /// capacities these sinks enforce, and `free`-shaped failures cannot happen
    /// because a plan that does not fit could not be created. Slots past the count
    /// are left untouched, so nothing half-written can be read back as real.
    pub fn commit(
        &self,
        descs: &[DescriptorRef],
        pa_limit: u64,
        serializer: &mut Serializer<'_>,
        side: &mut [MemoryRegion],
        snapshot_id: u64,
    ) -> Classification {
        let mut side_written = 0usize;
        let counts = self.write_to(descs, pa_limit, |index, disposition, region| {
            match disposition {
                Disposition::Allocate => {
                    let admitted = serializer.push_memmap(*region).is_ok();
                    assert!(
                        admitted,
                        "preflight proved the memmap partition holds {} entries",
                        self.counts.conventional
                    );
                }
                Disposition::ReservedMapped => {
                    let admitted = serializer.push_reserved(*region).is_ok();
                    assert!(
                        admitted,
                        "preflight proved the reserved partition holds {} entries",
                        self.counts.reserved
                    );
                }
                Disposition::Side => {
                    side[side_written] = *region;
                    side_written += 1;
                }
                Disposition::Excluded | Disposition::DevicePath => return,
            }
            // Provenance: every structurally valid descriptor that produces a
            // range gets a record, whether or not it lands in an ordinary
            // table. `kind` is this project's role; `raw_type` keeps the
            // firmware's own type so a `Side` descriptor does not lose its
            // identity.
            let kind = match disposition {
                Disposition::Allocate => SourceKind::Memory,
                Disposition::ReservedMapped => SourceKind::ReservedOrdinary,
                _ => SourceKind::UefiExcluded,
            };
            let record = SourceRecord {
                id: index as u16,
                kind,
                origin: SourceOrigin::UefiDescriptor {
                    index: index as u16,
                },
                range: PhysicalRange::new(
                    region.base.0,
                    region.base.0 as u128 + region.len as u128,
                ),
                input_object: minix_boot::InputObjectId::UefiMemoryMap,
                address_domain: minix_boot::memmap_source::AddressDomain::Physical,
                local_source: index as u16,
                snapshot_id,
                attribute: descs[index].attribute,
                virtual_start: descs[index].virtual_start,
                raw_type: descs[index].raw_type,
            };
            let admitted = serializer.push_source(&record).is_ok();
            assert!(
                admitted,
                "preflight proved the source partition holds {} records",
                self.counts.conventional + self.counts.reserved
            );
        });
        // Descriptors the classifier emits no range for (`Excluded`,
        // `DevicePath`) still carry provenance: their raw extent and firmware
        // type enter the same record partition.
        for (index, desc) in descs.iter().enumerate() {
            let Ok((disposition, len)) = adjudicate(desc, pa_limit) else {
                continue;
            };
            if len != 0
                || !matches!(disposition, Disposition::Excluded | Disposition::DevicePath)
            {
                continue;
            }
            let end = desc
                .physical_start
                .saturating_add(desc.number_of_pages.saturating_mul(PAGE_SIZE));
            let record = SourceRecord {
                id: index as u16,
                raw_type: desc.raw_type,
                kind: SourceKind::UefiExcluded,
                origin: SourceOrigin::UefiDescriptor {
                    index: index as u16,
                },
                range: PhysicalRange::new(desc.physical_start, end as u128),
                input_object: minix_boot::InputObjectId::UefiMemoryMap,
                // Type 12's start/count live in I/O-port space; the record
                // keeps the raw fact but must not claim a physical address.
                address_domain: if desc.raw_type == ty::MMIO_PORT_SPACE {
                    minix_boot::memmap_source::AddressDomain::UefiMmioPortSpace
                } else {
                    minix_boot::memmap_source::AddressDomain::Physical
                },
                local_source: index as u16,
                snapshot_id,
                attribute: desc.attribute,
                virtual_start: desc.virtual_start,
            };
            let admitted = serializer.push_source(&record).is_ok();
            assert!(
                admitted,
                "preflight proved the source partition holds every descriptor"
            );
        }
        serializer.finish_memmap();
        serializer.finish_reserved();
        serializer.finish_sources();
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PA_LIMIT: u64 = 1 << 48;
    /// The attribute a usable descriptor normally carries.
    const WB: u64 = attr::WRITE_BACK;

    /// One descriptor of `desc_size` bytes (40 + zero padding).
    fn push_desc(
        buf: &mut [u8],
        index: usize,
        desc_size: usize,
        ty: u32,
        phys: u64,
        pages: u64,
        attribute: u64,
    ) {
        let off = index * desc_size;
        buf[off..off + 4].copy_from_slice(&ty.to_le_bytes());
        buf[off + 4..off + 8].copy_from_slice(&0u32.to_le_bytes());
        buf[off + 8..off + 16].copy_from_slice(&phys.to_le_bytes());
        buf[off + 16..off + 24].copy_from_slice(&0u64.to_le_bytes());
        buf[off + 24..off + 32].copy_from_slice(&pages.to_le_bytes());
        buf[off + 32..off + 40].copy_from_slice(&attribute.to_le_bytes());
        for byte in &mut buf[off + 40..off + desc_size] {
            *byte = 0;
        }
    }

    fn get_u64(arena: &[u8], off: usize) -> u64 {
        u64::from_le_bytes(arena[off..off + 8].try_into().unwrap())
    }

    fn desc(t: u32, attribute: u64) -> DescriptorRef {
        DescriptorRef {
            raw_type: t,
            physical_start: 0x10_0000,
            virtual_start: 0,
            number_of_pages: 1,
            attribute,
        }
    }

    /// The disposition of one ordinary page-sized range.
    fn disp(t: u32, attribute: u64) -> Result<Disposition, PolicyError> {
        adjudicate(&desc(t, attribute), PA_LIMIT).map(|(d, _)| d)
    }

    fn classify_err(map: &[u8], map_size: usize, desc_size: usize, version: u32) -> MapError {
        let mut storage = MapStorage::new();
        let mut arena = [0u8; minix_boot::ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        storage
            .parse_raw_map(map, map_size, desc_size, version, 0)
            .and_then(|()| storage.classify_into_handoff(PA_LIMIT, &mut s))
            .unwrap_err()
    }

    #[test]
    fn conventional_with_write_back_is_allocatable() {
        assert_eq!(disp(ty::CONVENTIONAL, WB), Ok(Disposition::Allocate));
    }

    #[test]
    fn candidate_types_with_write_back_are_reserved_but_mapped() {
        for t in [
            ty::LOADER_CODE,
            ty::LOADER_DATA,
            ty::BOOT_SERVICES_CODE,
            ty::BOOT_SERVICES_DATA,
            ty::ACPI_RECLAIM,
        ] {
            assert_eq!(disp(t, WB), Ok(Disposition::ReservedMapped), "type {t}");
        }
    }

    #[test]
    fn candidate_types_without_write_back_lose_the_ordinary_mapping() {
        // The admission gate has no back door: every type that would otherwise
        // reach an ordinary table lands in the side list when WB is absent.
        for t in [
            ty::CONVENTIONAL,
            ty::LOADER_CODE,
            ty::LOADER_DATA,
            ty::BOOT_SERVICES_CODE,
            ty::BOOT_SERVICES_DATA,
            ty::ACPI_RECLAIM,
        ] {
            assert_eq!(disp(t, 0), Ok(Disposition::Side), "type {t}");
        }
    }

    #[test]
    fn runtime_or_nv_or_specific_purpose_strips_both_ordinary_roles() {
        for t in [
            ty::CONVENTIONAL,
            ty::LOADER_CODE,
            ty::LOADER_DATA,
            ty::BOOT_SERVICES_CODE,
            ty::BOOT_SERVICES_DATA,
            ty::ACPI_RECLAIM,
        ] {
            for flag in [attr::RUNTIME] {
                assert_eq!(
                    disp(t, WB | flag),
                    Ok(Disposition::Side),
                    "type {t} + {flag:#x}"
                );
            }
        }
    }

    #[test]
    fn conventional_with_runtime_is_not_allocatable() {
        // A range firmware marks as needing a runtime virtual address map cannot
        // be handed to the memory manager: it would be reallocated out from under
        // the obligation to map it.
        assert_eq!(
            disp(ty::CONVENTIONAL, WB | attr::RUNTIME),
            Ok(Disposition::Side)
        );
    }

    #[test]
    fn types_that_earn_no_ordinary_mapping_go_to_the_side_list() {
        for t in [
            ty::RUNTIME_SERVICES_CODE,
            ty::RUNTIME_SERVICES_DATA,
            ty::ACPI_NVS,
            ty::PERSISTENT,
        ] {
            assert_eq!(disp(t, WB), Ok(Disposition::Side), "type {t}");
        }
    }

    #[test]
    fn excluded_and_device_types_stay_out_of_every_list() {
        for t in [ty::RESERVED, ty::UNUSABLE] {
            assert_eq!(disp(t, WB), Ok(Disposition::Excluded), "type {t}");
        }
        for t in [ty::MMIO, ty::MMIO_PORT_SPACE] {
            assert_eq!(disp(t, WB), Ok(Disposition::DevicePath), "type {t}");
        }
    }

    #[test]
    fn record_only_attributes_never_change_the_disposition() {
        // Other cache capabilities, and the physical-protection bits, must not
        // reclassify a range: only WB takes a decision.
        for extra in [
            attr::UNCACHEABLE,
            attr::WRITE_COMBINE,
            attr::WRITE_THROUGH,
            attr::UNCACHABLE_EXPORTED,
        ] {
            assert_eq!(
                disp(ty::CONVENTIONAL, WB | extra),
                Ok(Disposition::Allocate),
                "extra {extra:#x}"
            );
            assert_eq!(
                disp(ty::LOADER_DATA, WB | extra),
                Ok(Disposition::ReservedMapped),
                "extra {extra:#x}"
            );
        }
    }

    #[test]
    fn unaccepted_and_pal_code_stop_the_boot() {
        assert_eq!(disp(ty::UNACCEPTED, WB), Err(PolicyError::RefuseToBoot));
        assert_eq!(disp(ty::PAL_CODE, WB), Err(PolicyError::RefuseToBoot));
    }

    #[test]
    fn unknown_types_are_a_classification_error() {
        // Just past the baseline, inside the reserved numeric band, and both
        // vendor bands.
        for t in [
            ty::LAST_KNOWN + 1,
            0x10,
            0x1_0000,
            ty::OEM_BAND,
            ty::OS_LOADER_BAND,
            u32::MAX,
        ] {
            assert_eq!(disp(t, WB), Err(PolicyError::UnknownType), "type {t:#x}");
        }
    }

    #[test]
    fn hot_pluggable_bit_fails_closed_as_unknown() {
        // UEFI 2.11 defines EFI_MEMORY_HOT_PLUGGABLE as 0x0010_0000; the pinned
        // uefi-raw 0.9 has no constant for it. A descriptor carrying it must be
        // refused, not silently classified as ordinary memory.
        assert_eq!(
            disp(ty::CONVENTIONAL, WB | 0x0010_0000),
            Err(PolicyError::UnknownAttribute)
        );
    }

    #[test]
    fn profile_only_attributes_fail_closed_without_a_pinned_profile() {
        // `RO`/`SP`/`CPU_CRYPTO` are standard attributes only in the
        // specification-repository/EDK2 encoding; the published tables this
        // project also honours do not define them at these positions. Rather
        // than silently adopting one branch, the bits fail closed until a
        // `UefiMemoryAttributeProfile` names the firmware actually booting
        // (x86 and aarch64 firmwares need not come from the same build).
        for bit in [0x2_0000u64, 0x4_0000, 0x8_0000] {
            assert_eq!(
                disp(ty::CONVENTIONAL, WB | bit),
                Err(PolicyError::ProfileAttribute),
                "bit {bit:#x}"
            );
        }
        // 0x10_0000 is not defined in the pinned baseline at all: unknown bit.
        assert_eq!(
            disp(ty::CONVENTIONAL, WB | 0x10_0000),
            Err(PolicyError::UnknownAttribute)
        );
    }

    #[test]
    fn every_contested_attribute_bit_fails_closed() {
        // The PDF and repository encodings disagree across 0x1000..=0x1_0000:
        // 0x4000 is SP (must be excluded) in one text and XP (metadata) in the
        // other, 0x8000 is CPU_CRYPTO vs NV, 0x1000 is MORE_RELIABLE vs WP.
        // Accepting either reading can silently admit memory the policy must
        // exclude, so each bit is refused before any interpretation.
        for bit in [0x1000u64, 0x2000, 0x4000, 0x8000, 0x1_0000] {
            assert_eq!(
                disp(ty::CONVENTIONAL, WB | bit),
                Err(PolicyError::AmbiguousAttribute),
                "bit {bit:#x}"
            );
            assert_eq!(
                disp(ty::LOADER_DATA, WB | bit),
                Err(PolicyError::AmbiguousAttribute),
                "bit {bit:#x} on a mapped type"
            );
        }
    }

    #[test]
    fn the_ambiguous_bit_fails_closed_under_either_name() {
        // The published 2.11 PDF puts HOT_PLUGGABLE at 0x0001_0000, while the
        // specification repository and EDK2 put MORE_RELIABLE there. Firmware
        // built from the PDF would report removable memory with this bit, and
        // this kernel cannot unplug memory; firmware built from the repository
        // would report a reliability hint this policy ignores. Neither may be
        // silently classified, so the bit is refused before the known-bit gate.
        assert_eq!(
            disp(ty::CONVENTIONAL, WB | 0x0001_0000),
            Err(PolicyError::AmbiguousAttribute)
        );
        assert_eq!(
            disp(ty::CONVENTIONAL, WB | attr::MORE_RELIABLE),
            Err(PolicyError::AmbiguousAttribute)
        );
    }

    #[test]
    fn isa_attributes_fail_closed_whatever_the_mask_says() {
        assert_eq!(
            disp(ty::CONVENTIONAL, WB | attr::ISA_VALID),
            Err(PolicyError::IsaAttributes)
        );
        assert_eq!(
            disp(ty::CONVENTIONAL, WB | attr::ISA_MASK),
            Err(PolicyError::IsaAttributes)
        );
        assert_eq!(
            disp(ty::CONVENTIONAL, WB | attr::ISA_VALID | 0x1_0000_0000_0000),
            Err(PolicyError::IsaAttributes)
        );
        assert_eq!(
            disp(ty::RESERVED, attr::ISA_MASK),
            Err(PolicyError::IsaAttributes),
            "an uninterpretable claim is refused before the type is consulted"
        );
    }

    #[test]
    fn attribute_bits_outside_the_baseline_are_unknown() {
        assert_eq!(
            disp(ty::CONVENTIONAL, WB | attr::UNKNOWN_FOR_TESTING),
            Err(PolicyError::UnknownAttribute)
        );
        assert_eq!(
            disp(
                ty::CONVENTIONAL,
                u64::MAX & !(attr::AMBIGUOUS | attr::PROFILE_ONLY)
            ),
            Err(PolicyError::UnknownAttribute)
        );
    }

    #[test]
    fn the_four_ambiguous_combinations_land_as_adjudicated() {
        // A type-based exclusion outranks an independent attribute, and is not
        // filed under the attribute's reason.
        assert_eq!(
            disp(ty::RESERVED, WB | attr::RUNTIME),
            Ok(Disposition::Excluded)
        );
        assert_eq!(
            disp(ty::UNUSABLE, WB | attr::RUNTIME),
            Ok(Disposition::Excluded)
        );
        assert_eq!(
            disp(ty::MMIO, WB | attr::RUNTIME),
            Ok(Disposition::DevicePath)
        );
        assert_eq!(
            disp(ty::PAL_CODE, WB | attr::RUNTIME),
            Err(PolicyError::RefuseToBoot)
        );
    }

    #[test]
    fn geometry_is_checked_for_ranges_that_are_emitted() {
        let mut d = desc(ty::CONVENTIONAL, WB);
        d.physical_start = 0x1001;
        assert_eq!(
            adjudicate(&d, PA_LIMIT).map(|x| x.0),
            Err(PolicyError::UnalignedStart)
        );
        d = desc(ty::CONVENTIONAL, WB);
        d.number_of_pages = 0;
        assert_eq!(
            adjudicate(&d, PA_LIMIT).map(|x| x.0),
            Err(PolicyError::ZeroPageCount)
        );
        d = desc(ty::CONVENTIONAL, WB);
        d.number_of_pages = u64::MAX;
        assert_eq!(
            adjudicate(&d, PA_LIMIT).map(|x| x.0),
            Err(PolicyError::LengthOverflow)
        );
        d = desc(ty::CONVENTIONAL, WB);
        d.physical_start = u64::MAX - PAGE_SIZE + 1;
        assert_eq!(
            adjudicate(&d, PA_LIMIT).map(|x| x.0),
            Err(PolicyError::RangeOverflow)
        );
        d = desc(ty::CONVENTIONAL, WB);
        d.physical_start = PA_LIMIT;
        assert_eq!(
            adjudicate(&d, PA_LIMIT).map(|x| x.0),
            Err(PolicyError::OutOfPhysRange)
        );
    }

    #[test]
    fn ranges_that_emit_nothing_are_not_geometry_checked() {
        // A device aperture or a firmware hole beyond the address ceiling must not
        // stop the boot: we never emit it, and its shape is firmware's business.
        let mut d = desc(ty::MMIO, WB);
        d.physical_start = PA_LIMIT + PAGE_SIZE;
        d.number_of_pages = u64::MAX;
        assert_eq!(
            adjudicate(&d, PA_LIMIT).map(|x| x.0),
            Ok(Disposition::DevicePath)
        );
        d = desc(ty::RESERVED, WB);
        d.number_of_pages = 0;
        assert_eq!(
            adjudicate(&d, PA_LIMIT).map(|x| x.0),
            Ok(Disposition::Excluded)
        );
    }

    #[test]
    fn records_carry_descriptor_identity() {
        let mut map = [0u8; 40];
        // UC|WC|WT|WB: a non-trivial attribute that still passes the
        // write-back admission (no TYPE_INDEPENDENT / profile-only bit).
        push_desc(&mut map, 0, 40, ty::CONVENTIONAL, 0x10_0000, 1, 0xF);
        map[16..24].copy_from_slice(&0xFFFF_8000_0000_2000u64.to_le_bytes());
        let mut storage = MapStorage::new();
        storage.parse_raw_map(&map, map.len(), 40, 1, 42).unwrap();
        let mut arena = [0u8; minix_boot::ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        storage.classify_into_handoff(PA_LIMIT, &mut s).unwrap();
        let off = minix_boot::WIRE_SOURCES_OFFSET;
        let u64_at = |o: usize| u64::from_le_bytes(arena[o..o + 8].try_into().unwrap());
        assert_eq!(u64_at(off + 24), 0xF, "attribute");
        assert_eq!(u64_at(off + 32), 0xFFFF_8000_0000_2000, "virtual_start");
        assert_eq!(u64_at(off + 40), 42, "map snapshot id");
        assert_eq!(
            u16::from_le_bytes(arena[off + 58..off + 60].try_into().unwrap()),
            0,
            "local source id"
        );
        assert_eq!(arena[off + 66], 1, "input object = UEFI snapshot");
    }

    #[test]
    fn a_range_above_the_ram_ceiling_still_reaches_the_ordinary_tables() {
        // Replaces the old "starts below the highest conventional end" filter:
        // membership is answered by type and attributes, not by where the largest
        // free region happens to stop.
        let mut map = [0u8; 40];
        push_desc(&mut map, 0, 40, ty::LOADER_DATA, 0xFD_0000_0000, 0x100, WB);
        let mut storage = MapStorage::new();
        storage.parse_raw_map(&map, map.len(), 40, 1, 0).unwrap();
        let mut arena = [0u8; minix_boot::ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let c = storage.classify_into_handoff(PA_LIMIT, &mut s).unwrap();
        assert_eq!(
            c,
            Classification {
                conventional: 0,
                reserved: 1,
                side: 0
            }
        );
        assert_eq!(
            get_u64(s.as_bytes(), minix_boot::WIRE_RESERVED_OFFSET),
            0xFD_0000_0000
        );
        assert_eq!(get_u64(s.as_bytes(), minix_boot::WIRE_MEMMAP_OFFSET), 0);
    }

    #[test]
    fn classified_map_writes_both_tables_and_the_side_list() {
        let desc_size = 40usize;
        let mut map = [0u8; 40 * 5];
        // free RAM
        push_desc(
            &mut map,
            0,
            desc_size,
            ty::CONVENTIONAL,
            0x10_0000,
            0x200,
            WB,
        );
        // shim's own data: mapped, never reallocatable
        push_desc(&mut map, 1, desc_size, ty::LOADER_DATA, 0x40_0000, 1, WB);
        // device aperture: neither table
        push_desc(&mut map, 2, desc_size, ty::MMIO, 0xFE00_0000, 0x100, WB);
        // firmware hole the OS may not use: no table at all
        push_desc(&mut map, 3, desc_size, ty::RESERVED, 0xFD00_0000, 0x100, WB);
        // non-volatile range: kept out of the ordinary tables, recorded
        push_desc(&mut map, 4, desc_size, ty::ACPI_NVS, 0x800_0000, 0x10, WB);

        let mut arena = [0u8; minix_boot::ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let mut storage = MapStorage::new();
        storage
            .parse_raw_map(&map, map.len(), desc_size, 1, 0)
            .unwrap();
        let plan = classify(storage.descriptors(), PA_LIMIT, storage.side_capacity()).unwrap();
        let mut side = [MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        }; SIDE_CAP];
        let c = plan.commit(storage.descriptors(), PA_LIMIT, &mut s, &mut side, 0);
        assert_eq!(
            c,
            Classification {
                conventional: 1,
                reserved: 1,
                side: 1
            }
        );
        let b = s.as_bytes();
        assert_eq!(get_u64(b, 128), 16, "memmap descriptor = 1 x 16");
        assert_eq!(get_u64(b, 136), 16, "reserved descriptor = 1 x 16");
        assert_eq!(get_u64(b, minix_boot::WIRE_MEMMAP_OFFSET), 0x10_0000);
        assert_eq!(get_u64(b, minix_boot::WIRE_MEMMAP_OFFSET + 8), 0x200 * 4096);
        assert_eq!(get_u64(b, minix_boot::WIRE_RESERVED_OFFSET), 0x40_0000);
        assert_eq!(get_u64(b, minix_boot::WIRE_RESERVED_OFFSET + 8), 4096);
        // The semantic partition mirrors the same entries.
        assert_eq!(get_u64(b, minix_boot::SEMANTIC_MEMMAP_OFFSET), 0x10_0000);
        assert_eq!(get_u64(b, minix_boot::SEMANTIC_RESERVED_OFFSET), 0x40_0000);
        // The side list holds the range that neither table received.
        assert_eq!(side[0].base.0, 0x800_0000);
        assert_eq!(side[0].len, 0x10 * 4096);
        assert_eq!(side[1].len, 0, "unused slots stay empty");
    }

    #[test]
    fn padded_descriptor_stride_is_honored() {
        let desc_size = 48usize;
        let mut map = [0u8; 48 * 2];
        push_desc(
            &mut map,
            0,
            desc_size,
            ty::CONVENTIONAL,
            0x10_0000,
            0x10,
            WB,
        );
        push_desc(
            &mut map,
            1,
            desc_size,
            ty::CONVENTIONAL,
            0x20_0000,
            0x10,
            WB,
        );
        let mut arena = [0u8; minix_boot::ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let mut storage = MapStorage::new();
        storage
            .parse_raw_map(&map, map.len(), desc_size, 1, 0)
            .unwrap();
        let c = storage.classify_into_handoff(PA_LIMIT, &mut s).unwrap();
        assert_eq!(c.conventional, 2);
        assert_eq!(get_u64(s.as_bytes(), 128), 32);
    }

    #[test]
    fn rejects_unsupported_version() {
        let map = [0u8; 40];
        assert_eq!(classify_err(&map, 40, 40, 2), MapError::UnsupportedVersion);
    }

    #[test]
    fn rejects_descriptor_smaller_than_prefix() {
        let map = [0u8; 80];
        assert_eq!(classify_err(&map, 80, 39, 1), MapError::DescriptorTooSmall);
    }

    #[test]
    fn rejects_unaligned_descriptor_size() {
        let map = [0u8; 84];
        assert_eq!(
            classify_err(&map, 82, 41, 1),
            MapError::DescriptorSizeNotAligned
        );
    }

    #[test]
    fn rejects_map_size_beyond_buffer() {
        let map = [0u8; 40];
        assert_eq!(classify_err(&map, 80, 40, 1), MapError::MapSizeBeyondBuffer);
    }

    #[test]
    fn rejects_map_size_not_multiple() {
        let map = [0u8; 120];
        assert_eq!(classify_err(&map, 60, 40, 1), MapError::MapSizeNotMultiple);
    }

    #[test]
    fn rejects_zero_page_count_on_an_emitted_range() {
        let mut map = [0u8; 40];
        push_desc(&mut map, 0, 40, ty::CONVENTIONAL, 0x10_0000, 0, WB);
        assert!(matches!(
            classify_err(&map, 40, 40, 1),
            MapError::RejectedDescriptor(DescriptorRejection {
                failed: PolicyError::ZeroPageCount,
                ..
            })
        ));
    }

    #[test]
    fn rejects_page_count_overflow() {
        let mut map = [0u8; 40];
        push_desc(&mut map, 0, 40, ty::CONVENTIONAL, 0x10_0000, u64::MAX, WB);
        assert!(matches!(
            classify_err(&map, 40, 40, 1),
            MapError::RejectedDescriptor(DescriptorRejection {
                failed: PolicyError::LengthOverflow,
                ..
            })
        ));
    }

    #[test]
    fn rejects_range_above_phys_limit() {
        let mut map = [0u8; 40];
        push_desc(&mut map, 0, 40, ty::CONVENTIONAL, PA_LIMIT, 1, WB);
        assert!(matches!(
            classify_err(&map, 40, 40, 1),
            MapError::RejectedDescriptor(DescriptorRejection {
                failed: PolicyError::OutOfPhysRange,
                ..
            })
        ));
    }

    #[test]
    fn rejects_unaligned_start() {
        let mut map = [0u8; 40];
        push_desc(&mut map, 0, 40, ty::CONVENTIONAL, 0x10_0001, 1, WB);
        assert!(matches!(
            classify_err(&map, 40, 40, 1),
            MapError::RejectedDescriptor(DescriptorRejection {
                failed: PolicyError::UnalignedStart,
                ..
            })
        ));
    }

    #[test]
    fn rejects_a_type_it_cannot_name_without_writing_anything() {
        let mut map = [0u8; 80];
        push_desc(&mut map, 0, 40, ty::CONVENTIONAL, 0x10_0000, 1, WB);
        push_desc(&mut map, 1, 40, ty::OEM_BAND, 0x20_0000, 1, WB);
        let mut arena = [0u8; minix_boot::ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let mut storage = MapStorage::new();
        storage.parse_raw_map(&map, map.len(), 40, 1, 0).unwrap();
        assert!(matches!(
            storage.classify_into_handoff(PA_LIMIT, &mut s).unwrap_err(),
            MapError::RejectedDescriptor(DescriptorRejection {
                failed: PolicyError::UnknownType,
                ..
            })
        ));
        // Nothing reached the wire: an un-nameable map is refused whole.
        assert_eq!(get_u64(s.as_bytes(), 128), 0);
        assert_eq!(get_u64(s.as_bytes(), 136), 0);
    }

    #[test]
    fn rejects_capacity_overflow_during_counting() {
        let entries = WIRE_MEMMAP_CAP + 1;
        let mut descs = [desc(ty::CONVENTIONAL, WB); 256];
        for (i, d) in descs[..entries].iter_mut().enumerate() {
            d.physical_start = 0x10_0000 + i as u64 * 0x2000;
        }
        assert_eq!(
            classify(&descs[..entries], PA_LIMIT, SIDE_CAP).unwrap_err(),
            MapError::Capacity(HandoffV1WriteError::TooManyMemmap)
        );
    }

    #[test]
    fn rejects_side_list_beyond_its_buffer() {
        let descs = [desc(ty::ACPI_NVS, WB), desc(ty::PERSISTENT, WB)];
        assert_eq!(
            classify(&descs, PA_LIMIT, 1).unwrap_err(),
            MapError::TooManySideEntries
        );
        assert_eq!(
            classify(&descs, PA_LIMIT, 2).unwrap().counts(),
            Classification {
                conventional: 0,
                reserved: 0,
                side: 2
            }
        );
    }

    /// The storage accepts exactly `MAX_MAP_DESCRIPTORS` descriptors, and a map
    /// that fills every output capacity at once still classifies: 128
    /// conventional (`WIRE_MEMMAP_CAP`), 64 reserved, 64 side (`SIDE_CAP`).
    /// A refused descriptor is reported with its own identity, not just an
    /// error code: index, type, geometry, attribute and the failed step.
    #[test]
    fn a_refused_descriptor_is_named_in_the_error() {
        let mut map = [0u8; 40];
        push_desc(&mut map, 0, 40, 0x100, 0x10_0000, 1, WB);
        let mut storage = MapStorage::new();
        storage.parse_raw_map(&map, map.len(), 40, 1, 0).unwrap();
        assert_eq!(
            classify(storage.descriptors(), PA_LIMIT, 0).unwrap_err(),
            MapError::RejectedDescriptor(DescriptorRejection {
                index: 0,
                raw_type: 0x100,
                physical_start: 0x10_0000,
                virtual_start: 0,
                number_of_pages: 1,
                attribute: WB,
                failed: PolicyError::UnknownType,
            })
        );
    }

    #[test]
    fn accepts_exactly_the_descriptor_and_partition_caps() {
        let mut map = [0u8; 40 * MAX_MAP_DESCRIPTORS];
        for i in 0..128 {
            push_desc(
                &mut map,
                i,
                40,
                ty::CONVENTIONAL,
                0x10_0000 + (i as u64) * 0x1000,
                1,
                WB,
            );
        }
        for i in 0..64 {
            push_desc(
                &mut map,
                128 + i,
                40,
                ty::LOADER_DATA,
                0x40_0000 + (i as u64) * 0x1000,
                1,
                WB,
            );
        }
        for i in 0..64 {
            push_desc(
                &mut map,
                192 + i,
                40,
                ty::ACPI_NVS,
                0x80_0000 + (i as u64) * 0x1000,
                1,
                WB,
            );
        }
        let mut storage = MapStorage::new();
        storage.parse_raw_map(&map, map.len(), 40, 1, 0).unwrap();
        let mut arena = [0u8; minix_boot::ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let c = storage.classify_into_handoff(PA_LIMIT, &mut s).unwrap();
        assert_eq!((c.conventional, c.reserved, c.side), (128, 64, 64));
    }

    #[test]
    fn rejects_a_map_larger_than_the_storage() {
        let map = [0u8; 40 * (MAX_MAP_DESCRIPTORS + 1)];
        let mut storage = MapStorage::new();
        assert_eq!(
            storage
                .parse_raw_map(&map, map.len(), 40, 1, 0)
                .unwrap_err(),
            MapError::TooManyDescriptors
        );
    }

    #[test]
    fn an_empty_map_classifies_to_nothing() {
        let map = [0u8; 40];
        let mut arena = [0u8; minix_boot::ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let mut storage = MapStorage::new();
        storage.parse_raw_map(&map, 0, 40, 1, 0).unwrap();
        let c = storage.classify_into_handoff(PA_LIMIT, &mut s).unwrap();
        assert_eq!(c, Classification::default());
    }
}
