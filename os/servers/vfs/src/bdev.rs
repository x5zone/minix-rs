//! `bdev` — direct block-driver dialogue: retry, dead-lettering, revival.
//!
//! Corresponds to Minix3's `bdev.c:1-282` (`bdev_sendrec`, `bdev_open`,
//! `bdev_close`, `bdev_ioctl`, `bdev_reply`, `bdev_up`).
//!
//! Design decisions (see 20-bdev.md §3):
//! - `SendTransport` trait scripts the driver dialogue (test doubles)
//! - `RetryState` types the five-restart fuse
//! - `SendFault` classifies dead letters (unknown failures harden to EIO)
//! - `resolve_driver/access_bits` type the open/close gate pair
//! - `ReplyCheck` types the three reply validations
//! - reopen/notify/root predicates type the driver-swap sweeps
//! - the ioctl guard duty reuses 19's `BLOCK_NEEDS_GUARD` (not redefined)

// BDEV 线上常量（RQ_BASE/OFF 三档/R_BIT/W_BIT）单一权威住
// `minix_types::types::device`（edge E-DEVWIRE），经下方 use 消费；
// u64 算术处由使用点自行拓宽。
pub use minix_types::{
    BDEV_CLOSE_OFF, BDEV_IOCTL_OFF, BDEV_OPEN_OFF, BDEV_R_BIT, BDEV_RQ_BASE, BDEV_W_BIT,
};

/// Kernel send statuses used as classifier inputs (never surfaced):
/// `EDEADSRCDST` = 202 (`errno.h:198`, cf. `os/kernel/src/errno.rs:236`),
/// `ELOCKED` = 208 (`errno.h:204`, cf. `os/kernel/src/errno.rs:248`),
/// `EDEADEPT` = 215 (`minix_types::EDEADEPT`).
pub const SEND_DEAD_SRC_DST: i32 = 202;
/// See the [`SEND_DEAD_SRC_DST`] group comment.
pub const SEND_LOCKED: i32 = 208;

/// Restart request from a driver (`ERESTART`)。
///
/// 这是**驱动回复载荷**里的状态字（`mess_lblockdriver_lbdev_reply.status`）：
/// 驱动是 `_SYSTEM` 构建，常量在线上带负号（`sys/sys/errno.h:187-192` 的
/// `_SIGN`），所以比较值取负。内核传输层的失败状态（`SendOutcome::Failed`、
/// `classify_send` 的输入）是另一个域——**正号**（`minix-sys` 的 TrapStatus
/// 约定），两个域不要混。
pub const SEND_RESTART: i32 = -(minix_types::ERESTART);

/// Retry fuse: at most five restarts (`bdev.c:41-54`).
pub const MAX_RETRIES: u8 = 5;

/// Block operation kinds (message selectors, `BDEV_RQ_BASE` + offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BdevOp {
    /// `BDEV_OPEN`: open a minor device.
    Open,
    /// `BDEV_CLOSE`: close a minor device.
    Close,
    /// `BDEV_IOCTL`: I/O control operation.
    Ioctl,
}

impl BdevOp {
    /// Wire message type for this operation.
    pub fn msg_type(self) -> u64 {
        BDEV_RQ_BASE as u64
            + match self {
                Self::Open => BDEV_OPEN_OFF,
                Self::Close => BDEV_CLOSE_OFF,
                Self::Ioctl => BDEV_IOCTL_OFF,
            } as u64
    }
}

/// One transport round outcome (what `drv_sendrec` reported).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// Transport ok; driver status word carried alongside.
    Ok(i32),
    /// Transport failed with a kernel status (dead/locked/…).
    Failed(i32),
}

/// Driver dialogue transport (`drv_sendrec` seam, `bdev.c:44`).
pub trait SendTransport {
    /// One send round: transport status plus driver status word.
    fn send(&mut self) -> SendOutcome;
}

/// Scripted transport: replays recorded outcomes in order (test double).
///
/// Each `send` consumes the next script entry; past the end it repeats
/// the last one, so short scripts drive long retries deterministically.
#[derive(Debug, Clone)]
#[cfg(test)]
pub struct ScriptedTransport {
    script: &'static [SendOutcome],
    at: usize,
}

#[cfg(test)]
impl ScriptedTransport {
    /// New transport replaying `script`.
    pub fn new(script: &'static [SendOutcome]) -> Self {
        Self { script, at: 0 }
    }

