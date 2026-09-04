//! KERN_LWP: classify state, explain sleeps, fill rows.
//!
//! Mirrors the pure halves of `get_lwp_stat` / `fill_lwp_common` /
//! `fill_lwp_kern` / `fill_lwp_user` / `mib_kern_lwp` (`proc.c:224-593`).
//! Reading whole tables (`sys_getproctab`, `getsysinfo`), reading the
//! clock (`getticks`, `sys_hz`), copying strings, and the `cpuavg`
//! math are transport effects (A-6, A-12); every *decision* — which
//! state wins, which wchan class, which word, which gate — is judged
//! here. Table *layouts* belong to kernel/PM/VFS (A-6); flag *values*
//! belong to their owners, so callers pass booleans and this module
//! only owns orders and shapes.
//!
//! 17-mib-proc-lwp.md.

use super::tables::EXTRA_PROCS;
use minix_types::{
    EINVAL, ESRCH, L_INMEM, L_SINTR, L_SYSTEM, LSDEAD, LSRUN, LSSLEEP, LSSTOP, LSZOMB,
};

/// Awake verdict: the four states that return at once, or sleeping.
///
/// C: `get_lwp_stat` head — proc.c:243-253. The if-chain order *is* the
/// priority: zombie beats exiting beats stopped beats runnable. The MP
/// flag values behind the booleans belong to PM (`mproc.h`); only the
/// order travels here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwakeVerdict {
    /// `TRACE_ZOMBIE | ZOMBIE` — awaiting collection. C: `:243-244`.
    Zombie,
    /// `EXITING` — almost a zombie. C: `:246-247`.
    Dead,
    /// `TRACE_STOPPED` or kernel `RTS_P_STOP` — debugged/held. C: `:249-250`.
    Stopped,
    /// `proc_is_runnable` — on a run queue, not yet running. C: `:252-253`.
    Runnable,
    /// None of the above — walk the sleep reasons. C: `:255-390`.
    Sleeping,
}

impl AwakeVerdict {
    /// NetBSD state code ps/top reads. C: `sys/sys/lwp.h:278-285`.
    pub const fn state(self) -> i32 {
        match self {
            AwakeVerdict::Zombie => LSZOMB,
            AwakeVerdict::Dead => LSDEAD,
            AwakeVerdict::Stopped => LSSTOP,
            AwakeVerdict::Runnable => LSRUN,
            AwakeVerdict::Sleeping => LSSLEEP,
        }
    }
}

/// Classify the awake lane: four booleans in, one verdict out.
///
/// C: `:243-253`. Every flag is pre-decoded by the caller (transport
/// reads the tables); the chain order is the whole semantic.
pub const fn classify_awake(
    zombie: bool,
    exiting: bool,
    stopped: bool,
    runnable: bool,
) -> AwakeVerdict {
    if zombie {
        AwakeVerdict::Zombie
    } else if exiting {
        AwakeVerdict::Dead
    } else if stopped {
        AwakeVerdict::Stopped
    } else if runnable {
        AwakeVerdict::Runnable
    } else {
        AwakeVerdict::Sleeping
    }
}

/// VFS block kind: the `fpl_blocked_on` value decoded.
///
/// C: `switch (fp->fpl_blocked_on)` — proc.c:295-322. The *values* are
/// a VFS contract (`servers/vfs/const.h:19-25`); MIB only owns the
/// value-to-word map, so decoding is an explicit function, not a bare
/// number carried around.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsBlock {
    /// Suspended on a pipe. C: `FP_BLOCKED_ON_PIPE (1)` — `:296-298`.
    Pipe,
    /// Suspended on a file lock. C: `FP_BLOCKED_ON_FLOCK (2)` — `:299-301`.
    Flock,
    /// Suspended on a pipe open. C: `FP_BLOCKED_ON_POPEN (3)` — `:302-304`.
    Popen,
    /// Suspended in select. C: `FP_BLOCKED_ON_SELECT (4)` — `:305-307`.
    Select,
    /// Blocked on character-device I/O. C: `FP_BLOCKED_ON_CDEV (5)` — `:308-317`.
    CharDev,
    /// Blocked on socket I/O. C: `FP_BLOCKED_ON_SDEV (6)` — `:309-317`.
    SockDev,
}

impl VfsBlock {
    /// The VFS raw value behind the variant. C: `vfs/const.h:19-25`.
    pub const fn kind(self) -> u8 {
        match self {
            VfsBlock::Pipe => 1,
            VfsBlock::Flock => 2,
            VfsBlock::Popen => 3,
            VfsBlock::Select => 4,
            VfsBlock::CharDev => 5,
            VfsBlock::SockDev => 6,
        }
    }
}

