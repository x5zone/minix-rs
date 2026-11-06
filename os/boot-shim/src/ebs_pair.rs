//! GetMemoryMap → classify → ExitBootServices pairing loop (scheme B, T1).
//!
//! The loop runs while Boot Services are alive: it obtains the memory map into
//! a caller-owned buffer, classifies it into the handoff arena, and calls
//! `ExitBootServices` with the map key from *that* map. Only the
//! classification paired with a successful MapKey may survive, so a failed
//! EBS triggers a re-fetch + re-classification with the partitions reset.
//!
//! The firmware calls are injected as plain `extern "C"` function pointers
//! ([`RawOps`]) so the retry/buffer discipline is host-testable with a mock
//! firmware; the production glue in `uefi_helpers` passes the raw UEFI boot
//! services fields (identical calling convention on every supported 64-bit
//! target).
//!
//! Hard contracts enforced here (design §1.5 (6).4):
//! - retry only on `EFI_INVALID_PARAMETER`, total EBS attempts = 2;
//! - other statuses fail immediately (no blanket "MapKey expired" retry);
//! - the buffer can grow only *before* the first EBS attempt
//!   ([`PairError::NeedBiggerBuffer`]); after a failed EBS the retry reuses the
//!   same buffer and a too-small result is fail-closed
//!   ([`PairError::BufferReuseInsufficient`]);
//! - stale classification is cleared before every re-classification.

use crate::ebs_map::{Classification, MapError, MapStorage};

/// The pairing loop's own descriptor and side-list storage.
///
/// Reserved at compile time on purpose: allocating between reading a memory map
/// and exiting boot services is what can change the map being classified, so the
/// loop has to be able to work without the allocator at all.
static mut PAIR_STORAGE: MapStorage = MapStorage::new();
use core::ffi::c_void;
use minix_boot::Serializer;

/// UEFI `EFI_ERROR_BIT` (the high bit of `EFI_STATUS`; same value as
/// `uefi_raw::Status::ERROR_BIT`).
pub const EFI_ERROR_BIT: usize = 1 << (usize::BITS - 1);

/// `EFI_SUCCESS`.
pub const EFI_SUCCESS: usize = 0;
/// `EFI_INVALID_PARAMETER` (status = error bit | 2) — the only status that
/// triggers a retry.
pub const EFI_INVALID_PARAMETER: usize = EFI_ERROR_BIT | 2;
/// `EFI_BUFFER_TOO_SMALL` (status = error bit | 5) — the buffer size
/// out-parameter holds the need.
pub const EFI_BUFFER_TOO_SMALL: usize = EFI_ERROR_BIT | 5;

/// Total `ExitBootServices` attempts allowed (round-9: fixed at 2).
pub const EBS_ATTEMPT_LIMIT: u32 = 2;

/// `EFI_GET_MEMORY_MAP` prototype (C ABI; EFIAPI == C on supported targets).
pub type GetMemoryMapFn = unsafe extern "C" fn(
    boot_services: *mut c_void,
    memory_map_size: *mut usize,
    memory_map: *mut u8,
    map_key: *mut usize,
    descriptor_size: *mut usize,
    descriptor_version: *mut u32,
) -> usize;

/// `EFI_EXIT_BOOT_SERVICES` prototype.
pub type ExitBootServicesFn =
    unsafe extern "C" fn(boot_services: *mut c_void, image_handle: usize, map_key: usize) -> usize;

/// Injected firmware calls.
#[derive(Clone, Copy)]
pub struct RawOps {
    pub get_memory_map: GetMemoryMapFn,
    pub exit_boot_services: ExitBootServicesFn,
}

