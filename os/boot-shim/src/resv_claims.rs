//! UEFI ↔ DTB static-reservation type coverage (brief O-11).
//!
//! The DTB's `/reserved-memory` children and memory-reservation-block entries
//! must appear in the final `GetMemoryMap` snapshot under the type their
//! semantics require: ordinary and `reusable` reservations under
//! `EfiBootServicesData`, `no-map` and MRB entries under
//! `EfiReservedMemoryType`. This module turns the DTB side into page-rounded
//! claims and checks them against the snapshot the EBS pairing loop just read.
//!
//! The verification is the brief's coverage formula
//! `Outward(R_reserved) ⊆ ⋃ { type-compatible descriptors }`, with two
//! conservative strengthenings stated by the same table: a page covered by a
//! descriptor of another type is a conflict even when a compatible descriptor
//! also covers it, and the configuration-table DTB's own pages accept
//! `EfiACPIReclaimMemory` (the UEFI 2.11 rule for a standard DTB table).

use crate::ebs_map::{DescriptorRef, ty};
use minix_boot::{MemoryRegion, SourceKind, SourceRecord};

/// Upper bound on emitted claims: at most 8 declared reservations × 8
/// `/memory` spans + 16 MRB entries × 8 spans is smaller, and one claim per
/// (record, panorama span) intersection is the most this module produces.
pub const MAX_CLAIMS: usize = 64;
/// Intervals collected per claim. A 256-descriptor map can contribute at most
/// one interval per descriptor; overflow is a refusal, never a drop.
const MAX_INTERVALS: usize = 256;

/// The EFI type a reservation page must be covered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedEfiType {
    BootServicesData,
    Reserved,
}

impl ExpectedEfiType {
    fn accepts(self, raw_type: u32) -> bool {
        match self {
            Self::BootServicesData => raw_type == ty::BOOT_SERVICES_DATA,
            Self::Reserved => raw_type == ty::RESERVED,
        }
    }
}

/// One page-rounded claim, already clipped to one `/memory` panorama span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claim {
    pub start: u64,
    pub end: u64,
    pub expected: ExpectedEfiType,
}

/// Why the claims could not be verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimError {
    /// More claims than the fixed buffer holds.
    TooManyClaims,
    /// More covering intervals than the fixed buffer holds.
    IntervalOverflow,
    /// A claimed page has no covering descriptor of the expected type.
    UncoveredPage {
        page: u64,
        expected: ExpectedEfiType,
    },
    /// A claimed page is covered by a descriptor of another type.
    IncompatibleType {
        page: u64,
        expected: ExpectedEfiType,
        actual: u32,
    },
}

/// Build the page-rounded claims behind a DTB's reservation records.
///
/// Only reservations that intersect the `/memory` panorama are claims on RAM;
/// a framebuffer-style reservation outside it (the collector keeps its source
/// record but projects nothing) must not be compared against the EFI map,
/// whose MMIO descriptors legitimately cover that address range.
pub fn claims_from_records(
    records: &[SourceRecord],
    panorama: &[MemoryRegion],
    out: &mut [Claim],
) -> Result<usize, ClaimError> {
    let mut n = 0usize;
    for record in records {
        // Only physical declarations are claims on the EFI physical map.
        if record.address_domain != minix_boot::AddressDomain::Physical {
            continue;
        }
        let expected = match record.kind {
            SourceKind::ReservedOrdinary | SourceKind::ReservedReusable => {
                ExpectedEfiType::BootServicesData
            }
            SourceKind::ReservedNoMap | SourceKind::MrbEntry => ExpectedEfiType::Reserved,
            SourceKind::Memory
            | SourceKind::FirmwarePrefix
            | SourceKind::StartupObject
            | SourceKind::UefiExcluded => continue,
        };
        let raw_start = record.range.start;
        let raw_end = record.range.end_exclusive;
        if raw_end <= raw_start as u128 {
            continue;
        }
        let start = raw_start & !0xfff;
        let rounded = (raw_end + 0xfff) & !0xfffu128;
        let end = if rounded > u64::MAX as u128 {
            u64::MAX
        } else {
            rounded as u64
        };
        if start >= end {
            continue;
        }
        for span in panorama {
            let span_start = span.base.0;
            let span_end = span.base.0 as u128 + span.len as u128;
            let clip_start = start.max(span_start);
            let clip_end = (end as u128).min(span_end);
            if clip_start as u128 >= clip_end {
                continue;
            }
            if n >= out.len() {
                return Err(ClaimError::TooManyClaims);
            }
            out[n] = Claim {
                start: clip_start,
                end: clip_end as u64,
                expected,
            };
            n += 1;
        }
    }
    Ok(n)
}

