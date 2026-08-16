//! Service-slot configuration: the `rs_start` request parameters.
//!
//! Mirrors `minix3/minix/include/minix/rs.h:104-151` (`struct rs_start`) and
//! the validation/parsing logic of `check_request`
//! (`minix3/minix/servers/rs/request.c:1265-1308`) and `build_cmd_dep`
//! (`manager.c:289-323`). 08-rs-slot-config.md.
//!
//! The `sys_datacopy`-coupled steps (`copy_rs_start` — manager.c:135-146,
//! `copy_label` — manager.c:151-169, the `edit_slot` field writes —
//! manager.c:1460-1703) are wired through 19-rs-external-interfaces.md; this
//! module owns the pure validation and parsing that does not touch the
//! kernel.

use crate::privilege::IoRange;
use crate::sched::NR_SCHED_QUEUES;
use crate::service_slot::{Label, MAX_COMMAND_LEN, MAX_IPC_LIST, RS_NR_CONTROL};
use alloc::vec::Vec;
use minix_types::{Endpoint, Errno};

/// C: `RSS_NR_IRQ` — rs.h:25.
pub const RSS_NR_IRQ: usize = 16;
/// C: `RSS_NR_IO` — rs.h:26.
pub const RSS_NR_IO: usize = 16;
/// C: `RSS_IRQ_ALL` — rs.h:27.
pub const RSS_IRQ_ALL: i32 = RSS_NR_IRQ as i32 + 1;
/// C: `RSS_IO_ALL` — rs.h:28.
pub const RSS_IO_ALL: i32 = RSS_NR_IO as i32 + 1;
/// C: `RS_CPU_DEFAULT` — rs.h:63.
pub const RS_CPU_DEFAULT: i32 = -1;
/// C: `RS_CPU_BSP` — rs.h:64.
pub const RS_CPU_BSP: i32 = -2;
/// C: `LAST_SPECIAL_PROC_NR` — com.h:70.
pub const LAST_SPECIAL_PROC_NR: i32 = 11;

bitflags::bitflags! {
    /// The `rs_start` flags. C: `rss_flags` — rs.h:106, values rs.h:33-52.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RssFlags: u32 {
        /// Keep an in-memory copy of the binary. C: `RSS_COPY` — rs.h:33.
        const COPY = 0x01;
        /// Try to reuse a previously copied binary. C: `RSS_REUSE` — rs.h:34.
        const REUSE = 0x04;
        /// Unblock caller immediately. C: `RSS_NOBLOCK` — rs.h:35.
        const NOBLOCK = 0x08;
        /// Keep a replica of the service. C: `RSS_REPLICA` — rs.h:36.
        const REPLICA = 0x10;
        /// Batch mode. C: `RSS_BATCH` — rs.h:37.
        const BATCH = 0x20;
        /// Perform self update. C: `RSS_SELF_LU` — rs.h:38.
        const SELF_LU = 0x40;
        /// Perform ASR update. C: `RSS_ASR_LU` — rs.h:39.
        const ASR_LU = 0x80;
        /// Force self update. C: `RSS_FORCE_SELF_LU` — rs.h:40.
        const FORCE_SELF_LU = 0x0100;
        /// Request prepare-only update. C: `RSS_PREPARE_ONLY_LU` — rs.h:41.
        const PREPARE_ONLY_LU = 0x0200;
        /// Force crash at initialization time (debugging). C: `RSS_FORCE_INIT_CRASH` — rs.h:42.
        const FORCE_INIT_CRASH = 0x0400;
        /// Force failure at initialization time (debugging). C: `RSS_FORCE_INIT_FAIL` — rs.h:43.
        const FORCE_INIT_FAIL = 0x0800;
        /// Force timeout at initialization time (debugging). C: `RSS_FORCE_INIT_TIMEOUT` — rs.h:44.
        const FORCE_INIT_TIMEOUT = 0x1000;
        /// Force default cb at initialization time (debugging). C: `RSS_FORCE_INIT_DEFCB` — rs.h:45.
        const FORCE_INIT_DEFCB = 0x2000;
        /// Include basic kernel calls. C: `RSS_SYS_BASIC_CALLS` — rs.h:46.
        const SYS_BASIC_CALLS = 0x4000;
        /// Include basic vm calls. C: `RSS_VM_BASIC_CALLS` — rs.h:47.
        const VM_BASIC_CALLS = 0x8000;
        /// Don't inherit mmapped regions. C: `RSS_NOMMAP_LU` — rs.h:48.
        const NOMMAP_LU = 0x10000;
        /// Detach on update/restart. C: `RSS_DETACH` — rs.h:49.
        const DETACH = 0x20000;
        /// Don't restart. C: `RSS_NORESTART` — rs.h:50.
        const NORESTART = 0x40000;
        /// Force state transfer at initialization time. C: `RSS_FORCE_INIT_ST` — rs.h:51.
        const FORCE_INIT_ST = 0x80000;
        /// Suppress binary exponential offset. C: `RSS_NO_BIN_EXP` — rs.h:52.
        const NO_BIN_EXP = 0x100000;
    }
}