    /// Rounds consumed so far.
    pub fn rounds(self) -> usize {
        self.at
    }
}

#[cfg(test)]
impl SendTransport for ScriptedTransport {
    fn send(&mut self) -> SendOutcome {
        let last = self.script.len().saturating_sub(1);
        let out = self.script[self.at.min(last)];
        self.at += 1;
        out
    }
}

/// Dead transport: every round reports a dead endpoint (test double).
///
/// Behaves differently from [`ScriptedTransport`] (fixed fate vs
/// programmable script), satisfying the "two behaviorally different
/// impls" rule for traits.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeadTransport;

impl SendTransport for DeadTransport {
    fn send(&mut self) -> SendOutcome {
        SendOutcome::Failed(SEND_DEAD_SRC_DST)
    }
}

/// Retry verdict for one driver status word (`bdev.c:48-58`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryVerdict {
    /// Status is final; deliver it.
    Done(i32),
    /// `ERESTART`: restore the message and send again.
    Again,
    /// Five restarts spent: fuse blown, report `EIO`.
    Exhausted,
}

/// Restart counter (`retry_count`, `bdev.c:36-54`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RetryState(pub u8);

impl RetryState {
    /// Fresh counter.
    pub fn new() -> Self {
        Self(0)
    }

    /// Rounds spent so far.
    pub fn spent(self) -> u8 {
        self.0
    }

    /// Classify one status word, advancing the counter on restarts.
    pub fn step(&mut self, status: i32) -> RetryVerdict {
        if status != SEND_RESTART {
            return RetryVerdict::Done(status);
        }
        self.0 += 1;
        if self.0 < MAX_RETRIES {
            RetryVerdict::Again
        } else {
            RetryVerdict::Exhausted
        }
    }
}

/// Dead-letter classes for transport failures (`bdev.c:60-70`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendFault {
    /// Endpoint died (`EDEADSRCDST/EDEADEPT`): unmap the driver, `EIO`.
    Dead,
    /// Deadlock talking to the driver (`ELOCKED`): log, `EIO`.
    Locked,
    /// Anything else (C panics): harden to `EIO` (ARCH, D3).
    Fatal,
}

/// Classify a failed round's kernel status.
pub fn classify_send(status: i32) -> SendFault {
    if status == SEND_DEAD_SRC_DST || status == minix_types::EDEADEPT {
        SendFault::Dead
    } else if status == SEND_LOCKED {
        SendFault::Locked
    } else {
        SendFault::Fatal
    }
}

/// Full direct dialogue (`bdev_sendrec` core, `bdev.c:43-72`).
///
/// Retries restarts up to the fuse, then maps transport failures through
/// [`classify_send`]. The message save/restore across retries stays with
/// the caller, which owns the message buffer.
pub fn transact<T: SendTransport>(transport: &mut T) -> Result<i32, BdevError> {
    let mut retry = RetryState::new();
    loop {
        match transport.send() {
            SendOutcome::Ok(status) => match retry.step(status) {
                RetryVerdict::Done(final_status) => return Ok(final_status),
                RetryVerdict::Again => continue,
                RetryVerdict::Exhausted => return Err(BdevError::Io),
            },
            SendOutcome::Failed(status) => {
                let _ = classify_send(status);
                // Dead endpoints are unmapped by the caller (it owns the
                // dmap); every class reports EIO here (`bdev.c:60-68`).
                return Err(BdevError::Io);
            }
        }
    }
}

/// Resolve the driver for a major number (`bdev_open:86-89` gate pair).
///
/// Out-of-range majors and unmapped rows both report `ENXIO`.
pub fn resolve_driver(major_valid: bool, driver: Option<i32>) -> Result<i32, BdevError> {
    if !major_valid {
        return Err(BdevError::NoDev);
    }
    driver.ok_or(BdevError::NoDev)
}

/// Combine open access bits (`bdev_open:91-93`): R→`BDEV_R_BIT`, W→`BDEV_W_BIT`.
pub fn access_bits(read: bool, write: bool) -> u8 {
    let mut access = 0;
    if read {
        access |= BDEV_R_BIT as u8;
    }
    if write {
        access |= BDEV_W_BIT as u8;
    }
    access
}

