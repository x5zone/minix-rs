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
//! Hard contracts enforced here (design §1.5 (六).4):
//! - retry only on `EFI_INVALID_PARAMETER`, total EBS attempts = 2;
//! - other statuses fail immediately (no blanket "MapKey expired" retry);
//! - the buffer can grow only *before* the first EBS attempt
//!   ([`PairError::NeedBiggerBuffer`]); after a failed EBS the retry reuses the
//!   same buffer and a too-small result is fail-closed
//!   ([`PairError::BufferReuseInsufficient`]);
//! - stale classification is cleared before every re-classification.

use crate::ebs_map::{classify_map, Classification, MapError};
use core::ffi::c_void;
use minix_boot::Serializer;

/// `EFI_SUCCESS`.
pub const EFI_SUCCESS: usize = 0;
/// `EFI_INVALID_PARAMETER` — the only status that triggers a retry.
pub const EFI_INVALID_PARAMETER: usize = 2;
/// `EFI_BUFFER_TOO_SMALL` — the buffer size out-parameter holds the need.
pub const EFI_BUFFER_TOO_SMALL: usize = 5;

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
        let classification = classify_map(
            map_buf,
            map_size,
            descriptor_size,
            descriptor_version,
            pa_limit,
            serializer,
        )
        .map_err(PairError::Map)?;

        // SAFETY: same contract as above; `map_key` comes from the map that
        // was just classified.
        let status = unsafe { (ops.exit_boot_services)(boot_services, image_handle, map_key) };
        if status == EFI_SUCCESS {
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
    use minix_boot::{Serializer, ARENA_CAP};
    use core::ffi::c_void;

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
    fn success_on_first_attempt_classifies_and_exits() {
        let mut fw = MockFw::new(
            &[(CONV, 0x10_0000, 2), (CONV, 0x40_0000, 1), (LOADER, 0x20_0000, 1)],
            &[],
        );
        let c = run(&mut fw).unwrap();
        assert_eq!(
            c,
            Classification {
                conventional: 2,
                reserved: 1
            }
        );
        assert_eq!(fw.get_calls, 1);
        assert_eq!(fw.ebs_calls, 1);
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
                reserved: 1
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
            &[(CONV, 0x10_0000, 1), (CONV, 0x20_0000, 1), (CONV, 0x30_0000, 1)],
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
        let get_u64 = |off: usize| {
            u64::from_le_bytes(s.as_bytes()[off..off + 8].try_into().unwrap())
        };
        assert_eq!(get_u64(128), 16, "descriptor must not accumulate entries");
    }
}
