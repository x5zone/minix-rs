//! O-11 first-read discipline for the configuration-table DTB.
//!
//! The DTB is firmware memory whose page types are only known from a memory
//! map. Parsing it in place would read bytes before any policy accepted the
//! pages. This module splits the read into the brief's stages:
//!
//! 1. [`prepare`] — before any map is read: preflight the 8-byte header (the
//!    brief's single header read), bound `totalsize`, and allocate the owned
//!    buffer. Allocating here keeps the later snapshot unperturbed.
//! 2. [`authorize_and_copy`] — against the map paired with the MapKey, verify
//!    that every 4 KiB page of the source range is `EfiACPIReclaimMemory` and
//!    carries no conflicting type; only then copy the body once.
//! 3. [`owned`] — the copy that `fdt_layout::validate`, the parser and the
//!    collector consume; the original is never read again.

use crate::ebs_map::{DescriptorRef, ty};
use alloc::{boxed::Box, vec};

pub const PAGE_BYTES: u64 = 4096;

/// Why the DTB pages are not usable as a standard DTB table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DtbPageError {
    /// A page of the DTB range is not covered by `EfiACPIReclaimMemory`.
    NotAcpiReclaim { page: u64 },
    /// A page is covered by a descriptor of another type as well.
    ConflictingType { page: u64, actual: u32 },
}

/// The owned copy plus the firmware range it was taken from.
#[derive(Debug, Clone, Copy)]
pub struct OwnedDtb {
    pub bytes: &'static [u8],
    pub source: (u64, u64),
}

struct Pending {
    source: (u64, u64),
    dst: *mut u8,
    len: usize,
}

static mut PENDING: Option<Pending> = None;
static mut READY: Option<OwnedDtb> = None;

/// Preflight the header and reserve the owned buffer.
///
/// Reads exactly the 8 bytes needed to size the copy; the body is read only
/// by [`authorize_and_copy`], after the page-type authorization.
pub fn prepare(dtb_pa: u64) {
    // SAFETY: single-threaded boot; `prepare` runs once before any map read
    // and nothing else touches these statics on this face.
    unsafe {
        core::ptr::addr_of_mut!(PENDING).write(None);
        core::ptr::addr_of_mut!(READY).write(None);
        assert!(
            minix_boot::dtb_blob_aligned(dtb_pa),
            "boot-shim: configuration-table DTB is not 8-byte aligned — halting"
        );
        // SAFETY: the configuration table points at a readable blob; only the
        // 8-byte header is read here (brief O-11 header preflight).
        let head = core::slice::from_raw_parts(dtb_pa as *const u8, 8);
        let len = checked_total(dtb_pa, head);
        let buffer: &'static mut [u8] = Box::leak(vec![0u8; len].into_boxed_slice());
        let dst = buffer.as_mut_ptr();
        let end = dtb_pa + len as u64;
        let dst_start = dst as u64;
        let dst_end = dst_start + len as u64;
        assert!(
            dst_end <= dtb_pa || end <= dst_start,
            "boot-shim: DTB copy target overlaps the firmware blob — halting"
        );
        core::ptr::addr_of_mut!(PENDING).write(Some(Pending {
            source: (dtb_pa, end),
            dst,
            len,
        }));
    }
}

/// Authorize the source pages against `descs`, then copy the body once.
///
/// Idempotent across EBS retries: after the first successful copy the ready
/// value is returned without touching the source again.
pub fn authorize_and_copy(descs: &[DescriptorRef]) -> Result<Option<OwnedDtb>, DtbPageError> {
    // SAFETY: same single-threaded contract as `prepare`.
    unsafe {
        let Some(pending) = core::ptr::addr_of!(PENDING).read() else {
            return Ok(core::ptr::addr_of!(READY).read());
        };
        check_pages(descs, pending.source)?;
        core::ptr::copy_nonoverlapping(pending.source.0 as *const u8, pending.dst, pending.len);
        let owned = OwnedDtb {
            bytes: core::slice::from_raw_parts(pending.dst, pending.len),
            source: pending.source,
        };
        core::ptr::addr_of_mut!(READY).write(Some(owned));
        core::ptr::addr_of_mut!(PENDING).write(None);
        Ok(Some(owned))
    }
}

/// The ready copy, if one was authorized and taken.
pub fn owned() -> Option<OwnedDtb> {
    // SAFETY: read-only access after the single-threaded write in
    // `authorize_and_copy`.
    unsafe { core::ptr::addr_of!(READY).read() }
}