/// The service-start parameters carried by `RS_UP`/`RS_EDIT`.
///
/// C: `struct rs_start` — rs.h:104-151. Byte-buffer fields keep the kernel
/// message payload shape; pointer fields (`rss_cmd`/`rss_ipc`/…) become
/// fixed arrays + lengths (the `sys_datacopy` sources, manager.c:1476-1483,
/// 1578-1582).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsStart {
    /// C: `rss_flags` — rs.h:106.
    pub flags: RssFlags,
    /// C: `rss_uid` — rs.h:109.
    pub uid: u32,
    /// C: `rss_sigmgr` — rs.h:110.
    pub sigmgr: Endpoint,
    /// C: `rss_scheduler` — rs.h:111.
    pub scheduler: Endpoint,
    /// C: `rss_priority` — rs.h:112.
    pub priority: i32,
    /// C: `rss_quantum` — rs.h:113.
    pub quantum: i32,
    /// C: `rss_cpu` — rs.h:150 (SMP tail; `RS_CPU_DEFAULT`/`RS_CPU_BSP`).
    pub cpu: i32,
    /// C: `rss_period` — rs.h:115.
    pub period: i64,
    /// C: `rss_restarts` — rs.h:119.
    pub restarts: i64,
    /// C: `rss_asr_count` — rs.h:118.
    pub asr_count: i64,
    /// C: `rss_cmd`/`rss_cmdlen` — rs.h:107-108.
    pub cmd: [u8; MAX_COMMAND_LEN],
    /// Length of `cmd`. C: `rss_cmdlen`.
    pub cmdlen: usize,
    /// C: `rss_ipc`/`rss_ipclen` — rs.h:133-134.
    pub ipc_list: [u8; MAX_IPC_LIST],
    /// Length of the IPC list. C: `rss_ipclen`.
    pub ipclen: usize,
    /// C: `rss_progname`/`rss_prognamelen` — rs.h:140-141.
    pub progname: Label,
    /// Number of control entries. C: `int rss_nr_control` — rs.h:136.
    /// `i32` (not `usize`): C `int` may carry the pre-validation negative
    /// "unset/invalid" state (R2 — todo §14.1); `ServiceSlot.nr_control`
    /// (type.h:106) already uses `i32`.
    pub nr_control: i32,
    /// C: `rss_control` — rs.h:137.
    pub control: [Label; RS_NR_CONTROL],
    /// Number of IRQs. C: `int rss_nr_irq` — rs.h:122.
    /// `i32`: must hold the `RSS_IRQ_ALL` sentinel (17, rs.h:27) and the
    /// pre-validation negative "invalid" state (manager.c:1492 checks
    /// `> NR_IRQ` on a possibly negative count).
    pub nr_irq: i32,
    /// C: `rss_irq` — rs.h:123.
    pub irq: [i32; RSS_NR_IRQ],
    /// Number of I/O ranges. C: `int rss_nr_io` — rs.h:124.
    /// `i32`: must hold the `RSS_IO_ALL` sentinel (17, rs.h:28) and the
    /// pre-validation negative "invalid" state (manager.c:1510 checks
    /// `> NR_IO_RANGE` on a possibly negative count).
    pub nr_io: i32,
    /// C: `rss_io` — rs.h:125 (`struct { unsigned base; unsigned len; }`).
    /// Single authority: `privilege::IoRange` (N9 — todo §11) is the same
    /// shape as the kernel `struct io_range` the entries are copied into
    /// (`edit_slot` → `s_io_tab`, manager.c:1516-1518); using it here avoids
    /// the C "two same-shape tables" duplication (D6).
    pub io: [IoRange; RSS_NR_IO],
}

