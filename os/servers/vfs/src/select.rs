//! `select` — readiness multiplexing: one call waits on many fds.
//!
//! Corresponds to Minix3's `select.c:1-1416` (`do_select`, `select_filter`,
//! `select_request_char/sock/file/pipe`, `tab2ops`/`ops2tab`, `copy_fdsets`,
//! `select_cancel_all`/`select_cancel_filp`, `select_return`,
//! `select_callback`, `init_select`, `select_forget`, `select_timeout_check`,
//! `select_unsuspend_by_endpt`, `select_reply1`, `select_cdev_reply1`,
//! `select_sdev_reply1`, `select_reply2`, `select_cdev_reply2`,
//! `select_sdev_reply2`, `select_restart_filps`, `filp_status`,
//! `restart_proc`, `wipe_select`, `select_lock_filp`, `select_dump`).
//!
//! Design decisions (see 23-select.md §3):
//! - `FdKind` types the four fd temperaments (file/pipe/char/sock)
//! - `SelOps` + `tab2ops`/`ops2tab_apply` translate fd sets both ways
//! - `filter_step` types the ask-the-driver-or-not state machine
//! - `TimeoutPlan` types poll/forever/until as three states, not two bools
//! - `is_deferred`/`should_return` share one leave-gate for three call sites
//! - `reply1_step`/`reply2_hit` account first/second wave replies purely
//! - `SelectDriver`/`PipeProbe` traits script the driver dialogue (test doubles)
//!
//! Scope note: `pipe_check` probing execution stays with 17-pipe.md;
//! `cdev_select`/`sdev_select` delivery stays with 21-cdev.md/22-sdev.md;
//! waiting (`suspend`), revival (`revive`), timers (`set_timer`), and the
//! fd-set copy execution (`sys_datacopy`) stay with 08/09 and the kernel
//! side; socket syscalls stay with 24-socket.md.
//! This module only decides: classify, translate, filter, plan, gate,
//! account, and route.
//!
//! Linux models the same choice as `file_operations.poll` (each file type
//! answers readiness its own way, `poll_table` gathers); Redox models it as
//! `Scheme::poll` on pollable handles. Here `FdKind` is the poll table and
//! [`SelectDriver`] is the per-type answer.

use crate::filp::FsfFlags;
use crate::fproc::OPEN_MAX;

/// `MAXSELECTS` (`select.c:31`): at most 25 pending `select()` calls.
pub const MAXSELECTS: usize = 25;

/// `USECPERSEC` (`select.c:35`): microseconds per second.
pub const USECPERSEC: i64 = 1_000_000;

/// `TMRDIFF_MAX` bound used for tick truncation (`do_select:327-328`).
/// Minix3 defines it in `<minix/timers.h>`; the value is timer-range bound.
/// Only the truncation behavior (saturate, never wrap) is modeled here.
pub const TMRDIFF_MAX_U64: u64 = u64::MAX >> 1;

bitflags::bitflags! {
    /// `SEL_*` operations (`const.h:41-44`, values shared with `CDEV_OP_*`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SelOps: u8 {
        const RD     = 0x01;
        const WR     = 0x02;
        const ERR    = 0x04;
        /// `SEL_NOTIFY`: not a real operation, asks the driver to keep state.
        const NOTIFY = 0x08;
    }
}

impl SelOps {
    /// Real operations (everything but `NOTIFY`).
    pub fn real(self) -> SelOps {
        self & (SelOps::RD | SelOps::WR | SelOps::ERR)
    }
    /// True if no real operation is selected.
    pub fn is_empty_real(self) -> bool {
        self.real().is_empty()
    }
}

/// The four fd temperaments (`fdtypes[]`, `select.c:81-90`).
///
/// Order matters: character and socket devices are matched before regular
/// files and pipes, mirroring the C table order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdKind {
    /// Character device: ask the character driver (`select_request_char`).
    Char,
    /// Socket device: ask the socket driver (`select_request_sock`).
    Sock,
    /// Regular file: always ready (`select_request_file`).
    File,
    /// Pipe/fifo: probe one byte each way (`select_request_pipe`).
    Pipe,
}

/// Classify an fd (`select.c:225-232` + predicates `368-404`).
///
/// Arguments are the four predicate answers in C table order; `None` means
/// no type matched (C: `se->type[fd] == -1` → `EBADF`).
pub fn classify(is_char: bool, is_sock: bool, is_reg: bool, is_fifo: bool) -> Option<FdKind> {
    if is_char {
        Some(FdKind::Char)
    } else if is_sock {
        Some(FdKind::Sock)
    } else if is_reg {
        Some(FdKind::File)
    } else if is_fifo {
        Some(FdKind::Pipe)
    } else {
        None
    }
}

/// `tab2ops` (`select.c:621-629`): read one fd's interest from three sets.
pub fn tab2ops(in_read: bool, in_write: bool, in_err: bool) -> SelOps {
    let mut ops = SelOps::empty();
    if in_read {
        ops |= SelOps::RD;
    }
    if in_write {
        ops |= SelOps::WR;
    }
    if in_err {
        ops |= SelOps::ERR;
    }
    ops
}

