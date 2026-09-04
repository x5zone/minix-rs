//! CTL_KERN subtree: constants, functions, and the mock that waits.
//!
//! Mirrors `kern.c` (all 508 lines): two verify callbacks, ten function
//! handlers, the data table (45 populated of 84 slots), and the mock
//! `kern.ipc` subtree that the IPC service covers when it runs (12).
//! Cross-service reads (`sys_hz`, `getticks`, `sys_getcputicks`,
//! `getsysinfo`, `svrctl`, `getuptime`, `cpuavg`) are transport effects
//! (A-12); every *decision* — bounds, modes, table shape — is judged here.
//!
//! 13-mib-subtree-kern.md.

use minix_types::{
    EINVAL, KERN_ARGMAX, KERN_BOOTTIME, KERN_CCPU, KERN_CLOCKRATE, KERN_CONSDEV, KERN_CP_TIME,
    KERN_DOMAINNAME, KERN_DRIVERS, KERN_DUMP_ON_PANIC, KERN_FORKFSLEEP, KERN_FSCALE, KERN_FSYNC,
    KERN_HARDCLOCK_TICKS, KERN_HOSTID, KERN_HOSTNAME, KERN_IOV_MAX, KERN_JOB_CONTROL, KERN_LWP,
    KERN_MAPPED_FILES, KERN_MAXFILES, KERN_MAXPARTITIONS, KERN_MAXPHYS, KERN_MAXPROC, KERN_MAXPTYS,
    KERN_MAXVNODES, KERN_MEMLOCK, KERN_MEMLOCK_RANGE, KERN_MEMORY_PROTECTION, KERN_MONOTONIC_CLOCK,
    KERN_MSGBUFSIZE, KERN_NGROUPS, KERN_OSRELEASE, KERN_OSREV, KERN_OSTYPE, KERN_POSIX1,
    KERN_PROC_ARGS, KERN_PROC2, KERN_PROF, KERN_ROOT_DEVICE, KERN_RTC_OFFSET, KERN_SAVED_IDS,
    KERN_SECURELVL, KERN_SYNCHRONIZED_IO, KERN_SYSVIPC, KERN_VERSION,
};

/// Handler behind a kern function node.
///
/// C: the ten `mib_kern_*` functions. `Proc2`/`ProcArgs`/`Lwp` bodies
/// live in 17/18/19 (proc.c); the rest are decided here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernFunc {
    /// `mib_kern_clockrate` — kern.c:36-58.
    Clockrate,
    /// `mib_kern_profiling` — always `EOPNOTSUPP` (:64-71, A-9).
    Profiling,
    /// `mib_kern_hardclock_ticks` — getticks snapshot (:77-90).
    HardclockTicks,
    /// `mib_kern_root_device` — PM `rootdevname` via svrctl (:96-114).
    RootDevice,
    /// `mib_kern_ccpu` — scheduler decay value (:120-129).
    Ccpu,
    /// `mib_kern_cp_time` — per-CPU tick vectors (:136-187).
    CpTime,
    /// `mib_kern_consdev` — console dev_t (:193-203).
    Consdev,
    /// `mib_kern_drivers` — VFS dmap table walk (:222-281).
    Drivers,
    /// `mib_kern_boottime` — uptime as timeval (:287-299).
    Boottime,
    /// `mib_kern_ipc_info` — mock, covered by IPC's mount (:306-316).
    IpcInfo,
    /// `mib_kern_proc2` — body in 18 (proc.c).
    Proc2,
    /// `mib_kern_proc_args` — body in 19 (proc.c).
    ProcArgs,
    /// `mib_kern_lwp` — body in 17 (proc.c).
    Lwp,
}

/// Verify callback behind a kern data node.
///
/// C: `mib_kern_securelvl` (:18-30), `mib_kern_forkfsleep` (:209-217).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernVerify {
    /// Securelevel only rises (mock: no real levels). C: `:29`.
    Securelvl,
    /// `0 <= v <= MAXSLP * 1000` (NetBSD rules). C: `:216`.
    Forkfsleep,
}