impl Default for RsStart {
    /// Mirrors the C caller-side defaults injected by minix-service
    /// (parse.c:1160-1169): `memset(rs_config, 0, sizeof)` zeroes every
    /// count/table, then `sigmgr=DSRV_SM=ROOT_SYS_PROC_NR`,
    /// `scheduler=DSRV_SCH=SCHED_PROC_NR`, `priority=DSRV_Q=USER_Q=7`,
    /// `quantum=DSRV_QT=USER_QUANTUM=200`, `cpu=DSRV_CPU=-1` (priv.h:84,
    /// 89, 94, 99, 103 + config.h:69/74/77). The previous `SELF`/`KERNEL`/
    /// `quantum=1` values were "unset" sentinels, not C defaults — any
    /// path constructing via `Default` then partially filling got a
    /// scheduler configuration 200× off from the C caller (D5).
    fn default() -> Self {
        Self {
            flags: RssFlags::empty(),
            uid: 0,
            sigmgr: Endpoint::RS,       // DSRV_SM = ROOT_SYS_PROC_NR (priv.h:84)
            scheduler: Endpoint::SCHED, // DSRV_SCH = SCHED_PROC_NR (priv.h:89)
            priority: crate::sched::USER_Q, // DSRV_Q (priv.h:94, config.h:69)
            quantum: crate::sched::USER_QUANTUM, // DSRV_QT (priv.h:99, config.h:74)
            cpu: RS_CPU_DEFAULT,
            period: 0,
            restarts: 0,
            asr_count: 0,
            cmd: [0; MAX_COMMAND_LEN],
            cmdlen: 0,
            ipc_list: [0; MAX_IPC_LIST],
            ipclen: 0,
            progname: Label::empty(),
            nr_control: 0,
            control: [Label::empty(); RS_NR_CONTROL],
            nr_irq: 0,
            irq: [0; RSS_NR_IRQ],
            nr_io: 0,
            io: [IoRange::default(); RSS_NR_IO],
        }
    }
}

