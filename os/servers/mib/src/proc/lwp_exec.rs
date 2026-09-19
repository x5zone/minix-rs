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
    // The light view is A-7 idle until C-21's second half; the lane stays
    // silent in the fill below.
    let _light = LightRows::new(&tables.vfs_tab);

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
            fill_user_row(
                &mut lwp,
                &kern,
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

/// `fill_lwp_user` — proc.c:487-505: the state machine first (which may
/// attach wchan/wmesg and the interruptible bit), then identity and times.
fn fill_user_row(
    l: &mut KinfoLwp,
    kern: &KernelRows,
    mslot: usize,
    row: &MProcSnap,
    now: u64,
    hz: u32,
    self_endpt: Endpoint,
) {
    let kp = kern.row(minix_types::NR_TASKS + mslot).unwrap_or_default();
    let zombie = PmRows::is_zombie(row);
    let exiting = row.mp_flags & mp_flags::EXITING != 0;
    let stopped = row.mp_flags & mp_flags::TRACE_STOPPED != 0 || KernelRows::is_p_stopped(&kp);
    let runnable = KernelRows::is_runnable(&kp);
    l.l_flag = L_INMEM as i32;
    let waiting = row.mp_flags & mp_flags::WAITING != 0;
    let sigsuspended = row.mp_flags & mp_flags::SIGSUSPENDED != 0;
    match classify_awake(zombie, exiting, stopped, runnable) {
        verdict if verdict != AwakeVerdict::Sleeping => {
            l.l_stat = verdict.state() as i8;
        }
        _ => {
            // VFS light rows are absent until C-21: the whole VFS lane is
            // idle (module note) — task_nr rides along unused.
            let target = block_target(&kp, self_endpt);
            let v = judge_sleep(waiting, sigsuspended, VfsLane::Idle, 0, target);
            l.l_stat = LSSLEEP as i8;
            l.l_wchan = v.wchan;
            l.l_flag = user_flag(v.sinter) as i32;
            render_wmesg(l, kern, v.wmesg, &kp);
        }
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
/// `fill_wmesg` calls (proc.c:287-386). `DriverName` cannot fire while the
/// light table is absent (A-7); it renders through the same name path as
/// `EndpointName` when it ever does.
fn render_wmesg(l: &mut KinfoLwp, kern: &KernelRows, wmesg: SleepWmesg, kp: &ProcInfoStruct) {
    let mut buf = [0u8; KI_WMESGLEN];
    match wmesg {
        SleepWmesg::DriverName | SleepWmesg::EndpointName => {
            let endpt = KernelRows::blocked_on(kp);
            write_wmesg_named(&mut buf, kern, &PmRows::new(&[]), endpt, true);
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
                SleepWmesg::DriverName | SleepWmesg::EndpointName => {
                    debug_unreachable()
                }
            };
            put_name(&mut buf, word.as_bytes());
        }
    }
    l.l_wmesg = buf;
}

/// `debug_unreachable` without a dependency: this arm is statically
/// excluded by the arm above (the pair is matched together); a release
/// build prints "unknown" like C's future-flag shrug.
#[inline(never)]
#[cold]
fn debug_unreachable() -> &'static str {
    "???"
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
        fill_user_row(&mut l, &kern, 5, &row, 1000, 50, Endpoint::NONE);
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
        fill_user_row(&mut l, &kern, 6, &row, 1000, 50, Endpoint::NONE);
        // PM WAITING wins first (proc.c:287-290): wchan 0x102, "wait",
        // interruptible.
        assert_eq!(l.l_wchan, 0x102);
        assert_eq!(&l.l_wmesg[..5], b"wait\0");
        assert_eq!(l.l_stat, LSSLEEP as i8);
        assert_eq!(l.l_flag, (minix_types::L_INMEM | minix_types::L_SINTR) as i32);
    }

    // ── walker 级端到端：producer（mock 双 seam）→ Tables 拉取 →
    // 真实 `sysctl()` 走 CTL_KERN/KERN_LWP → 拷出字节逐行解码断言。──

    use crate::auth::CallAuth;
    use crate::heap::MibBudget;
    use crate::io::relay::{RelayDir, RemoteCall, RemoteReplyWire};
    use crate::tree::arena::MibTree;
    use crate::walker::{self, Request};
    use crate::io::copy::{Newp, Oldp};
    use crate::proc::Tables;
    use crate::transport::{MibKernel, MibServices};
    use alloc::string::String;
    use minix_types::{Endpoint, KI_LNAMELEN, KI_WMESGLEN, SI_PROC_TAB};

    /// 内核侧 mock：getproctab 吐整表字节；datacopy_to 记进"调用方缓冲"。
    struct FakeKernel {
        proctab: Vec<u8>,
        ticks: u64,
        hz: u32,
        sink: core::cell::RefCell<Vec<u8>>,
    }
    impl MibKernel for FakeKernel {
        fn datacopy_from(&mut self, _s: Endpoint, _a: u64, _b: &mut [u8]) -> Result<(), i32> {
            Err(minix_types::EIO)
        }
        fn datacopy_to(&mut self, _d: Endpoint, a: u64, b: &[u8]) -> Result<(), i32> {
            // 地址即调用方缓冲偏移（测试约定 addr 从 0 起）。
            let at = a as usize;
            if self.sink.borrow_mut().len() < at + b.len() {
                self.sink.borrow_mut().resize(at + b.len(), 0);
            }
            self.sink.borrow_mut()[at..at + b.len()].copy_from_slice(b);
            Ok(())
        }
        fn grant_magic(&mut self, _: Endpoint, _: u64, _: u64, _: RelayDir) -> Result<minix_types::GrantId, i32> { Ok(1) }
        fn grant_revoke(&mut self, _: minix_types::GrantId) {}
        fn getproctab(&mut self, buf: &mut [u8]) -> Result<(), i32> {
            let n = buf.len().min(self.proctab.len());
            buf[..n].copy_from_slice(&self.proctab[..n]);
            Ok(())
        }
        fn getticks(&mut self) -> Result<u64, i32> { Ok(self.ticks) }
        fn hz(&mut self) -> Result<u32, i32> { Ok(self.hz) }
    }

    /// 服务侧 mock：PM 的 SI_PROC_TAB 吐整表；VFS light 缺席（A-7 降级路径）。
    struct FakeServices {
        pm_tab: Vec<u8>,
    }
    impl MibServices for FakeServices {
        fn getnuid(&mut self, _: Endpoint) -> Result<u32, i32> { Ok(0) }
        fn getsysinfo(&mut self, t: Endpoint, what: i32, buf: &mut [u8]) -> Result<(), i32> {
            if t == Endpoint::VFS {
                return Err(minix_types::EIO); // 生产者缺席（A-7）
            }
            assert_eq!((t, what), (Endpoint::PM, SI_PROC_TAB));
            let n = buf.len().min(self.pm_tab.len());
            buf[..n].copy_from_slice(&self.pm_tab[..n]);
            Ok(())
        }
        fn ds_retrieve_label_name(&mut self, _: Endpoint, _: &mut [u8]) -> Result<usize, i32> { Err(minix_types::EIO) }
        fn remote_info(&mut self, _: Endpoint, _: &mut [u8], _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
        fn remote_call(&mut self, _: Endpoint, _: RemoteCall, _: &mut RemoteReplyWire) -> Result<(), i32> { Err(minix_types::EIO) }
        fn vm_info(&mut self, _: i32, _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
        fn pm_getparam(&mut self, _: i32, _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
    }

    /// 场景：任务 1 名 "memory"；用户槽 5 = 可运行 "ps"(pid 100)、
    /// 槽 6 = WAITING 挂起 "sh"(pid 101)、槽 7 = 僵尸(pid 103, 不进列表)。
    fn fixture() -> (FakeKernel, FakeServices) {
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
        (
            FakeKernel { proctab, ticks: 1000, hz: 50, sink: core::cell::RefCell::new(Vec::new()) },
            FakeServices { pm_tab: pm },
        )
    }

    fn e2e_ctx<'a>(
        kernel: &'a mut FakeKernel,
        svc: &'a mut FakeServices,
        tables: &'a mut Tables,
    ) -> MibCtx<'a, FakeKernel, FakeServices> {
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
        // 5 个任务槽 + 2 个在用用户槽（僵尸被过滤）——off = 7 × elsz。
        assert_eq!(out, SysctlOutcome::Done(7 * lsz as u64));

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

        // 用户槽 5（元素 5）："ps" 可运行 → LSRUN，无 wchan。
        let u5 = read(minix_types::NR_TASKS);
        assert_eq!(u5.l_pid, 100);
        assert_eq!(u5.l_stat, minix_types::LSRUN as i8);
        assert_eq!(&u5.l_name[..3], b"ps\0");
        assert_eq!(u5.l_swtime, 12); // (1000-400)/50
        assert_eq!(u5.l_wchan, 0);
        assert_eq!(u5.l_lid, 5);
        assert_eq!(u5.l_priority, 2);

        // 用户槽 6（元素 6）："sh" 在 PM WAITING、内核 SENDING 到 5 →
        // PM 词保留（Direct 不覆盖）：wchan 0x102、"wait"、可中断。
        let u6 = read(minix_types::NR_TASKS + 1);
        assert_eq!(u6.l_pid, 101);
        assert_eq!(u6.l_stat, minix_types::LSSLEEP as i8);
        assert_eq!(u6.l_wchan, 0x102);
        assert_eq!(&u6.l_wmesg[..5], b"wait\0");
        assert_eq!(u6.l_flag, (minix_types::L_INMEM | minix_types::L_SINTR) as i32);

        // 7 个元素（任务 0~4 + 用户 5、6），僵尸没有第 8 个。
        assert_eq!(sink.len(), 7 * core::mem::size_of::<KinfoLwp>());
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
        fill_user_row(&mut l, &kern, 7, &row, 1000, 50, Endpoint::NONE);
        // Direct send: wchan (endpt<<8)|0xff, "(5)" — the peer row is not in
        // the PM table so the raw number is the name (proc.c:213-214,
        // :380-382).
        assert_eq!(l.l_wchan, (5u64 << 8) | 0xff);
        assert_eq!(&l.l_wmesg[..4], b"(5)\0");
        assert_eq!(l.l_stat, LSSLEEP as i8);
    }
}
