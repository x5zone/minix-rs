//! `sdev` — socket-driver dialogue: suspend records, revival groups, and
//! the driver-death stop plan, over the shared wire vocabulary.
//!
//! Corresponds to the caller-owned half of Minix3's `sdev.c:1-1114`
//! (`sdev_suspend`, `sdev_finish`, `sdev_stop`, and the grant bookkeeping
//! of `sdev_readwrite`). The wire vocabulary itself — request numbers,
//! operation/reply classification, flag packing, the error-to-errno map,
//! reply routing — lives in `minix-sockdriver` (`edge E-SDEVOWN` single
//! point); this module keeps only what reads VFS-owned tables: the fproc
//! call/aux enums and the smap.
//!
//! Design decisions (see 22-sdev.md §3):
//! - `suspend_aux` validates suspend records, reusing 02's `SdevAux`
//! - `trio_access` delegates direction bits to the cdev cross
//! - `finish_kind` routes revival into simple/recv/accept groups
//! - `stop` fails a suspended call with `EIO` when the driver dies
//!
//! Scope note: transports (`asynsend3`), waiting, revival execution, and
//! the upper socket layer (24) stay outside; select replies stay with 23.

use minix_sockdriver::sdev::SdevError;
use minix_types::{DevId, Endpoint, VirBytes, EIO};

use crate::cdev::grant_dir;
use crate::device_map::{smap_endpt_by_dev, SmapTable};
use crate::fproc::{SdevAux, SdevCall};

/// Validate a suspend record (`sdev_suspend:93-107`), reusing 02's
/// [`SdevAux`] instead of redefining the three shapes.
///
/// `fd == -1` means "no fd"; `buf == 0` means "no buffer".
pub fn suspend_aux(call: SdevCall, fd: i32, buf: VirBytes) -> Result<SdevAux, SdevError> {
    match call {
        SdevCall::Accept => {
            if fd != -1 && buf == VirBytes::new(0) {
                Ok(SdevAux::Fd(fd as usize))
            } else {
                Err(SdevError::Inval)
            }
        }
        SdevCall::Recvmsg => {
            if fd == -1 {
                Ok(SdevAux::Buf(buf))
            } else {
                Err(SdevError::Inval)
            }
        }
        _ => {
            if fd == -1 && buf == VirBytes::new(0) {
                Ok(SdevAux::None)
            } else {
                Err(SdevError::Inval)
            }
        }
    }
}

/// Grant access bits for one direction (delegates to the shared cross).
pub fn trio_access(is_read: bool) -> u32 {
    grant_dir(is_read)
}

/// Revival group for `sdev_finish` (`sdev.c:785-892`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishGroup {
    /// `SDEV_REPLY` status-only calls (bind/connect/write/send*/ioctl/close).
    Simple,
    /// `SDEV_RECV_REPLY` calls (read/recvfrom/recvmsg).
    Recv,
    /// `SDEV_ACCEPT_REPLY` calls (accept, failed path only here).
    Accept,
    /// Anything else (C panics; hardens to `EIO`).
    Unknown,
}

/// Route one suspended call to its revival group.
pub fn finish_kind(call: SdevCall) -> FinishGroup {
    match call {
        SdevCall::Bind
        | SdevCall::Connect
        | SdevCall::Write
        | SdevCall::Sendto
        | SdevCall::Sendmsg
        | SdevCall::Ioctl
        | SdevCall::Close => FinishGroup::Simple,
        SdevCall::Read | SdevCall::Recvfrom | SdevCall::Recvmsg => FinishGroup::Recv,
        SdevCall::Accept => FinishGroup::Accept,
    }
}

/// Stop plan for a vanished socket driver (`sdev_stop:910-925`).
///
/// The blocked-on state clears and the suspended call finishes with `EIO`
/// as the reply type — one convention shared with worker-thread stopping,
/// and the in-flight twin of select's death wake (`unsuspend_hit` marks the
/// fd ready so the *next* call surfaces the error; `stop` fails the call
/// that is already suspended).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopPlan {
    /// Revival group — same routing as a live reply (`sdev_finish`).
    pub group: FinishGroup,
    /// The reply value: always `EIO` for a driver death (`sdev.c:921-923`).
    pub reply: i32,
}

/// Pure stop decision for one suspended socket call.
pub fn stop(call: SdevCall) -> StopPlan {
    StopPlan {
        group: finish_kind(call),
        reply: EIO,
    }
}