/// Outcome of applying ready operations to one fd's ready set.
///
/// Mirrors the three dedup guards in `ops2tab` (`select.c:637-653`):
/// the fd must be interested, not already recorded, and the user must
/// actually want that set back (`vir_*fds != NULL`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyMark {
    /// Newly ready real operations (each counted once in `nreadyfds`).
    pub newly: SelOps,
}

/// `ops2tab` per-fd application (`select.c:635-654`).
///
/// - `want`: ready operations reported for this fd.
/// - `interested`: the fd's interest (`readfds`/`writefds`/`errorfds`).
/// - `already`: operations already recorded in the ready sets.
/// - `has_vir`: per-set user pointers (`vir_readfds`/`vir_writefds`/
///   `vir_errorfds`), each gating its own bit.
///
/// Returns the newly ready operations (caller adds `newly` bit count to
/// `nreadyfds`).
pub fn ops2tab_apply(
    want: SelOps,
    interested: SelOps,
    already: SelOps,
    has_vir: SelOps,
) -> ReadyMark {
    let mut newly = SelOps::empty();
    for bit in [SelOps::RD, SelOps::WR, SelOps::ERR] {
        if want.contains(bit)
            && interested.contains(bit)
            && !already.contains(bit)
            && has_vir.contains(bit)
        {
            newly |= bit;
        }
    }
    ReadyMark { newly }
}

/// `fd_set` copy direction (`copy_fdsets`, `select.c:660-708`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyDir {
    /// User → kernel (`FROM_PROC`).
    FromProc,
    /// Kernel → user (`TO_PROC`, only `nfds` bits, ready sets).
    ToProc,
}

/// `fd_set` byte size for `nfds` bits (`select.c:674`, `howmany` rounding).
///
/// Returns `None` for out-of-range `nfds` (C: `EINVAL` at `do_select:118`,
/// `panic` at `copy_fdsets:671` — the panic is unreachable because the
/// caller validates first; `None` makes that explicit).
pub fn fdset_bytes(nfds: usize) -> Option<usize> {
    if nfds > OPEN_MAX {
        return None;
    }
    const NFDBITS: usize = 64;
    const MASK_BYTES: usize = 8;
    Some(nfds.div_ceil(NFDBITS) * MASK_BYTES)
}

/// `select_filter` outcome (`select.c:409-457`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOutcome {
    /// Nothing to do right now (`return 0`).
    ReadyNone,
    /// A query is already in flight (`return SUSPEND`).
    Suspend,
    /// Send a fresh query: new operations plus flag updates to apply.
    Query {
        /// Operations to send the driver (includes `NOTIFY` when blocking).
        rops: SelOps,
        /// `FSF_UPDATE` is newly set (caller sets it).
        set_update: bool,
        /// Newly set blocking bits (`FSF_RD/WR/ERR_BLOCK` subset).
        set_block: SelOps,
    },
}

/// `select_filter` as a pure state machine (`select.c:409-457`).
///
/// - `flags`: current `filp_select_flags` (`FSF_*`).
/// - `rops`: requested real operations (`*ops` on entry).
/// - `block`: whether this select may block.
///
/// The non-blocking fast path (`select.c:433-443`, self-described as
/// "a dangerous case of premature optimization") prunes operations the
/// driver already watches: stable flags (neither `UPDATE` nor `BUSY`) plus
/// a standing `BLOCKED` watch means "assume not ready yet".
pub fn filter_step(flags: FsfFlags, rops: SelOps, block: bool) -> FilterOutcome {
    let mut want = rops;
    if !block
        && !flags.intersects(FsfFlags::UPDATE | FsfFlags::BUSY)
        && flags.intersects(FsfFlags::BLOCKED)
    {
        if want.contains(SelOps::RD) && flags.contains(FsfFlags::RD_BLOCK) {
            want -= SelOps::RD;
        }
        if want.contains(SelOps::WR) && flags.contains(FsfFlags::WR_BLOCK) {
            want -= SelOps::WR;
        }
        if want.contains(SelOps::ERR) && flags.contains(FsfFlags::ERR_BLOCK) {
            want -= SelOps::ERR;
        }
        if want.is_empty_real() {
            return FilterOutcome::ReadyNone;
        }
    }

    let mut out = want;
    let mut set_block = SelOps::empty();
    if block {
        out |= SelOps::NOTIFY;
        if out.contains(SelOps::RD) {
            set_block |= SelOps::RD;
        }
        if out.contains(SelOps::WR) {
            set_block |= SelOps::WR;
        }
        if out.contains(SelOps::ERR) {
            set_block |= SelOps::ERR;
        }
    }
    if flags.contains(FsfFlags::BUSY) {
        return FilterOutcome::Suspend;
    }
    FilterOutcome::Query {
        rops: out,
        set_update: true,
        set_block,
    }
}

/// Timeout plan (`do_select:140-167` + `320-336`).
///
/// C spreads this over two bools (`do_timeout`, `se->block`); two bools
/// admit four combinations of which one is impossible, so the plan is an
/// enum with exactly three states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutPlan {
    /// Timeout `(0,0)` — poll and return (`se->block = 0`).
    Poll,
    /// No timeout — block forever.
    Forever,
    /// Block up to `ticks` (already truncated, never zero).
    Until {
        /// Clock ticks to wait.
        ticks: u64,
    },
}