/// Validates the scheduling/CPU/signal-manager parameters of a request.
///
/// C: `check_request` — request.c:1265-1308. The CPU special values
/// (`RS_CPU_BSP`/`RS_CPU_DEFAULT`) resolve against `machine` state that this
/// pure function does not see; callers pass `bsp_id`/`processors_count`
/// (01/19: `sys_getmachine`). R13: C rewrites `rs_start.rss_cpu` in place
/// (request.c:1286-1296); the returned resolved `cpu` MUST be consumed by
/// the 12 wiring (write it back to the slot's `cpu`) — dropping it leaves
/// `RS_CPU_BSP` (-2) in the slot and the scheduler sees a bogus affinity.
pub fn check_request(rs_start: &RsStart, bsp_id: u32, processors_count: u32) -> Result<i32, Errno> {
    // Scheduler must be KERNEL or a valid special process (request.c:1268-1274).
    if rs_start.scheduler != Endpoint::KERNEL
        && (rs_start.scheduler.get() < 0 || rs_start.scheduler.get() > LAST_SPECIAL_PROC_NR)
    {
        return Err(Errno::EINVAL);
    }
    // Priority must be within the scheduling queues (request.c:1275-1279).
    if rs_start.priority >= NR_SCHED_QUEUES {
        return Err(Errno::EINVAL);
    }
    // Quantum must be positive (request.c:1280-1284).
    if rs_start.quantum <= 0 {
        return Err(Errno::EINVAL);
    }
    // CPU resolution (request.c:1286-1296):
    //   BSP → bsp_id; DEFAULT → keep; negative → EINVAL; > count → BSP.
    let cpu = match rs_start.cpu {
        RS_CPU_BSP => bsp_id as i32,
        RS_CPU_DEFAULT => rs_start.cpu,
        c if c < 0 => return Err(Errno::EINVAL),
        c if c as u32 > processors_count => bsp_id as i32,
        c => c,
    };
    // Signal manager must be SELF or a valid special process
    // (request.c:1299-1305).
    if rs_start.sigmgr != Endpoint::SELF
        && (rs_start.sigmgr.get() < 0 || rs_start.sigmgr.get() > LAST_SPECIAL_PROC_NR)
    {
        return Err(Errno::EINVAL);
    }
    Ok(cpu)
}

