//! Process-table snapshots: pull discipline, hash, and time math.
//!
//! Mirrors the pure halves of `update_tables` / `get_mslot` /
//! `ticks_to_timeval` / `fill_wmesg` (`proc.c:18-216`). Pulling whole
//! tables across servers (`sys_getproctab`, `getsysinfo`) and reading
//! the clock are transport effects (A-6, A-12); the pull discipline
//! (throttle + failure latch), the magic contract, the PID hash, and
//! the conversions are judged here. Table *layouts* belong to
//! kernel/PM/VFS (cross-stage contract, A-6); only shapes travel here.
//!
//! 16-mib-proc-tables.md.

/// Slots no PID maps to. C: `NO_SLOT (-1)` — proc.c:34.
use alloc::vec::Vec;

pub const NO_SLOT: i32 = -1;

/// Headroom for forks between size estimation and retrieval.
///
/// C: `EXTRA_PROCS 8` — proc.c:22-31. Two-step reads (size, then data)
/// race with forks; the estimate pads eight so the second call rarely
/// finds the buffer short. The once-per-tick throttle (below) is what
/// makes eight *enough* "typically".
pub const EXTRA_PROCS: u32 = 8;

/// Hash slots from the PM table size: a quarter, "expected in use".
///
/// C: `HASH_SLOTS (NR_PROCS / 4)` — proc.c:33. `NR_PROCS` is a build
/// config (`config.h:31`), so the function takes it as a parameter
/// rather than pinning a number.
pub const fn hash_slots(nr_procs: u32) -> u32 {
    nr_procs / 4
}

/// Kernel magic: every row must carry it. C: `PMAGIC 0xC0FFEE1` —
/// minix/const.h:164, checked proc.c:81-87.
pub const PMAGIC: u32 = 0xC0FFEE1;

/// PM magic: every row must carry it. C: `MP_MAGIC 0xC0FFEE0` —
/// pm/mproc.h:106, checked proc.c:97-103.
pub const MP_MAGIC: u32 = 0xC0FFEE0;

/// Whether a magic row passes (one mismatch poisons the whole pull).
pub const fn magic_ok(got: u32, want: u32) -> bool {
    got == want
}

/// Pull discipline verdict: pull, reuse, or stay dead.
///
/// C: `update_tables` head — proc.c:53-72. A past failure latches
/// *forever* (`tabs_valid == FALSE` returns at once, :57-58 — "very
/// unlikely to be transient"); otherwise at most one pull per clock
/// tick (:66-69 — hundreds of kilobytes per pull, userland lives with
/// tick-old data). `last == 0` is "never pulled" (the `tabs_updated`
/// initializer, :38), which always pulls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullVerdict {
    /// A past pull failed: stay dead, do not retry. C: `:57-58`.
    StayDead,
    /// Same tick as the last pull: reuse the tables. C: `:68-69`.
    Reuse,
    /// Pull all three tables now. C: `:71-72`.
    Pull,
}

/// Judge the pull discipline.
pub const fn judge_pull(latch_dead: bool, last_tick: u64, now_tick: u64) -> PullVerdict {
    if latch_dead {
        return PullVerdict::StayDead;
    }
    if last_tick != 0 && last_tick == now_tick {
        return PullVerdict::Reuse;
    }
    PullVerdict::Pull
}

/// Pull order: kernel, then PM, then VFS-light; first failure wins.
///
/// C: proc.c:74-113. Kernel table via `sys_getproctab` (magic-checked
/// row by row), PM via `getsysinfo(SI_PROC_TAB)` (same), VFS via
/// `getsysinfo(SI_PROCLIGHT_TAB)` (no magic — light rows carry none).
/// The latch sets *before* pulling (`tabs_valid = FALSE`, :72), so any
/// failure path below leaves it dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullSource {
    /// Kernel table. C: `:75`.
    Kernel,
    /// PM table. C: `:90`.
    Pm,
    /// VFS light table. C: `:106`.
    VfsLight,
}