/// Shape of one populated kern slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernKind {
    /// Constant integer leaf with a literal value. C: `MIB_INT(_P | _RO, literal, ...)`.
    ConstInt(i32),
    /// Constant integer leaf backed by a build macro (value owned by the
    /// build/headers, named here). C: `MIB_INT(_P | _RO, MACRO, ...)`.
    BuildInt(&'static str),
    /// Constant string leaf. C: `MIB_STRING(_P | _RO, text, ...)`.
    ConstStr(&'static str),
    /// Writable string leaf (hostname/domainname buffers). C: `MIB_STRING(_P | _RW, ...)`.
    VarStr,
    /// Writable integer leaf. C: `MIB_INT(_P | _RW, ...)`.
    VarInt,
    /// Verified integer leaf. C: `MIB_INTV(_P | _RW, init, verify, ...)`.
    VerifyInt {
        /// Initial value. C: `-1` (securelvl), `0` (forkfsleep).
        init: i32,
        /// Gate for new values.
        verify: KernVerify,
    },
    /// Function node. C: `MIB_FUNC(...)`.
    Func(KernFunc),
    /// The mock IPC subtree (covered by IPC's mount when it runs).
    /// C: `MIB_NODE(..., mib_kern_ipc_table, ...)` — kern.c:492.
    IpcTable,
}

/// One populated kern slot: id, name, shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernEntry {
    /// Slot id (`KERN_*`). C: table index (`[KERN_X]`).
    pub id: i32,
    /// Node name. C: `node_name`.
    pub name: &'static str,
    /// Node shape. C: macro + handler/verify.
    pub kind: KernKind,
}

