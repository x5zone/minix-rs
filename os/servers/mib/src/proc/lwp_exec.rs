//! KERN_LWP assembly: pull, judge, encode, copy out.
//!
//! Mirrors the *effect* half of `mib_kern_lwp` (`proc.c:509-593`): the
//! decision pieces live in 17 ([`super::lwp`]) and the pull discipline in 16
//! ([`super::tables`]); this module walks the slots, fills one
//! [`minix_types::KinfoLwp`] per task/process, and steps the sink by `elsz`
//! exactly like C. What C reads from its static tables, the views in
//! [`super::rows`] decode here.
//!
//! `[ARCH: A-7]` two fields cannot be produced yet because the kernel
//! GET_PROCTAB row (edge1's producer) does not carry `p_dequeued` and
//! `p_cpuavg` yet (edge4 §2 C-21 first half): `l_slptime`, `l_pctcpu` and
//! `l_cpticks` answer 0 — registered incompleteness, not silent divergence.
//! The VFS light table degrades to "idle" the same way (its producer is
//! fail-closed), so the cdev/sdev sleep lanes never fire.
//!
//! 17-mib-proc-lwp.md.

use minix_types::{
    mp_flags, Endpoint, KinfoLwp, KI_WMESGLEN, EINVAL, ESRCH, L_INMEM, LSSLEEP,
    MProcSnap, ProcInfoStruct, RTS_NO_PRIV, RTS_NO_QUANTUM, RTS_PAGEFAULT, RTS_PROC_STOP,
    RTS_SIGNALED, RTS_VMREQTARGET,
};

use super::lwp::{
    check_lwp_args, classify_awake, extra_headroom, judge_kern_lwp, judge_sleep, judge_times,
    user_flag, AwakeVerdict, BlockTarget, RtsCause, SleepWmesg, VfsLane,
};
use super::rows::{build_pid_hash, write_wmesg_named, KernelRows, LightRows, PmRows};
use super::tables::{chain_lookup, hash_slots, NO_SLOT};
use crate::dispatch::SysctlOutcome;
use crate::io::copy::{in_range, Oldp};
use crate::transport::{MibKernel, MibServices};
use crate::walker::MibCtx;

/// Walk the LWP list for one request. C: `mib_kern_lwp` — proc.c:509-593.
///
/// `args` is the name tail past the `kern.lwp` function node: `[pid, elsz,
/// elmax]`; `oldp` is the paired sink (its absence means a size estimate).
pub fn kern_lwp<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    args: &[i32],
    oldp: Option<&Oldp>,
) -> SysctlOutcome {
    // Call shape gate — proc.c:519-528 (`call_namelen != 3` is EINVAL).
    let (pid, elsz, elmax) = match args {
        [pid, elsz, elmax] => (*pid, *elsz, *elmax),
        _ => return SysctlOutcome::err(EINVAL),
    };
    if check_lwp_args(args.len() as u32, pid, elsz, elmax).is_err() {
        return SysctlOutcome::err(EINVAL);
    }

    // Field-level split: the row views borrow the snapshot buffers for the
    // whole walk while the copy seam keeps its own `&mut` — a whole-`&mut
    // ctx` pass cannot do both (same shape as the remote arm's split).
    let MibCtx { tables, kernel, svc, self_endpt, .. } = &mut *ctx;
    let self_endpt = *self_endpt;

    // `update_tables` — proc.c:530 (`if (!update_tables()) return EINVAL`).
    let now = match kernel.getticks() {
        Ok(t) => t,
        Err(code) => return SysctlOutcome::err(code),
    };
    if !tables.update(now, &mut **kernel, &mut **svc) {
        return SysctlOutcome::err(EINVAL);
    }
    let hz = match kernel.hz() {
        Ok(h) => h,
        Err(code) => return SysctlOutcome::err(code),
    };

    let kern = KernelRows::new(&tables.kernel_tab);
    let pm = PmRows::new(&tables.pm_tab);
    // C-22 后半：light 表随 VFS 生产者接通——cdev/sdev 阻塞的 wchan/
    // wmesg 车道由此点亮；表缺席时（A-7 降级）每槽退化为 idle。
    let light_view = LightRows::new(&tables.vfs_tab);

    let old_len = oldp.map(|o| o.left);
    let mut elmax = elmax;
    let mut off: u64 = 0;
    let copysz = (core::mem::size_of::<KinfoLwp>() as i32).min(elsz) as usize;

    // Kernel tasks model as LWPs of the kernel (PID 0): the whole listing
    // (pid < 0) starts with them; pid == 0 stops after. proc.c:540-560.
    if pid <= 0 {
        for kslot in 0..minix_types::NR_TASKS {
            if in_range(old_len, off) && elmax > 0 {
                let mut lwp = zeroed_lwp();
                fill_task_row(&mut lwp, &kern, kslot, now, hz);
                if let Err(code) = copy_lwp(&mut **kernel, oldp, off, &lwp, copysz) {
                    return SysctlOutcome::err(code);
                }
                elmax -= 1;
            }
            off += elsz as u64;
        }
        if pid == 0 {
            return SysctlOutcome::Done(off);
        }
    }

    // One specific process (pid > 0, ESRCH on missing/zombie) or the whole
    // user range (pid < 0). proc.c:562-570.
    let (first, last) = if pid > 0 {
        let (hash, next, pids) = build_pid_hash(&pm);
        let mslot = chain_lookup(&hash, &next, &pids, pid, hash_slots(minix_types::NR_PROCS as u32));
        let zombie = mslot != NO_SLOT
            && pm
                .row(mslot as usize)
                .map(|r| PmRows::is_zombie(&r))
                .unwrap_or(false);
        // Missing slot or zombie: ESRCH — proc.c:564-566.
        if mslot == NO_SLOT || zombie {
            return SysctlOutcome::err(ESRCH);
        }
        (mslot, mslot)
    } else {
        (0, minix_types::NR_PROCS as i32 - 1)
    };

    for mslot in first..=last {
        let row: MProcSnap = match pm.row(mslot as usize) {
            Some(r) => r,
            None => continue,
        };
        // `(flags & (IN_USE|TRACE_ZOMBIE|ZOMBIE)) == IN_USE` — proc.c:577.
        let mask =
            mp_flags::IN_USE | mp_flags::TRACE_ZOMBIE | mp_flags::ZOMBIE;
        if row.mp_flags & mask != mp_flags::IN_USE {
            continue;
        }
        if in_range(old_len, off) && elmax > 0 {
            let mut lwp = zeroed_lwp();
            let light = light_view.row(mslot as usize);
            fill_user_row(
                &mut lwp,
                &kern,
                &pm,
                light,
                mslot as usize,
                &row,
                now,
                hz,
                self_endpt,
            );
            if let Err(code) = copy_lwp(&mut **kernel, oldp, off, &lwp, copysz) {
                return SysctlOutcome::err(code);
            }
            elmax -= 1;
        }
        off += elsz as u64;
    }

    // Size-estimate headroom for forks between the two calls — proc.c:588-590
    // (`EXTRA_PROCS * elsz`, whole-list reads only).
    if oldp.is_none() && pid < 0 {
        off += extra_headroom(true, true, elsz as u32);
    }
    SysctlOutcome::Done(off)
}