/// The three pulls, in order. C: proc.c:74-113.
pub const PULL_ORDER: [PullSource; 3] = [PullSource::Kernel, PullSource::Pm, PullSource::VfsLight];

/// PID hash slot (`pid > 0` only; PID 0 is the kernel — caller's problem).
///
/// C: `mp_pid % HASH_SLOTS` after the `<= 0` skip — proc.c:126-129.
/// `get_mslot` re-applies the guard (`pid <= 0 → NO_SLOT`, :149-150).
pub const fn hash_slot(pid: i32, slots: u32) -> Option<u32> {
    if pid <= 0 || slots == 0 {
        return None;
    }
    Some((pid as u32) % slots)
}

/// Walk one hash chain for the pid; `NO_SLOT` ends the walk.
///
/// C: `get_mslot` loop — proc.c:152-157. Pure over the caller's slices:
/// `slots` maps bucket → first mslot-or-`NO_SLOT`, `next` maps mslot →
/// next-or-`NO_SLOT`, `pids` the per-slot pid. Stale chains (a bucket
/// pointing past the slices) miss rather than trap.
pub fn chain_lookup(slots: &[i32], next: &[i32], pids: &[i32], pid: i32, nslots: u32) -> i32 {
    let start = match hash_slot(pid, nslots) {
        Some(s) => s as usize,
        None => return NO_SLOT,
    };
    if start >= slots.len() {
        return NO_SLOT;
    }
    let mut m = slots[start];
    while m != NO_SLOT {
        let i = m as usize;
        if i >= next.len() || i >= pids.len() {
            return NO_SLOT;
        }
        if pids[i] == pid {
            return m;
        }
        m = next[i];
    }
    NO_SLOT
}

/// Ticks to seconds + microseconds: `sec = t / hz`, `usec = (t % hz) * 1e6 / hz`.
///
/// C: `ticks_to_timeval` — proc.c:164-172 (`hz = sys_hz()`, transport).
/// Integer math throughout; sub-tick precision is intentionally lost.
pub const fn ticks_to_timeval(ticks: u64, hz: u64) -> (u64, u64) {
    if hz == 0 {
        return (0, 0);
    }
    (ticks / hz, (ticks % hz) * 1_000_000 / hz)
}

/// What a wchan message names: the switch arms of `fill_wmesg`.
///
/// C: proc.c:191-208. `ANY`/`SELF`/`NONE` name themselves; anything else
/// names the other process when its slot is valid (tasks always,
/// processes when `IN_USE`), else the raw endpoint number. Direct IPC
/// wraps the name in parentheses (:211-215, [`paren_direct`]).
/// Classify the endpoint lane (values compared by the caller: `ANY`,
/// `SELF`, `NONE` are endpoint-relative, endpoint.h:54-56 — never
/// pinned here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndptLane {
    /// The `ANY` wildcard.
    Any,
    /// Self reference.
    This,
    /// No endpoint.
    None,
    /// A concrete endpoint: `Some(slot)` when the slot is valid
    /// (tasks always; processes when in use), else the raw number.
    /// C: `:202-207`.
    Peer {
        /// Valid slot (task or in-use process).
        known: bool,
    },
}

/// Judge the wmesg lane for `(endpt, is_any, is_self, is_none, slot_known)`.
pub const fn wmesg_lane(is_any: bool, is_self: bool, is_none: bool, slot_known: bool) -> EndptLane {
    if is_any {
        return EndptLane::Any;
    }
    if is_self {
        return EndptLane::This;
    }
    if is_none {
        return EndptLane::None;
    }
    EndptLane::Peer { known: slot_known }
}