/// Sorted union helper: append `[start, end)` keeping the list ordered by
/// start, merging touching or overlapping intervals.
struct IntervalSet {
    items: [(u64, u64); MAX_INTERVALS],
    len: usize,
}

impl IntervalSet {
    const fn new() -> Self {
        Self {
            items: [(0, 0); MAX_INTERVALS],
            len: 0,
        }
    }

    fn push(&mut self, start: u64, end: u64) -> Result<(), ClaimError> {
        if start >= end {
            return Ok(());
        }
        if self.len == self.items.len() {
            return Err(ClaimError::IntervalOverflow);
        }
        let mut at = self.len;
        while at > 0 && self.items[at - 1].0 > start {
            self.items[at] = self.items[at - 1];
            at -= 1;
        }
        self.items[at] = (start, end);
        self.len += 1;
        Ok(())
    }

    /// First cursor position that no interval reaches, scanning from `start`.
    fn uncovered_from(&self, start: u64, end: u64) -> Option<u64> {
        let mut cursor = start;
        let mut i = 0;
        while cursor < end {
            let mut next = cursor;
            while i < self.len && self.items[i].0 <= cursor {
                if self.items[i].1 > next {
                    next = self.items[i].1;
                }
                i += 1;
            }
            if next == cursor {
                return Some(cursor);
            }
            cursor = next;
        }
        None
    }
}

/// Verify every claim against the classified snapshot.
///
/// `dtb` is the raw `[start, end)` of the configuration-table DTB: a page
/// inside it may be covered by `EfiACPIReclaimMemory` regardless of the
/// reservation type, because the standard DTB table's type is mandatory and
/// outranks the MRB's `should` (brief O-11).
pub fn check(
    claims: &[Claim],
    descs: &[DescriptorRef],
    dtb: Option<(u64, u64)>,
) -> Result<(), ClaimError> {
    for claim in claims {
        let mut covered = IntervalSet::new();
        let mut conflict: Option<(u64, u32)> = None;
        for desc in descs {
            let desc_start = desc.physical_start;
            // The classifier rejects overflowing geometry for every range it
            // emits; only `Excluded`/`DevicePath` descriptors skip that gate,
            // and such a descriptor cannot cover a claim page.
            let Some(desc_len) = desc.number_of_pages.checked_mul(4096) else {
                continue;
            };
            let desc_end = desc_start.saturating_add(desc_len);
            let start = desc_start.max(claim.start);
            let end = desc_end.min(claim.end);
            if start >= end {
                continue;
            }
            let dtb_exception = desc.raw_type == ty::ACPI_RECLAIM
                && dtb.is_some_and(|(a, b)| desc_start < b && a < desc_end);
            if claim.expected.accepts(desc.raw_type) || dtb_exception {
                covered.push(start, end)?;
            } else if conflict.is_none_or(|(at, _)| start < at) {
                conflict = Some((start, desc.raw_type));
            }
        }
        if let Some((page, actual)) = conflict {
            return Err(ClaimError::IncompatibleType {
                page,
                expected: claim.expected,
                actual,
            });
        }
        if let Some(page) = covered.uncovered_from(claim.start, claim.end) {
            return Err(ClaimError::UncoveredPage {
                page,
                expected: claim.expected,
            });
        }
    }
    Ok(())
}

// ── capture / read-back (single-threaded boot state) ─────────────────────

static mut DTB_RECORDS: [SourceRecord; minix_boot::DTB_SOURCE_CAP] =
    [minix_boot::EMPTY_SOURCE_RECORD; minix_boot::DTB_SOURCE_CAP];
static mut DTB_RECORDS_N: usize = 0;

/// The configuration-table DTB's own source records (at most
/// [`minix_boot::DTB_SOURCE_CAP`]). Empty when the DTB is absent or
/// unparsable; the halt for that case is `cross_validate_config_dtb`'s.
pub fn records() -> &'static [SourceRecord] {
    // SAFETY: write-once inside `capture`, read-only afterwards; same
    // single-threaded boot contract as the claim list.
    unsafe {
        let n = core::ptr::addr_of!(DTB_RECORDS_N).read();
        core::slice::from_raw_parts(core::ptr::addr_of!(DTB_RECORDS) as *const SourceRecord, n)
    }
}

