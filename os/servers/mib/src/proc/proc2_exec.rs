//! KERN_PROC2 assembly: filters, fills, copy out.
//!
//! Mirrors the *effect* half of `mib_kern_proc2` (`proc.c:791-913`) and its
//! three fill helpers (`fill_proc2_common` :600-656, `fill_proc2_kern`
//! :657-686, `fill_proc2_user` :687-790). The row filters and state mapping
//! are decided in 18 ([`super::proc2`]); the state chain comes from 17
//! via [`super::lwp_exec::judge_row_state`]; the tables and clock from 16.
//!
//! `[ARCH: A-7]` the registered incompleteness carries over from 17 plus
//! two proc2-specific lanes:
//! - `l_pctcpu`/`l_cpticks` (the kernel row lacks `p_cpuavg`):
//!   `p_pctcpu`/`p_cpticks` answer 0;
//! - the VFS light table is absent: `fpl_tty` reads
//!   `NO_DEV` for every row, so the controlling-terminal lanes
//!   (`EPROC_CTTY`/`P_CONTROLT`/`p_tdev`/`p_tpgid`) never fire;
//! - `vm_info_usage` failures are ignored *in C* (proc.c:619 — the
//!   struct stays zeroed), so a dead VM producer is C-faithful here: the
//!   memory columns answer 0 until the VM producer lands.
//!
//! 18-mib-proc2.md.

use minix_types::{
    mp_flags, Endpoint, KinfoProc2, KI_WMESGLEN, KERN_PROC_TTY_NODEV, KERN_PROC_TTY_REVOKE,
    LSSLEEP, MProcSnap, ProcInfoStruct, SIGNAL_CHILD, SACTIVE, VMIW_USAGE,
};

use super::lwp_exec::{judge_row_state, RowState};
use super::proc2::{
    carry_sinter, check_proc2_args, copy_size, decode_req, eflag, groups_capped, headroom,
    match_kernel, match_row, map_stat, nice_output, nlwps, pflag, zombie_tty, Proc2Req,
    ProcIdentity,
};
use super::rows::{write_wmesg_named, KernelRows, PmRows};
use super::tables::ticks_to_timeval;
use crate::io::copy::Oldp;
use crate::transport::{MibKernel, MibServices};
use crate::dispatch::SysctlOutcome;
use crate::io::copy::in_range;
use crate::walker::MibCtx;