/// Plan the wait from the user timeout (`do_select:141-167,320-336`).
///
/// - `has_tv`: the user passed a non-null timeout pointer.
/// - `sec`/`usec`: the copied `struct timeval` (`EINVAL` on negative
///   fields or `tv_usec >= USECPERSEC`, `select.c:146-148`).
/// - `hz`: `system_hz` ticks per second.
///
/// Truncation (`select.c:327-328`) saturates at `TMRDIFF_MAX`, rounding up
/// partial microseconds (`select.c:330-331`).
pub fn plan_timeout(
    has_tv: bool,
    sec: i64,
    usec: i64,
    hz: u64,
) -> Result<TimeoutPlan, SelectError> {
    if !has_tv {
        return Ok(TimeoutPlan::Forever);
    }
    // C-mirror (`select.c:146-148`): keep the two-sided check verbatim.
    #[allow(clippy::manual_range_contains)]
    if sec < 0 || usec < 0 || usec >= USECPERSEC {
        return Err(SelectError::Inval);
    }
    if sec == 0 && usec == 0 {
        return Ok(TimeoutPlan::Poll);
    }
    let ticks = if sec as u64 >= (TMRDIFF_MAX_U64 - 1) / hz.max(1) {
        TMRDIFF_MAX_U64
    } else {
        // C-mirror (`select.c:330-331`): round up partial microseconds.
        #[allow(clippy::manual_div_ceil)]
        let frac = (usec as u64 * hz + USECPERSEC as u64 - 1) / USECPERSEC as u64;
        sec as u64 * hz + frac
    };
    if ticks == 0 {
        Ok(TimeoutPlan::Poll)
    } else {
        Ok(TimeoutPlan::Until { ticks })
    }
}

/// `se->block` derived from the plan (`do_select:161-166`).
pub fn block_of(plan: TimeoutPlan) -> bool {
    match plan {
        TimeoutPlan::Poll => false,
        TimeoutPlan::Forever | TimeoutPlan::Until { .. } => true,
    }
}

/// `is_deferred` (`select.c:346-362`): initial replies still in flight.
///
/// `starting` covers setup; `any_update_or_busy` covers any involved filp
/// with `FSF_UPDATE | FSF_BUSY` set.
pub fn is_deferred(starting: bool, any_update_or_busy: bool) -> bool {
    starting || any_update_or_busy
}

/// The shared leave-gate (`do_select:299-300`, `restart_proc:1311`,
/// `select_timeout_check:874`).
///
/// All three C sites test "(ready, error, or must not block) and not
/// deferred"; one function keeps them from drifting apart.
pub fn should_return(nready: usize, has_error: bool, block: bool, deferred: bool) -> bool {
    (nready > 0 || has_error || !block) && !deferred
}

/// First-wave reply accounting (`select_reply1`, `select.c:956-999`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reply1Out {
    /// New `filp_select_ops` (what is still owed).
    pub ops: SelOps,
    /// New `filp_select_flags`.
    pub flags: FsfFlags,
    /// Operations to broadcast to owners (`filp_status`).
    pub broadcast: SelOps,
}

/// Account one initial driver reply (`select.c:965-998`).
///
/// `status > 0` carries ready operations; `status == 0` carries none;
/// `status < 0` is an error. The three-branch ops rule (`977-981`):
/// done (neither `UPDATE` nor `BLOCKED`) clears everything; pending work
/// with ready bits subtracts only those; otherwise keeps the mask.
/// Errors always drop the `BLOCKED` watch (`992-994`).
pub fn reply1_step(flags: FsfFlags, ops: SelOps, status: i32) -> Reply1Out {
    let mut new_flags = flags & !FsfFlags::BUSY;
    let mut new_ops = ops;
    if !(flags.contains(FsfFlags::UPDATE) || flags.intersects(FsfFlags::BLOCKED)) {
        new_ops = SelOps::empty();
    } else if status > 0 && !flags.contains(FsfFlags::UPDATE) {
        let ready =
            SelOps::from_bits_truncate(status as u8) & (SelOps::RD | SelOps::WR | SelOps::ERR);
        new_ops &= !ready;
    }
    let broadcast: SelOps;
    if status == 0 && flags.intersects(FsfFlags::BLOCKED) {
        broadcast = SelOps::empty();
    } else if status > 0 {
        let ready =
            SelOps::from_bits_truncate(status as u8) & (SelOps::RD | SelOps::WR | SelOps::ERR);
        if ready.contains(SelOps::RD) {
            new_flags &= !FsfFlags::RD_BLOCK;
        }
        if ready.contains(SelOps::WR) {
            new_flags &= !FsfFlags::WR_BLOCK;
        }
        if ready.contains(SelOps::ERR) {
            new_flags &= !FsfFlags::ERR_BLOCK;
        }
        broadcast = ready;
    } else if status < 0 {
        new_flags &= !FsfFlags::BLOCKED;
        broadcast = SelOps::empty();
    } else {
        broadcast = SelOps::empty();
    }
    Reply1Out {
        ops: new_ops,
        flags: new_flags,
        broadcast,
    }
}