/// Whether a suspended socket slot belongs to the dying driver
/// (`pipe.c:347-350`'s match): the smap row for the suspended device must
/// exist and be owned by the vanished endpoint.
pub fn stop_matches(dev: DevId, smap_table: &SmapTable, dead: Endpoint) -> bool {
    smap_endpt_by_dev(smap_table, dev) == Some(dead)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_plans_eio_by_group() {
        // Driver death finishes every suspended call with `EIO`, routed by
        // the same revival groups as a live reply (`sdev.c:921-925`).
        assert_eq!(
            stop(SdevCall::Read),
            StopPlan {
                group: FinishGroup::Recv,
                reply: minix_types::EIO,
            }
        );
        assert_eq!(
            stop(SdevCall::Accept),
            StopPlan {
                group: FinishGroup::Accept,
                reply: minix_types::EIO,
            }
        );
        assert_eq!(
            stop(SdevCall::Bind),
            StopPlan {
                group: FinishGroup::Simple,
                reply: minix_types::EIO,
            }
        );
    }

    #[test]
    fn test_stop_matches_smap_row() {
        // Only slots whose suspended device belongs to the vanished driver
        // stop (`pipe.c:347-350`); free rows never match.
        let mut stbl = SmapTable::default();
        stbl.entries[0].endpt = Some(Endpoint::from_generation_slot(0, 9));
        let dev = crate::device_map::make_smap_dev(1, 7);
        assert!(stop_matches(dev, &stbl, Endpoint::from_generation_slot(0, 9)));
        assert!(!stop_matches(dev, &stbl, Endpoint::from_generation_slot(0, 8)));
        assert!(!stop_matches(
            crate::device_map::make_smap_dev(2, 7),
            &stbl,
            Endpoint::from_generation_slot(0, 8)
        ));
    }

    #[test]
    fn test_suspend_aux_shapes() {
        // Accept wants an fd, recvmsg a buffer, the rest neither.
        assert_eq!(
            suspend_aux(SdevCall::Accept, 3, VirBytes::new(0)).unwrap(),
            SdevAux::Fd(3)
        );
        assert_eq!(
            suspend_aux(SdevCall::Recvmsg, -1, VirBytes::new(0x800)).unwrap(),
            SdevAux::Buf(VirBytes::new(0x800))
        );
        assert_eq!(
            suspend_aux(SdevCall::Bind, -1, VirBytes::new(0)).unwrap(),
            SdevAux::None
        );
        // Shape violations refuse (`sdev_suspend:93-107` asserts).
        assert_eq!(
            suspend_aux(SdevCall::Accept, -1, VirBytes::new(0)).unwrap_err(),
            SdevError::Inval
        );
        assert_eq!(
            suspend_aux(SdevCall::Recvmsg, 3, VirBytes::new(0)).unwrap_err(),
            SdevError::Inval
        );
        assert_eq!(
            suspend_aux(SdevCall::Bind, 3, VirBytes::new(0)).unwrap_err(),
            SdevError::Inval
        );
        assert_eq!(
            suspend_aux(SdevCall::Bind, -1, VirBytes::new(8)).unwrap_err(),
            SdevError::Inval
        );
        assert_eq!(SdevError::Inval.to_errno(), minix_types::EINVAL);
    }

    #[test]
    fn test_finish_groups() {
        // Simple group: seven calls share SDEV_REPLY handling.
        for call in [
            SdevCall::Bind,
            SdevCall::Connect,
            SdevCall::Write,
            SdevCall::Sendto,
            SdevCall::Sendmsg,
            SdevCall::Ioctl,
            SdevCall::Close,
        ] {
            assert_eq!(finish_kind(call), FinishGroup::Simple, "{call:?}");
        }
        for call in [SdevCall::Read, SdevCall::Recvfrom, SdevCall::Recvmsg] {
            assert_eq!(finish_kind(call), FinishGroup::Recv, "{call:?}");
        }
        assert_eq!(finish_kind(SdevCall::Accept), FinishGroup::Accept);
    }

    #[test]
    fn test_trio_access_delegates_to_cdev_cross() {
        assert_eq!(trio_access(false), crate::cdev::grant_dir(false));
        assert_eq!(trio_access(true), crate::cdev::grant_dir(true));
    }
}