/// Walk the process list for one request. C: `mib_kern_proc2` —
/// proc.c:791-913.
///
/// `args` is the name tail past the `kern.proc2` function node:
/// `[req, arg, elsz, elmax]`.
pub fn proc2_exec<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    args: &[i32],
    oldp: Option<&Oldp>,
) -> SysctlOutcome {
    let (req, arg, elsz, elmax) = match args {
        [req, arg, elsz, elmax] => (*req, *arg, *elsz, *elmax),
        _ => return SysctlOutcome::err(minix_types::EINVAL),
    };
    let Some(kind) = decode_req(req) else {
        return SysctlOutcome::err(minix_types::EINVAL);
    };
    if check_proc2_args(args.len() as u32, elsz, elmax).is_err() {
        return SysctlOutcome::err(minix_types::EINVAL);
    }
    // The kernel has no PM/VFS slot: matched separately — proc.c:815-833.
    let kmatch = match_kernel(kind, arg as i64, KERN_PROC_TTY_NODEV);

    // update_tables — proc.c:839.
    let MibCtx { tables, kernel, svc, self_endpt, .. } = &mut *ctx;
    let self_endpt = *self_endpt;
    let now = match kernel.getticks() {
        Ok(t) => t,
        Err(code) => return SysctlOutcome::err(code),
    };
    if !tables.update(now, &mut **kernel, &mut **svc) {
        return SysctlOutcome::err(minix_types::EINVAL);
    }
    let hz = match kernel.hz() {
        Ok(h) => h,
        Err(code) => return SysctlOutcome::err(code),
    };
    let boottime = match kernel.boottime() {
        Ok(b) => b,
        Err(code) => return SysctlOutcome::err(code),
    };

    let kern = KernelRows::new(&tables.kernel_tab);
    let pm = PmRows::new(&tables.pm_tab);
    // light table wired up: proc2's tty columns (p_tdev/eflag/P_CONTROLT) light up
    // from it; when the table is absent the columns degrade to NO_DEV.
    let light_rows = super::rows::LightRows::new(&tables.vfs_tab);
    let old_len = oldp.map(|o| o.left);
    let mut elmax = elmax;
    let mut off: u64 = 0;
    let copysz = copy_size(elsz as u64, core::mem::size_of::<KinfoProc2>() as u64) as usize;

    if kmatch {
        if in_range(old_len, off) && elmax > 0 {
            let mut p = zeroed_proc2();
            fill_kern_row(&mut p, &kern, now, hz, &mut **svc);
            if let Err(code) = copy_row(&mut **kernel, oldp, off, &p, copysz) {
                return SysctlOutcome::err(code);
            }
            elmax -= 1;
        }
        off += elsz as u64;
    }

    for mslot in 0..minix_types::NR_PROCS {
        let row: MProcSnap = match pm.row(mslot) {
            Some(r) => r,
            None => continue,
        };
        if row.mp_flags & mp_flags::IN_USE == 0 {
            continue; // proc.c:859-860
        }
        // light table wired up: fpl_tty is the process's controlling terminal;
        // zombies do not read the file slot (proc.c:707's
        // `(!zombie) ? fpl_tty : NO_DEV`).
        let zombie = super::rows::PmRows::is_zombie(&row);
        let tty = zombie_tty(
            zombie,
            light_rows.row(mslot).map(|l| l.tty as i64).unwrap_or(minix_types::NO_DEV as i64),
            minix_types::NO_DEV as i64,
        );
        let parent_pid = pm
            .row(row.mp_parent.max(0) as usize)
            .filter(|_| row.mp_parent >= 0 && (row.mp_parent as usize) < minix_types::NR_PROCS)
            .map(|parent| parent.mp_pid)
            .unwrap_or(0);
        let identity = ProcIdentity {
            pid: row.mp_pid as i64,
            procgrp: row.mp_procgrp as i64,
            effuid: row.mp_effuid,
            realuid: row.mp_realuid,
            effgid: row.mp_effgid,
            realgid: row.mp_realgid,
            tty,
            tty_nodev: KERN_PROC_TTY_NODEV,
            tty_revoke: KERN_PROC_TTY_REVOKE,
            no_dev: minix_types::NO_DEV as i64,
        };
        if !match_row(kind, arg as i64, &identity) {
            continue;
        }
        if in_range(old_len, off) && elmax > 0 {
            let mut p = zeroed_proc2();
            let light = light_rows.row(mslot);
            fill_user_row(
                &mut p,
                &kern,
                &pm,
                light,
                mslot,
                &row,
                tty,
                parent_pid,
                now,
                hz,
                boottime,
                self_endpt,
                &mut **svc,
            );
            if let Err(code) = copy_row(&mut **kernel, oldp, off, &p, copysz) {
                return SysctlOutcome::err(code);
            }
            elmax -= 1;
        }
        off += elsz as u64;
    }

    // Whole-table estimates reserve fork headroom; a single-PID estimate
    // cannot grow — proc.c:909-910.
    if oldp.is_none() && kind != Proc2Req::Pid {
        off += headroom(true, false, elsz as u64);
    }
    SysctlOutcome::Done(off)
}

/// A POD-zero `KinfoProc2` (C's `memset(p, 0, sizeof(*p))` — proc.c:662,
/// :694).
fn zeroed_proc2() -> KinfoProc2 {
    // SAFETY: `KinfoProc2` is `#[repr(C)]` over integers, byte arrays and
    // POD structs — all-zero is valid for every field.
    unsafe { core::mem::zeroed() }
}

/// `mib_copyout(oldp, off, &p, copysz)` — proc.c:850-852/:886-888.
fn copy_row<K: MibKernel>(
    kernel: &mut K,
    oldp: Option<&Oldp>,
    off: u64,
    p: &KinfoProc2,
    copysz: usize,
) -> Result<(), i32> {
    let Some(oldp) = oldp else {
        return Ok(());
    };
    // SAFETY: `KinfoProc2` is `#[repr(C)]` Copy POD; the byte view feeds the
    // copy seam only.
    let bytes =
        unsafe { core::slice::from_raw_parts(p as *const KinfoProc2 as *const u8, copysz) };
    oldp.copyout(kernel, off, bytes).map(|_| ())
}