/// Second-wave per-entry hit (`select_reply2` inner loop, `select.c:1126-1158`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reply2Out {
    /// New `filp_select_ops`.
    pub ops: SelOps,
    /// New `filp_select_flags`.
    pub flags: FsfFlags,
    /// Ready operations to mark for this fd (`ops2tab` input).
    pub ready: SelOps,
    /// Error to store in the entry (`se->error`), if any.
    pub error: Option<i32>,
    /// Whether this fd matched (`found`, gates `restart_proc`).
    pub matched: bool,
}

/// Apply one second-wave reply to one entry fd (`select.c:1133-1151`).
///
/// Non-matching devices return `matched: false` (caller skips `restart`).
/// Ready replies clear replied bits unless `UPDATE` is set and drop the
/// matching `BLOCK` watches; errors drop `BLOCKED` and stash the error.
pub fn reply2_hit(flags: FsfFlags, ops: SelOps, dev_match: bool, status: i32) -> Reply2Out {
    if !dev_match {
        return Reply2Out {
            ops,
            flags,
            ready: SelOps::empty(),
            error: None,
            matched: false,
        };
    }
    if status > 0 {
        let ready =
            SelOps::from_bits_truncate(status as u8) & (SelOps::RD | SelOps::WR | SelOps::ERR);
        let mut new_flags = flags;
        let mut new_ops = ops;
        if !flags.contains(FsfFlags::UPDATE) {
            new_ops &= !ready;
        }
        if ready.contains(SelOps::RD) {
            new_flags &= !FsfFlags::RD_BLOCK;
        }
        if ready.contains(SelOps::WR) {
            new_flags &= !FsfFlags::WR_BLOCK;
        }
        if ready.contains(SelOps::ERR) {
            new_flags &= !FsfFlags::ERR_BLOCK;
        }
        Reply2Out {
            ops: new_ops,
            flags: new_flags,
            ready,
            error: None,
            matched: true,
        }
    } else {
        let new_flags = flags & !FsfFlags::BLOCKED;
        Reply2Out {
            ops,
            flags: new_flags,
            ready: SelOps::empty(),
            error: Some(status),
            matched: true,
        }
    }
}

/// Per-filp select ledger (mirrors `filp.rs` select fields, `file.h:26-32`).
///
/// Owned by the generic select code (`file.h:22-25`); fd-type-specific
/// code must not touch `selectors`/`select_ops`/`select_flags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilpSel {
    /// `filp_selectors`: selecting processes on this filp.
    pub selectors: u32,
    /// `filp_select_ops`: still-owed `SEL_*` operations.
    pub ops: SelOps,
    /// `filp_select_flags`: `FSF_*` state.
    pub flags: FsfFlags,
    /// `filp_pipe_select_ops`: parked pipe interest (`select.c:612-613`).
    pub pipe_ops: SelOps,
    /// `filp_select_dev`: stashed device (`NO_DEV` = none).
    pub dev: Option<u64>,
}

impl Default for FilpSel {
    fn default() -> Self {
        Self {
            selectors: 0,
            ops: SelOps::empty(),
            flags: FsfFlags::empty(),
            pipe_ops: SelOps::empty(),
            dev: None,
        }
    }
}

/// Release one selection on a filp (`select_cancel_filp`, `select.c:740-778`).
///
/// Returns the stale device binding to clear (`Some(dev)` = the caller
/// must null the matching `dmap`/`smap` select filp, leaving `busy` set).
/// The last selector zeroes ops/flags/pipe interest (`753-757`).
pub fn cancel_one(sel: &mut FilpSel) -> Option<u64> {
    debug_assert!(sel.selectors > 0);
    sel.selectors = sel.selectors.saturating_sub(1);
    if sel.selectors > 0 {
        return None;
    }
    sel.ops = SelOps::empty();
    sel.flags = FsfFlags::empty();
    sel.pipe_ops = SelOps::empty();
    sel.dev.take()
}

/// Driver bid: at most one select query in flight per driver.
///
/// Models `dmap_sel_busy`/`dmap_sel_filp` (`dmap.h`) and
/// `smap_sel_busy`/`smap_sel_filp` (`smap.c`) uniformly: the busy flag
/// serializes queries, the owner filp routes the first-wave reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DriverBid {
    /// Query in flight.
    pub busy: bool,
    /// Owner filp index (`None` = stale, requestor went away).
    pub owner: Option<usize>,
}

/// Answer to a select query: the driver dialogue behind a trait.
///
/// Character (`cdev_select`, `cdev.c`) and socket (`sdev_select`, `sdev.c`)
/// queries share one shape — map, filter, busy-gate, send, mark — so one
/// trait covers both (D7). The busy-gate and flag bookkeeping stay with
/// the caller; only the send itself is abstracted.
pub trait SelectDriver {
    /// Send the select query for `dev` with `rops` (must include any
    /// notify bit already). `Ok` = sent, now waiting; `Err` = driver
    /// refused synchronously (`select.c:514,554`).
    fn query(&mut self, dev: u64, rops: SelOps) -> Result<(), SelectError>;
}

/// Scripted driver (test double with programmed answers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedDriver {
    /// Scripted results, consumed in order.
    pub script: [Option<SelectError>; 8],
    /// Next script index.
    pub pos: usize,
    /// Devices queried (observable dialogue).
    pub queried: [u64; 8],
    /// Number of queries made.
    pub nqueried: usize,
}