/// Decode lane: idle, known, or a kind from the future.
///
/// C: `:293` (`!= FP_BLOCKED_ON_NONE`) + `:318-321` (`default` →
/// `"???"`). A kind MIB has never seen is *not* a crash: it reports
/// the wchan with a shrug word, exactly like C.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsLane {
    /// `FP_BLOCKED_ON_NONE (0)` — VFS has nothing to say. C: `:293`.
    Idle,
    /// A kind with a known word. C: `:296-317`.
    Known(VfsBlock),
    /// A kind added after this code — wchan still formed. C: `:318-321`.
    Unknown(u8),
}

/// Decode a raw `fpl_blocked_on` byte. C: `:293-322`.
pub const fn decode_blocked_on(kind: u8) -> VfsLane {
    match kind {
        0 => VfsLane::Idle,
        1 => VfsLane::Known(VfsBlock::Pipe),
        2 => VfsLane::Known(VfsBlock::Flock),
        3 => VfsLane::Known(VfsBlock::Popen),
        4 => VfsLane::Known(VfsBlock::Select),
        5 => VfsLane::Known(VfsBlock::CharDev),
        6 => VfsLane::Known(VfsBlock::SockDev),
        k => VfsLane::Unknown(k),
    }
}

/// Sleep message: every word `get_lwp_stat` can emit.
///
/// C: the string literals across `:287-386`. `DriverName` and
/// `EndpointName` are rendered by `fill_wmesg` (16) in transport — the
/// verdict only names the lane, never the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepWmesg {
    /// `"wait"` — PM wait. C: `:289`.
    Wait,
    /// `"pause"` — PM sigsuspend. C: `:292`.
    Pause,
    /// `"pipe"`. C: `:297`.
    Pipe,
    /// `"flock"`. C: `:300`.
    Flock,
    /// `"popen"`. C: `:303`.
    Popen,
    /// `"select"`. C: `:306`.
    Select,
    /// Driver name via `fill_wmesg(task, FALSE)`. C: `:315-316`.
    DriverName,
    /// `"???"` — kind from the future. C: `:320`.
    Unknown,
    /// `"sysctl"` — blocked on MIB itself (pure decor). C: `:341`.
    Sysctl,
    /// `"kstop"`. C: `:351`.
    Kstop,
    /// `"ksignal"`. C: `:354`.
    Ksignal,
    /// `"knopriv"`. C: `:356`.
    Knopriv,
    /// `"fault"`. C: `:359`.
    Fault,
    /// `"sched"`. C: `:361`.
    Sched,
    /// `"kflag"` — none of the flags matched. C: `:363`.
    Kflag,
    /// `"(name)"` via `fill_wmesg(endpt, TRUE)`. C: `:382`.
    EndpointName,
}

/// Kernel RTS cause, pre-decoded to booleans.
///
/// C: the `RTS_ISSET` chain — proc.c:350-363. Flag *values* belong to
/// the kernel; only the precedence travels here. Note C tests
/// `RTS_SIGNALED` twice (`:352-353`, harmless typo); one boolean
/// covers both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtsCause {
    /// `RTS_PROC_STOP`. C: `:350-351`.
    pub kstop: bool,
    /// `RTS_SIGNALED` (either copy). C: `:352-354`.
    pub ksignal: bool,
    /// `RTS_NO_PRIV`. C: `:355-356`.
    pub knopriv: bool,
    /// `RTS_PAGEFAULT | RTS_VMREQTARGET`. C: `:357-359`.
    pub fault: bool,
    /// `RTS_NO_QUANTUM`. C: `:360-361`.
    pub sched: bool,
}

/// Pick the RTS word by precedence. C: `:350-363`.
pub const fn pick_rts(cause: RtsCause) -> SleepWmesg {
    if cause.kstop {
        SleepWmesg::Kstop
    } else if cause.ksignal {
        SleepWmesg::Ksignal
    } else if cause.knopriv {
        SleepWmesg::Knopriv
    } else if cause.fault {
        SleepWmesg::Fault
    } else if cause.sched {
        SleepWmesg::Sched
    } else {
        SleepWmesg::Kflag
    }
}