/// `fill_proc2_kern` — proc.c:657-686: the kernel pseudo-process with PID 0.
fn fill_kern_row<K: MibServices>(
    p: &mut KinfoProc2,
    kern: &KernelRows,
    now: u64,
    hz: u32,
    svc: &mut K,
) {
    p.p_flag = (minix_types::L_INMEM | minix_types::L_SYSTEM | minix_types::L_SINTR) as i32;
    p.p_stat = LSSLEEP as i8;
    // NetBSD's niceness zero point (proc.c:666).
    p.p_nice = minix_types::NZERO as u8;
    // C uses the KERNEL endpoint number (com.h:51 = -1) for the wchan so ps
    // and top agree with the LWP view (proc.c:669-670).
    // `| 0x00` in C expresses a "class 0x00" construct; the Rust side omits it (a no-op).
    p.p_wchan = (Endpoint::KERNEL.0 as i64 as u64) << 8;
    put_name(&mut p.p_wmesg, b"kernel\0");
    put_name(&mut p.p_comm, b"kernel\0");
    p.p_realflag = (minix_types::P_INMEM | minix_types::P_SYSTEM | minix_types::P_SINTR) as u64;
    p.p_realstat = SACTIVE as u64;
    p.p_nlwps = minix_types::NR_TASKS as u64;
    // The KERNEL task's own row feeds the usage average (proc.c:681-684).
    let kp = kern
        .row((Endpoint::KERNEL.0 + minix_types::NR_TASKS as i32) as usize)
        .unwrap_or_default();
    fill_common(p, &kp, now, hz, svc);
}

/// `fill_proc2_user` — proc.c:687-790.
#[allow(clippy::too_many_arguments)]
fn fill_user_row<K: MibServices>(
    p: &mut KinfoProc2,
    kern: &KernelRows,
    pm: &PmRows,
    light: Option<super::rows::LightRow>,
    mslot: usize,
    row: &MProcSnap,
    tty: i64,
    parent_pid: i32,
    now: u64,
    hz: u32,
    boottime: u64,
    self_endpt: Endpoint,
    svc: &mut K,
) {
    let kp = kern
        .row(minix_types::NR_TASKS + mslot)
        .unwrap_or_default();
    let zombie = PmRows::is_zombie(row);
    let has_tty = tty != minix_types::NO_DEV as i64;

    // Extended flags: terminal + session leadership by pid==pgrp (C leaves the
    // job-control part unimplemented — proc.c:709-713).
    p.p_eflag = eflag(has_tty, row.mp_pid == row.mp_procgrp) as i32;
    p.p_exitsig = SIGNAL_CHILD;
    p.p_flag = pflag(
        row.mp_flags & mp_flags::TAINTED != 0,
        row.mp_tracer != 0, // NO_TRACER = 0 (pm const.h:11)
        has_tty,
    ) as i32;
    p.p_pid = row.mp_pid;
    p.p_ppid = parent_pid;
    p.p_sid = row.mp_procgrp; // job-control part unimplemented, proc.c:727
    p.p_pgid = row.mp_procgrp;
    p.p_tpgid = if has_tty { row.mp_procgrp } else { 0 };
    p.p_uid = row.mp_effuid;
    p.p_ruid = row.mp_realuid;
    p.p_gid = row.mp_effgid;
    p.p_rgid = row.mp_realgid;
    let groups = groups_capped(row.mp_ngroups as usize);
    p.p_groups[..groups].copy_from_slice(&row.mp_sgroups[..groups]);
    p.p_ngroups = groups as i16;
    // Signal words: the snapshot carries the first word of each set
    // (the same ruling as dump printing only the first word); the remaining words stay zero.
    p.p_siglist = ki_sigset(row.mp_sigpending0);
    p.p_sigmask = ki_sigset(row.mp_sigmask0);
    p.p_sigcatch = ki_sigset(row.mp_catch0);
    p.p_sigignore = ki_sigset(row.mp_ignore0);
    p.p_nice = nice_output(row.mp_nice) as u8;
    put_name(&mut p.p_comm, &row.mp_name);
    // User of the row: start wall time is boot time + the started ticks
    // (proc.c:744-747); child times ride the snapshot.
    p.p_uvalid = 1;
    let (s_sec, s_usec) = ticks_to_timeval(row.mp_started, hz as u64);
    p.p_ustart_sec = (boottime.saturating_add(s_sec)) as u32;
    p.p_ustart_usec = s_usec as u32;
    let (c_sec, c_usec) =
        ticks_to_timeval(row.mp_child_utime.saturating_add(row.mp_child_stime), hz as u64);
    p.p_uctime_sec = c_sec as u32;
    p.p_uctime_usec = c_usec as u32;
    p.p_realflag = p.p_flag as u64;
    p.p_nlwps = nlwps(zombie) as u64;
    p.p_svuid = row.mp_svuid;
    p.p_svgid = row.mp_svgid;

    // The state chain is 17's — identical to the LWP fill (proc.c:755-757).
    let st: RowState = judge_row_state(&kp, row, light, self_endpt);
    p.p_stat = st.stat as i8;
    if st.stat == LSSLEEP {
        p.p_wchan = st.wchan;
        // Prerequisite for C's `if (p->p_flag & L_SINTR) p->p_realflag |= P_SINTR`:
        // a sleep row's L_SINTR bit is judged by carry_sinter (proc.c:767-768); here
        // the p_flag bit lands first, following the fill shape.
        if st.sinter {
            p.p_flag |= minix_types::L_SINTR as i32;
        }
        let mut buf = [0u8; KI_WMESGLEN];
        write_wmesg_named(&mut buf, kern, pm, KernelRows::blocked_on(&kp), true);
        p.p_wmesg = buf;
    }
    let (realstat, display) = map_stat(st.stat);
    p.p_realstat = realstat as u64;
    p.p_stat = display as i8;
    if carry_sinter(st.stat, st.sinter) {
        p.p_realflag |= minix_types::P_SINTR as u64;
    }

    if !zombie {
        // Zombies never reach the kernel row's usage half (proc.c:788-789).
        fill_common(p, &kp, now, hz, svc);
    }
}