impl ScriptedDriver {
    /// All-accepting script.
    pub fn accept_all() -> Self {
        Self {
            script: [None; 8],
            pos: 0,
            queried: [0; 8],
            nqueried: 0,
        }
    }
    /// Script with failures at programmed positions.
    pub fn scripted(script: [Option<SelectError>; 8]) -> Self {
        Self {
            script,
            pos: 0,
            queried: [0; 8],
            nqueried: 0,
        }
    }
}

impl SelectDriver for ScriptedDriver {
    fn query(&mut self, dev: u64, rops: SelOps) -> Result<(), SelectError> {
        let _ = rops;
        if self.nqueried < self.queried.len() {
            self.queried[self.nqueried] = dev;
        }
        self.nqueried += 1;
        let err = self.script[self.pos % self.script.len()];
        self.pos += 1;
        match err {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }
}

/// Refusing driver (test double that always refuses).
///
/// Behaves differently from [`ScriptedDriver`] (blanket refusal vs
/// programmed answers), satisfying the "two behaviorally different impls"
/// rule for traits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefusingDriver;

impl SelectDriver for RefusingDriver {
    fn query(&mut self, _dev: u64, _rops: SelOps) -> Result<(), SelectError> {
        Err(SelectError::NoDev)
    }
}

/// One-byte pipe probe outcome (`pipe_check` check-only, `pipe.c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOut {
    /// Data/space available.
    Ready,
    /// Would block (`SUSPEND` from check-only probe).
    WouldBlock,
    /// Probe failed with an error (negative `err`, `select.c:594,605`).
    Failed(i32),
}

/// Pipe readiness probe: the one-byte check behind a trait.
///
/// `select_request_pipe` (`select.c:577-616`) never asks a driver; it
/// probes readability/writability with `pipe_check(..., 1, check-only)`.
/// The probe execution stays with 17-pipe.md; only the answers are modeled.
pub trait PipeProbe {
    /// Probe one byte for reading.
    fn probe_read(&self) -> ProbeOut;
    /// Probe one byte for writing.
    fn probe_write(&self) -> ProbeOut;
}

/// Scripted probe (test double with programmed answers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptedProbe {
    /// Answer for the read probe.
    pub read: ProbeOut,
    /// Answer for the write probe.
    pub write: ProbeOut,
}

impl PipeProbe for ScriptedProbe {
    fn probe_read(&self) -> ProbeOut {
        self.read
    }
    fn probe_write(&self) -> ProbeOut {
        self.write
    }
}

/// Closed-pipe probe (test double: both directions failed).
///
/// Behaves differently from [`ScriptedProbe`] (fixed failure vs programmed
/// answers), satisfying the "two behaviorally different impls" rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosedProbe(pub i32);

impl PipeProbe for ClosedProbe {
    fn probe_read(&self) -> ProbeOut {
        ProbeOut::Failed(self.0)
    }
    fn probe_write(&self) -> ProbeOut {
        ProbeOut::Failed(self.0)
    }
}

/// Pipe request outcome (`select_request_pipe` result, `select.c:577-616`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeRequestOut {
    /// Ready operations (`*ops`, already masked with the request).
    pub ready: SelOps,
    /// Pipe interest to park when blocking and nothing is ready.
    pub park: SelOps,
}

/// Combine pipe probes (`select.c:587-614`).
///
/// Read interest probes one readable byte (`SEL_RD`, plus `SEL_ERR` on
/// failure); write interest probes one writable byte (`SEL_WR`, plus
/// `SEL_ERR` on failure). Results are masked with the original request
/// (`select.c:610`); when blocking and nothing is ready, the original
/// interest parks in `filp_pipe_select_ops` (`select.c:612-613`).
pub fn pipe_request<P: PipeProbe>(probe: &P, want: SelOps, block: bool) -> PipeRequestOut {
    let orig = want;
    let mut ready = SelOps::empty();
    if want.intersects(SelOps::RD | SelOps::ERR) {
        match probe.probe_read() {
            ProbeOut::Ready => ready |= SelOps::RD,
            ProbeOut::WouldBlock => {}
            ProbeOut::Failed(_) => ready |= SelOps::ERR,
        }
    }
    if want.intersects(SelOps::WR | SelOps::ERR) {
        match probe.probe_write() {
            ProbeOut::Ready => ready |= SelOps::WR,
            ProbeOut::WouldBlock => {}
            ProbeOut::Failed(_) => ready |= SelOps::ERR,
        }
    }
    ready &= orig;
    let park = if ready.is_empty_real() && block {
        orig
    } else {
        SelOps::empty()
    };
    PipeRequestOut { ready, park }
}

/// Death classification (`select_unsuspend_by_endpt`, `select.c:884-951`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathKind {
    /// The exiting process itself selected here: cancel the whole entry
    /// (`select.c:903-907`, caller asserts `FP_EXITING`).
    ExitingProcess,
    /// A character driver died and this fd watches it: mark ready.
    CharDriverGone,
    /// A socket driver died and this fd watches it: mark ready.
    SockDriverGone,
    /// Unrelated endpoint: skip the expensive checks (`select.c:910-911`).
    Unrelated,
}