/// Block target: what the kernel table says the process waits on.
///
/// C: `switch (endpt = P_BLOCKEDON(kp))` — proc.c:335-373. Endpoint
/// *values* (`MIB_PROC_NR`, `NONE`, `ANY`) are endpoint-relative (16
/// D4: never pinned); the caller maps them with endpoint-side helpers
/// and this module only judges the shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockTarget {
    /// Blocked on MIB itself — decor address, no flag. C: `:338-342`.
    Mib,
    /// No IPC partner — the RTS word decides. C: `:343-364`.
    KernelStop {
        /// Raw RTS word, shifted into the address. C: `:349`.
        rts_flags: u32,
        /// Pre-decoded cause for the word. C: `:350-363`.
        cause: RtsCause,
    },
    /// Blocked sending/receiving on an endpoint (covers `ANY`: the
    /// caller passes `sinter = true` for it). C: `:365-373` + `:380-382`.
    Direct {
        /// Raw endpoint number for the fallback address. C: `:381`.
        endpt_nr: u32,
        /// `true` when the endpoint is `ANY`. C: `:371`.
        sinter: bool,
    },
}

/// Sleep verdict: the address, the word, and the interruptible mark.
///
/// C: the `*wcptr` / `wmptr` / `*flag` triple of `get_lwp_stat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SleepVerdict {
    /// Wait channel: low 8 bits are the class. C: `:264-274`.
    pub wchan: u64,
    /// Human word for ps. C: the literals across `:287-386`.
    pub wmesg: SleepWmesg,
    /// `L_SINTR` accumulated (`|=` only, never cleared). C: `:328/:371`.
    pub sinter: bool,
}

/// wchan low-8-bit classes. C: `:265-271`.
pub const WCHAN_CLASS_TASK: u8 = 0x00;
/// Kernel RTS block. C: `:267`.
pub const WCHAN_CLASS_RTS: u8 = 0x01;
/// PM call. C: `:268`.
pub const WCHAN_CLASS_PM: u8 = 0x02;
/// VFS call. C: `:269`.
pub const WCHAN_CLASS_VFS: u8 = 0x03;
/// MIB call (decor). C: `:270`.
pub const WCHAN_CLASS_MIB: u8 = 0x04;
/// Blocked on a process endpoint. C: `:271`.
pub const WCHAN_CLASS_ENDPT: u8 = 0xff;

/// Build a wchan from class info and class. C: `(x << 8) | class`.
pub const fn wchan_with_class(info: u64, class: u8) -> u64 {
    (info << 8) | (class as u64)
}

/// Judge the whole sleep path: PM/VFS first, IPC/RTS second.
///
/// C: proc.c:276-389. The second half *overwrites* the address set by
/// the first (`Mib`/`KernelStop` assign unconditionally, `:340/:349`)
/// but the interruptible mark only ever accumulates (`:328`, `:371`).
/// `Direct` keeps the first half's address when one exists and falls
/// back to the endpoint lane otherwise (`:380-382`).
pub const fn judge_sleep(
    waiting: bool,
    sigsuspended: bool,
    blocked: VfsLane,
    task_nr: u32,
    target: BlockTarget,
) -> SleepVerdict {
    let mut sinter = false;
    let mut prior: Option<(u64, SleepWmesg)> = None;
    if waiting {
        prior = Some((0x102, SleepWmesg::Wait));
        sinter = true;
    } else if sigsuspended {
        prior = Some((0x202, SleepWmesg::Pause));
        sinter = true;
    } else {
        match blocked {
            VfsLane::Idle => {}
            VfsLane::Known(b) => {
                let wchan = match b {
                    VfsBlock::CharDev | VfsBlock::SockDev => {
                        wchan_with_class(b.kind() as u64, WCHAN_CLASS_VFS)
                            | ((task_nr as u64) << 16)
                    }
                    _ => wchan_with_class(b.kind() as u64, WCHAN_CLASS_VFS),
                };
                let wmesg = match b {
                    VfsBlock::Pipe => SleepWmesg::Pipe,
                    VfsBlock::Flock => SleepWmesg::Flock,
                    VfsBlock::Popen => SleepWmesg::Popen,
                    VfsBlock::Select => SleepWmesg::Select,
                    VfsBlock::CharDev | VfsBlock::SockDev => SleepWmesg::DriverName,
                };
                prior = Some((wchan, wmesg));
                sinter = true;
            }
            VfsLane::Unknown(k) => {
                prior = Some((
                    wchan_with_class(k as u64, WCHAN_CLASS_VFS),
                    SleepWmesg::Unknown,
                ));
                sinter = true;
            }
        }
    }
    match target {
        BlockTarget::Mib => SleepVerdict {
            wchan: WCHAN_CLASS_MIB as u64,
            wmesg: SleepWmesg::Sysctl,
            sinter,
        },
        BlockTarget::KernelStop { rts_flags, cause } => SleepVerdict {
            wchan: wchan_with_class(rts_flags as u64, WCHAN_CLASS_RTS),
            wmesg: pick_rts(cause),
            sinter,
        },
        BlockTarget::Direct {
            endpt_nr,
            sinter: s,
        } => {
            sinter = sinter || s;
            match prior {
                Some((wchan, wmesg)) => SleepVerdict {
                    wchan,
                    wmesg,
                    sinter,
                },
                None => SleepVerdict {
                    wchan: wchan_with_class(endpt_nr as u64, WCHAN_CLASS_ENDPT),
                    wmesg: SleepWmesg::EndpointName,
                    sinter,
                },
            }
        }
    }
}