/// `fill_proc2_common` — proc.c:600-656: the LWP-common numbers plus the
/// per-time splits and the VM usage columns.
fn fill_common<K: MibServices>(
    p: &mut KinfoProc2,
    kp: &ProcInfoStruct,
    now: u64,
    hz: u32,
    svc: &mut K,
) {
    p.p_rtime_sec = {
        let (sec, usec) = ticks_to_timeval(
            kp.p_user_time.saturating_add(kp.p_sys_time),
            hz as u64,
        );
        p.p_rtime_usec = usec as u32;
        sec as u32
    };
    // p_cpuavg rides the kernel row once the kernel row carries it.
    p.p_cpticks = 0;
    p.p_pctcpu = 0;
    let (swtime, slptime) = super::lwp::judge_times(true, now, 0, now, hz);
    p.p_swtime = swtime;
    p.p_slptime = slptime;
    p.p_uticks = kp.p_user_time;
    p.p_sticks = kp.p_sys_time;
    // User/system time each as its own timeval (proc.c:625-630).
    let (uu_sec, uu_usec) = ticks_to_timeval(kp.p_user_time, hz as u64);
    p.p_uutime_sec = uu_sec as u32;
    p.p_uutime_usec = uu_usec as u32;
    let (us_sec, us_usec) = ticks_to_timeval(kp.p_sys_time, hz as u64);
    p.p_ustime_sec = us_sec as u32;
    p.p_ustime_usec = us_usec as u32;
    p.p_priority = kp.p_priority as u8;
    p.p_usrpri = kp.p_priority as u8;
    p.p_cpuid = kp.p_cpu as u64;

    // VM usage columns. C ignores failures (proc.c:617-620 — `memset` then
    // `(void)vm_info_usage(...)`), so a dead VM producer answers zeroed
    // columns, byte-for-byte C's dead-VM behavior.
    let mut vui = [0u8; 64];
    // C `vm_info_usage(kp->p_endpoint, &vui)` — ep is the process being queried. The
    // inline encoding of VM's reply is still a registered gap (26-vm-queries §3.7);
    // failures fold to zero columns per C semantics.
    //[SD19-gate] vm_info is best-effort and folds to zero columns; a failure fills
    // zero rather than breaking collection
    #[allow(clippy::let_underscore_must_use)]
    let _ = svc.vm_info(VMIW_USAGE, Endpoint(kp.p_endpoint), &mut vui);
    let word = |i: usize| -> u64 {
        u64::from_le_bytes(vui[i * 8..i * 8 + 8].try_into().unwrap())
    };
    // `struct vm_usage_info` field order — vm.h:48-58: total, common,
    // shared, virtual, mvirtual, maxrss, minflt, majflt.
    const PAGE_SIZE: u64 = 4096;
    let howmany = |v: u64| v.div_ceil(PAGE_SIZE);
    p.p_vm_rssize = howmany(word(0)) as i32;
    p.p_vm_vsize = howmany(word(3)) as i64;
    p.p_vm_msize = howmany(word(4)) as i64;
    p.p_uru_maxrss = word(5);
    p.p_uru_minflt = word(6);
    p.p_uru_majflt = word(7);
}