/// Per-fd death hit for driver loss (`select.c:915-936`).
///
/// A dead driver cannot answer, yet the user waits for readability — so
/// the fd is marked `RD|WR` ready (`select.c:922,930`) and its ledger
/// released; the caller restarts the entry (`restart_proc:938-939`).
/// This is the dual of 22-sdev.md's `EIO` convention: select reports
/// readiness and lets the next read/write surface the error.
pub fn unsuspend_hit(kind: DeathKind) -> Option<SelOps> {
    match kind {
        DeathKind::CharDriverGone | DeathKind::SockDriverGone => Some(SelOps::RD | SelOps::WR),
        DeathKind::ExitingProcess | DeathKind::Unrelated => None,
    }
}

/// `select_lock_filp` lock kind (`select.c:1337-1351`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockKind {
    /// Shared (`VNODE_READ`, default).
    Read,
    /// Exclusive (`VNODE_WRITE`): write or error interest needs it.
    Write,
}

/// Lock kind from requested operations (`select.c:1346-1348`).
pub fn lock_kind(ops: SelOps) -> LockKind {
    if ops.intersects(SelOps::WR | SelOps::ERR) {
        LockKind::Write
    } else {
        LockKind::Read
    }
}

/// What `do_select` tells the main loop (ARCH A-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectVerdict {
    /// Reply now with the ready count (or error, carried separately).
    Done,
    /// `suspend(FP_BLOCKED_ON_SELECT)`: the second wave will revive.
    Suspend,
}

/// Errors of this module, each mapping to one Minix3 errno.
///
/// `SUSPEND` is a verdict (08/09 own suspension), not an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectError {
    /// `EBADF`: bad fd, unknown fd type, mode-bit mismatch is ready (not
    /// an error) — this variant covers lookup/type failures.
    BadFd,
    /// `EINVAL`: bad `nfds`, bad `timeval`.
    Inval,
    /// `ENOSPC`: no free select slot (`select.c:124`).
    NoSpace,
    /// `ENXIO`: missing driver mapping (`select.c:490,542`).
    NoDev,
    /// `EIO`: driver confusion, e.g. two controlling TTYs on one filp
    /// (`select.c:501`) or invalidated filp surfacing as I/O error.
    Io,
}

