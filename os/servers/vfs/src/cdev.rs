//! `cdev` — character-device dialogue: redirection, open/close, I/O, cancel.
//!
//! Corresponds to Minix3's `cdev.c:1-508` (`cdev_map`, `cdev_get`,
//! `cdev_clone`, `cdev_opcl`, `cdev_open`, `cdev_close`, `cdev_io`,
//! `cdev_select`, `cdev_cancel`, `cdev_generic_reply`, `cdev_reply`).
//!
//! Design decisions (see 21-cdev.md §3):
//! - `tty_redirect` is a pure rerouting function over an abstract TTY source
//! - `resolve_gate` converges map+lookup+endpoint-check into one gate
//! - `access_bits` combines the open access bits as a pure function
//! - `noctty_force` types the three-way NOCTTY rule
//! - `open_effects` decodes clone/TTY post-open effects from status words
//! - grant direction reuses 19's cross semantics (read pairs with write)
//! - replies classify into five kinds; EAGAIN/EINTR swap both ways
//!
//! Scope note: `CTTY_MAJOR` is reused from 19-device-map.md (single source);
//! `NO_DEV` semantics (`None` = absent) mirror 18-mount.md. Transport
//! (`asynsend3`), waiting (`worker_wait`/`suspend`), and revival stay with
//! the kernel side and 08/09; PFS node creation stays with 12.

/// `CDEV_R_BIT/W_BIT` (`minix3/minix/include/minix/com.h:940-941`).
pub const CDEV_R_BIT: u8 = 0x01;
/// See [`CDEV_R_BIT`].
pub const CDEV_W_BIT: u8 = 0x02;
/// `CDEV_NOCTTY` (`com.h:942`): not to become the controlling TTY.
pub const CDEV_NOCTTY: u8 = 0x04;

/// `CDEV_NONBLOCK` (`com.h:946`): do not suspend the I/O request.
pub const CDEV_NONBLOCK: u8 = 0x01;

/// `CDEV_CLONED` (`com.h:955`): reply carries a fresh minor number.
pub const CDEV_CLONED: i32 = 0x2000_0000;
/// `CDEV_CTTY` (`com.h:956`): reply grants the controlling TTY.
pub const CDEV_CTTY: i32 = 0x4000_0000;

/// Controlling-terminal source: whoever holds `fp_tty`.
///
/// Isolates the proc-table read (`rfp->fp_tty`, `cdev.c:46-49`) so the
/// rerouting rule is unit-testable without a live process table.
pub trait TtySource {
    /// Controlling terminal device (`None` = `NO_DEV`, has none).
    fn controlling_tty(&self) -> Option<u64>;
}

/// Fixed TTY source (test double with an answer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedTty(pub Option<u64>);

impl TtySource for FixedTty {
    fn controlling_tty(&self) -> Option<u64> {
        self.0
    }
}

/// TTY-less source (test double that has none).
///
/// Behaves differently from [`FixedTty`] (answer vs refusal), satisfying
/// the "two behaviorally different impls" rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NoTty;

impl TtySource for NoTty {
    fn controlling_tty(&self) -> Option<u64> {
        None
    }
}

/// Rerouting verdict for `cdev_map` (`cdev.c:35-56`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectVerdict {
    /// Not `/dev/tty`, or already redirected: use as-is.
    Keep(u64),
    /// `/dev/tty` with a controlling terminal: substitute it.
    Substitute(u64),
    /// No terminal, or major out of range: `NO_DEV`.
    NoDev,
}

/// Pure `/dev/tty` rerouting (`cdev_map` core, `cdev.c:35-56`).
///
/// - `is_ctty` mirrors `major(dev) == CTTY_MAJOR`.
/// - `tty` is the caller's controlling terminal (`None` = `NO_DEV`).
/// - `major_valid` mirrors the post-substitution bounds check (`:53`).
pub fn tty_redirect(
    dev: u64,
    is_ctty: bool,
    tty: Option<u64>,
    major_valid: bool,
) -> RedirectVerdict {
    if is_ctty {
        match tty {
            Some(t) => RedirectVerdict::Substitute(t),
            None => RedirectVerdict::NoDev,
        }
    } else if major_valid {
        RedirectVerdict::Keep(dev)
    } else {
        RedirectVerdict::NoDev
    }
}

