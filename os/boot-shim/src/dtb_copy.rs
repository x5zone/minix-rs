//! O-11 two-snapshot discipline for the configuration-table DTB.
//!
//! The DTB is firmware memory whose page types are only known from a memory
//! map. Reading it before a policy accepted the pages would trust firmware
//! twice: once for the bytes and once for the absence of disagreement. The
//! module therefore splits the read into three stages, and each stage names
//! the snapshot that authorizes it:
//!
//! 1. [`note`] — records the configuration table's DTB address. No byte is
//!    read; a pointer is not evidence.
//! 2. [`preflight`] — against the first snapshot (the *preflight map*):
//!    verify the page(s) holding the 40-byte header are `EfiACPIReclaimMemory`,
//!    read that header exactly once, bound `totalsize`, and reserve the owned
//!    copy buffer. That reservation is an allocation and changes the live
//!    map, which is why the final snapshot has to be a second
//!    `GetMemoryMap` rather than the one that just authorized the header.
//! 3. [`authorize_and_copy`] — against the second snapshot (the *final map*,
//!    the one paired with the MapKey that `ExitBootServices` consumes):
//!    verify every page of the whole range again, then fill the reserved
//!    buffer — header from the bytes captured in preflight, body from the
//!    source — without allocating. A failed `ExitBootServices` retry reuses
//!    the copy; the source is never read again.
//!
//! [`owned`] hands the copy to the parser and the collector; the original is
//! never read after the copy.

use crate::ebs_map::{DescriptorRef, ty};
use alloc::{boxed::Box, vec};

pub const PAGE_BYTES: u64 = 4096;

/// The devicetree header fixed by the specification: magic, totalsize, the
/// three structure offsets, version, last compatible version, boot CPU id and
/// the two structure sizes.
pub const HEADER_LEN: usize = 40;

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

#[derive(Clone, Copy)]
enum Phase {
    /// No configuration-table DTB on this boot face.
    Idle,
    /// The address is known; not a single byte has been read.
    Noted(u64),
    /// The preflight map accepted the header pages; the copy buffer is
    /// reserved and the header bytes are held in `header`.
    Preflighted {
        source: (u64, u64),
        dst: *mut u8,
        len: usize,
        header: [u8; HEADER_LEN],
    },
    /// The final map accepted the whole range and the copy is complete.
    Ready(OwnedDtb),
}

static mut PHASE: Phase = Phase::Idle;

/// Record the configuration table's DTB address.
///
/// Stores the pointer only. The header read is [`preflight`]'s job because it
/// needs a memory map that already accepted the header pages.
pub fn note(dtb_pa: u64) {
    // SAFETY: single-threaded boot; this face runs once and nothing else
    // touches `PHASE` on it.
    unsafe {
        assert!(
            minix_boot::dtb_blob_aligned(dtb_pa),
            "boot-shim: configuration-table DTB is not 8-byte aligned — halting"
        );
        core::ptr::addr_of_mut!(PHASE).write(Phase::Noted(dtb_pa));
    }
}

/// True while an address is recorded but its header has not been read.
///
/// The pairing loop uses this to decide whether it needs the extra preflight
/// `GetMemoryMap`; a boot face without a configuration-table DTB pays no
/// call.
pub fn awaiting_preflight() -> bool {
    // SAFETY: read-only access to the single-threaded boot state.
    unsafe { matches!(core::ptr::addr_of!(PHASE).read(), Phase::Noted(_)) }
}

/// Authorize the header page(s) on the preflight map, read the header once,
/// size the copy and reserve its buffer.
///
/// Idempotent: a repeated call on an already preflighted or ready stage does
/// nothing, so a grown map buffer can re-run the preflight snapshot without
/// re-reading the source.
pub fn preflight(descs: &[DescriptorRef]) -> Result<(), DtbPageError> {
    // SAFETY: single-threaded boot; `PHASE` is written only by this module.
    unsafe {
        match core::ptr::addr_of!(PHASE).read() {
            Phase::Idle | Phase::Preflighted { .. } | Phase::Ready(_) => Ok(()),
            Phase::Noted(pa) => {
                let header_end = pa.checked_add(HEADER_LEN as u64).expect(
                    "boot-shim: configuration-table DTB header overflows the address space",
                );
                check_pages(descs, (pa, header_end))?;
                // SAFETY: the header pages were accepted by the snapshot just
                // read, and this is the single header read (brief O-11).
                let mut header = [0u8; HEADER_LEN];
                core::ptr::copy_nonoverlapping(pa as *const u8, header.as_mut_ptr(), HEADER_LEN);
                let len = checked_total(pa, &header);
                let buffer: &'static mut [u8] = Box::leak(vec![0u8; len].into_boxed_slice());
                let dst = buffer.as_mut_ptr();
                let end = pa + len as u64;
                assert!(
                    !copy_target_overlaps(dst as u64, len, (pa, end)),
                    "boot-shim: DTB copy target overlaps the firmware blob — halting"
                );
                core::ptr::addr_of_mut!(PHASE).write(Phase::Preflighted {
                    source: (pa, end),
                    dst,
                    len,
                    header,
                });
                Ok(())
            }
        }
    }
}