/// Every page of `range` (rounded outward) must be `EfiACPIReclaimMemory` and
/// must not be covered by a descriptor of another type.
pub fn check_pages(descs: &[DescriptorRef], range: (u64, u64)) -> Result<(), DtbPageError> {
    let start = range.0 & !(PAGE_BYTES - 1);
    let end = (range.1 + PAGE_BYTES - 1) & !(PAGE_BYTES - 1);
    let mut page = start;
    while page < end {
        let mut accepted = false;
        let mut conflict: Option<u32> = None;
        for desc in descs {
            let Some(len) = desc.number_of_pages.checked_mul(PAGE_BYTES) else {
                continue;
            };
            let desc_end = desc.physical_start.saturating_add(len);
            if desc.physical_start <= page && page < desc_end {
                if desc.raw_type == ty::ACPI_RECLAIM {
                    accepted = true;
                } else if conflict.is_none() {
                    conflict = Some(desc.raw_type);
                }
            }
        }
        if !accepted {
            return Err(DtbPageError::NotAcpiReclaim { page });
        }
        if let Some(actual) = conflict {
            return Err(DtbPageError::ConflictingType { page, actual });
        }
        page += PAGE_BYTES;
    }
    Ok(())
}

fn checked_total(pa: u64, head: &[u8]) -> usize {
    let total_be = u32::from_be_bytes(head[4..8].try_into().expect("DTB totalsize"));
    assert!(
        minix_boot::dtb_len_ok(total_be),
        "boot-shim: configuration-table DTB totalsize outside the accepted range — halting"
    );
    assert!(
        pa.checked_add(total_be as u64).is_some(),
        "boot-shim: configuration-table DTB range overflows the address space — halting"
    );
    total_be as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desc(raw_type: u32, start: u64, pages: u64) -> DescriptorRef {
        DescriptorRef {
            raw_type,
            physical_start: start,
            number_of_pages: pages,
            attribute: 0,
        }
    }

    #[test]
    fn all_pages_acpi_reclaim_passes() {
        let descs = [
            desc(ty::ACPI_RECLAIM, 0x4000_0000, 2),
            desc(ty::ACPI_RECLAIM, 0x4000_2000, 1),
        ];
        assert_eq!(check_pages(&descs, (0x4000_0100, 0x4000_2800)), Ok(()));
    }

    #[test]
    fn a_reserved_page_is_not_acpi_reclaim() {
        let descs = [
            desc(ty::ACPI_RECLAIM, 0x4000_0000, 1),
            desc(ty::RESERVED, 0x4000_1000, 1),
        ];
        assert_eq!(
            check_pages(&descs, (0x4000_0000, 0x4000_2000)),
            Err(DtbPageError::NotAcpiReclaim { page: 0x4000_1000 })
        );
    }

    #[test]
    fn an_uncovered_page_is_rejected() {
        let descs = [desc(ty::ACPI_RECLAIM, 0x4000_0000, 1)];
        assert_eq!(
            check_pages(&descs, (0x4000_0000, 0x4000_2000)),
            Err(DtbPageError::NotAcpiReclaim { page: 0x4000_1000 })
        );
    }

    #[test]
    fn a_conflicting_type_beside_acpi_reclaim_is_rejected() {
        let descs = [
            desc(ty::ACPI_RECLAIM, 0x4000_0000, 1),
            desc(ty::CONVENTIONAL, 0x4000_0000, 1),
        ];
        assert_eq!(
            check_pages(&descs, (0x4000_0000, 0x4000_1000)),
            Err(DtbPageError::ConflictingType {
                page: 0x4000_0000,
                actual: ty::CONVENTIONAL
            })
        );
    }

    #[test]
    fn the_range_is_page_rounded_outward() {
        let descs = [desc(ty::ACPI_RECLAIM, 0x4000_0000, 2)];
        assert_eq!(check_pages(&descs, (0x4000_0fff, 0x4000_1001)), Ok(()));
    }

    #[test]
    #[should_panic(expected = "totalsize outside the accepted range")]
    fn a_too_small_header_is_refused() {
        let mut head = [0u8; 8];
        head[4..8].copy_from_slice(&8u32.to_be_bytes());
        checked_total(0x4000_0000, &head);
    }
}