impl SelectError {
    /// The Minix3 errno value.
    pub fn to_errno(self) -> i32 {
        match self {
            Self::BadFd => minix_types::EBADF,
            Self::Inval => minix_types::EINVAL,
            Self::NoSpace => minix_types::ENOSPC,
            Self::NoDev => minix_types::ENXIO,
            Self::Io => minix_types::EIO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_covers_four_kinds() {
        // Table order: char/sock before file/pipe (`select.c:85-90`).
        assert_eq!(classify(true, true, true, true), Some(FdKind::Char));
        assert_eq!(classify(false, true, true, true), Some(FdKind::Sock));
        assert_eq!(classify(false, false, true, true), Some(FdKind::File));
        assert_eq!(classify(false, false, false, true), Some(FdKind::Pipe));
        // Unknown type -> EBADF (`do_select:234-237`).
        assert_eq!(classify(false, false, false, false), None);
        assert_eq!(SelectError::BadFd.to_errno(), minix_types::EBADF);
    }

    #[test]
    fn test_tab2ops_ops2tab_roundtrip() {
        // `tab2ops:621-629` reads three sets into SEL bits.
        assert_eq!(tab2ops(true, false, true), SelOps::RD | SelOps::ERR);
        assert!(tab2ops(false, false, false).is_empty());
        // `ops2tab:637-653` dedups: interested + not recorded + wanted.
        let mark = ops2tab_apply(
            SelOps::RD | SelOps::WR,
            SelOps::RD | SelOps::WR | SelOps::ERR,
            SelOps::empty(),
            SelOps::RD | SelOps::WR | SelOps::ERR,
        );
        assert_eq!(mark.newly, SelOps::RD | SelOps::WR);
        // Already recorded bits are not counted twice.
        let again = ops2tab_apply(
            SelOps::RD | SelOps::WR,
            SelOps::RD | SelOps::WR,
            SelOps::RD,
            SelOps::RD | SelOps::WR,
        );
        assert_eq!(again.newly, SelOps::WR);
        // The user did not ask for error back: no mark even if ready.
        let unwanted = ops2tab_apply(SelOps::ERR, SelOps::ERR, SelOps::empty(), SelOps::RD);
        assert!(unwanted.newly.is_empty());
        // `fdset_bytes:674` rounds up to whole words; over-range is None.
        assert_eq!(fdset_bytes(0), Some(0));
        assert_eq!(fdset_bytes(1), Some(8));
        assert_eq!(fdset_bytes(255), Some(32));
        assert_eq!(fdset_bytes(256), None);
    }

    #[test]
    fn test_filter_matrix() {
        // Cold blocking query: UPDATE set, NOTIFY added, BLOCK armed.
        match filter_step(FsfFlags::empty(), SelOps::RD, true) {
            FilterOutcome::Query {
                rops,
                set_update,
                set_block,
            } => {
                assert!(rops.contains(SelOps::NOTIFY | SelOps::RD));
                assert!(set_update);
                assert_eq!(set_block, SelOps::RD);
            }
            other => panic!("expected query, got {other:?}"),
        }
        // Busy filp: suspend even for a fresh blocking query (`453-454`).
        assert_eq!(
            filter_step(FsfFlags::BUSY, SelOps::RD, true),
            FilterOutcome::Suspend
        );
        // Non-blocking fast path (`433-443`): watched RD pruned to none.
        // NOTE: `BLOCKED` already contains all three `*_BLOCK` bits, so a
        // lone `RD_BLOCK` is the precise "only RD watched" state.
        assert_eq!(
            filter_step(FsfFlags::RD_BLOCK, SelOps::RD, false),
            FilterOutcome::ReadyNone
        );
        // Non-blocking with unwatched WR still queries (no NOTIFY added).
        match filter_step(FsfFlags::RD_BLOCK, SelOps::WR, false) {
            FilterOutcome::Query {
                rops, set_block, ..
            } => {
                assert!(!rops.contains(SelOps::NOTIFY));
                assert!(set_block.is_empty());
            }
            other => panic!("expected query, got {other:?}"),
        }
        // Unstable flags (UPDATE set): fast path disabled, full query.
        match filter_step(FsfFlags::UPDATE | FsfFlags::BLOCKED, SelOps::RD, false) {
            FilterOutcome::Query { rops, .. } => assert!(rops.contains(SelOps::RD)),
            other => panic!("expected query, got {other:?}"),
        }
    }

    #[test]
    fn test_timeout_plans() {
        // No timeout pointer: wait forever (`161-162`).
        assert_eq!(plan_timeout(false, 0, 0, 100), Ok(TimeoutPlan::Forever));
        // (0,0): poll (`165-166`).
        assert_eq!(plan_timeout(true, 0, 0, 100), Ok(TimeoutPlan::Poll));
        assert!(!block_of(TimeoutPlan::Poll));
        assert!(block_of(TimeoutPlan::Forever));
        // 1.5 s at 100 Hz: 100 + 50 ticks, rounded up (`330-331`).
        assert_eq!(
            plan_timeout(true, 1, 500_000, 100),
            Ok(TimeoutPlan::Until { ticks: 150 })
        );
        // Partial microsecond rounds up.
        assert_eq!(
            plan_timeout(true, 0, 1, 100),
            Ok(TimeoutPlan::Until { ticks: 1 })
        );
        // Nonsense timeval rejected (`146-148`).
        assert_eq!(plan_timeout(true, -1, 0, 100), Err(SelectError::Inval));
        assert_eq!(
            plan_timeout(true, 0, USECPERSEC, 100),
            Err(SelectError::Inval)
        );
        assert_eq!(plan_timeout(true, 0, -5, 100), Err(SelectError::Inval));
        // Huge timeout saturates instead of wrapping (`327-328`).
        assert_eq!(
            plan_timeout(true, i64::MAX / 2, 0, 100),
            Ok(TimeoutPlan::Until {
                ticks: TMRDIFF_MAX_U64
            })
        );
    }

    #[test]
    fn test_leave_gate_shared() {
        // Ready/error/poll each open the gate; deferred always closes it.
        assert!(should_return(1, false, true, false));
        assert!(should_return(0, true, true, false));
        assert!(should_return(0, false, false, false));
        assert!(!should_return(0, false, true, false));
        assert!(!should_return(3, true, false, true));
    }

    #[test]
    fn test_reply1_branches() {
        // Done selecting (no UPDATE/BLOCKED): ops cleared (`977-978`).
        let out = reply1_step(FsfFlags::empty(), SelOps::RD | SelOps::WR, 0);
        assert!(out.ops.is_empty());
        assert!(out.broadcast.is_empty());
        // Pending second query (UPDATE set): ops kept, BUSY cleared.
        let out = reply1_step(FsfFlags::UPDATE, SelOps::RD, 0);
        assert_eq!(out.ops, SelOps::RD);
        // Ready reply with pending work but no second query (BLOCKED set,
        // UPDATE clear): subtract only ready bits (`979-981`). NOTE: with
        // UPDATE set the mask must survive — another select on the same
        // filp still needs those bits — so UPDATE is excluded here.
        let out = reply1_step(
            FsfFlags::RD_BLOCK | FsfFlags::WR_BLOCK,
            SelOps::RD | SelOps::WR,
            SelOps::RD.bits() as i32,
        );
        assert_eq!(out.ops, SelOps::WR);
        assert_eq!(out.broadcast, SelOps::RD);
        // UPDATE set: the owed mask survives even a ready reply (`979`).
        let out = reply1_step(
            FsfFlags::UPDATE | FsfFlags::RD_BLOCK,
            SelOps::RD | SelOps::WR,
            SelOps::RD.bits() as i32,
        );
        assert_eq!(out.ops, SelOps::RD | SelOps::WR);
        // BLOCKED + zero status: silence, keep the watch (`984`).
        let out = reply1_step(FsfFlags::BLOCKED, SelOps::RD, 0);
        assert!(out.broadcast.is_empty());
        // Error: the BLOCKED watch always drops (`992-994`).
        let out = reply1_step(FsfFlags::BLOCKED, SelOps::RD, -minix_types::EIO);
        assert!(!out.flags.intersects(FsfFlags::BLOCKED));
    }

    #[test]
    fn test_reply2_and_restart() {
        // Matching ready reply marks the fd and clears the watch.
        let out = reply2_hit(
            FsfFlags::RD_BLOCK,
            SelOps::RD | SelOps::WR,
            true,
            SelOps::RD.bits() as i32,
        );
        assert!(out.matched);
        assert_eq!(out.ready, SelOps::RD);
        assert!(!out.flags.contains(FsfFlags::RD_BLOCK));
        // UPDATE set: the owed mask survives (`1137-1138`).
        let out = reply2_hit(FsfFlags::UPDATE, SelOps::RD, true, SelOps::RD.bits() as i32);
        assert_eq!(out.ops, SelOps::RD);
        // Error path stores the error and drops BLOCKED (`1147-1150`).
        let out = reply2_hit(FsfFlags::BLOCKED, SelOps::RD, true, -minix_types::EIO);
        assert_eq!(out.error, Some(-minix_types::EIO));
        assert!(!out.flags.intersects(FsfFlags::BLOCKED));
        // Wrong device: no match, no restart (`1131`).
        let out = reply2_hit(
            FsfFlags::empty(),
            SelOps::RD,
            false,
            SelOps::RD.bits() as i32,
        );
        assert!(!out.matched);
        assert!(out.error.is_none());
    }

    #[test]
    fn test_pipe_request_and_drivers() {
        let both = ScriptedProbe {
            read: ProbeOut::Ready,
            write: ProbeOut::Ready,
        };
        let out = pipe_request(&both, SelOps::RD | SelOps::WR, true);
        assert_eq!(out.ready, SelOps::RD | SelOps::WR);
        assert!(out.park.is_empty());
        // Would-block read with block: parks the original interest (`612-613`).
        let half = ScriptedProbe {
            read: ProbeOut::WouldBlock,
            write: ProbeOut::Ready,
        };
        let out = pipe_request(&half, SelOps::RD | SelOps::WR, true);
        assert_eq!(out.ready, SelOps::WR);
        assert!(out.park.is_empty());
        let out = pipe_request(&half, SelOps::RD, true);
        assert!(out.ready.is_empty_real());
        assert_eq!(out.park, SelOps::RD);
        // No parking without block.
        let out = pipe_request(&half, SelOps::RD, false);
        assert!(out.park.is_empty());
        // Failed probe surfaces ERR (`594-595,605-606`); closed probe double.
        let failing = ScriptedProbe {
            read: ProbeOut::Failed(-5),
            write: ProbeOut::WouldBlock,
        };
        let out = pipe_request(&failing, SelOps::RD | SelOps::ERR, true);
        assert!(out.ready.contains(SelOps::ERR));
        let closed = ClosedProbe(-5);
        let out = pipe_request(&closed, SelOps::RD | SelOps::WR | SelOps::ERR, true);
        assert!(out.ready.contains(SelOps::ERR));
        // Drivers: scripted answers vs blanket refusal.
        let mut good = ScriptedDriver::accept_all();
        assert!(good.query(7, SelOps::RD).is_ok());
        assert_eq!(good.nqueried, 1);
        let mut bad = ScriptedDriver::scripted([
            Some(SelectError::Io),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ]);
        assert_eq!(bad.query(7, SelOps::RD), Err(SelectError::Io));
        let mut refusing = RefusingDriver;
        assert_eq!(refusing.query(7, SelOps::RD), Err(SelectError::NoDev));
        // Cancel releases the ledger; last selector clears and yields dev.
        let mut sel = FilpSel {
            selectors: 2,
            ops: SelOps::RD,
            flags: FsfFlags::BUSY,
            pipe_ops: SelOps::RD,
            dev: Some(9),
        };
        assert_eq!(cancel_one(&mut sel), None);
        assert_eq!(cancel_one(&mut sel), Some(9));
        assert!(sel.ops.is_empty() && sel.flags.is_empty() && sel.dev.is_none());
        // Death marks readiness; unrelated deaths stay silent.
        assert_eq!(
            unsuspend_hit(DeathKind::CharDriverGone),
            Some(SelOps::RD | SelOps::WR)
        );
        assert_eq!(
            unsuspend_hit(DeathKind::SockDriverGone),
            Some(SelOps::RD | SelOps::WR)
        );
        assert_eq!(unsuspend_hit(DeathKind::Unrelated), None);
        assert_eq!(unsuspend_hit(DeathKind::ExitingProcess), None);
        // Write interest needs the exclusive lock (`1346-1348`).
        assert_eq!(lock_kind(SelOps::RD), LockKind::Read);
        assert_eq!(lock_kind(SelOps::WR), LockKind::Write);
        assert_eq!(lock_kind(SelOps::ERR), LockKind::Write);
    }

    #[test]
    fn test_errno_map_covers_select_c() {
        for (err, errno) in [
            (SelectError::BadFd, minix_types::EBADF),
            (SelectError::Inval, minix_types::EINVAL),
            (SelectError::NoSpace, minix_types::ENOSPC),
            (SelectError::NoDev, minix_types::ENXIO),
            (SelectError::Io, minix_types::EIO),
        ] {
            assert_eq!(err.to_errno(), errno, "{err:?}");
        }
        // `MAXSELECTS` full is the only ENOSPC source (`select.c:124`).
        assert_eq!(MAXSELECTS, 25);
    }
}