/// The populated kern slots, id-sorted (45 of 84; the rest are A-9).
///
/// C: `mib_kern_table[]` — kern.c:332-498. Values for `ConstInt` are the
/// table literals (`NR_VNODES`, `ARG_MAX`, `1` for fsync/mapped_files,
/// `0` for the unsupported-option zeros, `4MB` maxphys); `ConstStr`
/// values (`OS_NAME` etc.) are build strings owned by the build, named
/// here by role.
pub const KERN_ENTRIES: &[KernEntry] = &[
    KernEntry {
        id: KERN_OSTYPE,
        name: "ostype",
        kind: KernKind::ConstStr("OS_NAME"),
    },
    KernEntry {
        id: KERN_OSRELEASE,
        name: "osrelease",
        kind: KernKind::ConstStr("OS_RELEASE"),
    },
    KernEntry {
        id: KERN_OSREV,
        name: "osrevision",
        kind: KernKind::BuildInt("OS_REV"),
    },
    KernEntry {
        id: KERN_VERSION,
        name: "version",
        kind: KernKind::ConstStr("OS_VERSION"),
    },
    KernEntry {
        id: KERN_MAXVNODES,
        name: "maxvnodes",
        kind: KernKind::BuildInt("NR_VNODES"),
    },
    KernEntry {
        id: KERN_MAXPROC,
        name: "maxproc",
        kind: KernKind::BuildInt("NR_PROCS"),
    },
    KernEntry {
        id: KERN_MAXFILES,
        name: "maxfiles",
        kind: KernKind::BuildInt("NR_VNODES"),
    },
    KernEntry {
        id: KERN_ARGMAX,
        name: "argmax",
        kind: KernKind::BuildInt("ARG_MAX"),
    },
    KernEntry {
        id: KERN_SECURELVL,
        name: "securelevel",
        kind: KernKind::VerifyInt {
            init: -1,
            verify: KernVerify::Securelvl,
        },
    },
    KernEntry {
        id: KERN_HOSTNAME,
        name: "hostname",
        kind: KernKind::VarStr,
    },
    KernEntry {
        id: KERN_HOSTID,
        name: "hostid",
        kind: KernKind::VarInt,
    },
    KernEntry {
        id: KERN_CLOCKRATE,
        name: "clockrate",
        kind: KernKind::Func(KernFunc::Clockrate),
    },
    KernEntry {
        id: KERN_PROF,
        name: "profiling",
        kind: KernKind::Func(KernFunc::Profiling),
    },
    KernEntry {
        id: KERN_POSIX1,
        name: "posix1version",
        kind: KernKind::BuildInt("_POSIX_VERSION"),
    },
    KernEntry {
        id: KERN_NGROUPS,
        name: "ngroups",
        kind: KernKind::BuildInt("NGROUPS_MAX"),
    },
    KernEntry {
        id: KERN_JOB_CONTROL,
        name: "job_control",
        kind: KernKind::ConstInt(0),
    },
    KernEntry {
        id: KERN_SAVED_IDS,
        name: "saved_ids",
        kind: KernKind::ConstInt(0),
    },
    KernEntry {
        id: KERN_DOMAINNAME,
        name: "domainname",
        kind: KernKind::VarStr,
    },
    KernEntry {
        id: KERN_MAXPARTITIONS,
        name: "maxpartitions",
        kind: KernKind::BuildInt("NR_PARTITIONS"),
    },
    KernEntry {
        id: KERN_RTC_OFFSET,
        name: "rtc_offset",
        kind: KernKind::VarInt,
    },
    KernEntry {
        id: KERN_ROOT_DEVICE,
        name: "root_device",
        kind: KernKind::Func(KernFunc::RootDevice),
    },
    KernEntry {
        id: KERN_MSGBUFSIZE,
        name: "msgbufsize",
        kind: KernKind::BuildInt("DIAG_BUFSIZE"),
    },
    KernEntry {
        id: KERN_FSYNC,
        name: "fsync",
        kind: KernKind::ConstInt(1),
    },
    KernEntry {
        id: KERN_SYNCHRONIZED_IO,
        name: "synchronized_io",
        kind: KernKind::ConstInt(0),
    },
    KernEntry {
        id: KERN_IOV_MAX,
        name: "iov_max",
        kind: KernKind::BuildInt("IOV_MAX"),
    },
    KernEntry {
        id: KERN_MAPPED_FILES,
        name: "mapped_files",
        kind: KernKind::ConstInt(1),
    },
    KernEntry {
        id: KERN_MEMLOCK,
        name: "memlock",
        kind: KernKind::ConstInt(0),
    },
    KernEntry {
        id: KERN_MEMLOCK_RANGE,
        name: "memlock_range",
        kind: KernKind::ConstInt(0),
    },
    KernEntry {
        id: KERN_MEMORY_PROTECTION,
        name: "memory_protection",
        kind: KernKind::ConstInt(0),
    },
    KernEntry {
        id: KERN_PROC2,
        name: "proc2",
        kind: KernKind::Func(KernFunc::Proc2),
    },
    KernEntry {
        id: KERN_PROC_ARGS,
        name: "proc_args",
        kind: KernKind::Func(KernFunc::ProcArgs),
    },
    KernEntry {
        id: KERN_FSCALE,
        name: "fscale",
        kind: KernKind::BuildInt("FSCALE"),
    },
    KernEntry {
        id: KERN_CCPU,
        name: "ccpu",
        kind: KernKind::Func(KernFunc::Ccpu),
    },
    KernEntry {
        id: KERN_CP_TIME,
        name: "cp_time",
        kind: KernKind::Func(KernFunc::CpTime),
    },
    KernEntry {
        id: KERN_CONSDEV,
        name: "consdev",
        kind: KernKind::Func(KernFunc::Consdev),
    },
    KernEntry {
        id: KERN_MAXPTYS,
        name: "maxptys",
        kind: KernKind::BuildInt("NR_PTYS"),
    },
    KernEntry {
        id: KERN_MAXPHYS,
        name: "maxphys",
        kind: KernKind::ConstInt(4 * 1024 * 1024),
    },
    KernEntry {
        id: KERN_MONOTONIC_CLOCK,
        name: "monotonic_clock",
        kind: KernKind::BuildInt("_POSIX_MONOTONIC_CLOCK"),
    },
    KernEntry {
        id: KERN_LWP,
        name: "lwp",
        kind: KernKind::Func(KernFunc::Lwp),
    },
    KernEntry {
        id: KERN_FORKFSLEEP,
        name: "forkfsleep",
        kind: KernKind::VerifyInt {
            init: 0,
            verify: KernVerify::Forkfsleep,
        },
    },
    KernEntry {
        id: KERN_DUMP_ON_PANIC,
        name: "dump_on_panic",
        kind: KernKind::ConstInt(0),
    },
    KernEntry {
        id: KERN_DRIVERS,
        name: "drivers",
        kind: KernKind::Func(KernFunc::Drivers),
    },
    KernEntry {
        id: KERN_HARDCLOCK_TICKS,
        name: "hardclock_ticks",
        kind: KernKind::Func(KernFunc::HardclockTicks),
    },
    KernEntry {
        id: KERN_SYSVIPC,
        name: "ipc",
        kind: KernKind::IpcTable,
    },
    KernEntry {
        id: KERN_BOOTTIME,
        name: "boottime",
        kind: KernKind::Func(KernFunc::Boottime),
    },
];