/// A POD-zero `KinfoLwp` to fill field-by-field, exactly like C's
/// `memset(&lwp, 0, sizeof(lwp))` (proc.c:466/:494).
fn zeroed_lwp() -> KinfoLwp {
    // SAFETY: `KinfoLwp` is `#[repr(C)]` over integers and byte arrays —
    // all-zero is a valid value for every field (no references, no niches).
    unsafe { core::mem::zeroed() }
}

/// `mib_copyout(oldp, off, &lwp, copysz)` — proc.c:549-551/:581-583. The
/// transport's errno is C's negative return.
fn copy_lwp<K: MibKernel>(
    kernel: &mut K,
    oldp: Option<&Oldp>,
    off: u64,
    lwp: &KinfoLwp,
    copysz: usize,
) -> Result<(), i32> {
    let Some(oldp) = oldp else {
        return Ok(()); // size estimate: nothing moves (in_range already gated)
    };
    // SAFETY: `KinfoLwp` is `#[repr(C)]` Copy POD; the byte view feeds the
    // copy seam only.
    let bytes =
        unsafe { core::slice::from_raw_parts(lwp as *const KinfoLwp as *const u8, copysz) };
    oldp.copyout(kernel, off, bytes).map(|_| ())
}

/// `fill_lwp_kern` — proc.c:461-482: always asleep, wrapped negative PID,
/// the task's own name in `l_wmesg` and "kernel" in `l_name`.
fn fill_task_row(l: &mut KinfoLwp, kern: &KernelRows, kslot: usize, now: u64, hz: u32) {
    let verdict = judge_kern_lwp(kslot as i32, minix_types::NR_TASKS as i32);
    l.l_flag = verdict.flag as i32;
    l.l_stat = verdict.stat as i8;
    l.l_pid = verdict.pid;
    l.l_wchan = verdict.wchan;
    // The kernel table is whole-table or nothing (a failed pull refuses the
    // whole call), so the missing-row arm only guards the slot math; the
    // row's own defaults keep the layout exact.
    let kp = kern.row(kslot).unwrap_or_default();
    put_name(&mut l.l_wmesg, &kp.p_name);
    put_name(&mut l.l_name, b"kernel\0");
    fill_common(l, &kp, None, now, hz);
}

/// One user row's state verdict — the `get_lwp_stat` chain shared by the
/// KERN_LWP and KERN_PROC2 fills. C: both `fill_lwp_user` (proc.c:501-502)
/// and `fill_proc2_user` (proc.c:755-757) call the same function.
pub(super) struct RowState {
    /// `L_*` state (LSDEAD kept as itself here; KERN_PROC2 maps it to
    /// LSZOMB for display via 18's `map_stat`).
    pub stat: i32,
    /// Wait channel — zero for the four awake states. C: `*wcptr` is only
    /// written on the sleep path.
    pub wchan: u64,
    /// The sleep word, rendered by the caller (needs the tables). C:
    /// `wmptr` — only written when sleeping.
    pub wmesg: SleepWmesg,
    /// `L_SINTR` accumulation. C: `*flag |= L_SINTR`.
    pub sinter: bool,
    /// The endpoint `DriverName`/`EndpointName` words name: the blocking
    /// driver's endpoint (no parens) for the former, the `P_BLOCKEDON`
    /// peer (parens) for the latter. C: `fill_wmesg` 的 `endpt` 实参
    /// （proc.c:318-321/:382）。
    pub wmesg_ep: i32,
}