/// Rerouting over an abstract TTY source: extracts the caller's
/// controlling terminal, then delegates to [`tty_redirect`].
pub fn tty_redirect_for<S: TtySource>(
    dev: u64,
    is_ctty: bool,
    source: &S,
    major_valid: bool,
) -> RedirectVerdict {
    tty_redirect(dev, is_ctty, source.controlling_tty(), major_valid)
}

/// Passed gate: driver endpoint plus the (possibly redirected) minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatePass {
    /// Driver endpoint to talk to.
    pub driver: i32,
    /// Minor number after any CTTY substitution.
    pub minor: u32,
}

/// Converged lookup gate (`cdev_get` core, `cdev.c:62-90`).
///
/// `mapped` is the post-`cdev_map` device (`None` = `NO_DEV`);
/// `driver` the dmap row's owner (`None` = `NONE`); `endpoint_ok` the
/// `isokendpt` verdict. All three failures collapse to one absence —
/// the caller chooses `ENXIO` (open) vs `EIO` (I/O).
pub fn resolve_gate(
    mapped: Option<u64>,
    driver: Option<i32>,
    endpoint_ok: bool,
) -> Option<GatePass> {
    let dev = mapped?;
    let driver = driver?;
    if !endpoint_ok {
        return None;
    }
    Some(GatePass {
        driver,
        minor: (dev & 0xffff_ffff) as u32,
    })
}

/// Combine open access bits (`cdev_opcl:199-202`).
pub fn access_bits(read: bool, write: bool, noctty: bool) -> u8 {
    let mut acc = 0;
    if read {
        acc |= CDEV_R_BIT;
    }
    if write {
        acc |= CDEV_W_BIT;
    }
    if noctty {
        acc |= CDEV_NOCTTY;
    }
    acc
}

/// Whether `O_NOCTTY` is forced (`cdev_opcl:185-191`).
///
/// Three-way OR: not a session leader, already has a terminal, or the
/// caller asked outright; otherwise a prior TTY-granting driver forces a
/// table scan whose hit is passed in as `seen_elsewhere`.
pub fn noctty_force(is_leader: bool, has_tty: bool, requested: bool, seen_elsewhere: bool) -> bool {
    if !is_leader || has_tty {
        return true;
    }
    if requested {
        return true;
    }
    seen_elsewhere
}

/// Post-open effects decoded from an open status word (`cdev.c:231-241`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpenEffects {
    /// Fresh minor for a cloned device (`CDEV_CLONED` payload).
    pub clone_minor: Option<u32>,
    /// Device to install as controlling terminal (`CDEV_CTTY` arm).
    pub grant_tty: Option<u64>,
}

/// Decode the open reply status word.
///
/// Negative statuses are driver errors and carry no effects; non-negative
/// words pack `CLONED`/`CTTY` flags around the fresh minor number.
pub fn open_effects(status: i32, dev: u64) -> OpenEffects {
    if status < 0 {
        return OpenEffects::default();
    }
    let mut effects = OpenEffects::default();
    if status & CDEV_CLONED != 0 {
        effects.clone_minor = Some((status & !(CDEV_CLONED | CDEV_CTTY)) as u32);
    }
    if status & CDEV_CTTY != 0 {
        effects.grant_tty = Some(dev);
    }
    effects
}

/// Grant direction for reads/writes (`cdev_io:306-308`).
///
/// Same cross as 19's `ioctl_access`: reading *out* grants the driver
/// *write* access, writing *in* grants *read*.
pub fn grant_dir(is_read: bool) -> u32 {
    if is_read {
        crate::device_map::CPF_WRITE
    } else {
        crate::device_map::CPF_READ
    }
}

/// Reply classes for `cdev_generic_reply` (`cdev.c:438-474`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyClass {
    /// Driver answered `SUSPEND`: ignore (`cdev.c:438-442`).
    DropSuspend,
    /// Requesting endpoint unknown: ignore (`cdev.c:444-448`).
    DropBadEndpt,
    /// A worker thread waits: hand it the message (`cdev.c:451-455`).
    DeliverWorker,
    /// Protocol mismatch (not blocked here): log (`cdev.c:456-463`).
    ProtocolMismatch,
    /// Blocked waiter: revive with this (possibly swapped) code.
    Revive(i32),
}