/// Unpopulated kern slots (A-9 exclusion contract, 39 of 84).
///
/// C: the `/* ... not yet ... */`, `/* obsolete */`, and
/// `/* incompatible ... */` rows — kern.c:361-363,379,385-388,399-402,
/// 409,425-426,443,452,456,458-459,461-463,466-473,477,481,484-485,491,497.
/// Kept as data (not prose) so the contract is greppable and the table
/// above cannot silently grow into a reserved slot.
pub const KERN_UNIMPLEMENTED: &[i32] = &[
    13, 14, 15, 21, 24, 25, 26, 27, 28, 33, 34, 35, 36, 39, 44, 45, 46, 52, 53, 56, 58, 59, 61, 62,
    63, 66, 67, 68, 69, 70, 71, 73, 74, 76, 77, 78, 79, 81, 84,
];

/// Find a populated entry by id (linear; 45 rows).
pub fn find_entry(id: i32) -> Option<&'static KernEntry> {
    KERN_ENTRIES.iter().find(|e| e.id == id)
}

/// Securelevel gate: only ever rises (mock).
///
/// C: `mib_kern_securelvl` — kern.c:17-30 (`v >= node_int`; "mock",
/// "TODO: actual support").
pub const fn securelvl_ok(current: i32, new: i32) -> bool {
    new >= current
}

/// Fork-sleep gate: NetBSD range in milliseconds.
///
/// C: `mib_kern_forkfsleep` — kern.c:208-217
/// (`0 <= v <= MAXSLP * 1000`; `MAXSLP` is 20, sys/param.h:447).
pub const MAXSLP: i32 = 20;

/// Upper bound in milliseconds (20 s of fork backoff, at most).
pub const FORKFSLEEP_MAX_MS: i32 = MAXSLP * 1000;

/// Judge a fork-sleep value.
pub const fn forkfsleep_ok(v: i32) -> bool {
    v >= 0 && v <= FORKFSLEEP_MAX_MS
}

/// Clockinfo tick: microsecond tick from Hz.
///
/// C: `clockinfo.tick = 1000000 / hz` (+ `tickadj = tick`, "I think",
/// with a TODO to ask the kernel) — kern.c:44-55. `hz/profhz/stathz`
/// all echo `sys_hz()`; only the division is decided here.
pub const fn clock_tick_us(hz: u32) -> u32 {
    1_000_000 / hz
}

/// cp_time name-length gate: at most one sub-id (a CPU number).
///
/// C: `call_namelen > 1 → EINVAL` — kern.c:150-151.
pub const fn cp_time_namelen_ok(namelen: u32) -> Result<(), i32> {
    if namelen > 1 {
        return Err(EINVAL);
    }
    Ok(())
}

/// cp_time read mode: one CPU, a sum, or the full array.
///
/// C: kern.c:153-186. A sub-id names the CPU; without one, a `NULL`
/// sink implies a summation-sized answer, a summation-sized sink sums,
/// anything else streams per-CPU vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpTimeMode {
    /// One CPU's vector. C: `:153-159`.
    SingleCpu,
    /// Summed over CPUs. C: `:161-184` (`oldp == NULL` or sink fits one vector).
    Sum,
    /// Per-CPU vectors back to back. C: `:176-186`.
    Array,
}

/// Judge the cp_time mode (`per_cpu` = one vector's bytes).
pub const fn cp_time_mode(
    has_subid: bool,
    sink_none: bool,
    sink_len: u64,
    per_cpu: u64,
) -> CpTimeMode {
    if has_subid {
        return CpTimeMode::SingleCpu;
    }
    if sink_none || sink_len == per_cpu {
        return CpTimeMode::Sum;
    }
    CpTimeMode::Array
}

/// ipc_info name-length gate: exactly one sub-id (the resource type).
///
/// C: `call_namelen != 1 → EINVAL`, then `EOPNOTSUPP` unconditionally
/// (mock — the IPC mount covers this when it runs) — kern.c:311-315.
pub const fn ipc_info_namelen_ok(namelen: u32) -> Result<(), i32> {
    if namelen != 1 {
        return Err(EINVAL);
    }
    Ok(())
}