/// Judge one user row through the awake-precedence and sleep-reason chain.
/// C: `get_lwp_stat` — proc.c:243-389 (minus the string copies).
pub(super) fn judge_row_state(
    kp: &ProcInfoStruct,
    row: &MProcSnap,
    light: Option<super::rows::LightRow>,
    self_endpt: Endpoint,
) -> RowState {
    let zombie = PmRows::is_zombie(row);
    let exiting = row.mp_flags & mp_flags::EXITING != 0;
    let stopped = row.mp_flags & mp_flags::TRACE_STOPPED != 0 || KernelRows::is_p_stopped(kp);
    let runnable = KernelRows::is_runnable(kp);
    let waiting = row.mp_flags & mp_flags::WAITING != 0;
    let sigsuspended = row.mp_flags & mp_flags::SIGSUSPENDED != 0;
    match classify_awake(zombie, exiting, stopped, runnable) {
        verdict if verdict != AwakeVerdict::Sleeping => RowState {
            stat: verdict.state(),
            wchan: 0,
            wmesg: SleepWmesg::Unknown,
            sinter: false,
            wmesg_ep: 0,
        },
        _ => {
            // The VFS lane lights up only through the light row (C-22):
            // blocked_on feeds the lane decode, fpl_task feeds the
            // cdev/sdev wchan word and the driver-name endpoint.
            let (lane, task_ep) = match light {
                Some(l) => (super::lwp::decode_blocked_on(l.blocked_on), l.task_ep),
                None => (VfsLane::Idle, 0),
            };
            let target = block_target(kp, self_endpt);
            let v = judge_sleep(waiting, sigsuspended, lane, task_ep as u32, target);
            let wmesg_ep = match v.wmesg {
                SleepWmesg::DriverName => task_ep,
                SleepWmesg::EndpointName => KernelRows::blocked_on(kp),
                _ => 0,
            };
            RowState {
                stat: LSSLEEP,
                wchan: v.wchan,
                wmesg: v.wmesg,
                sinter: v.sinter,
                wmesg_ep,
            }
        }
    }
}

/// `fill_lwp_user` — proc.c:487-505: the state machine first (which may
/// attach wchan/wmesg and the interruptible bit), then identity and times.
fn fill_user_row(
    l: &mut KinfoLwp,
    kern: &KernelRows,
    pm: &PmRows,
    light: Option<super::rows::LightRow>,
    mslot: usize,
    row: &MProcSnap,
    now: u64,
    hz: u32,
    self_endpt: Endpoint,
) {
    let kp = kern.row(minix_types::NR_TASKS + mslot).unwrap_or_default();
    l.l_flag = L_INMEM as i32;
    let st = judge_row_state(&kp, row, light, self_endpt);
    l.l_stat = st.stat as i8;
    if st.stat == LSSLEEP {
        l.l_wchan = st.wchan;
        l.l_flag = user_flag(st.sinter) as i32;
        render_wmesg(l, kern, pm, st.wmesg, st.wmesg_ep);
    }
    l.l_pid = row.mp_pid as u32;
    put_name(&mut l.l_name, &row.mp_name);
    // started rides the PM row since C-21; dequeued is still pending the
    // kernel row (C-21 first half) and answers 0 (module note).
    fill_common(l, &kp, Some(row.mp_started), now, hz);
}

/// `fill_lwp_common` — proc.c:399-456. `started` is `None` for kernel tasks
/// (their swtime is raw uptime, proc.c:430-433); `p_dequeued`/`p_cpuavg`
/// ride the kernel row once C-21's first half lands, until then
/// `l_slptime`/`l_pctcpu`/`l_cpticks` answer 0 — the registered A-7
/// incompleteness.
fn fill_common(l: &mut KinfoLwp, kp: &ProcInfoStruct, started: Option<u64>, now: u64, hz: u32) {
    l.l_lid = kp.p_endpoint;
    // p_dequeued 缺席：传 now 让 slptime 为 0（"此刻未睡"），比编造一个
    // uptime/hz 的假值诚实——A-7 锚点。
    let (swtime, slptime) = judge_times(
        started.is_none(),
        now,
        started.unwrap_or(0),
        now,
        hz,
    );
    l.l_swtime = swtime;
    l.l_slptime = slptime;
    l.l_priority = kp.p_priority as u8;
    l.l_usrpri = kp.p_priority as u8;
    l.l_cpuid = kp.p_cpu as u64;
    let (sec, usec) =
        super::tables::ticks_to_timeval(kp.p_user_time.saturating_add(kp.p_sys_time), hz as u64);
    l.l_rtime_sec = sec as u32;
    l.l_rtime_usec = usec as u32;
    // l_pctcpu / l_cpticks: p_cpuavg lands with C-21's first half; 0 until
    // then (A-7).
}