static mut CLAIMS: [Claim; MAX_CLAIMS] = [Claim {
    start: 0,
    end: 0,
    expected: ExpectedEfiType::Reserved,
}; MAX_CLAIMS];
static mut CLAIMS_N: usize = 0;
static mut DTB_RAW: Option<(u64, u64)> = None;

/// Parse the configuration-table DTB and record its reservation claims.
///
/// Any parse failure leaves the claim list empty: the boot is halted later by
/// `cross_validate_config_dtb` on the same bytes, so this function must not
/// invent a second failure path for the same input. Claim *capacity* is the
/// exception: no later gate re-counts claims, so an overflow must surface
/// here instead of leaving `check_current` with an empty list.
pub fn capture(bytes: &'static [u8], source: (u64, u64)) -> Result<(), ClaimError> {
    // SAFETY: single-threaded boot; capture runs once inside the pairing loop
    // after the owned copy was authorized, and the reader runs on the same
    // thread immediately afterwards.
    unsafe {
        CLAIMS_N = 0;
        DTB_RECORDS_N = 0;
        DTB_RAW = Some(source);
    }
    if minix_boot::fdt_layout::validate(0, bytes).is_err() {
        return Ok(());
    }
    let Ok(fdt) = fdt::Fdt::new(bytes) else {
        return Ok(());
    };
    let Ok((_, _, sources, panorama)) =
        minix_boot::DtbMemoryMap::new(&fdt, 0).collect_with_panorama()
    else {
        return Ok(());
    };
    capture_claims(sources, panorama, source)
}

/// Fill the static claim and record stores from already-parsed DTB facts.
///
/// Capacity is the one failure this layer cannot defer: no later gate
/// re-counts claims, so `TooManyClaims` must reach the pairing loop.
fn capture_claims(
    records: &[SourceRecord],
    panorama: &[minix_boot::MemoryRegion],
    source: (u64, u64),
) -> Result<(), ClaimError> {
    // SAFETY: single-threaded boot; same contract as `capture`.
    unsafe {
        CLAIMS_N = 0;
        DTB_RECORDS_N = 0;
        DTB_RAW = Some(source);
        for (i, record) in records.iter().enumerate() {
            if i >= minix_boot::DTB_SOURCE_CAP {
                break;
            }
            (core::ptr::addr_of_mut!(DTB_RECORDS) as *mut SourceRecord)
                .add(i)
                .write(*record);
            DTB_RECORDS_N = i + 1;
        }
        let mut claims = [Claim {
            start: 0,
            end: 0,
            expected: ExpectedEfiType::Reserved,
        }; MAX_CLAIMS];
        let count = claims_from_records(records, panorama, &mut claims)?;
        let base = core::ptr::addr_of_mut!(CLAIMS) as *mut Claim;
        for (i, claim) in claims.iter().enumerate().take(count) {
            // SAFETY: `base` points at the static array, `i < count <= MAX_CLAIMS`.
            base.add(i).write(*claim);
        }
        CLAIMS_N = count;
        Ok(())
    }
}