/// First-word sigset: the snapshot carries `__bits[0]`; the rest stay zero.
fn ki_sigset(word0: u32) -> minix_types::KiSigset {
    minix_types::KiSigset { __bits: [word0, 0, 0, 0] }
}

/// `strlcpy` a NUL-terminated name into a fixed buffer (shared shape with
/// 17's fill).
fn put_name(dst: &mut [u8], src: &[u8]) {
    let end = src.iter().position(|b| *b == 0).unwrap_or(src.len());
    let keep = end.min(dst.len() - 1);
    dst[..keep].copy_from_slice(&src[..keep]);
    dst[keep] = 0;
}

/// Unit tests: the two fills over synthetic rows (the walker-level
/// end-to-end lives at the bottom).
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_types::RTS_SENDING;

    fn kern_table() -> Vec<u8> {
        let krow = core::mem::size_of::<ProcInfoStruct>();
        let mut v = vec![0u8; (minix_types::NR_TASKS + minix_types::NR_PROCS) * krow];
        // KERNEL task row (kslot 4): the pseudo-process's usage source.
        // SAFETY(test): repr(C) POD write into the slot window.
        unsafe {
            core::ptr::write_unaligned(
                v.as_mut_ptr().add(4 * krow) as *mut ProcInfoStruct,
                ProcInfoStruct {
                    p_name: name_bytes("kernel"),
                    p_endpoint: -1,
                    ..ProcInfoStruct::default()
                },
            );
        }
        // User slot 5: runnable "ps".
        unsafe {
            core::ptr::write_unaligned(
                v.as_mut_ptr().add((minix_types::NR_TASKS + 5) * krow) as *mut ProcInfoStruct,
                ProcInfoStruct {
                    p_nr: 5,
                    p_endpoint: 5,
                    p_name: name_bytes("ps"),
                    p_priority: 2,
                    p_user_time: 30,
                    p_sys_time: 10,
                    ..ProcInfoStruct::default()
                },
            );
        }
        v
    }

    fn pm_table() -> Vec<u8> {
        let prow = core::mem::size_of::<MProcSnap>();
        let mut v = vec![0u8; minix_types::NR_PROCS * prow];
        // SAFETY(test): repr(C) POD write into the slot window.
        unsafe {
            core::ptr::write_unaligned(
                v.as_mut_ptr() as *mut MProcSnap,
                MProcSnap {
                    mp_pid: 1,
                    mp_procgrp: 1,
                    mp_flags: mp_flags::IN_USE,
                    mp_name: name_bytes("init"),
                    mp_effuid: 0,
                    mp_realuid: 0,
                    mp_started: 400,
                    mp_child_utime: 25,
                    mp_child_stime: 50,
                    mp_ngroups: 2,
                    mp_sgroups: [10, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                    ..MProcSnap::default()
                },
            );
        }
        v
    }

    /// C fills proc2's common half through an LWP channel other than
    /// `cpuavg_getstats` — this test pins the independent split of uticks/sticks and
    /// user/system times (proc.c:623-630).
    #[test]
    fn test_kern_row_shape() {
        let ktab = kern_table();
        let kern = KernelRows::new(&ktab);
        struct NoVm;
        impl crate::transport::MibServices for NoVm {
            fn getnuid(&mut self, _: Endpoint) -> Result<u32, i32> { Ok(0) }
            fn getsysinfo(&mut self, _: Endpoint, _: i32, _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
            fn ds_retrieve_label_name(&mut self, _: Endpoint, _: &mut [u8]) -> Result<usize, i32> { Err(minix_types::EIO) }
            fn remote_info(&mut self, _: Endpoint, _: &mut [u8], _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
            fn remote_call(&mut self, _: Endpoint, _: crate::io::relay::RemoteCall, _: &mut crate::io::relay::RemoteReplyWire) -> Result<(), i32> { Err(minix_types::EIO) }
            fn vm_info(&mut self, _: i32, _: Endpoint, _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
            fn pm_getparam(&mut self, _: i32, _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
        }
        let mut svc = NoVm;
        let mut p = zeroed_proc2();
        fill_kern_row(&mut p, &kern, 1000, 50, &mut svc);
        assert_eq!(p.p_pid, 0);
        assert_eq!(p.p_stat, LSSLEEP as i8);
        assert_eq!(p.p_nice, minix_types::NZERO as u8);
        assert_eq!(p.p_wchan, ((Endpoint::KERNEL.0 as i64 as u64) << 8));
        assert_eq!(&p.p_wmesg[..7], b"kernel\0");
        assert_eq!(p.p_nlwps, minix_types::NR_TASKS as u64);
        assert_eq!(p.p_realstat, SACTIVE as u64);
        // VM absent → all-zero columns (C's dead-VM behavior, proc.c:617-620).
        assert_eq!(p.p_vm_rssize, 0);
        assert_eq!(p.p_uru_maxrss, 0);
    }

    // ── walker-level end to end: producer (shared mock) → Tables pull → real
    // `sysctl()` through CTL_KERN/KERN_PROC2 → copy out bytes and decode row by row
    // for assertions. ──

    use crate::auth::CallAuth;
    use crate::heap::MibBudget;
    use crate::io::copy::{Oldp, Newp};
    use crate::proc::test_mocks::{name_bytes, put_row, FakeKernel, FakeServices};
    use crate::proc::Tables;
    use crate::tree::arena::MibTree;
    use crate::walker::{self, Request};
    use core::cell::RefCell;
    use minix_types::{KI_NGROUPS, KERN_PROC_ALL, KERN_PROC_PID, KERN_PROC_TTY, KERN_PROC_UID};

    /// Scenario: the kernel pseudo-process + two user processes (init has an
    /// all-zero kernel row → runnable; "sh" has supplementary groups and child
    /// clocks, and its kernel row is SENDING to 5 → the PM word is kept by the Direct
    /// lane).
    fn fixture() -> (FakeKernel, FakeServices) {
        let krow = core::mem::size_of::<ProcInfoStruct>();
        let mut proctab = vec![0u8; (minix_types::NR_TASKS + minix_types::NR_PROCS) * krow];
        put_row(
            &mut proctab,
            (Endpoint::KERNEL.0 + minix_types::NR_TASKS as i32) as usize,
            &ProcInfoStruct {
                p_name: name_bytes("kernel"),
                p_endpoint: -1,
                ..ProcInfoStruct::default()
            },
        );
        put_row(
            &mut proctab,
            minix_types::NR_TASKS + 6,
            &ProcInfoStruct {
                p_nr: 6,
                p_rts_flags: RTS_SENDING,
                p_sendto_e: 5,
                p_getfrom_e: Endpoint::NONE.0,
                p_name: name_bytes("sh"),
                ..ProcInfoStruct::default()
            },
        );
        let prow = core::mem::size_of::<MProcSnap>();
        let mut pm = vec![0u8; minix_types::NR_PROCS * prow];
        put_row(&mut pm, 1, &MProcSnap {
            mp_pid: 1,
            mp_procgrp: 1,
            mp_flags: mp_flags::IN_USE,
            mp_name: name_bytes("init"),
            mp_started: 400,
            mp_ngroups: 2,
            mp_sgroups: {
                let mut g = [0u32; KI_NGROUPS];
                g[0] = 10;
                g[1] = 20;
                g
            },
            ..MProcSnap::default()
        });
        put_row(&mut pm, 6, &MProcSnap {
            mp_pid: 101,
            mp_procgrp: 101,
            mp_flags: mp_flags::IN_USE | mp_flags::WAITING,
            mp_name: name_bytes("sh"),
            mp_started: 500,
            ..MProcSnap::default()
        });
        (
            FakeKernel {
                proctab,
                ticks: 1000,
                hz: 50,
                boot: 172_800,
                sink: RefCell::new(Vec::new()),
                target_base: 0,
                target_mem: Vec::new(),
            },
            FakeServices { pm_tab: pm, light_tab: Vec::new() },
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

    fn query(
        c: &mut MibCtx<FakeKernel, FakeServices>,
        req: i32,
        arg: i32,
        elsz: i32,
        elmax: i32,
        with_oldp: bool,
    ) -> SysctlOutcome {
        walker::sysctl(
            c,
            &mut Request {
                name: &[minix_types::CTL_KERN, minix_types::KERN_PROC2, req, arg, elsz, elmax],
                oldp: if with_oldp {
                    Some(Oldp { endpt: Endpoint::PM, addr: 0, left: 1 << 20 })
                } else {
                    None
                },
                newp: None,
            },
        )
    }

    #[test]
    fn test_proc2_end_to_end_listing() {
        let (mut k, mut s) = fixture();
        let mut tables = Tables::new();
        let lsz = core::mem::size_of::<KinfoProc2>() as i32;
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        // KERN_PROC_ALL: kernel pseudo-process + two in-use rows (init, sh) = 3 elements.
        let out = query(&mut c, KERN_PROC_ALL, 0, lsz, 100, true);
        assert_eq!(out, SysctlOutcome::Done(3 * lsz as u64));

        let sink = c.kernel.sink.borrow();
        let read = |i: usize| -> KinfoProc2 {
            // SAFETY(test): repr(C) POD read; i is within the copied-out range.
            unsafe {
                core::ptr::read_unaligned(
                    sink.as_ptr().add(i * core::mem::size_of::<KinfoProc2>()) as *const KinfoProc2,
                )
            }
        };

        // Element 0: the kernel pseudo-process (the proc.c:657-686 shape).
        let k = read(0);
        assert_eq!(k.p_pid, 0);
        assert_eq!(k.p_stat, LSSLEEP as i8);
        assert_eq!(k.p_wchan, (Endpoint::KERNEL.0 as i64 as u64) << 8);
        assert_eq!(&k.p_comm[..7], b"kernel\0");
        assert_eq!(k.p_nlwps, minix_types::NR_TASKS as u64);
        assert_eq!(k.p_realstat, SACTIVE as u64);
        assert_eq!(k.p_vm_rssize, 0, "VM 缺席 → 零列（C 死 VM 行为）");

        // Element 1: "init" — group table, ustart, session leader, runnable (all-zero kernel row).
        let i1 = read(1);
        assert_eq!(i1.p_pid, 1);
        assert_eq!(i1.p_stat, minix_types::LSRUN as i8);
        assert_eq!(i1.p_ngroups, 2);
        assert_eq!(i1.p_groups[0], 10);
        assert_eq!(i1.p_groups[1], 20);
        assert_eq!(i1.p_ustart_sec, 172_800 + 8);
        assert_eq!(i1.p_eflag & minix_types::EPROC_SLEADER as i32, minix_types::EPROC_SLEADER as i32);
        assert_eq!(i1.p_uvalid, 1);
        assert_eq!(i1.p_svuid, 0);
    }

    #[test]
    fn test_proc2_end_to_end_filters_and_gates() {
        let (mut k, mut s) = fixture();
        let mut tables = Tables::new();
        let lsz = core::mem::size_of::<KinfoProc2>() as i32;

        // Exact PID lookup: the kernel row does not match (arg≠0) → only the "init" row.
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = query(&mut c, KERN_PROC_PID, 1, lsz, 100, false);
        assert_eq!(out, SysctlOutcome::Done(lsz as u64));

        // Unknown pid: kmatch=false and no matching rows → length 0.
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = query(&mut c, KERN_PROC_PID, 999, lsz, 100, false);
        assert_eq!(out, SysctlOutcome::Done(0));

        // UID filter: with arg=0 the kernel pseudo-process matches via kmatch
        // (proc.c:823-828), plus init and sh with effuid==0 = 3 rows (oldp present →
        // no extra headroom entry).
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = query(&mut c, KERN_PROC_UID, 0, lsz, 100, true);
        assert_eq!(out, SysctlOutcome::Done(3 * lsz as u64));

        // TTY NODEV sentinel: the light table is absent → every row is "no terminal"
        // (including the kernel pseudo-process: kmatch is true for NODEV).
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = query(&mut c, KERN_PROC_TTY, KERN_PROC_TTY_NODEV as i32, lsz, 100, true);
        assert_eq!(out, SysctlOutcome::Done(3 * lsz as u64));

        // Unknown filter number → EINVAL (proc.c:832-833).
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = query(&mut c, 9, 0, lsz, 100, false);
        assert_eq!(out, SysctlOutcome::err(minix_types::EINVAL));

        // Length estimation (oldp=NULL): a whole-table estimate includes the
        // EXTRA_PROCS headroom; a single-PID estimate does not (proc.c:909-910).
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = query(&mut c, KERN_PROC_ALL, 0, lsz, 100, false);
        assert_eq!(
            out,
            SysctlOutcome::Done((3 * lsz as u64) + 8 * lsz as u64)
        );
        let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
        let out = query(&mut c, KERN_PROC_PID, 1, lsz, 100, false);
        assert_eq!(out, SysctlOutcome::Done(lsz as u64));
        let _ = Newp { endpt: Endpoint::NONE, addr: 0, len: 0 };
    }

    #[test]
    fn test_user_row_shape() {
        let ktab = kern_table();
        let kern = KernelRows::new(&ktab);
        let ptab = pm_table();
        let pm = PmRows::new(&ptab);
        struct NoVm;
        impl crate::transport::MibServices for NoVm {
            fn getnuid(&mut self, _: Endpoint) -> Result<u32, i32> { Ok(0) }
            fn getsysinfo(&mut self, _: Endpoint, _: i32, _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
            fn ds_retrieve_label_name(&mut self, _: Endpoint, _: &mut [u8]) -> Result<usize, i32> { Err(minix_types::EIO) }
            fn remote_info(&mut self, _: Endpoint, _: &mut [u8], _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
            fn remote_call(&mut self, _: Endpoint, _: crate::io::relay::RemoteCall, _: &mut crate::io::relay::RemoteReplyWire) -> Result<(), i32> { Err(minix_types::EIO) }
            fn vm_info(&mut self, _: i32, _: Endpoint, _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
            fn pm_getparam(&mut self, _: i32, _: &mut [u8]) -> Result<(), i32> { Err(minix_types::EIO) }
        }
        let mut svc = NoVm;
        let row = pm.row(0).unwrap();
        let mut p = zeroed_proc2();
        fill_user_row(
            &mut p, &kern, &pm, None, 0, &row,
            minix_types::NO_DEV as i64, // legacy state: no light row supplied → no terminal
            0, // the root process has no parent
            1000, 50, 86400 * 2, Endpoint::NONE, &mut svc,
        );
        assert_eq!(p.p_pid, 1);
        // All-zero kernel row → rts_flags==0 → runnable (proc_is_runnable, proc.h:170).
        assert_eq!(p.p_stat, minix_types::LSRUN as i8);
        assert_eq!(p.p_realstat, SACTIVE as u64);
        assert_eq!(&p.p_comm[..5], b"init\0");
        assert_eq!(p.p_uid, 0);
        assert_eq!(p.p_ngroups, 2);
        assert_eq!(p.p_groups[0], 10);
        assert_eq!(p.p_groups[1], 20);
        // ustart = boottime(172800) + started(400/50 = 8s).
        assert_eq!(p.p_ustart_sec, 172800 + 8);
        // uctime = (25+50)/50 = 1s.
        assert_eq!(p.p_uctime_sec, 1);
        assert_eq!(p.p_svuid, 0);
        // Session leader: pid == pgrp (proc.c:711-712).
        assert_eq!(
            p.p_eflag & minix_types::EPROC_SLEADER as i32,
            minix_types::EPROC_SLEADER as i32
        );
        // No controlling terminal: the CTTY lane does not trigger.
        assert_eq!(p.p_eflag & minix_types::EPROC_CTTY as i32, 0);
        assert_eq!(p.p_tpgid, 0);
    }
}