/// Root-device truncation: cap at the buffer minus the terminator.
///
/// C: `name[min(vallen, sizeof(name) - 1)] = '\0'` — kern.c:111.
pub const fn truncate_len(vallen: usize, cap: usize) -> usize {
    if vallen < cap { vallen } else { cap }
}

/// PTY alias hack: list "pts" when the PTY driver runs under another name.
///
/// C: kern.c:246-255. NetBSD userland expects "pts"; MINIX3 adds the
/// alias row (block major −1, char major `PTY_MAJOR`) when needed.
pub const fn pty_alias_needed(driver_running: bool, label_is_pts: bool) -> bool {
    driver_running && !label_is_pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_shape() {
        // 45 populated of 84 (kern.c:332-498); sorted by id.
        assert_eq!(KERN_ENTRIES.len(), 45);
        assert_eq!(KERN_UNIMPLEMENTED.len(), 39);
        let mut prev = 0;
        for e in KERN_ENTRIES {
            assert!(e.id > prev, "table must stay id-sorted");
            assert!(!KERN_UNIMPLEMENTED.contains(&e.id));
            prev = e.id;
        }
        // Full coverage: populated + excluded = 84 slots.
        assert_eq!(KERN_ENTRIES.len() + KERN_UNIMPLEMENTED.len(), 84);
        // Spot checks against the C rows.
        assert_eq!(find_entry(9).unwrap().name, "securelevel");
        assert_eq!(find_entry(12).unwrap().name, "clockrate");
        assert_eq!(
            find_entry(47).unwrap().kind,
            KernKind::Func(KernFunc::Proc2)
        );
        assert_eq!(find_entry(65).unwrap().name, "forkfsleep");
        assert_eq!(find_entry(82).unwrap().kind, KernKind::IpcTable);
        assert_eq!(find_entry(13), None);
        assert_eq!(find_entry(84), None);
    }

    #[test]
    fn test_verify_gates() {
        // Securelevel only rises (mock, kern.c:29).
        assert!(securelvl_ok(-1, -1));
        assert!(securelvl_ok(-1, 0));
        assert!(!securelvl_ok(0, -1));
        // Fork-sleep NetBSD range, MAXSLP 20 (kern.c:216, param.h:447).
        assert_eq!((MAXSLP, FORKFSLEEP_MAX_MS), (20, 20000));
        assert!(forkfsleep_ok(0));
        assert!(forkfsleep_ok(20000));
        assert!(!forkfsleep_ok(-1));
        assert!(!forkfsleep_ok(20001));
    }

    #[test]
    fn test_handler_verdicts() {
        // clock tick math (kern.c:45,55).
        assert_eq!(clock_tick_us(100), 10000);
        assert_eq!(clock_tick_us(1000), 1000);
        // cp_time gates and modes (:150-186).
        assert_eq!(cp_time_namelen_ok(0), Ok(()));
        assert_eq!(cp_time_namelen_ok(1), Ok(()));
        assert_eq!(cp_time_namelen_ok(2), Err(EINVAL));
        assert_eq!(cp_time_mode(true, false, 0, 32), CpTimeMode::SingleCpu);
        assert_eq!(cp_time_mode(false, true, 0, 32), CpTimeMode::Sum);
        assert_eq!(cp_time_mode(false, false, 32, 32), CpTimeMode::Sum);
        assert_eq!(cp_time_mode(false, false, 64, 32), CpTimeMode::Array);
        // ipc_info: one sub-id, then unsupported (mock, :311-315).
        assert_eq!(ipc_info_namelen_ok(1), Ok(()));
        assert_eq!(ipc_info_namelen_ok(0), Err(EINVAL));
        assert_eq!(ipc_info_namelen_ok(2), Err(EINVAL));
        assert_eq!(
            find_entry(KERN_PROF).unwrap().kind,
            KernKind::Func(KernFunc::Profiling)
        );
        // rootdev truncation (:111) and pty alias (:246-247).
        assert_eq!(truncate_len(5, 10), 5);
        assert_eq!(truncate_len(10, 10), 10);
        assert_eq!(truncate_len(99, 10), 10);
        assert!(pty_alias_needed(true, false));
        assert!(!pty_alias_needed(true, true));
        assert!(!pty_alias_needed(false, false));
    }
}