/// Pairing-loop failure. All variants are fail-closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairError {
    /// First map fetch did not fit: the caller may grow the buffer and call
    /// again (allowed because no EBS attempt has happened yet).
    NeedBiggerBuffer(usize),
    /// A post-EBS retry's re-fetch did not fit the reused buffer.
    BufferReuseInsufficient,
    /// `GetMemoryMap` returned a status other than success/too-small.
    GetMapStatus(usize),
    /// Descriptor validation or classification failed.
    Map(MapError),
    /// `ExitBootServices` returned a status other than success/INVALID_PARAMETER.
    ExitStatus(usize),
    /// Two EBS attempts were both rejected with `EFI_INVALID_PARAMETER`.
    RetryExhausted,
    /// The configuration-table DTB pages are not `EfiACPIReclaimMemory`, or a
    /// page carries a conflicting type (brief O-11).
    DtbPages(crate::dtb_copy::DtbPageError),
    /// The DTB's static reservations are not covered by EFI descriptors of
    /// the type their semantics require (brief O-11). The map paired with the
    /// key is real but contradicts the platform description, so the boot
    /// stops before `ExitBootServices` rather than accepting it.
    ReservationCoverage(crate::resv_claims::ClaimError),
}

/// Run the pairing loop over `map_buf`, dual-writing the accepted
/// classification into `serializer`.
///
/// `image_handle` is the value passed to `ExitBootServices`; `boot_services`
/// is the opaque `EFI_BOOT_SERVICES*` handed back to the injected calls.
///
/// # Safety
///
/// `ops` must be the live firmware `GetMemoryMap`/`ExitBootServices` entries
/// matching the injected calling convention, `boot_services` must be the boot
/// services table those entries belong to, and `map_buf` must be writable by
/// `GetMemoryMap` for `map_buf.len()` bytes. Boot is single-threaded, so no
/// concurrent access to the buffer or the firmware tables is possible.
pub unsafe fn pair_and_classify(
    ops: &RawOps,
    boot_services: *mut c_void,
    image_handle: usize,
    map_buf: &mut [u8],
    pa_limit: u64,
    serializer: &mut Serializer<'_>,
) -> Result<Classification, PairError> {
    let mut ebs_attempts = 0u32;
    let mut ebs_failed = false;
    // Snapshot instance identity: one increment per successful GetMemoryMap.
    // This is *not* the MapKey — the key is firmware's version credential for
    // ExitBootServices, while the snapshot id names the observation. Two reads
    // returning the same key are still two snapshots.
    let mut snapshot_seq = 0u64;

    loop {
        let mut map_size = map_buf.len();
        let mut map_key = 0usize;
        let mut descriptor_size = 0usize;
        let mut descriptor_version = 0u32;
        // SAFETY: `ops.get_memory_map` is the firmware entry point; the
        // buffer and out-parameters are valid for the call's duration and no
        // other code touches them concurrently (single-threaded boot).
        let status = unsafe {
            (ops.get_memory_map)(
                boot_services,
                &mut map_size,
                map_buf.as_mut_ptr(),
                &mut map_key,
                &mut descriptor_size,
                &mut descriptor_version,
            )
        };
        if status == EFI_BUFFER_TOO_SMALL {
            return Err(if ebs_failed {
                PairError::BufferReuseInsufficient
            } else {
                PairError::NeedBiggerBuffer(map_size)
            });
        }
        if status != EFI_SUCCESS {
            return Err(PairError::GetMapStatus(status));
        }

        // Only the map paired with the current MapKey may be serialized.
        serializer.reset_map_partitions();
        // SAFETY: boot is single-threaded and this loop is the storage's only
        // holder.
        let storage = unsafe { &mut *core::ptr::addr_of_mut!(PAIR_STORAGE) };
        storage
            .parse_raw_map(
                map_buf,
                map_size,
                descriptor_size,
                descriptor_version,
                {
                    snapshot_seq += 1;
                    snapshot_seq
                },
            )
            .map_err(PairError::Map)?;
        let classification = storage
            .classify_into_handoff(pa_limit, serializer)
            .map_err(PairError::Map)?;
        // O-11: static `/reserved-memory` and MRB claims must be covered by
        // descriptors of the type their semantics require. The claims were
        // captured from the configuration-table DTB before this loop; checking
        // them before `ExitBootServices` keeps the failure while firmware
        // services can still print.
        // O-11: authorize the DTB pages against this snapshot, copy the body
        // once, then build the reservation claims from the copy.
        if let Some(owned) = crate::dtb_copy::authorize_and_copy(storage.descriptors())
            .map_err(PairError::DtbPages)?
        {
            crate::resv_claims::capture(owned.bytes, owned.source);
        }
        crate::resv_claims::check_current(storage.descriptors())
            .map_err(PairError::ReservationCoverage)?;

        // SAFETY: same contract as above; `map_key` comes from the map that
        // was just classified.
        let status = unsafe { (ops.exit_boot_services)(boot_services, image_handle, map_key) };
        if status == EFI_SUCCESS {
            // Reported only now: between reading the map and exiting boot
            // services the loop must not touch anything that could perturb the
            // very map it classified. Ranges that policy kept out of both
            // ordinary tables are invisible to the kernel by design, so name
            // their count here rather than leaving the omission to be inferred.
            if classification.side != 0 {
                uefi::println!(
                    "boot-shim: memory map left {} range(s) out of both ordinary tables",
                    classification.side
                );
            }
            return Ok(classification);
        }
        if status == EFI_INVALID_PARAMETER {
            ebs_attempts += 1;
            if ebs_attempts >= EBS_ATTEMPT_LIMIT {
                return Err(PairError::RetryExhausted);
            }
            ebs_failed = true;
            continue;
        }
        return Err(PairError::ExitStatus(status));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_void;
    use minix_boot::{ARENA_CAP, Serializer};

    const CONV: u32 = 7;
    const LOADER: u32 = 4;
    const KEY: usize = 0x1234;

    /// Mock firmware state; `boot_services` points at it.
    struct MockFw {
        /// Two map images: attempt 0 and attempt 1.
        maps: [[u8; 160]; 2],
        map_lens: [usize; 2],
        desc_size: usize,
        version: u32,
        get_need: [usize; 2],
        ebs_status: [usize; 2],
        get_calls: usize,
        ebs_calls: usize,
        last_key: usize,
    }

    impl MockFw {
        fn new(descs0: &[(u32, u64, u64)], descs1: &[(u32, u64, u64)]) -> Self {
            let mut fw = Self {
                maps: [[0u8; 160]; 2],
                map_lens: [0; 2],
                desc_size: 40,
                version: 1,
                get_need: [0; 2],
                ebs_status: [0; 2],
                get_calls: 0,
                ebs_calls: 0,
                last_key: 0,
            };
            for (img, descs) in [descs0, descs1].into_iter().enumerate() {
                for (i, &(ty, phys, pages)) in descs.iter().enumerate() {
                    let off = i * 40;
                    fw.maps[img][off..off + 4].copy_from_slice(&ty.to_le_bytes());
                    fw.maps[img][off + 8..off + 16].copy_from_slice(&phys.to_le_bytes());
                    fw.maps[img][off + 24..off + 32].copy_from_slice(&pages.to_le_bytes());
                    // Real firmware declares the cache capability of a usable
                    // range, and a descriptor that asserts none is not eligible
                    // for an ordinary write-back mapping. The fixtures stand for
                    // usable memory, so they say so.
                    fw.maps[img][off + 32..off + 40].copy_from_slice(&0x8u64.to_le_bytes());
                }
                fw.map_lens[img] = descs.len() * 40;
            }
            fw
        }
    }

    unsafe extern "C" fn mock_get(
        bs: *mut c_void,
        map_size: *mut usize,
        map: *mut u8,
        map_key: *mut usize,
        desc_size: *mut usize,
        version: *mut u32,
    ) -> usize {
        // SAFETY: `bs` is a live `MockFw` in these tests.
        let fw = unsafe { &mut *(bs as *mut MockFw) };
        let attempt = fw.get_calls.min(1);
        fw.get_calls += 1;
        if fw.get_need[attempt] != 0 {
            unsafe { *map_size = fw.get_need[attempt] };
            return EFI_BUFFER_TOO_SMALL;
        }
        let len = fw.map_lens[attempt];
        if unsafe { *map_size } < len {
            unsafe { *map_size = len };
            return EFI_BUFFER_TOO_SMALL;
        }
        // SAFETY: caller provides a buffer of at least `*map_size` bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(fw.maps[attempt].as_ptr(), map, len);
            *map_size = len;
            *map_key = KEY + attempt;
            *desc_size = fw.desc_size;
            *version = fw.version;
        }
        fw.last_key = KEY + attempt;
        EFI_SUCCESS
    }

    unsafe extern "C" fn mock_ebs(bs: *mut c_void, _image: usize, map_key: usize) -> usize {
        // SAFETY: `bs` is a live `MockFw` in these tests.
        let fw = unsafe { &mut *(bs as *mut MockFw) };
        let attempt = fw.ebs_calls.min(1);
        fw.ebs_calls += 1;
        assert_eq!(map_key, fw.last_key, "EBS must use the current map key");
        fw.ebs_status[attempt]
    }

    fn ops() -> RawOps {
        RawOps {
            get_memory_map: mock_get,
            exit_boot_services: mock_ebs,
        }
    }

    /// Minimal envelope so the receiver-side decoders accept the arena; the
    /// pairing loop only owns the map partitions and the source records.
    fn write_min_envelope(s: &mut Serializer<'_>) {
        use minix_boot::KernelInfo;
        use minix_types::{PhysBytes, VirBytes};

        let carrier = KernelInfo {
            memmap: &[],
            reserved_regions: &[],
            memmap_sources: &[],
            startup_objects: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x20_0000),
            kern_size: 0x20_0000,
            free_upper_idx: None,
            user_sp: VirBytes(0x8000_0000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        };
        s.write_envelope(&carrier).unwrap();
    }

    fn run(fw: &mut MockFw) -> Result<Classification, PairError> {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let mut map_buf = [0u8; 256];
        // SAFETY: the mock functions expect `fw` behind the `bs` pointer.
        unsafe {
            pair_and_classify(
                &ops(),
                fw as *mut MockFw as *mut c_void,
                0,
                &mut map_buf,
                1 << 48,
                &mut s,
            )
        }
    }

    #[test]
    fn status_constants_use_the_uefi_error_bit_encoding() {
        // EFI_STATUS is `error_bit | code`; the low-byte-only values (2/5)
        // would never match real firmware statuses.
        assert_eq!(EFI_SUCCESS, 0);
        assert_eq!(EFI_INVALID_PARAMETER & !EFI_ERROR_BIT, 2);
        assert_eq!(EFI_BUFFER_TOO_SMALL & !EFI_ERROR_BIT, 5);
        assert_ne!(EFI_INVALID_PARAMETER, 2);
        assert_ne!(EFI_BUFFER_TOO_SMALL, 5);
        assert_eq!(EFI_ERROR_BIT, 1 << (usize::BITS - 1));
    }

    #[test]
    fn success_on_first_attempt_classifies_and_exits() {
        let mut fw = MockFw::new(
            &[
                (CONV, 0x10_0000, 2),
                (CONV, 0x40_0000, 1),
                (LOADER, 0x20_0000, 1),
            ],
            &[],
        );
        let c = run(&mut fw).unwrap();
        assert_eq!(
            c,
            Classification {
                conventional: 2,
                reserved: 1,
                side: 0,
            }
        );
        assert_eq!(fw.get_calls, 1);
        assert_eq!(fw.ebs_calls, 1);
    }

    /// The UEFI leg's per-descriptor provenance: one record per emitted
    /// ordinary-table entry, origin = the descriptor index in the accepted
    /// snapshot, range = exactly the emitted region. The records must explain
    /// both projections — the same audit the kernel runs at its consumer
    /// boundary.
    #[test]
    fn source_records_cover_the_classified_map_per_descriptor() {
        use minix_boot::{
            MemoryRegion, PhysicalRange, SourceKind, SourceOrigin, SourceRecord, WIRE_SOURCES_CAP,
            audit_source_coverage, decode_envelope, decode_memmap, decode_reserved_regions,
            decode_sources,
        };
        use minix_types::PhysBytes;

        let mut fw = MockFw::new(
            &[
                (CONV, 0x10_0000, 2),
                (LOADER, 0x20_0000, 1),
                (CONV, 0x40_0000, 1),
            ],
            &[],
        );
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        write_min_envelope(&mut s);
        let mut map_buf = [0u8; 256];
        // SAFETY: the mock functions expect `fw` behind the `bs` pointer.
        let c = unsafe {
            pair_and_classify(
                &ops(),
                &mut fw as *mut MockFw as *mut c_void,
                0,
                &mut map_buf,
                1 << 48,
                &mut s,
            )
        }
        .unwrap();
        assert_eq!((c.conventional, c.reserved), (2, 1));

        s.patch_runtime_fields(minix_boot::RuntimeFields {
            root_page: 0x1000,
            bump_base: 0x20_0000,
            bump_next: 0x20_0000,
            bump_end: 0x30_0000,
        });
        let scalars = decode_envelope(s.as_bytes()).unwrap();
        let empty = MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        };
        let mut free = [empty; 4];
        let nf = decode_memmap(s.as_bytes(), &scalars, &mut free).unwrap();
        let mut reserved = [empty; 4];
        let nr = decode_reserved_regions(s.as_bytes(), &scalars, &mut reserved).unwrap();
        let mut sources = [SourceRecord {
            input_object: minix_boot::memmap_source::InputObjectId::DtbBootInput,
            address_domain: minix_boot::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 0,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MrbEntry(0),
            range: PhysicalRange::new(0, 0),
        }; WIRE_SOURCES_CAP];
        let ns = decode_sources(s.as_bytes(), &scalars, &mut sources).unwrap();

        assert_eq!(ns, c.conventional + c.reserved);
        assert_eq!(
            sources[0].origin,
            SourceOrigin::UefiDescriptor { index: 0 },
            "record 0 must name descriptor 0"
        );
        assert_eq!(sources[0].kind, SourceKind::Memory);
        assert_eq!(sources[1].origin, SourceOrigin::UefiDescriptor { index: 1 });
        assert_eq!(sources[1].kind, SourceKind::ReservedOrdinary);
        assert_eq!(sources[2].origin, SourceOrigin::UefiDescriptor { index: 2 });
        assert_eq!(sources[2].kind, SourceKind::Memory);
        audit_source_coverage(&free[..nf], &reserved[..nr], &sources[..ns])
            .expect("per-descriptor records must explain both projections");
    }

    /// Every structurally valid descriptor gets a record — including the ones
    /// the classifier keeps out of both ordinary tables — and the record keeps
    /// the firmware's own Type. The snapshot id is the observation sequence,
    /// not the MapKey.
    #[test]
    fn excluded_descriptors_keep_their_provenance_and_snapshot_id_is_not_the_map_key() {
        use minix_boot::{
            audit_source_coverage, decode_envelope, decode_memmap, decode_sources, MemoryRegion,
            PhysicalRange, SourceKind, SourceOrigin, SourceRecord, WIRE_SOURCES_CAP,
        };
        use minix_types::PhysBytes;

        // RESERVED is structurally valid but produces no ordinary-table entry.
        let mut fw = MockFw::new(
            &[(CONV, 0x10_0000, 1), (0u32, 0x30_0000, 1)],
            &[],
        );
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        write_min_envelope(&mut s);
        let mut map_buf = [0u8; 256];
        // SAFETY: the mock functions expect `fw` behind the `bs` pointer.
        let c = unsafe {
            pair_and_classify(
                &ops(),
                &mut fw as *mut MockFw as *mut c_void,
                0,
                &mut map_buf,
                1 << 48,
                &mut s,
            )
        }
        .unwrap();
        assert_eq!((c.conventional, c.reserved, c.side), (1, 0, 0));

        s.patch_runtime_fields(minix_boot::RuntimeFields {
            root_page: 0x1000,
            bump_base: 0x20_0000,
            bump_next: 0x20_0000,
            bump_end: 0x30_0000,
        });
        let scalars = decode_envelope(s.as_bytes()).unwrap();
        let empty = MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        };
        let mut free = [empty; 4];
        let nf = decode_memmap(s.as_bytes(), &scalars, &mut free).unwrap();
        let mut sources = [SourceRecord {
            input_object: minix_boot::InputObjectId::DtbBootInput,
            address_domain: minix_boot::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 0,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MrbEntry(0),
            range: PhysicalRange::new(0, 0),
        }; WIRE_SOURCES_CAP];
        let ns = decode_sources(s.as_bytes(), &scalars, &mut sources).unwrap();
        assert_eq!(ns, 2, "one record per descriptor, not per emitted range");
        assert_eq!(sources[0].kind, SourceKind::Memory);
        assert_eq!(sources[1].kind, SourceKind::UefiExcluded);
        assert_eq!(sources[1].origin, SourceOrigin::UefiDescriptor { index: 1 });
        assert_eq!(sources[1].raw_type, 0, "RESERVED is EFI type 0");
        assert_eq!(sources[1].range.start, 0x30_0000);
        assert_eq!(
            sources[1].snapshot_id, 1,
            "snapshot identity is the observation sequence, not MapKey {KEY:#x}"
        );
        assert!(
            audit_source_coverage(&free[..nf], &[], &sources[..ns]).is_ok(),
            "excluded records must not disturb free-span coverage"
        );
    }

    /// A rejected `ExitBootServices` re-classifies a fresh map; the source
    /// partition must carry only the accepted snapshot's records, never the
    /// stale ones from the rejected attempt.
    #[test]
    fn source_partition_resets_between_ebs_attempts() {
        use minix_boot::{
            PhysicalRange, SourceKind, SourceOrigin, SourceRecord, WIRE_SOURCES_CAP,
            decode_envelope, decode_sources,
        };

        let mut fw = MockFw::new(
            &[(CONV, 0x10_0000, 4), (CONV, 0x20_0000, 4)],
            &[(LOADER, 0x30_0000, 1)],
        );
        let mut fw = MockFw::new(
            &[(CONV, 0x10_0000, 4), (CONV, 0x20_0000, 4)],
            &[(LOADER, 0x30_0000, 1)],
        );
        fw.ebs_status = [EFI_INVALID_PARAMETER, EFI_SUCCESS];
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        write_min_envelope(&mut s);
        let mut map_buf = [0u8; 256];
        // SAFETY: the mock functions expect `fw` behind the `bs` pointer.
        let c = unsafe {
            pair_and_classify(
                &ops(),
                &mut fw as *mut MockFw as *mut c_void,
                0,
                &mut map_buf,
                1 << 48,
                &mut s,
            )
        }
        .unwrap();
        assert_eq!((c.conventional, c.reserved), (0, 1), "second map only");

        s.patch_runtime_fields(minix_boot::RuntimeFields {
            root_page: 0x1000,
            bump_base: 0x20_0000,
            bump_next: 0x20_0000,
            bump_end: 0x30_0000,
        });
        let scalars = decode_envelope(s.as_bytes()).unwrap();
        let mut sources = [SourceRecord {
            input_object: minix_boot::memmap_source::InputObjectId::DtbBootInput,
            address_domain: minix_boot::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 0,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MrbEntry(0),
            range: PhysicalRange::new(0, 0),
        }; WIRE_SOURCES_CAP];
        let ns = decode_sources(s.as_bytes(), &scalars, &mut sources).unwrap();
        assert_eq!(ns, 1, "the rejected attempt's records must be cleared");
        assert_eq!(sources[0].origin, SourceOrigin::UefiDescriptor { index: 0 });
        assert_eq!(sources[0].kind, SourceKind::ReservedOrdinary);
        assert_eq!(sources[0].range.start, 0x30_0000);
    }

    #[test]
    fn buffer_too_small_before_ebs_asks_caller_to_grow() {
        let mut fw = MockFw::new(&[(CONV, 0x10_0000, 1)], &[]);
        fw.get_need[0] = 4096;
        assert_eq!(run(&mut fw), Err(PairError::NeedBiggerBuffer(4096)));
        assert_eq!(fw.ebs_calls, 0, "no EBS attempt before the map fits");
    }

    #[test]
    fn invalid_parameter_retries_once_and_uses_the_new_map() {
        // First map: 2 conventional; second map: 1 conventional above a
        // loader-data hole (so the hole sits below the RAM top and becomes a
        // reserved entry).
        let mut fw = MockFw::new(
            &[(CONV, 0x10_0000, 1), (CONV, 0x20_0000, 1)],
            &[(LOADER, 0x18_0000, 1), (CONV, 0x20_0000, 1)],
        );
        fw.ebs_status = [EFI_INVALID_PARAMETER, EFI_SUCCESS];
        let c = run(&mut fw).unwrap();
        assert_eq!(
            c,
            Classification {
                conventional: 1,
                reserved: 1,
                side: 0,
            },
            "the accepted classification must come from the second map"
        );
        assert_eq!(fw.get_calls, 2);
        assert_eq!(fw.ebs_calls, 2);
    }

    #[test]
    fn two_invalid_parameters_exhaust_the_retry_budget() {
        let mut fw = MockFw::new(&[(CONV, 0x10_0000, 1)], &[(CONV, 0x10_0000, 1)]);
        fw.ebs_status = [EFI_INVALID_PARAMETER, EFI_INVALID_PARAMETER];
        assert_eq!(run(&mut fw), Err(PairError::RetryExhausted));
        assert_eq!(fw.get_calls, 2);
        assert_eq!(fw.ebs_calls, 2);
    }

    #[test]
    fn other_ebs_status_fails_immediately() {
        let mut fw = MockFw::new(&[(CONV, 0x10_0000, 1)], &[]);
        fw.ebs_status = [3, 0];
        assert_eq!(run(&mut fw), Err(PairError::ExitStatus(3)));
        assert_eq!(fw.get_calls, 1);
        assert_eq!(fw.ebs_calls, 1);
    }

    #[test]
    fn too_small_on_retry_is_fail_closed() {
        let mut fw = MockFw::new(&[(CONV, 0x10_0000, 1)], &[]);
        fw.ebs_status = [EFI_INVALID_PARAMETER, EFI_SUCCESS];
        fw.get_need[1] = 8192;
        assert_eq!(run(&mut fw), Err(PairError::BufferReuseInsufficient));
        assert_eq!(fw.ebs_calls, 1, "only the first EBS attempt happened");
    }

    #[test]
    fn classification_error_prevents_ebs() {
        let mut fw = MockFw::new(&[(CONV, u64::MAX - 0x1000, 2)], &[]);
        assert!(matches!(run(&mut fw), Err(PairError::Map(_))));
        assert_eq!(fw.ebs_calls, 0);
    }

    #[test]
    fn retry_resets_stale_partition_entries() {
        // First map fills 3 conventional; the retry's map has 1 conventional.
        // The descriptor after the retry must reflect the second map only.
        let mut fw = MockFw::new(
            &[
                (CONV, 0x10_0000, 1),
                (CONV, 0x20_0000, 1),
                (CONV, 0x30_0000, 1),
            ],
            &[(CONV, 0x10_0000, 1)],
        );
        fw.ebs_status = [EFI_INVALID_PARAMETER, EFI_SUCCESS];
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        let mut map_buf = [0u8; 256];
        let c = unsafe {
            pair_and_classify(
                &ops(),
                &mut fw as *mut MockFw as *mut c_void,
                0,
                &mut map_buf,
                1 << 48,
                &mut s,
            )
        }
        .unwrap();
        assert_eq!(c.conventional, 1);
        let get_u64 =
            |off: usize| u64::from_le_bytes(s.as_bytes()[off..off + 8].try_into().unwrap());
        assert_eq!(get_u64(128), 16, "descriptor must not accumulate entries");
    }

    /// The production sender writes the envelope and the non-map payloads
    /// before entering the pairing loop; classification must then fill only
    /// the map partitions and finish both descriptors. This pins the order the
    /// aarch64 `prepare_and_enter_kernel_image` relies on.
    #[test]
    fn producer_sequence_survives_the_pairing_loop() {
        use minix_boot::{
            BootModule, DTB, KernelInfo, PlatformDescSource, RuntimeFields, decode_descriptors,
            decode_envelope, patch_runtime_fields_in,
        };
        use minix_types::{PhysBytes, USER_STACK_TOP, VirBytes};

        static MODULES: [BootModule; 1] = [BootModule {
            name: "vm",
            start: PhysBytes(0x20_0000),
            len: 0x1000,
        }];
        static SOURCES: [PlatformDescSource; 1] =
            [PlatformDescSource::new(DTB, PhysBytes(0x4000_0000))];

        let carrier = KernelInfo {
            memmap: &[],
            reserved_regions: &[],
            memmap_sources: &[],
            startup_objects: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x20_0000),
            kern_size: 0x20_0000,
            free_upper_idx: None,
            user_sp: VirBytes(USER_STACK_TOP as u64),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &MODULES,
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &SOURCES,
            param_buf: b"ARCH=test\0",
        };
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        s.write_envelope(&carrier).unwrap();
        s.write_boot_modules(&MODULES).unwrap();
        s.write_platform_sources(&SOURCES).unwrap();
        s.write_param_buf(carrier.param_buf).unwrap();

        let mut fw = MockFw::new(&[(CONV, 0x10_0000, 0x200), (LOADER, 0x20_0000, 4)], &[]);
        let mut map_buf = [0u8; 256];
        let c = unsafe {
            pair_and_classify(
                &ops(),
                &mut fw as *mut MockFw as *mut c_void,
                0,
                &mut map_buf,
                1 << 48,
                &mut s,
            )
        }
        .unwrap();
        assert_eq!(c.conventional, 1);
        assert_eq!(c.reserved, 1);

        let d = decode_descriptors(s.as_bytes()).unwrap();
        assert_eq!(d.memmap_byte_len, 16);
        assert_eq!(d.reserved_byte_len, 16);
        assert_eq!(d.modules_byte_len, 32);
        assert_eq!(d.platform_byte_len, 16);
        assert_eq!(d.param_byte_len, 10);
        drop(s);

        // The runtime fields are patched after the root build; the full
        // envelope read then succeeds with the same descriptors.
        patch_runtime_fields_in(
            &mut arena,
            RuntimeFields {
                root_page: 0x1000,
                bump_base: 0x20_0000,
                bump_next: 0x21_0000,
                bump_end: 0x30_0000,
            },
        );
        let scalars = decode_envelope(&arena).unwrap();
        assert_eq!(scalars.memmap_byte_len, 16);
        assert_eq!(scalars.reserved_byte_len, 16);
        assert_eq!(scalars.param_byte_len, 10);
        assert_eq!(scalars.kern_virt_base, 0xFFFF_8000_0000_0000);
    }
}