/// `BDEV_OPEN` 请求消息（C `bdev_open:94-104`：`memset` 之后只填
/// `minor`/`access`/`id`，`pos` 留 0）。
///
/// 偏移表 [`minix_types::lblockdriver_msg_off`]——**首格是 `pos`**，`minor`
/// 在 8 而不是 0；这条形状由 [`minix_types::lblockdriver_msg_off`] 的钉值
/// 测试与本函数的测试双面锁住。
pub fn open_request(minor: u32, access: u8) -> minix_types::Message {
    let mut m = minix_types::Message {
        m_type: BdevOp::Open.msg_type() as i32,
        ..minix_types::Message::default()
    };
    // SAFETY: `mess_lbdev_lblockdriver_msg` 无专属 union 成员，按共享偏移表
    // 写字节（ipc.h:338-353）。
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[minix_types::lblockdriver_msg_off::MINOR
            ..minix_types::lblockdriver_msg_off::MINOR + 4]
            .copy_from_slice(&(minor as i32).to_le_bytes());
        raw[minix_types::lblockdriver_msg_off::ACCESS
            ..minix_types::lblockdriver_msg_off::ACCESS + 4]
            .copy_from_slice(&(access as i32).to_le_bytes());
    }
    m
}

/// `BDEV_CLOSE` 请求消息（C `bdev_close:125-135`：只填 `minor`，`id` 留 0）。
pub fn close_request(minor: u32) -> minix_types::Message {
    let mut m = minix_types::Message {
        m_type: BdevOp::Close.msg_type() as i32,
        ..minix_types::Message::default()
    };
    // SAFETY: 同 [`open_request`]（ipc.h:338-353）。
    unsafe {
        let raw = &mut m.m_u.raw;
        raw[minix_types::lblockdriver_msg_off::MINOR
            ..minix_types::lblockdriver_msg_off::MINOR + 4]
            .copy_from_slice(&(minor as i32).to_le_bytes());
    }
    m
}

/// Reply validations for `bdev_reply` (`bdev.c:198-215`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplyCheck {
    /// A dmap row names the replier (`get_dmap_by_endpt` hit).
    pub known: bool,
    /// That row has a worker servicing it (`!= INVALID_THREAD`).
    pub servicing: bool,
    /// The worker waits for exactly this driver (`w_task` + slot set).
    pub worker_ok: bool,
}

/// Why a driver reply is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyIgnore {
    /// Unknown driver (`bdev.c:198-202`).
    UnknownDriver,
    /// Nobody servicing (`bdev.c:204-208`).
    NoServicing,
    /// No worker waiting for this driver (`bdev.c:211-215`).
    NoWorker,
}

/// Pure three-gate check; delivery itself stays with 09-main-loop.md.
///
/// `MUST NOT block` (`bdev.c:190`) holds trivially: no call here blocks.
pub fn check_reply(check: ReplyCheck) -> Result<(), ReplyIgnore> {
    if !check.known {
        return Err(ReplyIgnore::UnknownDriver);
    }
    if !check.servicing {
        return Err(ReplyIgnore::NoServicing);
    }
    if !check.worker_ok {
        return Err(ReplyIgnore::NoWorker);
    }
    Ok(())
}

/// Whether one filp needs reopening on a driver swap (`bdev_up:243-247`).
///
/// Four-way conjunction: live filp, vnode attached, same major, block type.
pub fn reopen_candidate(count: i64, has_vnode: bool, major_match: bool, is_blk: bool) -> bool {
    count >= 1 && has_vnode && major_match && is_blk
}

/// Whether one vmnt needs the new-driver notice (`bdev_up:262-263`).
pub fn notify_vmnt(mnt_major_valid: bool, mnt_major: u32, maj: u32) -> bool {
    mnt_major_valid && mnt_major == maj
}

/// Whether the root FS gets the extra notice (`bdev_up:277-281`).
///
/// Sent whenever any block-special file was open for the major at all —
/// deliberately over-broad ("more work to check", `bdev.c:279-280`).
pub fn root_notify(any_open: bool) -> bool {
    any_open
}

/// Post-reopen effect: a failed reopen abandons the whole swap
/// (`bdev_up:251-256`: clear recovering, give up entirely).
pub fn reopen_failed_aborts(ok: bool) -> bool {
    !ok
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// `ERESTART` never surfaces (retried or fused); kernel send statuses
/// (202/208/215) are classifier inputs, not error variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BdevError {
    /// `ENXIO`: major out of range or driver unmapped.
    NoDev,
    /// `EIO`: fused retries, dead letters, hardening cases.
    Io,
}