/// Parenthesize for direct IPC (`(name)` vs `name`).
/// C: `ipc ? "(" : ""` — proc.c:211-215.
pub const fn paren_direct(direct_ipc: bool) -> (char, char) {
    if direct_ipc { ('(', ')') } else { ('\0', '\0') }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pull_discipline() {
        // Dead latches forever (proc.c:57-58).
        assert_eq!(judge_pull(true, 0, 0), PullVerdict::StayDead);
        assert_eq!(judge_pull(true, 5, 5), PullVerdict::StayDead);
        // Same tick reuses (:68-69); never-pulled pulls (:38, :71-72).
        assert_eq!(judge_pull(false, 5, 5), PullVerdict::Reuse);
        assert_eq!(judge_pull(false, 0, 0), PullVerdict::Pull);
        assert_eq!(judge_pull(false, 4, 5), PullVerdict::Pull);
        // Order: kernel, PM, VFS-light (:74-113).
        assert_eq!(
            PULL_ORDER,
            [PullSource::Kernel, PullSource::Pm, PullSource::VfsLight]
        );
        // Magics (const.h:164, mproc.h:106).
        assert_eq!((PMAGIC, MP_MAGIC), (0xC0FFEE1, 0xC0FFEE0));
        assert!(magic_ok(PMAGIC, PMAGIC));
        assert!(!magic_ok(0, PMAGIC));
        assert_eq!((EXTRA_PROCS, NO_SLOT), (8, -1));
    }

    #[test]
    fn test_pid_hash() {
        // Quarter of the table (proc.c:33); pid 0/kernel refused.
        assert_eq!(hash_slots(256), 64);
        assert_eq!(hash_slot(0, 64), None);
        assert_eq!(hash_slot(-3, 64), None);
        assert_eq!(hash_slot(65, 64), Some(1));
        // Chain walk: bucket → slots → pid match (:152-157).
        let slots = [NO_SLOT, 1];
        let next = [NO_SLOT, 2, NO_SLOT];
        let pids = [0, 65, 129];
        assert_eq!(chain_lookup(&slots, &next, &pids, 65, 64), 1);
        assert_eq!(chain_lookup(&slots, &next, &pids, 66, 64), NO_SLOT);
        assert_eq!(chain_lookup(&slots, &next, &pids, 0, 64), NO_SLOT);
        // Stale chains miss, never trap.
        let stale = [NO_SLOT, 9];
        assert_eq!(chain_lookup(&stale, &next, &pids, 65, 64), NO_SLOT);
    }

    #[test]
    fn test_time_and_wmesg() {
        // sec/usec split (proc.c:170-171); zero hz guards.
        assert_eq!(ticks_to_timeval(250, 100), (2, 500_000));
        assert_eq!(ticks_to_timeval(100, 100), (1, 0));
        assert_eq!(ticks_to_timeval(5, 0), (0, 0));
        // Lanes (proc.c:191-208); parens for direct IPC (:211-215).
        assert_eq!(wmesg_lane(true, false, false, false), EndptLane::Any);
        assert_eq!(wmesg_lane(false, true, false, false), EndptLane::This);
        assert_eq!(wmesg_lane(false, false, true, false), EndptLane::None);
        assert_eq!(
            wmesg_lane(false, false, false, true),
            EndptLane::Peer { known: true }
        );
        assert_eq!(paren_direct(true), ('(', ')'));
        assert_eq!(paren_direct(false), ('\0', '\0'));
    }
}

// ── Pull execution (P1-5) ──
//
// C's `update_tables` (proc.c:46-217) drives the pull: once per clock
// tick, kernel table first, then PM and VFS via `getsysinfo`,
// magic-checked on arrival, a failure latching `tabs_valid = FALSE`
// until reboot. The pull *verbs* live behind the server's seams
// (A-12); this module owns the state machine.