/// Authorize the whole range on the final map, then copy header and body once.
///
/// The header bytes come from [`preflight`]'s single read; only the body is
/// read here. Idempotent across EBS retries: after the first successful copy
/// the ready value is returned without touching the source again.
pub fn authorize_and_copy(descs: &[DescriptorRef]) -> Result<Option<OwnedDtb>, DtbPageError> {
    // SAFETY: same single-threaded contract as `preflight`.
    unsafe {
        match core::ptr::addr_of!(PHASE).read() {
            Phase::Idle => Ok(None),
            Phase::Noted(_) => {
                panic!("boot-shim: DTB copy requested before its header preflight — halting")
            }
            Phase::Preflighted {
                source,
                dst,
                len,
                header,
            } => {
                check_pages(descs, source)?;
                core::ptr::copy_nonoverlapping(header.as_ptr(), dst, HEADER_LEN.min(len));
                if len > HEADER_LEN {
                    core::ptr::copy_nonoverlapping(
                        (source.0 + HEADER_LEN as u64) as *const u8,
                        dst.add(HEADER_LEN),
                        len - HEADER_LEN,
                    );
                }
                let owned = OwnedDtb {
                    bytes: core::slice::from_raw_parts(dst, len),
                    source,
                };
                core::ptr::addr_of_mut!(PHASE).write(Phase::Ready(owned));
                Ok(Some(owned))
            }
            Phase::Ready(owned) => Ok(Some(owned)),
        }
    }
}

/// The ready copy, if one was authorized and taken.
pub fn owned() -> Option<OwnedDtb> {
    // SAFETY: read-only access after the single-threaded writes above.
    unsafe {
        match core::ptr::addr_of!(PHASE).read() {
            Phase::Ready(owned) => Some(owned),
            _ => None,
        }
    }
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

/// Whether the freshly allocated copy `[dst, dst + len)` overlaps the
/// firmware source `[start, end)`.
///
/// Under Boot Services the shim runs on the firmware's identity mapping, so
/// the pool pointer is the allocation's physical address. This is the
/// actual-allocation intersection check the brief requires, not a prediction
/// from the allocator's usual behaviour.
pub fn copy_target_overlaps(dst: u64, len: usize, source: (u64, u64)) -> bool {
    let dst_end = dst.saturating_add(len as u64);
    !(dst_end <= source.0 || source.1 <= dst)
}

fn checked_total(pa: u64, head: &[u8; HEADER_LEN]) -> usize {
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

/// Test-only reset of the module's single-threaded stage.
#[cfg(test)]
pub(crate) fn reset() {
    // SAFETY: tests run with `RUST_TEST_THREADS=1`.
    unsafe { core::ptr::addr_of_mut!(PHASE).write(Phase::Idle) }
}

/// Test-only guard: cleans the stage even when an assertion panics.
#[cfg(test)]
pub(crate) struct ResetOnDrop;

#[cfg(test)]
impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        reset();
    }
}

/// Page-aligned scratch blob that stands in for firmware memory in host
/// tests, shared with the pairing-loop timing tests.
#[cfg(test)]
pub(crate) mod test_blob {
    use std::alloc::{Layout, alloc_zeroed, dealloc};

    pub struct Blob {
        ptr: *mut u8,
        layout: Layout,
        len: usize,
    }

    impl Blob {
        pub fn new(len: usize) -> Self {
            assert!(len >= 40, "a DTB header is 40 bytes");
            let layout = Layout::from_size_align(len.max(0x2000), 4096).expect("test layout");
            // SAFETY: the layout is non-zero and page aligned.
            let ptr = unsafe { alloc_zeroed(layout) };
            assert!(!ptr.is_null(), "test blob allocation failed");
            let blob = Self { ptr, layout, len };
            blob.set_total(len as u32);
            // SAFETY: four bytes fit in the allocation.
            unsafe {
                core::ptr::copy_nonoverlapping(0xd00d_feedu32.to_be_bytes().as_ptr(), blob.ptr, 4);
            }
            blob
        }