/// `P_BLOCKEDON` + the special-endpoint lanes, judged into a
/// [`BlockTarget`] for [`judge_sleep`]. C: proc.c:335-373. `self_endpt` is
/// `NONE` until the server models its own endpoint, so the "sysctl" decor
/// lane cannot fire yet (the C lane is aesthetics-only, proc.c:338-342).
fn block_target(kp: &ProcInfoStruct, self_endpt: Endpoint) -> BlockTarget {
    let endpt = KernelRows::blocked_on(kp);
    if self_endpt != Endpoint::NONE && endpt == self_endpt.0 {
        BlockTarget::Mib
    } else if endpt == Endpoint::NONE.0 {
        BlockTarget::KernelStop {
            rts_flags: kp.p_rts_flags,
            cause: RtsCause {
                kstop: kp.p_rts_flags & RTS_PROC_STOP != 0,
                ksignal: kp.p_rts_flags & RTS_SIGNALED != 0,
                knopriv: kp.p_rts_flags & RTS_NO_PRIV != 0,
                fault: kp.p_rts_flags & (RTS_PAGEFAULT | RTS_VMREQTARGET) != 0,
                sched: kp.p_rts_flags & RTS_NO_QUANTUM != 0,
            },
        }
    } else {
        BlockTarget::Direct {
            endpt_nr: endpt as u32,
            sinter: endpt == Endpoint::ANY.0,
        }
    }
}

/// Render the sleep word into `l_wmesg`. C: the literal lanes plus the two
/// `fill_wmesg` calls (proc.c:287-386) — `DriverName` names the driver
/// without parens（cdev/sdev 车道，C-22 后半点亮）, `EndpointName` names
/// the `P_BLOCKEDON` peer with parens.
fn render_wmesg(
    l: &mut KinfoLwp,
    kern: &KernelRows,
    pm: &PmRows,
    wmesg: SleepWmesg,
    wmesg_ep: i32,
) {
    let mut buf = [0u8; KI_WMESGLEN];
    match wmesg {
        SleepWmesg::DriverName => {
            write_wmesg_named(&mut buf, kern, pm, wmesg_ep, false);
        }
        SleepWmesg::EndpointName => {
            write_wmesg_named(&mut buf, kern, pm, wmesg_ep, true);
        }
        other => {
            let word: &str = match other {
                SleepWmesg::Wait => "wait",
                SleepWmesg::Pause => "pause",
                SleepWmesg::Pipe => "pipe",
                SleepWmesg::Flock => "flock",
                SleepWmesg::Popen => "popen",
                SleepWmesg::Select => "select",
                SleepWmesg::Sysctl => "sysctl",
                SleepWmesg::Kstop => "kstop",
                SleepWmesg::Ksignal => "ksignal",
                SleepWmesg::Knopriv => "knopriv",
                SleepWmesg::Fault => "fault",
                SleepWmesg::Sched => "sched",
                SleepWmesg::Kflag => "kflag",
                SleepWmesg::Unknown => "???",
                SleepWmesg::DriverName | SleepWmesg::EndpointName => unreachable!(),
            };
            put_name(&mut buf, word.as_bytes());
        }
    }
    l.l_wmesg = buf;
}

/// `strlcpy` a NUL-terminated name into a fixed buffer. C:
/// `strlcpy(l->l_wmesg, ...)` — proc.c:479/:481/:501.
fn put_name(dst: &mut [u8], src: &[u8]) {
    let end = src.iter().position(|b| *b == 0).unwrap_or(src.len());
    let keep = end.min(dst.len() - 1);
    dst[..keep].copy_from_slice(&src[..keep]);
    dst[keep] = 0;
}