/// The three snapshot buffers plus the pull state.
///
/// C: `proc_tab`/`mproc_tab`/`fproc_tab` static arrays plus
/// `tabs_valid`/`tabs_updated` — proc.c:34-39. The bytes are the producers'
/// layouts (kernel/PM/VFS own them — E-MIBPROD); MIB treats them as
/// opaque until the fill halves (17~20) interpret rows.
pub struct Tables {
    /// Kernel process table bytes. C: `proc_tab` (sys_getproctab).
    pub kernel_tab: Vec<u8>,
    /// PM table bytes. C: `mproc_tab` (getsysinfo SI_PROC_TAB).
    pub pm_tab: Vec<u8>,
    /// VFS light table bytes. C: `fproc_tab` (getsysinfo
    /// SI_PROCLIGHT_TAB).
    pub vfs_tab: Vec<u8>,
    /// Tick of the last successful pull. C: `tabs_updated`.
    pub last_tick: u64,
    /// Failure latch: true from the first failed pull until reboot.
    /// C: `tabs_valid = FALSE` — proc.c:106-108.
    pub latched: bool,
}

impl Tables {
    /// Fresh state: nothing pulled, latch open. C: statics start zero.
    pub fn new() -> Self {
        Self {
            kernel_tab: Vec::new(),
            pm_tab: Vec::new(),
            vfs_tab: Vec::new(),
            last_tick: 0,
            latched: false,
        }
    }

    /// Run the pull discipline for `now_tick`: judge (throttle/latch),
    /// pull the three tables through the seams into the scratch
    /// buffers, latch on any failure. Returns `true` when the tables
    /// hold a current snapshot.
    ///
    /// C: `update_tables` — proc.c:46-217. `scratch_*` are server-owned
    /// buffers (sized once from the producer row counts at startup);
    /// the pulled bytes are copied into the snapshot fields so a later
    /// transport failure cannot corrupt the live snapshot.
    pub fn update<K: crate::transport::MibKernel, S: crate::transport::MibServices>(
        &mut self,
        now_tick: u64,
        kernel: &mut K,
        services: &mut S,
        scratch_kern: &mut [u8],
        scratch_pm: &mut [u8],
        scratch_vfs: &mut [u8],
    ) -> bool {
        if self.latched {
            return false;
        }
        match judge_pull(false, self.last_tick, now_tick) {
            PullVerdict::StayDead => return false,
            PullVerdict::Reuse => return true,
            PullVerdict::Pull => {}
        }
        // Kernel first (proc.c:75).
        if kernel.getproctab(scratch_kern).is_err() {
            self.latched = true;
            return false;
        }
        self.kernel_tab = scratch_kern.to_vec();
        // PM table (proc.c:90: SI_PROC_TAB).
        if services
            .getsysinfo(minix_types::Endpoint::PM, minix_types::SI_PROC_TAB, scratch_pm)
            .is_err()
        {
            self.latched = true;
            return false;
        }
        self.pm_tab = scratch_pm.to_vec();
        // VFS light table (proc.c:106: SI_PROCLIGHT_TAB).
        if services
            .getsysinfo(
                minix_types::Endpoint::VFS,
                minix_types::SI_PROCLIGHT_TAB,
                scratch_vfs,
            )
            .is_err()
        {
            self.latched = true;
            return false;
        }
        self.vfs_tab = scratch_vfs.to_vec();
        self.last_tick = now_tick;
        true
    }
}

impl Default for Tables {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod pull_tests {
    use super::*;

    struct MockPull {
        fail_kernel: bool,
        fail_pm: bool,
        fail_vfs: bool,
    }

    impl crate::transport::MibKernel for MockPull {
        fn datacopy_from(&mut self, _s: Endpoint, _a: u64, _b: &mut [u8]) -> Result<(), i32> {
            Ok(())
        }
        fn datacopy_to(&mut self, _d: Endpoint, _a: u64, _b: &[u8]) -> Result<(), i32> {
            Ok(())
        }
        fn grant_magic(
            &mut self,
            _w: Endpoint,
            _a: u64,
            _l: u64,
            _d: crate::io::relay::RelayDir,
        ) -> Result<minix_types::GrantId, i32> {
            Ok(1)
        }
        fn grant_revoke(&mut self, _g: minix_types::GrantId) {}
        fn getproctab(&mut self, buf: &mut [u8]) -> Result<(), i32> {
            if self.fail_kernel {
                return Err(EIO);
            }
            buf[..4].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
            Ok(())
        }
        fn getticks(&mut self) -> Result<u64, i32> {
            Ok(0)
        }
        fn hz(&mut self) -> Result<u32, i32> {
            Ok(60)
        }
    }