/// Builds the argument vector from the raw command string.
///
/// C: `build_cmd_dep` — manager.c:289-323: the format is
/// `path, arguments..., NULL`; spaces separate arguments; an empty trailing
/// argument is dropped; the vector is capped at `ARGV_ELEMENTS - 1` entries
/// (one slot reserved for the terminating `None`).
///
/// The tokens borrow `cmd` and are **not** truncated at `RS_MAX_LABEL_LEN`:
/// C stores the full token bytes in `r_args` (manager.c:301-302) and points
/// argv at them — `Vec<&[u8]>` is the Rust equivalent of that argv layout.
/// Parsing stops at the first NUL, mirroring the C `while (*cmd_ptr != '\0')`
/// loop (manager.c:306) and `strcpy` (manager.c:301): bytes after the NUL
/// padding of a fixed-size `cmd` buffer are not part of the command.
pub fn build_cmd_dep(cmd: &[u8]) -> Vec<&[u8]> {
    let mut args = Vec::new();
    let rest = cmd;
    let mut i = 0;
    while i < rest.len() {
        // Skip spaces (C: manager.c:308).
        while i < rest.len() && rest[i] == b' ' {
            i += 1;
        }
        if i >= rest.len() || rest[i] == 0 {
            break; // C: manager.c:306/309 — string end, no argument follows
        }
        let start = i;
        while i < rest.len() && rest[i] != b' ' && rest[i] != 0 {
            i += 1;
        }
        if args.len() >= crate::service_slot::ARGV_ELEMENTS - 1 {
            break; // C: manager.c:311-315 — arg vector full
        }
        args.push(&rest[start..i]);
        if i >= rest.len() || rest[i] == 0 {
            break;
        }
    }
    if args.is_empty() {
        // N11: C unconditionally assigns `r_argv[0] = r_args` before parsing
        // (manager.c:299-300) — an empty, all-space or NUL-leading command
        // yields `argv = [""]` (argc=1), never an empty vector. Aligning
        // keeps the 09/10 exec rebuild faithful: C execs an empty path
        // (ENOENT); argc=0 would take a different failure path.
        args.push(&rest[..0]);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_request_ok() {
        let s = RsStart {
            scheduler: Endpoint::SCHED,
            priority: 5,
            quantum: 100,
            cpu: RS_CPU_DEFAULT,
            sigmgr: Endpoint::SELF,
            ..RsStart::default()
        };
        assert_eq!(check_request(&s, 0, 4), Ok(RS_CPU_DEFAULT));
    }

    #[test]
    fn test_check_request_scheduler() {
        let mut s = RsStart::default();
        s.scheduler = Endpoint::VFS; // 1, within LAST_SPECIAL_PROC_NR
        assert!(check_request(&s, 0, 4).is_ok());
        s.scheduler = Endpoint::from_generation_slot(0, 12); // > 11
        assert_eq!(check_request(&s, 0, 4), Err(Errno::EINVAL));
    }

    #[test]
    fn test_check_request_priority_quantum() {
        let mut s = RsStart::default();
        s.priority = NR_SCHED_QUEUES;
        assert_eq!(check_request(&s, 0, 4), Err(Errno::EINVAL));
        s.priority = 0;
        s.quantum = 0;
        assert_eq!(check_request(&s, 0, 4), Err(Errno::EINVAL));
    }

    #[test]
    fn test_check_request_cpu() {
        let s = RsStart {
            cpu: RS_CPU_BSP,
            ..RsStart::default()
        };
        assert_eq!(check_request(&s, 2, 4), Ok(2));
        let s = RsStart {
            cpu: 3,
            ..RsStart::default()
        };
        assert_eq!(check_request(&s, 2, 4), Ok(3));
        let s = RsStart {
            cpu: 5,
            ..RsStart::default()
        }; // > count(4) → BSP
        assert_eq!(check_request(&s, 2, 4), Ok(2));
        let s = RsStart {
            cpu: -3,
            ..RsStart::default()
        };
        assert_eq!(check_request(&s, 2, 4), Err(Errno::EINVAL));
    }

    #[test]
    fn test_check_request_sigmgr() {
        let mut s = RsStart::default();
        s.sigmgr = Endpoint::PM;
        assert!(check_request(&s, 0, 4).is_ok());
        s.sigmgr = Endpoint::from_generation_slot(0, 12);
        assert_eq!(check_request(&s, 0, 4), Err(Errno::EINVAL));
    }

    #[test]
    fn test_rs_start_default_matches_c_caller() {
        // D5: minix-service parse.c:1164-1169 — DSRV_SM=ROOT_SYS_PROC_NR,
        // DSRV_SCH=SCHED_PROC_NR, DSRV_Q=USER_Q=7, DSRV_QT=USER_QUANTUM=200,
        // DSRV_CPU=USER_DEFAULT_CPU=-1 (priv.h:84/89/94/99/103, config.h:69/74/77).
        // Resource counts/tables are zeroed by the caller's
        // `memset(rs_config, 0, sizeof)` (parse.c:1160).
        let s = RsStart::default();
        assert_eq!(s.sigmgr, Endpoint::RS);
        assert_eq!(s.scheduler, Endpoint::SCHED);
        assert_eq!(s.priority, crate::sched::USER_Q);
        assert_eq!(s.priority, 7);
        assert_eq!(s.quantum, crate::sched::USER_QUANTUM);
        assert_eq!(s.quantum, 200);
        assert_eq!(s.cpu, RS_CPU_DEFAULT);
        assert_eq!(s.nr_control, 0);
        assert_eq!(s.nr_irq, 0);
        assert!(s.irq.iter().all(|&i| i == 0));
        assert_eq!(s.nr_io, 0);
        assert!(s.io.iter().all(|r| *r == IoRange::default()));
        // Valid under check_request (the C path validates before use).
        assert!(check_request(&s, 0, 4).is_ok());
    }

    #[test]
    fn test_rs_start_resource_counts_are_i32_like_c_int() {
        // R20a/§15: C counts are `int` (rs.h:122/124/136). `i32` keeps the
        // pre-`edit_slot` states representable (manager.c:1486-1521):
        //   - `RSS_IRQ_ALL`/`RSS_IO_ALL` = 17 sentinels ("all resources");
        //   - negative counts (invalid, rejected by the `> NR_IRQ`/`> NR_IO_RANGE`
        //     checks in edit_slot, 19 wiring).
        // `usize` would silently accept both and blur the "unset" vs "invalid"
        // distinction the C `int` carries before validation (R2 rationale).
        let mut s = RsStart::default();
        s.nr_irq = RSS_IRQ_ALL;
        s.nr_io = RSS_IO_ALL;
        assert_eq!(s.nr_irq, 17);
        assert_eq!(s.nr_io, 17);
        s.nr_irq = -1;
        s.nr_io = -1;
        s.nr_control = -1;
        assert_eq!((s.nr_irq, s.nr_io, s.nr_control), (-1, -1, -1));
    }

    #[test]
    fn test_build_cmd_dep() {
        let args = build_cmd_dep(b"/sbin/tty -a -b");
        let strs: Vec<&str> = args
            .iter()
            .map(|t| core::str::from_utf8(t).unwrap())
            .collect();
        assert_eq!(strs, vec!["/sbin/tty", "-a", "-b"]);
    }

    #[test]
    fn test_build_cmd_dep_trailing_spaces() {
        let args = build_cmd_dep(b"/bin/echo hi   ");
        let strs: Vec<&str> = args
            .iter()
            .map(|t| core::str::from_utf8(t).unwrap())
            .collect();
        assert_eq!(strs, vec!["/bin/echo", "hi"]);
    }

    #[test]
    fn test_build_cmd_dep_empty_cmd_keeps_argv0() {
        // N11: C unconditionally assigns argv[0] = r_args (manager.c:299-
        // 300) — an empty/all-space command yields argv=[""], argc=1, not
        // argc=0 (exec of an empty path then fails with ENOENT like C).
        assert_eq!(build_cmd_dep(b"").len(), 1);
        assert!(build_cmd_dep(b"")[0].is_empty());
        assert_eq!(build_cmd_dep(b"   ").len(), 1);
        assert!(build_cmd_dep(b"   ")[0].is_empty());
        assert_eq!(build_cmd_dep(b"\0").len(), 1);
        assert!(build_cmd_dep(b"\0")[0].is_empty());
    }

    #[test]
    fn test_build_cmd_dep_stops_at_nul() {
        // C: manager.c:306 — parsing stops at the NUL terminator; bytes in the
        // fixed-size cmd buffer after the NUL are padding, not arguments.
        let args = build_cmd_dep(b"/sbin/tty -a\0garbage -b");
        let strs: Vec<&str> = args
            .iter()
            .map(|t| core::str::from_utf8(t).unwrap())
            .collect();
        assert_eq!(strs, vec!["/sbin/tty", "-a"]);
    }

    #[test]
    fn test_build_cmd_dep_long_token_not_truncated() {
        // Tokens are not capped at RS_MAX_LABEL_LEN (16): C keeps full bytes
        // in r_args (manager.c:301-302); argv points into that buffer.
        let cmd = b"/usr/sbin/very-long-service-name --flag";
        let args = build_cmd_dep(cmd);
        assert_eq!(args[0], b"/usr/sbin/very-long-service-name".as_slice());
        assert_eq!(args[1], b"--flag".as_slice());
        assert!(args[0].len() > crate::service_slot::RS_MAX_LABEL_LEN);
    }

    #[test]
    fn test_build_cmd_dep_argv_cap() {
        let mut cmd = Vec::new();
        cmd.extend_from_slice(b"/sbin/x");
        for i in 0..crate::service_slot::ARGV_ELEMENTS {
            cmd.push(b' ');
            cmd.extend_from_slice(format!("a{i}").as_bytes());
        }
        let args = build_cmd_dep(&cmd);
        // One slot reserved for the terminating NULL (manager.c:311-315).
        assert!(args.len() <= crate::service_slot::ARGV_ELEMENTS - 1);
    }

    #[test]
    fn test_rss_constants() {
        assert_eq!(RSS_IRQ_ALL, 17);
        assert_eq!(RSS_IO_ALL, 17);
        assert_eq!(RS_CPU_DEFAULT, -1);
        assert_eq!(RS_CPU_BSP, -2);
        assert_eq!(RssFlags::COPY.bits(), 0x01);
        assert_eq!(RssFlags::NO_BIN_EXP.bits(), 0x100000);
    }
}