        pub fn set_total(&self, total: u32) {
            // SAFETY: bytes 4..8 are inside the allocation.
            unsafe {
                core::ptr::copy_nonoverlapping(total.to_be_bytes().as_ptr(), self.ptr.add(4), 4);
            }
        }

        pub fn pa(&self) -> u64 {
            self.ptr as u64
        }

        pub fn fill(&self, from: usize, byte: u8) {
            assert!(from <= self.len);
            // SAFETY: `from..len` is inside the allocation.
            unsafe { core::ptr::write_bytes(self.ptr.add(from), byte, self.len - from) }
        }

        pub fn as_slice(&self) -> &[u8] {
            // SAFETY: the allocation is live for `self`'s lifetime.
            unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
        }
    }

    impl Drop for Blob {
        fn drop(&mut self) {
            // SAFETY: the pointer and layout come from `alloc_zeroed`.
            unsafe { dealloc(self.ptr, self.layout) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dtb_copy::test_blob::Blob;

    fn desc(raw_type: u32, start: u64, pages: u64) -> DescriptorRef {
        DescriptorRef {
            raw_type,
            physical_start: start,
            virtual_start: 0,
            number_of_pages: pages,
            attribute: 0,
        }
    }

    fn acpi(start: u64, pages: u64) -> DescriptorRef {
        desc(ty::ACPI_RECLAIM, start, pages)
    }

    fn phase() -> Phase {
        // SAFETY: tests run with `RUST_TEST_THREADS=1`.
        unsafe { core::ptr::addr_of!(PHASE).read() }
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
        let mut head = [0u8; HEADER_LEN];
        head[4..8].copy_from_slice(&8u32.to_be_bytes());
        checked_total(0x4000_0000, &head);
    }

    #[test]
    fn preflight_authorizes_the_header_then_the_final_map_copies_once() {
        let _reset = ResetOnDrop;
        let blob = Blob::new(0x100);
        blob.fill(HEADER_LEN, b'A');
        let pa = blob.pa();
        let map = [acpi(pa, 1)];

        note(pa);
        assert!(awaiting_preflight());
        assert_eq!(preflight(&map), Ok(()));
        assert!(!awaiting_preflight());
        match phase() {
            Phase::Preflighted { dst, len, .. } => {
                assert_eq!(len, 0x100, "the copy is sized from the preflight header");
                // SAFETY: the reserved buffer is live until `reset`.
                let copy = unsafe { core::slice::from_raw_parts(dst, len) };
                assert!(
                    copy.iter().all(|&b| b == 0),
                    "preflight must not copy the body"
                );
            }
            _ => panic!("preflight must leave the stage waiting for the final map"),
        }

        let owned = authorize_and_copy(&map).unwrap().expect("copy authorized");
        assert_eq!(owned.source, (pa, pa + 0x100));
        assert_eq!(owned.bytes, blob.as_slice(), "copy equals the source bytes");
        assert!(matches!(phase(), Phase::Ready(_)));
    }

    #[test]
    fn a_final_map_type_change_stops_before_the_body_copy() {
        let _reset = ResetOnDrop;
        let blob = Blob::new(0x1100);
        let pa = blob.pa();
        note(pa);
        assert_eq!(preflight(&[acpi(pa, 2)]), Ok(()));

        let final_map = [acpi(pa, 1), desc(ty::RESERVED, pa + 0x1000, 1)];
        assert!(matches!(
            authorize_and_copy(&final_map),
            Err(DtbPageError::NotAcpiReclaim { page }) if page == pa + 0x1000
        ));
        assert!(owned().is_none());
        match phase() {
            Phase::Preflighted { dst, len, .. } => {
                // SAFETY: the reserved buffer is live until `reset`.
                let copy = unsafe { core::slice::from_raw_parts(dst, len) };
                assert!(
                    copy.iter().all(|&b| b == 0),
                    "a rejected final map must not leave a partial body"
                );
            }
            _ => panic!("a rejected final map must leave the stage preflighted"),
        }
    }

    #[test]
    fn the_final_map_may_segment_the_same_range_differently() {
        let _reset = ResetOnDrop;
        let blob = Blob::new(0x1100);
        blob.fill(HEADER_LEN, b'S');
        let pa = blob.pa();
        note(pa);
        assert_eq!(preflight(&[acpi(pa, 2)]), Ok(()));

        let final_map = [acpi(pa, 1), acpi(pa + 0x1000, 1)];
        let owned = authorize_and_copy(&final_map)
            .unwrap()
            .expect("segmentation does not change coverage");
        assert_eq!(owned.bytes, blob.as_slice());
    }

    #[test]
    fn the_header_is_read_once_and_the_body_once() {
        let _reset = ResetOnDrop;
        let blob = Blob::new(0x100);
        blob.fill(HEADER_LEN, b'A');
        let pa = blob.pa();
        note(pa);
        assert_eq!(preflight(&[acpi(pa, 1)]), Ok(()));
        let dst_before = match phase() {
            Phase::Preflighted { dst, .. } => dst,
            _ => panic!("preflight stage"),
        };
        let mut original_header = [0u8; HEADER_LEN];
        original_header.copy_from_slice(&blob.as_slice()[..HEADER_LEN]);

        // Firmware rewrites the source after the preflight snapshot: the
        // header must stay as captured, the body must show the new bytes.
        blob.set_total(0x200);
        blob.fill(HEADER_LEN, b'B');

        let owned = authorize_and_copy(&[acpi(pa, 1)]).unwrap().unwrap();
        assert_eq!(
            &owned.bytes[..HEADER_LEN],
            &original_header,
            "the header must not be re-read after preflight"
        );
        assert_eq!(
            owned.bytes.len(),
            0x100,
            "length comes from the preflight header"
        );
        assert!(
            owned.bytes[HEADER_LEN..].iter().all(|&b| b == b'B'),
            "the body must be read at copy time"
        );
        match phase() {
            Phase::Ready(o) => {
                assert_eq!(o.bytes.as_ptr(), dst_before, "the copy never reallocates")
            }
            _ => panic!("ready stage"),
        }
    }

    #[test]
    fn the_copy_target_overlap_check_uses_the_allocated_range() {
        let source = (0x1000, 0x3000);
        assert!(copy_target_overlaps(0x2000, 0x1000, source));
        assert!(copy_target_overlaps(0x0800, 0x1000, source));
        assert!(copy_target_overlaps(0x2800, 0x1000, source));
        assert!(
            !copy_target_overlaps(0x3000, 0x1000, source),
            "touching the source end is not an overlap"
        );
        assert!(
            !copy_target_overlaps(0x800, 0x800, source),
            "touching the source start is not an overlap"
        );
        assert!(!copy_target_overlaps(0x5000, 0x1000, source));
    }

    #[test]
    fn the_final_map_phase_allocates_nothing() {
        let _reset = ResetOnDrop;
        let blob = Blob::new(0x100);
        let pa = blob.pa();
        note(pa);
        preflight(&[acpi(pa, 1)]).unwrap();

        let before = crate::test_allocations();
        let owned = authorize_and_copy(&[acpi(pa, 1)]).unwrap().unwrap();
        let after = crate::test_allocations();
        assert_eq!(before, after, "the authorized copy must not allocate");
        assert_eq!(owned.bytes, blob.as_slice());
    }

    #[test]
    fn the_copy_is_reused_when_the_final_map_is_read_again() {
        let _reset = ResetOnDrop;
        let blob = Blob::new(0x100);
        let pa = blob.pa();
        note(pa);
        preflight(&[acpi(pa, 1)]).unwrap();
        let first = authorize_and_copy(&[acpi(pa, 1)]).unwrap().unwrap();

        // A retry snapshot that no longer accepts the source pages must not
        // re-open the copy: the bytes are already owned.
        let retry = authorize_and_copy(&[desc(ty::RESERVED, pa, 1)])
            .unwrap()
            .unwrap();
        assert_eq!(first.bytes.as_ptr(), retry.bytes.as_ptr());
        assert_eq!(first.bytes, retry.bytes);
    }

    #[test]
    #[should_panic(expected = "header preflight")]
    fn copy_without_preflight_is_fail_closed() {
        let _reset = ResetOnDrop;
        let blob = Blob::new(0x100);
        let pa = blob.pa();
        note(pa);
        let _ = authorize_and_copy(&[acpi(pa, 1)]);
    }

    #[test]
    fn an_absent_dtb_needs_no_copy() {
        let _reset = ResetOnDrop;
        assert!(!awaiting_preflight());
        assert!(authorize_and_copy(&[]).unwrap().is_none());
    }
}