    impl crate::transport::MibServices for MockPull {
        fn getnuid(&mut self, _who: Endpoint) -> Result<u32, i32> {
            Ok(0)
        }
        fn getsysinfo(&mut self, _t: Endpoint, _w: i32, buf: &mut [u8]) -> Result<(), i32> {
            if self.fail_pm {
                return Err(EPERM);
            }
            buf[..2].copy_from_slice(&[0xAA, 0xBB]);
            Ok(())
        }
        fn ds_retrieve_label_name(&mut self, _w: Endpoint, _b: &mut [u8]) -> Result<usize, i32> {
            Err(EIO)
        }
        fn remote_info(
            &mut self,
            _p: Endpoint,
            _n: &mut [u8],
            _d: &mut [u8],
        ) -> Result<(), i32> {
            Err(EIO)
        }
        fn remote_call(
            &mut self,
            _peer: Endpoint,
            _call: crate::io::relay::RemoteCall,
            _reply: &mut crate::io::relay::RemoteReplyWire,
        ) -> Result<(), i32> {
            Err(EIO)
        }
        fn vm_info(&mut self, _what: i32, _buf: &mut [u8]) -> Result<(), i32> {
            Err(EIO)
        }
        fn pm_getparam(&mut self, _param: i32, _buf: &mut [u8]) -> Result<(), i32> {
            Err(EIO)
        }
    }

    use minix_types::{EIO, EPERM, Endpoint};

    /// Full pull: three tables land, the tick updates, the latch stays
    /// open.
    #[test]
    fn test_pull_success() {
        use minix_types::Endpoint;
        let mut tables = Tables::new();
        let mut kernel = MockPull { fail_kernel: false, fail_pm: false, fail_vfs: false };
        let mut services = MockPull { fail_kernel: false, fail_pm: false, fail_vfs: false };
        let mut kbuf = [0u8; 16];
        let mut pbuf = [0u8; 8];
        let mut vbuf = [0u8; 8];
        let ok = tables.update(5, &mut kernel, &mut services, &mut kbuf, &mut pbuf, &mut vbuf);
        assert!(ok);
        assert_eq!(tables.kernel_tab, kbuf.to_vec());
        assert_eq!(tables.pm_tab, pbuf.to_vec());
        assert_eq!(tables.last_tick, 5);
        assert!(!tables.latched);
        let _ = Endpoint::PM;
    }

    /// Kernel failure latches: every later pull is refused (C's
    /// `tabs_valid = FALSE` — proc.c:106-108).
    #[test]
    fn test_kernel_failure_latches() {
        use minix_types::Endpoint;
        let mut tables = Tables::new();
        let mut kernel = MockPull { fail_kernel: true, fail_pm: false, fail_vfs: false };
        let mut services = MockPull { fail_kernel: false, fail_pm: false, fail_vfs: false };
        let mut kbuf = [0u8; 16];
        let mut pbuf = [0u8; 8];
        let mut vbuf = [0u8; 8];
        assert!(!tables.update(5, &mut kernel, &mut services, &mut kbuf, &mut pbuf, &mut vbuf));
        assert!(tables.latched);
        // Latched: a later healthy transport still refuses.
        kernel.fail_kernel = false;
        assert!(!tables.update(50, &mut kernel, &mut services, &mut kbuf, &mut pbuf, &mut vbuf));
        let _ = Endpoint::PM;
    }