/// Kernel-task row verdict: always asleep, negative PID, kernel name.
///
/// C: `fill_lwp_kern` — proc.c:461-482. `l_pid` is `uint32_t`
/// (`sysctl.h:668`), so the negative slot difference *wraps* before it
/// is widened into the wchan (`:477` zero-extends the wrapped value —
/// not a sign extension). Replicated exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernLwpVerdict {
    /// Always `L_INMEM | L_SINTR | L_SYSTEM`. C: `:468`.
    pub flag: u32,
    /// Always `LSSLEEP`. C: `:469`.
    pub stat: i32,
    /// `kslot - NR_TASKS` wrapped to `uint32_t`. C: `:470`.
    pub pid: u32,
    /// `(pid << 8) | 0x00` over the wrapped value. C: `:477`.
    pub wchan: u64,
}

/// Judge one kernel-task row. C: `:461-482` (minus the string copies).
pub const fn judge_kern_lwp(kslot: i32, nr_tasks: i32) -> KernLwpVerdict {
    let pid = kslot.wrapping_sub(nr_tasks) as u32;
    KernLwpVerdict {
        flag: L_INMEM | L_SINTR | L_SYSTEM,
        stat: LSSLEEP,
        pid,
        wchan: wchan_with_class(pid as u64, WCHAN_CLASS_TASK),
    }
}

/// User-row flag base: `INMEM`, plus `SINTR` when the state machine
/// said interruptible. C: `:497-499` (`l_flag` then `|= L_SINTR`
/// inside `get_lwp_stat`).
pub const fn user_flag(sinter: bool) -> u32 {
    if sinter { L_INMEM | L_SINTR } else { L_INMEM }
}

/// Time books: swapped-in and sleep seconds from tick readings.
///
/// C: `fill_lwp_common` time half — proc.c:425-438. Tasks never sleep
/// (`:435-436`); processes count since `mp_started` / `p_dequeued`.
/// `hz == 0` can never happen (`sys_hz` guarantees it) — returning
/// zeros there is a modeling guard, same family as 16's stale-chain
/// defense. Returns `(swtime_secs, slptime_secs)`.
pub const fn judge_times(
    is_task: bool,
    uptime: u64,
    started: u64,
    dequeued: u64,
    hz: u32,
) -> (u32, u32) {
    if hz == 0 {
        return (0, 0);
    }
    let hz64 = hz as u64;
    if is_task {
        ((uptime / hz64) as u32, 0)
    } else {
        (
            (uptime.saturating_sub(started) / hz64) as u32,
            (uptime.saturating_sub(dequeued) / hz64) as u32,
        )
    }
}

/// `mib_kern_lwp` argument gate: exact name length plus value bounds.
///
/// C: `:520-528`. A short name is not a short read — it is `EINVAL`.
pub const fn check_lwp_args(namelen: u32, pid: i32, elsz: i32, elmax: i32) -> Result<(), i32> {
    if namelen != 3 {
        return Err(EINVAL);
    }
    if pid < -1 || elsz <= 0 || elmax < 0 {
        return Err(EINVAL);
    }
    Ok(())
}

/// Single-PID slot gate: missing slot or a zombie is `ESRCH`.
///
/// C: `:564-566` (`get_mslot(pid) == NO_SLOT || ... & ZOMBIE`).
pub const fn judge_pid_slot(noslot: bool, zombie: bool) -> Result<(), i32> {
    if noslot || zombie {
        return Err(ESRCH);
    }
    Ok(())
}

