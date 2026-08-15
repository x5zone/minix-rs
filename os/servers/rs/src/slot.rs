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

use crate::service_slot::{Label, MAX_COMMAND_LEN, MAX_IPC_LIST, RS_NR_CONTROL};
use alloc::vec::Vec;
use minix_types::{EINVAL, Endpoint};

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
/// C: `NR_SCHED_QUEUES` — config.h:66.
pub const NR_SCHED_QUEUES: i32 = 16;
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
    /// Number of control entries. C: `rss_nr_control` — rs.h:136.
    pub nr_control: usize,
    /// C: `rss_control` — rs.h:137.
    pub control: [Label; RS_NR_CONTROL],
    /// Number of IRQs. C: `rss_nr_irq` — rs.h:122.
    pub nr_irq: usize,
    /// C: `rss_irq` — rs.h:123.
    pub irq: [i32; RSS_NR_IRQ],
}

impl Default for RsStart {
    fn default() -> Self {
        Self {
            flags: RssFlags::empty(),
            uid: 0,
            sigmgr: Endpoint::SELF,
            scheduler: Endpoint::KERNEL,
            priority: 0,
            quantum: 1,
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
        }
    }
}

/// Validates the scheduling/CPU/signal-manager parameters of a request.
///
/// C: `check_request` — request.c:1265-1308. The CPU special values
/// (`RS_CPU_BSP`/`RS_CPU_DEFAULT`) resolve against `machine` state that this
/// pure function does not see; callers pass `bsp_id`/`processors_count`
/// (01/19: `sys_getmachine`).
pub fn check_request(rs_start: &RsStart, bsp_id: u32, processors_count: u32) -> Result<i32, i32> {
    // Scheduler must be KERNEL or a valid special process (request.c:1268-1274).
    if rs_start.scheduler != Endpoint::KERNEL
        && (rs_start.scheduler.get() < 0 || rs_start.scheduler.get() > LAST_SPECIAL_PROC_NR)
    {
        return Err(EINVAL);
    }
    // Priority must be within the scheduling queues (request.c:1275-1279).
    if rs_start.priority >= NR_SCHED_QUEUES {
        return Err(EINVAL);
    }
    // Quantum must be positive (request.c:1280-1284).
    if rs_start.quantum <= 0 {
        return Err(EINVAL);
    }
    // CPU resolution (request.c:1286-1296):
    //   BSP → bsp_id; DEFAULT → keep; negative → EINVAL; > count → BSP.
    let cpu = match rs_start.cpu {
        RS_CPU_BSP => bsp_id as i32,
        RS_CPU_DEFAULT => rs_start.cpu,
        c if c < 0 => return Err(EINVAL),
        c if c as u32 > processors_count => bsp_id as i32,
        c => c,
    };
    // Signal manager must be SELF or a valid special process
    // (request.c:1299-1305).
    if rs_start.sigmgr != Endpoint::SELF
        && (rs_start.sigmgr.get() < 0 || rs_start.sigmgr.get() > LAST_SPECIAL_PROC_NR)
    {
        return Err(EINVAL);
    }
    Ok(cpu)
}

/// Builds the argument vector from the raw command string.
///
/// C: `build_cmd_dep` — manager.c:289-323: the format is
/// `path, arguments..., NULL`; spaces separate arguments; an empty trailing
/// argument is dropped; the vector is capped at `ARGV_ELEMENTS - 1` entries
/// (one slot reserved for the terminating `None`).
pub fn build_cmd_dep(cmd: &[u8]) -> Vec<Label> {
    let mut args = Vec::new();
    // C: strcpy(rp->r_args, rp->r_cmd); argv[0] = r_args (manager.c:301-302).
    // The path is the first whitespace-delimited token.
    let rest = cmd;
    let mut i = 0;
    while i < rest.len() {
        // Skip spaces (C: manager.c:308).
        while i < rest.len() && rest[i] == b' ' {
            i += 1;
        }
        if i >= rest.len() {
            break; // C: manager.c:309 — no argument follows trailing spaces
        }
        let start = i;
        while i < rest.len() && rest[i] != b' ' {
            i += 1;
        }
        if args.len() >= crate::service_slot::ARGV_ELEMENTS - 1 {
            break; // C: manager.c:311-315 — arg vector full
        }
        args.push(Label::from_bytes(&rest[start..i]));
        if i >= rest.len() {
            break;
        }
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
        assert_eq!(check_request(&s, 0, 4), Err(EINVAL));
    }

    #[test]
    fn test_check_request_priority_quantum() {
        let mut s = RsStart::default();
        s.priority = NR_SCHED_QUEUES;
        assert_eq!(check_request(&s, 0, 4), Err(EINVAL));
        s.priority = 0;
        s.quantum = 0;
        assert_eq!(check_request(&s, 0, 4), Err(EINVAL));
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
        assert_eq!(check_request(&s, 2, 4), Err(EINVAL));
    }

    #[test]
    fn test_check_request_sigmgr() {
        let mut s = RsStart::default();
        s.sigmgr = Endpoint::PM;
        assert!(check_request(&s, 0, 4).is_ok());
        s.sigmgr = Endpoint::from_generation_slot(0, 12);
        assert_eq!(check_request(&s, 0, 4), Err(EINVAL));
    }

    #[test]
    fn test_build_cmd_dep() {
        let args = build_cmd_dep(b"/sbin/tty -a -b");
        let strs: Vec<&str> = args.iter().filter_map(|l| l.as_str()).collect();
        assert_eq!(strs, vec!["/sbin/tty", "-a", "-b"]);
    }

    #[test]
    fn test_build_cmd_dep_trailing_spaces() {
        let args = build_cmd_dep(b"/bin/echo hi   ");
        let strs: Vec<&str> = args.iter().filter_map(|l| l.as_str()).collect();
        assert_eq!(strs, vec!["/bin/echo", "hi"]);
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