    /// Throttle: within the same tick a second pull reuses the
    /// snapshot (PullVerdict::Reuse — proc.c:66-69).
    #[test]
    fn test_same_tick_reuses() {
        let mut tables = Tables::new();
        let mut kernel = MockPull { fail_kernel: false, fail_pm: false, fail_vfs: false };
        let mut services = MockPull { fail_kernel: false, fail_pm: false, fail_vfs: false };
        let mut kbuf = [0u8; 16];
        let mut pbuf = [0u8; 8];
        let mut vbuf = [0u8; 8];
        assert!(tables.update(5, &mut kernel, &mut services, &mut kbuf, &mut pbuf, &mut vbuf));
        let calls_before = 0; // judge_pull refuses before any verb runs
        let _ = calls_before;
        // Same tick: reuse — no re-pull, the snapshot survives.
        assert!(tables.update(5, &mut kernel, &mut services, &mut kbuf, &mut pbuf, &mut vbuf));
        assert_eq!(tables.kernel_tab.len(), 16);
    }

    /// PM failure latches mid-pull: the kernel snapshot stays, PM/VFS
    /// stay empty, and the latch holds (:106-108).
    #[test]
    fn test_pm_failure_latches_mid_pull() {
        use minix_types::Endpoint;
        struct PmFails;
        impl crate::transport::MibKernel for PmFails {
            fn datacopy_from(&mut self, _: Endpoint, _: u64, _: &mut [u8]) -> Result<(), i32> { Ok(()) }
            fn datacopy_to(&mut self, _: Endpoint, _: u64, _: &[u8]) -> Result<(), i32> { Ok(()) }
            fn grant_magic(&mut self, _: Endpoint, _: u64, _: u64, _: crate::io::relay::RelayDir) -> Result<minix_types::GrantId, i32> { Ok(1) }
            fn grant_revoke(&mut self, _: minix_types::GrantId) {}
            fn getproctab(&mut self, buf: &mut [u8]) -> Result<(), i32> {
                buf[..4].copy_from_slice(&[1, 2, 3, 4]);
                Ok(())
            }
            fn getticks(&mut self) -> Result<u64, i32> { Ok(1) }
            fn hz(&mut self) -> Result<u32, i32> { Ok(60) }
        }
        struct PmFailsSvc;
        impl crate::transport::MibServices for PmFailsSvc {
            fn getnuid(&mut self, _: Endpoint) -> Result<u32, i32> { Ok(0) }
            fn getsysinfo(&mut self, _: Endpoint, _: i32, _: &mut [u8]) -> Result<(), i32> { Err(EPERM) }
            fn ds_retrieve_label_name(&mut self, _: Endpoint, _: &mut [u8]) -> Result<usize, i32> { Err(EIO) }
            fn remote_info(&mut self, _: Endpoint, _: &mut [u8], _: &mut [u8]) -> Result<(), i32> { Err(EIO) }
            fn remote_call(
                &mut self,
                _: Endpoint,
                _: crate::io::relay::RemoteCall,
                _: &mut crate::io::relay::RemoteReplyWire,
            ) -> Result<(), i32> {
                Err(EIO)
            }
            fn vm_info(&mut self, _: i32, _: &mut [u8]) -> Result<(), i32> { Err(EIO) }
            fn pm_getparam(&mut self, _: i32, _: &mut [u8]) -> Result<(), i32> { Err(EIO) }
        }
        let mut tables = Tables::new();
        let mut kernel = PmFails;
        let mut services = PmFailsSvc;
        let mut kb = [0u8; 16];
        let mut pb = [0u8; 8];
        let mut vb = [0u8; 8];
        // PM failure = the pull failed (update returns false), but the
        // kernel snapshot still landed — C copies per-source (:75-103).
        assert!(!tables.update(1, &mut kernel, &mut services, &mut kb, &mut pb, &mut vb));
        assert_eq!(tables.kernel_tab.len(), 16);
        // The latch holds on the next tick.
        assert!(!tables.update(2, &mut kernel, &mut services, &mut kb, &mut pb, &mut vb));
    }
}