/// Size-estimate headroom: only the `oldp == NULL`, whole-list estimate
/// is padded. C: `:589-590` (`EXTRA_PROCS`, 16).
pub const fn extra_headroom(oldp_null: bool, pid_negative: bool, elsz: u32) -> u64 {
    if oldp_null && pid_negative {
        (EXTRA_PROCS as u64) * (elsz as u64)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_awake_precedence() {
        // Order is priority: zombie beats everything (proc.c:243-253).
        assert_eq!(classify_awake(true, true, true, true), AwakeVerdict::Zombie);
        assert_eq!(classify_awake(true, true, true, true).state(), LSZOMB);
        assert_eq!(classify_awake(false, true, true, true), AwakeVerdict::Dead);
        assert_eq!(classify_awake(false, true, true, true).state(), LSDEAD);
        assert_eq!(
            classify_awake(false, false, true, true),
            AwakeVerdict::Stopped
        );
        assert_eq!(classify_awake(false, false, true, true).state(), LSSTOP);
        assert_eq!(
            classify_awake(false, false, false, true),
            AwakeVerdict::Runnable
        );
        assert_eq!(classify_awake(false, false, false, true).state(), LSRUN);
        assert_eq!(
            classify_awake(false, false, false, false),
            AwakeVerdict::Sleeping
        );
        assert_eq!(classify_awake(false, false, false, false).state(), LSSLEEP);
    }

    #[test]
    fn test_pm_vfs_block() {
        // wait pins a fixed address and sets the mark (:287-289);
        // a plain endpoint target keeps it (:380-382 keep prior).
        let v = judge_sleep(
            true,
            false,
            VfsLane::Idle,
            0,
            BlockTarget::Direct {
                endpt_nr: 3,
                sinter: false,
            },
        );
        assert_eq!(
            (v.wchan, v.wmesg, v.sinter),
            (0x102, SleepWmesg::Wait, true)
        );
        let v = judge_sleep(
            false,
            true,
            VfsLane::Idle,
            0,
            BlockTarget::Direct {
                endpt_nr: 3,
                sinter: false,
            },
        );
        assert_eq!(
            (v.wchan, v.wmesg, v.sinter),
            (0x202, SleepWmesg::Pause, true)
        );
        // pipe: kind lane (1 << 8) | 0x03 (:294-297).
        let v = judge_sleep(
            false,
            false,
            VfsLane::Known(VfsBlock::Pipe),
            0,
            BlockTarget::Direct {
                endpt_nr: 3,
                sinter: false,
            },
        );
        assert_eq!((v.wchan, v.wmesg), (0x103, SleepWmesg::Pipe));
        // character device folds the driver endpoint into bits 16+ (:315).
        let v = judge_sleep(
            false,
            false,
            VfsLane::Known(VfsBlock::CharDev),
            9,
            BlockTarget::Direct {
                endpt_nr: 3,
                sinter: false,
            },
        );
        assert_eq!(v.wchan, 0x503 | (9 << 16));
        assert_eq!(v.wmesg, SleepWmesg::DriverName);
        // unknown kind: address still formed, word is "???" (:318-321).
        let v = judge_sleep(
            false,
            false,
            VfsLane::Unknown(7),
            0,
            BlockTarget::Direct {
                endpt_nr: 3,
                sinter: false,
            },
        );
        assert_eq!((v.wchan, v.wmesg), (0x703, SleepWmesg::Unknown));
        // decode round-trips the VFS contract values (vfs/const.h:19-25).
        assert_eq!(decode_blocked_on(0), VfsLane::Idle);
        assert_eq!(decode_blocked_on(4), VfsLane::Known(VfsBlock::Select));
        assert_eq!(decode_blocked_on(9), VfsLane::Unknown(9));
    }

    #[test]
    fn test_sleep_compose() {
        // MIB decor overwrites the wait address but keeps the mark (:340).
        let v = judge_sleep(true, false, VfsLane::Idle, 0, BlockTarget::Mib);
        assert_eq!(
            (v.wchan, v.wmesg, v.sinter),
            (0x04, SleepWmesg::Sysctl, true)
        );
        // Same, with no prior block: no mark (:338-342 set no flag).
        let v = judge_sleep(false, false, VfsLane::Idle, 0, BlockTarget::Mib);
        assert_eq!(
            (v.wchan, v.wmesg, v.sinter),
            (0x04, SleepWmesg::Sysctl, false)
        );
        // No prior block + plain endpoint: endpoint lane + name (:380-382).
        let v = judge_sleep(
            false,
            false,
            VfsLane::Idle,
            0,
            BlockTarget::Direct {
                endpt_nr: 5,
                sinter: false,
            },
        );
        assert_eq!((v.wchan, v.wmesg), (0x5ff, SleepWmesg::EndpointName));
        // ANY only adds the mark; a prior address survives (:365-372).
        let v = judge_sleep(
            false,
            false,
            VfsLane::Known(VfsBlock::Flock),
            0,
            BlockTarget::Direct {
                endpt_nr: 0,
                sinter: true,
            },
        );
        assert_eq!(
            (v.wchan, v.wmesg, v.sinter),
            (0x203, SleepWmesg::Flock, true)
        );
        // wchan class builder matches the pinned literals.
        assert_eq!(wchan_with_class(1, WCHAN_CLASS_PM), 0x102);
        assert_eq!(wchan_with_class(2, WCHAN_CLASS_PM), 0x202);
        assert_eq!(wchan_with_class(0, WCHAN_CLASS_MIB), 0x04);
    }

    #[test]
    fn test_rts_cause_order() {
        let all = RtsCause {
            kstop: true,
            ksignal: true,
            knopriv: true,
            fault: true,
            sched: true,
        };
        // kstop first, kflag last (proc.c:350-363).
        assert_eq!(pick_rts(all), SleepWmesg::Kstop);
        let sig = RtsCause {
            kstop: false,
            ksignal: true,
            knopriv: true,
            fault: true,
            sched: true,
        };
        assert_eq!(pick_rts(sig), SleepWmesg::Ksignal);
        let none = RtsCause {
            kstop: false,
            ksignal: false,
            knopriv: false,
            fault: false,
            sched: false,
        };
        assert_eq!(pick_rts(none), SleepWmesg::Kflag);
        // KernelStop verdict carries the shifted RTS word (:349).
        let v = judge_sleep(
            false,
            false,
            VfsLane::Idle,
            0,
            BlockTarget::KernelStop {
                rts_flags: 0x40,
                cause: all,
            },
        );
        assert_eq!(
            (v.wchan, v.wmesg, v.sinter),
            (0x4001, SleepWmesg::Kstop, false)
        );
    }

    #[test]
    fn test_kern_lwp() {
        // kslot 0 of 5 tasks: pid wraps (uint32_t field), wchan
        // zero-extends the wrapped value (proc.c:470, :477).
        let v = judge_kern_lwp(0, 5);
        assert_eq!(v.flag, L_INMEM | L_SINTR | L_SYSTEM);
        assert_eq!(v.stat, LSSLEEP);
        assert_eq!(v.pid, 0xFFFF_FFFBu32);
        assert_eq!(v.wchan, 0xFFFF_FFFB_00u64);
        // User rows start from INMEM; the machine ORs SINTR in (:497-499).
        assert_eq!(user_flag(false), L_INMEM);
        assert_eq!(user_flag(true), L_INMEM | L_SINTR);
    }

    #[test]
    fn test_lwp_args() {
        // Name must be exactly (pid, elsz, elmax): anything else EINVAL.
        assert_eq!(check_lwp_args(2, -1, 200, 5), Err(EINVAL));
        assert_eq!(check_lwp_args(3, -2, 200, 5), Err(EINVAL));
        assert_eq!(check_lwp_args(3, 0, 0, 5), Err(EINVAL));
        assert_eq!(check_lwp_args(3, 0, 200, -1), Err(EINVAL));
        assert_eq!(check_lwp_args(3, -1, 200, 5), Ok(()));
        // Missing slot or a zombie share ESRCH (:564-566).
        assert_eq!(judge_pid_slot(true, false), Err(ESRCH));
        assert_eq!(judge_pid_slot(false, true), Err(ESRCH));
        assert_eq!(judge_pid_slot(false, false), Ok(()));
        // Headroom only for the whole-list size estimate (:589-590).
        assert_eq!(extra_headroom(true, true, 200), 1600);
        assert_eq!(extra_headroom(false, true, 200), 0);
        assert_eq!(extra_headroom(true, false, 200), 0);
    }

    #[test]
    fn test_times() {
        // Tasks bill uptime; processes bill since start/dequeue (:425-438).
        assert_eq!(judge_times(true, 1000, 0, 500, 100), (10, 0));
        assert_eq!(judge_times(false, 1000, 100, 200, 100), (9, 8));
        // Zero hz is a modeling guard (sys_hz never yields it).
        assert_eq!(judge_times(false, 1000, 100, 200, 0), (0, 0));
    }
}