impl minix_types::ToErrno for BdevError {
    fn to_errno(&self) -> minix_types::Errno {
        minix_types::Errno::from_i32((*self).to_errno())
    }
}

impl BdevError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::NoDev => minix_types::ENXIO,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_op_selectors() {
        // Message selectors ride `BDEV_RQ_BASE` (`com.h:963-976`).
        assert_eq!(BdevOp::Open.msg_type(), 0x500);
        assert_eq!(BdevOp::Close.msg_type(), 0x501);
        assert_eq!(BdevOp::Ioctl.msg_type(), 0x506);
        // Access combination (`bdev_open:91-93`).
        assert_eq!(access_bits(true, false), BDEV_R_BIT as u8);
        assert_eq!(access_bits(false, true), BDEV_W_BIT as u8);
        assert_eq!(access_bits(true, true), (BDEV_R_BIT | BDEV_W_BIT) as u8);
        assert_eq!(access_bits(false, false), 0);
    }

    #[test]
    fn test_request_shapes() {
        // `BDEV_OPEN`：`minor` 在 8（首格是 `pos`）、`access` 在 16、其余 0。
        let m = open_request(0x0205, BDEV_R_BIT as u8 | BDEV_W_BIT as u8);
        assert_eq!(m.m_type, 0x500);
        // SAFETY(test): 按共享偏移表读回，逐格核对。
        let raw = unsafe { &m.m_u.raw };
        assert_eq!(
            u64::from_le_bytes(
                raw[minix_types::lblockdriver_msg_off::POS
                    ..minix_types::lblockdriver_msg_off::POS + 8]
                    .try_into()
                    .unwrap()
            ),
            0,
            "pos 留 0"
        );
        assert_eq!(
            i32::from_le_bytes(
                raw[minix_types::lblockdriver_msg_off::MINOR
                    ..minix_types::lblockdriver_msg_off::MINOR + 4]
                    .try_into()
                    .unwrap()
            ),
            0x0205
        );
        assert_eq!(
            i32::from_le_bytes(
                raw[minix_types::lblockdriver_msg_off::ID
                    ..minix_types::lblockdriver_msg_off::ID + 4]
                    .try_into()
                    .unwrap()
            ),
            0
        );
        assert_eq!(
            i32::from_le_bytes(
                raw[minix_types::lblockdriver_msg_off::ACCESS
                    ..minix_types::lblockdriver_msg_off::ACCESS + 4]
                    .try_into()
                    .unwrap()
            ),
            (BDEV_R_BIT | BDEV_W_BIT) as i32
        );
        // `BDEV_CLOSE`：只带 minor。
        let m = close_request(7);
        assert_eq!(m.m_type, 0x501);
        // SAFETY(test): 同上。
        let raw = unsafe { &m.m_u.raw };
        assert_eq!(
            i32::from_le_bytes(
                raw[minix_types::lblockdriver_msg_off::MINOR
                    ..minix_types::lblockdriver_msg_off::MINOR + 4]
                    .try_into()
                    .unwrap()
            ),
            7
        );
    }

    #[test]
    fn test_resolve_gate_pair() {
        // Out-of-range and unmapped rows both refuse (`:88-89,123-124`).
        assert_eq!(
            resolve_driver(false, Some(7)).unwrap_err(),
            BdevError::NoDev
        );
        assert_eq!(resolve_driver(true, None).unwrap_err(), BdevError::NoDev);
        assert_eq!(BdevError::NoDev.to_errno(), minix_types::ENXIO);
        assert_eq!(resolve_driver(true, Some(7)).unwrap(), 7);
    }

    #[test]
    fn test_retry_fuse() {
        // Fresh counter delivers non-restart statuses untouched.
        let mut retry = RetryState::new();
        assert_eq!(retry.step(0), RetryVerdict::Done(0));
        assert_eq!(retry.spent(), 0);
        assert_eq!(retry.step(-5), RetryVerdict::Done(-5));
        // Four restarts ask again; the fifth blows the fuse (`:48-58`).
        let mut retry = RetryState::new();
        for _ in 0..4 {
            assert_eq!(retry.step(SEND_RESTART), RetryVerdict::Again);
        }
        assert_eq!(retry.spent(), 4);
        assert_eq!(retry.step(SEND_RESTART), RetryVerdict::Exhausted);
    }

    #[test]
    fn test_send_fault_classes() {
        // Dead endpoints (unmap + EIO), deadlock (log + EIO).
        assert_eq!(classify_send(SEND_DEAD_SRC_DST), SendFault::Dead);
        assert_eq!(classify_send(minix_types::EDEADEPT), SendFault::Dead);
        assert_eq!(classify_send(SEND_LOCKED), SendFault::Locked);
        // Anything else hardens (C panics; ARCH D3).
        assert_eq!(classify_send(-999), SendFault::Fatal);
    }

    #[test]
    fn test_transact_scripts() {
        // Immediate success costs one round.
        let mut t = ScriptedTransport::new(&[SendOutcome::Ok(0)]);
        assert_eq!(transact(&mut t).unwrap(), 0);
        assert_eq!(t.rounds(), 1);
        // Two restarts then success: three rounds, final status delivered.
        let mut t = ScriptedTransport::new(&[
            SendOutcome::Ok(SEND_RESTART),
            SendOutcome::Ok(SEND_RESTART),
            SendOutcome::Ok(3),
        ]);
        assert_eq!(transact(&mut t).unwrap(), 3);
        assert_eq!(t.rounds(), 3);
        // Endless restarts blow the fuse after five rounds.
        let mut t = ScriptedTransport::new(&[SendOutcome::Ok(SEND_RESTART)]);
        assert_eq!(transact(&mut t).unwrap_err(), BdevError::Io);
        assert_eq!(t.rounds(), 5);
        // Dead letters fail fast in one round.
        let mut t = ScriptedTransport::new(&[SendOutcome::Failed(SEND_DEAD_SRC_DST)]);
        assert_eq!(transact(&mut t).unwrap_err(), BdevError::Io);
        assert_eq!(BdevError::Io.to_errno(), minix_types::EIO);
        // Gate D: the fixed-fate double behaves differently via one bound.
        fn via<T: SendTransport>(t: &mut T) -> bool {
            transact(t).is_ok()
        }
        let mut live = ScriptedTransport::new(&[SendOutcome::Ok(0)]);
        let mut dead = DeadTransport;
        assert!(via(&mut live));
        assert!(!via(&mut dead));
    }

    #[test]
    fn test_reply_triple_gate() {
        let open = ReplyCheck {
            known: true,
            servicing: true,
            worker_ok: true,
        };
        assert!(check_reply(open).is_ok());
        // Each gate drops with its own reason (`bdev.c:198-215`).
        let unknown = ReplyCheck {
            known: false,
            servicing: true,
            worker_ok: true,
        };
        assert_eq!(
            check_reply(unknown).unwrap_err(),
            ReplyIgnore::UnknownDriver
        );
        let idle = ReplyCheck {
            known: true,
            servicing: false,
            worker_ok: true,
        };
        assert_eq!(check_reply(idle).unwrap_err(), ReplyIgnore::NoServicing);
        let stray = ReplyCheck {
            known: true,
            servicing: true,
            worker_ok: false,
        };
        assert_eq!(check_reply(stray).unwrap_err(), ReplyIgnore::NoWorker);
    }

    #[test]
    fn test_swap_predicates() {
        // Reopen needs all four (`bdev_up:243-247`).
        assert!(reopen_candidate(1, true, true, true));
        assert!(!reopen_candidate(0, true, true, true));
        assert!(!reopen_candidate(1, false, true, true));
        assert!(!reopen_candidate(1, true, false, true));
        assert!(!reopen_candidate(1, true, true, false));
        // Notify on major match; root on any opening (`:262,277`).
        assert!(notify_vmnt(true, 8, 8));
        assert!(!notify_vmnt(true, 8, 9));
        assert!(!notify_vmnt(false, 8, 8));
        assert!(root_notify(true));
        assert!(!root_notify(false));
        // A failed reopen abandons the swap (`:251-256`).
        assert!(reopen_failed_aborts(false));
        assert!(!reopen_failed_aborts(true));
    }

    #[test]
    fn test_errno_map_covers_bdev_c() {
        let cases = [
            (BdevError::NoDev, minix_types::ENXIO),
            (BdevError::Io, minix_types::EIO),
        ];
        for (err, errno) in cases {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
    }
}