/// Pure reply classification.
///
/// - `status_is_suspend`: driver answered `SUSPEND`.
/// - `endpoint_ok`: `isokendpt(proc_e)` passed.
/// - `worker_waiting`: a worker thread holds this driver's slot.
/// - `blocked_here`: process blocked on CDEV for this very driver.
/// - `status`: driver status word (EINTR-swapped on revive, `cdev.c:473`).
pub fn classify_reply(
    status_is_suspend: bool,
    endpoint_ok: bool,
    worker_waiting: bool,
    blocked_here: bool,
    status: i32,
) -> ReplyClass {
    if status_is_suspend {
        return ReplyClass::DropSuspend;
    }
    if !endpoint_ok {
        return ReplyClass::DropBadEndpt;
    }
    if worker_waiting {
        return ReplyClass::DeliverWorker;
    }
    if !blocked_here {
        return ReplyClass::ProtocolMismatch;
    }
    ReplyClass::Revive(if status == minix_types::EINTR {
        minix_types::EAGAIN
    } else {
        status
    })
}

/// `cdev_cancel` error swap (`cdev.c:418`): services mix the two codes,
/// so cancellation reports `EINTR` for `EAGAIN`.
pub fn cancel_map(status: i32) -> i32 {
    if status == minix_types::EAGAIN {
        minix_types::EINTR
    } else {
        status
    }
}

/// `cdev_select` bypass rule (`cdev.c:346-361`): no CTTY mapping here —
/// the caller already mapped, and `fp` may be wrong. The asserts become
/// a verdict: only non-tty, in-range devices proceed.
pub fn select_bypass(dev_is_ctty: bool, major_valid: bool) -> Result<(), CdevError> {
    if dev_is_ctty || !major_valid {
        return Err(CdevError::Inval);
    }
    Ok(())
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// `SUSPEND` is a verdict (08/09 own suspension), not an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdevError {
    /// `ENXIO`: rerouting/lookup failure.
    NoDev,
    /// `EIO`: lookup failure on the I/O path, grant failures.
    Io,
    /// `EINTR`: cancelled (post-swap) interruptions.
    Intr,
    /// `EAGAIN`: revived-as-again, non-blocking fast failures.
    Again,
    /// `EINVAL`: reserved (bad operation codes, the C asserts' domain).
    Inval,
}