/// Check the captured claims against the snapshot the pairing loop read.
/// An empty list (no configuration-table DTB, or an unparsable one) passes:
/// that leg has no DTB-side claims to reconcile.
pub fn check_current(descs: &[DescriptorRef]) -> Result<(), ClaimError> {
    // SAFETY: same single-threaded contract as `capture`; the pairing loop
    // reads what capture wrote on the same thread.
    let (claims, count, dtb) = unsafe {
        (
            core::ptr::addr_of!(CLAIMS) as *const Claim,
            core::ptr::addr_of!(CLAIMS_N).read(),
            core::ptr::addr_of!(DTB_RAW).read(),
        )
    };
    if count == 0 {
        return Ok(());
    }
    let claims = unsafe { core::slice::from_raw_parts(claims, count) };
    check(claims, descs, dtb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::PhysBytes;

    fn desc(raw_type: u32, start: u64, pages: u64) -> DescriptorRef {
        DescriptorRef {
            raw_type,
            physical_start: start,
            virtual_start: 0,
            number_of_pages: pages,
            attribute: 0,
        }
    }

    fn claim(start: u64, end: u64, expected: ExpectedEfiType) -> Claim {
        Claim {
            start,
            end,
            expected,
        }
    }

    fn record(kind: SourceKind, start: u64, end: u128) -> SourceRecord {
        SourceRecord {
            id: 0,
            raw_type: 0,
            input_object: minix_boot::memmap_source::InputObjectId::DtbBootInput,
            address_domain: minix_boot::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            kind,
            origin: minix_boot::SourceOrigin::UefiDescriptor { index: 0 },
            range: minix_boot::SourceRange::new(start, end),
        }
    }

    #[test]
    fn ordinary_reservation_requires_boot_services_data() {
        let claims = [claim(0x2000, 0x3000, ExpectedEfiType::BootServicesData)];
        assert_eq!(
            check(&claims, &[desc(ty::BOOT_SERVICES_DATA, 0x1000, 4)], None),
            Ok(())
        );
    }

    /// Form 1: the first page is classified correctly and a later page of the
    /// same reservation is not — the coverage formula must reject it.
    #[test]
    fn first_page_correct_and_later_page_wrong_is_rejected() {
        let claims = [claim(0x2000, 0x4000, ExpectedEfiType::BootServicesData)];
        assert_eq!(
            check(
                &claims,
                &[
                    desc(ty::BOOT_SERVICES_DATA, 0x2000, 1),
                    desc(ty::RESERVED, 0x3000, 1),
                ],
                None
            ),
            Err(ClaimError::IncompatibleType {
                page: 0x3000,
                expected: ExpectedEfiType::BootServicesData,
                actual: ty::RESERVED
            })
        );
    }

    /// Form 2: several consecutive descriptors of the compatible type cover
    /// the reservation completely.
    #[test]
    fn consecutive_compatible_descriptors_cover_the_claim() {
        let claims = [claim(0x2000, 0x5000, ExpectedEfiType::BootServicesData)];
        assert_eq!(
            check(
                &claims,
                &[
                    desc(ty::BOOT_SERVICES_DATA, 0x2000, 2),
                    desc(ty::BOOT_SERVICES_DATA, 0x4000, 1),
                ],
                None
            ),
            Ok(())
        );
    }

    /// Form 3: a middle page of the reservation carries an incompatible type.
    #[test]
    fn incompatible_middle_page_is_reported_with_its_type() {
        let claims = [claim(0x2000, 0x5000, ExpectedEfiType::BootServicesData)];
        assert_eq!(
            check(
                &claims,
                &[
                    desc(ty::BOOT_SERVICES_DATA, 0x2000, 1),
                    desc(ty::MMIO, 0x3000, 1),
                    desc(ty::BOOT_SERVICES_DATA, 0x4000, 1),
                ],
                None
            ),
            Err(ClaimError::IncompatibleType {
                page: 0x3000,
                expected: ExpectedEfiType::BootServicesData,
                actual: ty::MMIO
            })
        );
    }

    #[test]
    fn uncovered_page_is_rejected() {
        let claims = [claim(0x2000, 0x4000, ExpectedEfiType::Reserved)];
        assert_eq!(
            check(&claims, &[desc(ty::RESERVED, 0x2000, 1)], None),
            Err(ClaimError::UncoveredPage {
                page: 0x3000,
                expected: ExpectedEfiType::Reserved
            })
        );
    }

    /// The standard DTB table's mandatory `EfiACPIReclaimMemory` outranks the
    /// MRB's `should`, but only for pages the DTB object actually occupies.
    #[test]
    fn dtb_pages_accept_acpi_reclaim_for_the_reservation_type() {
        let claims = [claim(0x2000, 0x3000, ExpectedEfiType::Reserved)];
        let descs = [desc(ty::ACPI_RECLAIM, 0x2000, 1)];
        assert_eq!(
            check(&claims, &descs, Some((0x4000, 0x5000))),
            Err(ClaimError::IncompatibleType {
                page: 0x2000,
                expected: ExpectedEfiType::Reserved,
                actual: ty::ACPI_RECLAIM
            })
        );
        assert_eq!(check(&claims, &descs, Some((0x2000, 0x3000))), Ok(()));
    }

    /// A descriptor whose geometry overflows 64 bits (only reachable for the
    /// `Excluded`/`DevicePath` types, which skip the classifier's geometry
    /// gate) must not panic the checker; it cannot cover a claim page.
    /// A multi-page MRB whose middle page is the DTB object keeps the
    /// reservation type on the other pages; the ACPI-reclaim exception is
    /// per page, not per claim.
    #[test]
    fn the_dtb_exception_applies_only_to_the_intersecting_page() {
        let claims = [claim(0x2000, 0x5000, ExpectedEfiType::Reserved)];
        let descs = [
            desc(ty::RESERVED, 0x2000, 1),
            desc(ty::ACPI_RECLAIM, 0x3000, 1),
            desc(ty::RESERVED, 0x4000, 1),
        ];
        assert_eq!(check(&claims, &descs, Some((0x3000, 0x4000))), Ok(()));
        assert_eq!(
            check(&claims, &descs, None),
            Err(ClaimError::IncompatibleType {
                page: 0x3000,
                expected: ExpectedEfiType::Reserved,
                actual: ty::ACPI_RECLAIM
            })
        );
    }

    #[test]
    fn overflowing_descriptor_geometry_is_ignored() {
        let claims = [claim(0x2000, 0x3000, ExpectedEfiType::Reserved)];
        let skewed = desc(ty::MMIO, 0x2000, u64::MAX);
        assert_eq!(
            check(&claims, &[skewed], None),
            Err(ClaimError::UncoveredPage {
                page: 0x2000,
                expected: ExpectedEfiType::Reserved
            })
        );
    }

    #[test]
    fn claims_are_clipped_to_the_memory_panorama() {
        let records = [
            record(SourceKind::ReservedOrdinary, 0x2000, 0x4000),
            record(SourceKind::ReservedNoMap, 0x9000_0000, 0x9000_1000),
        ];
        let panorama = [MemoryRegion {
            base: PhysBytes(0x1000),
            len: 0x3000,
        }];
        let mut out = [claim(0, 0, ExpectedEfiType::Reserved); 4];
        let n = claims_from_records(&records, &panorama, &mut out).expect("fits");
        assert_eq!(n, 1, "the reservation outside /memory is not a RAM claim");
        assert_eq!(
            out[0],
            claim(0x2000, 0x4000, ExpectedEfiType::BootServicesData)
        );
    }

    #[test]
    fn mrb_and_no_map_both_require_the_reserved_type() {
        let records = [
            record(SourceKind::MrbEntry, 0x2000, 0x3000),
            record(SourceKind::ReservedNoMap, 0x3000, 0x4000),
            record(SourceKind::ReservedReusable, 0x4000, 0x5000),
        ];
        let panorama = [MemoryRegion {
            base: PhysBytes(0x1000),
            len: 0x5000,
        }];
        let mut out = [claim(0, 0, ExpectedEfiType::Reserved); 4];
        let n = claims_from_records(&records, &panorama, &mut out).expect("fits");
        assert_eq!(n, 3);
        assert_eq!(out[0].expected, ExpectedEfiType::Reserved);
        assert_eq!(out[1].expected, ExpectedEfiType::Reserved);
        assert_eq!(out[2].expected, ExpectedEfiType::BootServicesData);
    }

    #[test]
    fn claim_capacity_overflow_is_a_refusal() {
        let records = [record(SourceKind::MrbEntry, 0x2000, 0x3000)];
        let panorama = [MemoryRegion {
            base: PhysBytes(0x1000),
            len: 0x5000,
        }];
        let mut out = [claim(0, 0, ExpectedEfiType::Reserved); 0];
        assert_eq!(
            claims_from_records(&records, &panorama, &mut out),
            Err(ClaimError::TooManyClaims)
        );
    }

    /// The production path must propagate the capacity refusal: an overflow
    /// that became an empty claim list would silently skip every coverage
    /// check for this DTB.
    #[test]
    fn capture_claims_surfaces_the_capacity_overflow() {
        let records = [record(SourceKind::MrbEntry, 0x2000, 0x3000); MAX_CLAIMS + 1];
        let panorama = [MemoryRegion {
            base: PhysBytes(0x1000),
            len: 0x5000,
        }];
        assert_eq!(
            capture_claims(&records, &panorama, (0x2000, 0x3000)),
            Err(ClaimError::TooManyClaims)
        );
        assert_eq!(check_current(&[]), Ok(()), "no claim survived the refusal");
    }

    /// A container the later gate owns leaves the claim list empty and the
    /// pairing loop running; `cross_validate_config_dtb` halts on the same
    /// bytes. Only capacity has no later gate.
    #[test]
    fn capture_leaves_container_failures_to_the_later_gate() {
        static BAD: [u8; 64] = [0u8; 64];
        assert_eq!(capture(&BAD, (0, 64)), Ok(()));
        assert_eq!(records().len(), 0);
        assert_eq!(check_current(&[]), Ok(()));
    }
}