/// Unit tests for the fill halves over synthetic tables (the walker-level
/// end-to-end lives with the walker).
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use minix_types::RTS_SENDING;

    fn name_bytes(name: &str) -> [u8; 16] {
        let mut n = [0u8; 16];
        let b = name.as_bytes();
        n[..b.len()].copy_from_slice(b);
        n
    }

    /// A kernel row for user slot 5: runnable, endpoint 5, named "ps".
    fn runnable_user() -> ProcInfoStruct {
        ProcInfoStruct {
            p_nr: 5,
            p_endpoint: 5,
            p_name: name_bytes("ps"),
            p_priority: 2,
            p_user_time: 30,
            p_sys_time: 10,
            ..ProcInfoStruct::default()
        }
    }

    #[test]
    fn test_task_row_shape() {
        let mut tab = vec![0u8; (minix_types::NR_TASKS + minix_types::NR_PROCS) * core::mem::size_of::<ProcInfoStruct>()];
        let task = ProcInfoStruct {
            p_name: name_bytes("memory"),
            p_endpoint: -1,
            p_user_time: 30,
            p_sys_time: 10,
            ..ProcInfoStruct::default()
        };
        // SAFETY(test): repr(C) POD write into the slot window.
        unsafe {
            core::ptr::write_unaligned(
                tab.as_mut_ptr().add(1 * core::mem::size_of::<ProcInfoStruct>())
                    as *mut ProcInfoStruct,
                task,
            );
        }
        let kern = KernelRows::new(&tab);
        let mut l = zeroed_lwp();
        fill_task_row(&mut l, &kern, 1, 1000, 50);
        // Negative PID wraps in u32 (proc.c:470-471); class 0x00 wchan.
        assert_eq!(l.l_pid, (1i32 - minix_types::NR_TASKS as i32) as u32);
        assert_eq!(l.l_wchan, ((1i32 - minix_types::NR_TASKS as i32) as u32 as u64) << 8);
        assert_eq!(l.l_stat, minix_types::LSSLEEP as i8);
        assert_eq!(
            l.l_flag,
            (minix_types::L_INMEM | minix_types::L_SINTR | minix_types::L_SYSTEM) as i32
        );
        assert_eq!(&l.l_wmesg[..7], b"memory\0");
        assert_eq!(&l.l_name[..7], b"kernel\0");
        assert_eq!(l.l_lid, -1);
        // Task swtime is raw uptime (proc.c:431-433).
        assert_eq!(l.l_swtime, 20);
        assert_eq!(l.l_slptime, 0);
        // 40 ticks at hz 50 = 0.8s.
        assert_eq!(l.l_rtime_sec, 0);
        assert_eq!(l.l_rtime_usec, 800_000);
        // A-7: pctcpu/cpticks stay 0 until the kernel row grows them.
        assert_eq!(l.l_pctcpu, 0);
        assert_eq!(l.l_cpticks, 0);
    }

    #[test]
    fn test_user_row_runnable() {
        let mut tab = vec![0u8; (minix_types::NR_TASKS + minix_types::NR_PROCS) * core::mem::size_of::<ProcInfoStruct>()];
        // SAFETY(test): repr(C) POD write into the slot window.
        unsafe {
            core::ptr::write_unaligned(
                tab.as_mut_ptr()
                    .add((minix_types::NR_TASKS + 5) * core::mem::size_of::<ProcInfoStruct>())
                    as *mut ProcInfoStruct,
                runnable_user(),
            );
        }
        let kern = KernelRows::new(&tab);
        let row = MProcSnap {
            mp_pid: 100,
            mp_started: 400,
            mp_flags: mp_flags::IN_USE,
            mp_name: name_bytes("ps"),
            ..MProcSnap::default()
        };
        let mut l = zeroed_lwp();
        let pm_tab = vec![0u8; minix_types::NR_PROCS * core::mem::size_of::<MProcSnap>()];
        let pm = PmRows::new(&pm_tab);
        fill_user_row(&mut l, &kern, &pm, None, 5, &row, 1000, 50, Endpoint::NONE);
        // Runnable: LSRUN, no wchan/word (proc.c:252-253).
        assert_eq!(l.l_stat, minix_types::LSRUN as i8);
        assert_eq!(l.l_wchan, 0);
        assert_eq!(l.l_pid, 100);
        assert_eq!(&l.l_name[..3], b"ps\0");
        // swtime = (1000-400)/50 = 12 (proc.c:430-433, user half).
        assert_eq!(l.l_swtime, 12);
    }

    #[test]
    fn test_user_row_wait_sleep() {
        let mut tab = vec![0u8; (minix_types::NR_TASKS + minix_types::NR_PROCS) * core::mem::size_of::<ProcInfoStruct>()];
        // Kernel side: blocked sending to endpoint 5 — a direct endpoint
        // target, so judge_sleep keeps the PM word (C 的 NONE/MIB 车道会
        // 用 RTS 词覆盖 PM 词，proc.c:343-364；Direct 保留，:380-382)。
        let kp = ProcInfoStruct {
            p_nr: 6,
            p_rts_flags: RTS_SENDING,
            p_getfrom_e: Endpoint::NONE.0,
            p_sendto_e: 5,
            p_name: name_bytes("sh"),
            ..ProcInfoStruct::default()
        };
        // SAFETY(test): repr(C) POD write into the slot window.
        unsafe {
            core::ptr::write_unaligned(
                tab.as_mut_ptr()
                    .add((minix_types::NR_TASKS + 6) * core::mem::size_of::<ProcInfoStruct>())
                    as *mut ProcInfoStruct,
                kp,
            );
        }
        let kern = KernelRows::new(&tab);
        let row = MProcSnap {
            mp_pid: 101,
            mp_flags: mp_flags::IN_USE | mp_flags::WAITING,
            mp_name: name_bytes("sh"),
            ..MProcSnap::default()
        };
        let mut l = zeroed_lwp();
        let pm_tab = vec![0u8; minix_types::NR_PROCS * core::mem::size_of::<MProcSnap>()];
        let pm = PmRows::new(&pm_tab);
        fill_user_row(&mut l, &kern, &pm, None, 6, &row, 1000, 50, Endpoint::NONE);
        // PM WAITING wins first (proc.c:287-290): wchan 0x102, "wait",
        // interruptible.
        assert_eq!(l.l_wchan, 0x102);
        assert_eq!(&l.l_wmesg[..5], b"wait\0");
        assert_eq!(l.l_stat, LSSLEEP as i8);
        assert_eq!(l.l_flag, (minix_types::L_INMEM | minix_types::L_SINTR) as i32);
    }

    // ── walker 级端到端：producer（mock 双 seam）→ Tables 拉取 →
    // 真实 `sysctl()` 走 CTL_KERN/KERN_LWP → 拷出字节逐行解码断言。──

    use crate::proc::test_mocks::{self as mocks, put_row};
    use crate::auth::CallAuth;
    use crate::heap::MibBudget;
    use crate::io::copy::{Newp, Oldp};
    use crate::proc::Tables;
    use crate::tree::arena::MibTree;
    use crate::walker::{self, Request};
    use alloc::string::String;
    use minix_types::{Endpoint, KI_LNAMELEN, KI_WMESGLEN};

    /// 场景：任务 1 名 "memory"；用户槽 5 = 可运行 "ps"(pid 100)、
    /// 槽 6 = WAITING 挂起 "sh"(pid 101)、槽 7 = 僵尸(pid 103, 不进列表)、
    /// 槽 8 = cdev 阻塞 "tty-read"(pid 104, light 表给 TTY 端点)。
    fn fixture() -> (mocks::FakeKernel, mocks::FakeServices) {
        let krow = core::mem::size_of::<ProcInfoStruct>();
        let mut proctab = vec![0u8; (minix_types::NR_TASKS + minix_types::NR_PROCS) * krow];
        let put_k = |v: &mut Vec<u8>, kslot: usize, row: &ProcInfoStruct| {
            // SAFETY(test): repr(C) POD 写进槽窗口，索引在界内。
            unsafe {
                core::ptr::write_unaligned(
                    v.as_mut_ptr().add(kslot * krow) as *mut ProcInfoStruct,
                    *row,
                );
            }
        };
        put_k(&mut proctab, 1, &ProcInfoStruct {
            p_name: name_bytes("memory"),
            p_endpoint: -1,
            p_user_time: 30,
            p_sys_time: 10,
            ..ProcInfoStruct::default()
        });
        put_k(&mut proctab, minix_types::NR_TASKS + 4, &ProcInfoStruct {
            p_nr: 4,
            p_endpoint: 4,
            p_name: name_bytes("ttydrv"),
            ..ProcInfoStruct::default()
        });
        put_k(&mut proctab, minix_types::NR_TASKS + 5, &ProcInfoStruct {
            p_nr: 5,
            p_endpoint: 5,
            p_name: name_bytes("ps"),
            p_priority: 2,
            p_user_time: 30,
            p_sys_time: 10,
            ..ProcInfoStruct::default()
        });
        put_k(&mut proctab, minix_types::NR_TASKS + 6, &ProcInfoStruct {
            p_nr: 6,
            p_rts_flags: RTS_SENDING,
            p_sendto_e: 5,
            p_getfrom_e: Endpoint::NONE.0,
            p_name: name_bytes("sh"),
            ..ProcInfoStruct::default()
        });
        let mut pm = vec![0u8; minix_types::NR_PROCS * core::mem::size_of::<MProcSnap>()];
        let put_m = |v: &mut Vec<u8>, mslot: usize, row: &MProcSnap| {
            // SAFETY(test): repr(C) POD 写进槽窗口，索引在界内。
            unsafe {
                core::ptr::write_unaligned(
                    v.as_mut_ptr().add(mslot * core::mem::size_of::<MProcSnap>())
                        as *mut MProcSnap,
                    *row,
                );
            }
        };
        put_m(&mut pm, 4, &MProcSnap {
            mp_pid: 90,
            mp_flags: mp_flags::IN_USE,
            mp_name: name_bytes("ttydrv"),
            ..MProcSnap::default()
        });
        put_m(&mut pm, 5, &MProcSnap {
            mp_pid: 100,
            mp_flags: mp_flags::IN_USE,
            mp_started: 400,
            mp_name: name_bytes("ps"),
            ..MProcSnap::default()
        });
        put_m(&mut pm, 6, &MProcSnap {
            mp_pid: 101,
            mp_flags: mp_flags::IN_USE | mp_flags::WAITING,
            mp_started: 500,
            mp_name: name_bytes("sh"),
            ..MProcSnap::default()
        });
        put_m(&mut pm, 7, &MProcSnap {
            mp_pid: 103,
            mp_flags: mp_flags::IN_USE | mp_flags::ZOMBIE,
            mp_name: name_bytes("dead"),
            ..MProcSnap::default()
        });
        // 内核行：槽 8 cdev 阻塞在 TTY 驱动（端点 -1 之外的普通端点 4）。
        put_row(
            &mut proctab,
            minix_types::NR_TASKS + 8,
            &ProcInfoStruct {
                p_nr: 8,
                p_rts_flags: RTS_SENDING,
                p_sendto_e: 4,
                p_getfrom_e: Endpoint::NONE.0,
                p_name: name_bytes("tty-read"),
                ..ProcInfoStruct::default()
            },
        );
        // light 表（C-22 后半）：槽 8 blocked_on=CDEV(5)、task=端点 4。
        let mut light = vec![0u8; minix_types::NR_PROCS * 16];
        let put_l = |v: &mut Vec<u8>, mslot: usize, tty: u64, blocked: u32, task: i32| {
            v[mslot * 16..mslot * 16 + 8].copy_from_slice(&tty.to_le_bytes());
            v[mslot * 16 + 8..mslot * 16 + 12].copy_from_slice(&blocked.to_le_bytes());
            v[mslot * 16 + 12..mslot * 16 + 16].copy_from_slice(&task.to_le_bytes());
        };
        // tty-read 的 PM 行（pid 104, IN_USE）——list 的行来源。
        put_m(&mut pm, 8, &MProcSnap {
            mp_pid: 104,
            mp_flags: mp_flags::IN_USE,
            mp_name: name_bytes("tty-read"),
            ..MProcSnap::default()
        });
        put_l(&mut light, 8, 0, 5, 4); // blocked=CDEV, task=端点 4（ttydrv）
        (
            mocks::FakeKernel {
                proctab,
                ticks: 1000,
                hz: 50,
                boot: 172_800,
                sink: core::cell::RefCell::new(Vec::new()),
                target_base: 0,
                target_mem: Vec::new(),
            },
            mocks::FakeServices { pm_tab: pm, light_tab: light },
        )
    }

    fn e2e_ctx<'a>(
        kernel: &'a mut mocks::FakeKernel,
        svc: &'a mut mocks::FakeServices,
        tables: &'a mut Tables,
    ) -> MibCtx<'a, mocks::FakeKernel, mocks::FakeServices> {
        let tree = Box::leak(Box::new(MibTree::init()));
        let budget = Box::leak(Box::new(MibBudget::new()));
        MibCtx {
            tree,
            budget,
            kernel,
            svc,
            tables,
            self_endpt: Endpoint::NONE,
            caller: Endpoint::PM,
            auth: CallAuth::Yes,
        }
    }

    #[test]
    fn test_kern_lwp_end_to_end_listing() {
        let (mut k, mut s) = fixture();
        let mut tables = Tables::new();
        let lsz = core::mem::size_of::<KinfoLwp>() as i32;
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = walker::sysctl(
            &mut c,
            &mut Request {
                name: &[minix_types::CTL_KERN, minix_types::KERN_LWP, -1, lsz, 1000],
                oldp: Some(Oldp { endpt: Endpoint::PM, addr: 0, left: 1 << 20 }),
                newp: None,
            },
        );
        // 5 个任务槽 + 4 个在用用户槽（僵尸被过滤）——off = 9 × elsz。
        assert_eq!(out, SysctlOutcome::Done(9 * lsz as u64));

        let sink = c.kernel.sink.borrow();
        let read = |i: usize| -> KinfoLwp {
            // SAFETY(test): repr(C) POD 读，i 在拷出范围内。
            unsafe { core::ptr::read_unaligned(sink.as_ptr().add(i * core::mem::size_of::<KinfoLwp>()) as *const KinfoLwp) }
        };

        // 任务槽 1："memory"，LSSLEEP，负 PID 回绕，wchan 类 0x00。
        let t1 = read(1);
        assert_eq!(t1.l_pid, (1i32 - minix_types::NR_TASKS as i32) as u32);
        assert_eq!(t1.l_stat, minix_types::LSSLEEP as i8);
        assert_eq!(&t1.l_wmesg[..7], b"memory\0");
        assert_eq!(&t1.l_name[..7], b"kernel\0");
        assert_eq!(t1.l_swtime, 20); // 1000/50
        assert_eq!(t1.l_rtime_sec, 0);
        assert_eq!(t1.l_rtime_usec, 800_000);

        // 用户槽 4（元素 5）："ttydrv" 可运行。
        let u4 = read(minix_types::NR_TASKS);
        assert_eq!(u4.l_pid, 90);
        assert_eq!(u4.l_stat, minix_types::LSRUN as i8);
        assert_eq!(&u4.l_name[..7], b"ttydrv\0");

        // 用户槽 5（元素 6）："ps" 可运行 → LSRUN，无 wchan。
        let u5 = read(minix_types::NR_TASKS + 1);
        assert_eq!(u5.l_pid, 100);
        assert_eq!(u5.l_stat, minix_types::LSRUN as i8);
        assert_eq!(&u5.l_name[..3], b"ps\0");
        assert_eq!(u5.l_swtime, 12); // (1000-400)/50
        assert_eq!(u5.l_wchan, 0);
        assert_eq!(u5.l_lid, 5);
        assert_eq!(u5.l_priority, 2);

        // 用户槽 6（元素 7）："sh" 在 PM WAITING、内核 SENDING 到 4 →
        // PM 词保留（Direct 不覆盖）：wchan 0x102、"wait"、可中断。
        let u6 = read(minix_types::NR_TASKS + 2);
        assert_eq!(u6.l_pid, 101);
        assert_eq!(u6.l_stat, minix_types::LSSLEEP as i8);
        assert_eq!(u6.l_wchan, 0x102);
        assert_eq!(&u6.l_wmesg[..5], b"wait\0");
        assert_eq!(u6.l_flag, (minix_types::L_INMEM | minix_types::L_SINTR) as i32);

        // 用户槽 8（元素 8）："tty-read" cdev 阻塞——light 表在（C-22
        // 后半）→ wchan = (task_ep<<16)|(5<<8)|0x03，wmesg 是**驱动名**
        // 无括号（DriverName 车道）。
        let u8 = read(minix_types::NR_TASKS + 3);
        assert_eq!(u8.l_pid, 104);
        assert_eq!(u8.l_stat, minix_types::LSSLEEP as i8);
        assert_eq!(u8.l_wchan, (4u64 << 16) | (5u64 << 8) | 0x03);
        assert_eq!(&u8.l_wmesg[..7], b"ttydrv\0");
        assert_eq!(u8.l_flag, (minix_types::L_INMEM | minix_types::L_SINTR) as i32);

        // 僵尸（槽 7）不进列表：缓冲区到元素 8 为止（9 行）。
        assert_eq!(sink.len(), 9 * core::mem::size_of::<KinfoLwp>());

        // 7 个元素（任务 0~4 + 用户 5、6），僵尸没有第 8 个。
        assert_eq!(sink.len(), 9 * core::mem::size_of::<KinfoLwp>());
    }

    #[test]
    fn test_kern_lwp_end_to_end_gates() {
        let (mut k, mut s) = fixture();
        let mut tables = Tables::new();
        let lsz = core::mem::size_of::<KinfoLwp>() as i32;

        // 未知 pid → ESRCH（proc.c:564-566）。
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = walker::sysctl(
            &mut c,
            &mut Request {
                name: &[minix_types::CTL_KERN, minix_types::KERN_LWP, 999, lsz, 4],
                oldp: None,
                newp: None,
            },
        );
        assert_eq!(out, SysctlOutcome::err(ESRCH));

        // 名字长度错 → EINVAL（proc.c:519-521，走 walker 的门）。
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = walker::sysctl(
            &mut c,
            &mut Request {
                name: &[minix_types::CTL_KERN, minix_types::KERN_LWP, 100],
                oldp: None,
                newp: None,
            },
        );
        assert_eq!(out, SysctlOutcome::err(EINVAL));

        // pid == 0：只回任务段（proc.c:558-560）。
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = walker::sysctl(
            &mut c,
            &mut Request {
                name: &[minix_types::CTL_KERN, minix_types::KERN_LWP, 0, lsz, 1000],
                oldp: None,
                newp: None,
            },
        );
        assert_eq!(out, SysctlOutcome::Done(minix_types::NR_TASKS as u64 * lsz as u64));
        let _ = (String::new(), KI_LNAMELEN, KI_WMESGLEN, Newp { endpt: Endpoint::NONE, addr: 0, len: 0 }, Oldp { endpt: Endpoint::NONE, addr: 0, left: 0 });
    }

    #[test]
    fn test_user_row_direct_ipc_block() {
        let mut tab = vec![0u8; (minix_types::NR_TASKS + minix_types::NR_PROCS) * core::mem::size_of::<ProcInfoStruct>()];
        // Blocked sending to endpoint 5 (slot 5, "ps" row absent → the
        // fallback prints the number).
        let kp = ProcInfoStruct {
            p_nr: 7,
            p_rts_flags: RTS_SENDING,
            p_sendto_e: 5,
            p_getfrom_e: Endpoint::NONE.0,
            p_name: name_bytes("cat"),
            ..ProcInfoStruct::default()
        };
        // SAFETY(test): repr(C) POD write into the slot window.
        unsafe {
            core::ptr::write_unaligned(
                tab.as_mut_ptr()
                    .add((minix_types::NR_TASKS + 7) * core::mem::size_of::<ProcInfoStruct>())
                    as *mut ProcInfoStruct,
                kp,
            );
        }
        let kern = KernelRows::new(&tab);
        let row = MProcSnap {
            mp_pid: 102,
            mp_flags: mp_flags::IN_USE,
            mp_name: name_bytes("cat"),
            ..MProcSnap::default()
        };
        let mut l = zeroed_lwp();
        let pm_tab = vec![0u8; minix_types::NR_PROCS * core::mem::size_of::<MProcSnap>()];
        let pm = PmRows::new(&pm_tab);
        fill_user_row(&mut l, &kern, &pm, None, 7, &row, 1000, 50, Endpoint::NONE);
        // Direct send: wchan (endpt<<8)|0xff, "(5)" — the peer row is not in
        // the PM table so the raw number is the name (proc.c:213-214,
        // :380-382).
        assert_eq!(l.l_wchan, (5u64 << 8) | 0xff);
        assert_eq!(&l.l_wmesg[..4], b"(5)\0");
        assert_eq!(l.l_stat, LSSLEEP as i8);
    }
}