impl CdevError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::NoDev => minix_types::ENXIO,
            Self::Io => minix_types::EIO,
            Self::Intr => minix_types::EINTR,
            Self::Again => minix_types::EAGAIN,
            Self::Inval => minix_types::EINVAL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_map::CTTY_MAJOR;

    #[test]
    fn test_tty_redirect_matrix() {
        // Plain devices keep or refuse on bounds (`cdev.c:53-55`).
        assert_eq!(
            tty_redirect(0x0401, false, None, true),
            RedirectVerdict::Keep(0x0401)
        );
        assert_eq!(
            tty_redirect(0x0401, false, None, false),
            RedirectVerdict::NoDev
        );
        // CTTY with a terminal substitutes (`cdev.c:44-50`).
        assert_eq!(
            tty_redirect(CTTY_MAJOR as u64, true, Some(0x0402), true),
            RedirectVerdict::Substitute(0x0402)
        );
        // CTTY without one refuses (`cdev.c:46`).
        assert_eq!(
            tty_redirect(CTTY_MAJOR as u64, true, None, true),
            RedirectVerdict::NoDev
        );
        // Gate D: the two TTY sources behave differently via one bound.
        fn via<S: TtySource>(s: &S) -> RedirectVerdict {
            tty_redirect_for(9, true, s, true)
        }
        assert_eq!(via(&FixedTty(Some(9))), RedirectVerdict::Substitute(9));
        assert_eq!(via(&NoTty), RedirectVerdict::NoDev);
        assert_eq!(via(&FixedTty(None)), RedirectVerdict::NoDev);
    }

    #[test]
    fn test_resolve_gate() {
        // All three present passes with the minor attached.
        let pass = resolve_gate(Some(0x0401), Some(7), true).unwrap();
        assert_eq!(pass.driver, 7);
        assert_eq!(pass.minor, 0x0401);
        // Any absence collapses (`cdev_get:72-85` all-NULL arms).
        assert!(resolve_gate(None, Some(7), true).is_none());
        assert!(resolve_gate(Some(0x0401), None, true).is_none());
        assert!(resolve_gate(Some(0x0401), Some(7), false).is_none());
    }

    #[test]
    fn test_access_bits_combination() {
        // Bitwise combination (`cdev_opcl:199-202`).
        assert_eq!(access_bits(true, false, false), CDEV_R_BIT);
        assert_eq!(access_bits(false, true, false), CDEV_W_BIT);
        assert_eq!(
            access_bits(true, true, true),
            CDEV_R_BIT | CDEV_W_BIT | CDEV_NOCTTY
        );
        assert_eq!(access_bits(false, false, false), 0);
        // NOCTTY forcing truth table (`cdev_opcl:185-191`).
        assert!(noctty_force(false, false, false, false));
        assert!(noctty_force(true, true, false, false));
        assert!(noctty_force(true, false, true, false));
        assert!(noctty_force(true, false, false, true));
        assert!(!noctty_force(true, false, false, false));
    }

    #[test]
    fn test_open_effects_decode() {
        // Errors carry nothing.
        assert_eq!(open_effects(-5, 9), OpenEffects::default());
        assert_eq!(open_effects(0, 9), OpenEffects::default());
        // Cloned bit unpacks the fresh minor (`cdev.c:231-235`).
        let fx = open_effects(CDEV_CLONED | 17, 9);
        assert_eq!(fx.clone_minor, Some(17));
        assert_eq!(fx.grant_tty, None);
        // CTTY bit installs this device (`cdev.c:238-241`).
        let fx = open_effects(CDEV_CTTY, 9);
        assert_eq!(fx.clone_minor, None);
        assert_eq!(fx.grant_tty, Some(9));
        // Both bits compose.
        let fx = open_effects(CDEV_CLONED | CDEV_CTTY | 17, 9);
        assert_eq!(fx.clone_minor, Some(17));
        assert_eq!(fx.grant_tty, Some(9));
    }

    #[test]
    fn test_grant_direction_cross() {
        // Same cross as 19: read-out pairs with write, write-in with read.
        assert_eq!(grant_dir(true), crate::device_map::CPF_WRITE);
        assert_eq!(grant_dir(false), crate::device_map::CPF_READ);
    }

    #[test]
    fn test_reply_classes() {
        // Priority order matches the C cascade (`cdev.c:438-474`).
        assert_eq!(
            classify_reply(true, true, true, true, 0),
            ReplyClass::DropSuspend
        );
        assert_eq!(
            classify_reply(false, false, true, true, 0),
            ReplyClass::DropBadEndpt
        );
        assert_eq!(
            classify_reply(false, true, true, true, 0),
            ReplyClass::DeliverWorker
        );
        assert_eq!(
            classify_reply(false, true, false, false, 0),
            ReplyClass::ProtocolMismatch
        );
        // Revival swaps EINTR for EAGAIN (`cdev.c:472-473`).
        assert_eq!(
            classify_reply(false, true, false, true, minix_types::EINTR),
            ReplyClass::Revive(minix_types::EAGAIN)
        );
        assert_eq!(
            classify_reply(false, true, false, true, 5),
            ReplyClass::Revive(5)
        );
        // Cancellation swaps the other way (`cdev.c:418`).
        assert_eq!(cancel_map(minix_types::EAGAIN), minix_types::EINTR);
        assert_eq!(cancel_map(5), 5);
        assert_eq!(CdevError::Intr.to_errno(), minix_types::EINTR);
        assert_eq!(CdevError::Again.to_errno(), minix_types::EAGAIN);
    }

    #[test]
    fn test_select_bypass_rule() {
        // CTTY and out-of-range devices refuse the bypass (`:358-361`).
        assert!(select_bypass(false, true).is_ok());
        assert_eq!(select_bypass(true, true).unwrap_err(), CdevError::Inval);
        assert_eq!(select_bypass(false, false).unwrap_err(), CdevError::Inval);
    }

    #[test]
    fn test_errno_map_covers_cdev_c() {
        let cases = [
            (CdevError::NoDev, minix_types::ENXIO),
            (CdevError::Io, minix_types::EIO),
            (CdevError::Intr, minix_types::EINTR),
            (CdevError::Again, minix_types::EAGAIN),
            (CdevError::Inval, minix_types::EINVAL),
        ];
        for (err, errno) in cases {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
