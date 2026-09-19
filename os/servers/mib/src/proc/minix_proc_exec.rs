//! The ProcFS door: MINIX_PROC LIST/DATA assembly.
//!
//! Mirrors the *effect* half of `mib_minix_proc_list` (proc.c:1177-1214)
//! and `mib_minix_proc_data` (proc.c:1217-1288) — the last two unwired
//! format functions of the process family. The decisions live in 20
//! ([`super::minix_proc`]); the tables and pull discipline in 16; the
//! blocked-on macro in [`super::rows`].
//!
//! ProcFS semantics (unlike the CTL_KERN nodes): a negative PID names a
//! kernel task, a positive one a user process, and PID 0 is nothing.
//!
//! `[ARCH: A-7]` `mpd_kipc_cycles`/`mpd_kcall_cycles` answer 0: the kernel
//! GET_PROCTAB row does not carry `p_kipc_cycles`/`p_kcall_cycles` yet
//! (edge4 §2 C-22 first half, edge1's producer).
//!
//! 20-mib-proc-minix.md.

use alloc::vec;
use crate::transport::MibServices;
use minix_types::{mp_flags, MinixProcData, MinixProcList, ProcInfoStruct};

use super::minix_proc::{
    check_data_namelen, data_flags, is_task_pid, list_flags, list_row_included, mflags_for,
    name_source, resolve_task_slot, NameSource, ProcState,
};
use super::rows::{build_pid_hash, KernelRows, PmRows};
use super::tables::{chain_lookup, hash_slots, NO_SLOT};
use crate::dispatch::SysctlOutcome;
use crate::io::copy::Oldp;
use crate::transport::MibKernel;
use crate::subtree::minix::ProcDoor;
use crate::walker::MibCtx;

/// Serve one ProcDoor request. C: `mib_minix_proc_list` (proc.c:1177-1214)
/// for [`ProcDoor::List`], `mib_minix_proc_data` (proc.c:1217-1288) for
/// [`ProcDoor::Data`].
pub fn proc_door_exec<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    args: &[i32],
    door: ProcDoor,
    oldp: Option<&Oldp>,
) -> SysctlOutcome {
    match door {
        ProcDoor::List => proc_list(ctx, oldp),
        ProcDoor::Data => proc_data(ctx, args, oldp),
    }
}

/// `mib_minix_proc_list` — proc.c:1177-1214: the whole-table snapshot,
/// always copied in full (the sink learns every slot, used or not).
fn proc_list<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    oldp: Option<&Oldp>,
) -> SysctlOutcome {
    let total = minix_types::NR_PROCS * core::mem::size_of::<MinixProcList>();
    if oldp.is_none() {
        return SysctlOutcome::Done(total as u64);
    }
    let now = match ctx.kernel.getticks() {
        Ok(t) => t,
        Err(code) => return SysctlOutcome::err(code),
    };
    if !ctx.tables.update(now, ctx.kernel, ctx.svc) {
        return SysctlOutcome::err(minix_types::EINVAL);
    }
    let pm = PmRows::new(&ctx.tables.pm_tab);

    // C fills the whole array, zeroing unused slots (proc.c:1189-1206).
    let mut mpl = vec![MinixProcList {
        mpl_flags: 0,
        mpl_pid: 0,
        mpl_uid: 0,
        mpl_gid: 0,
    }; minix_types::NR_PROCS];
    for (mslot, slot) in mpl.iter_mut().enumerate() {
        let Some(row) = pm.row(mslot) else { continue };
        if !list_row_included(PmRows::in_use(&row), row.mp_pid) {
            continue;
        }
        *slot = MinixProcList {
            mpl_flags: list_flags(PmRows::is_zombie(&row)),
            mpl_pid: row.mp_pid,
            mpl_uid: row.mp_effuid,
            mpl_gid: row.mp_effgid,
        };
    }

    // SAFETY: `MinixProcList` is `#[repr(C)]` Copy POD; the byte view feeds
    // the copy seam only.
    let bytes = unsafe {
        core::slice::from_raw_parts(mpl.as_ptr() as *const u8, total)
    };
    match oldp.unwrap().copyout(ctx.kernel, 0, bytes) {
        Ok(_) => SysctlOutcome::Done(total as u64),
        Err(code) => SysctlOutcome::err(code),
    }
}

/// `mib_minix_proc_data` — proc.c:1217-1288: one row by PID, ProcFS slot
/// semantics (negative = kernel task, 0 = nothing).
fn proc_data<K: MibKernel, S: MibServices>(
    ctx: &mut MibCtx<K, S>,
    args: &[i32],
    oldp: Option<&Oldp>,
) -> SysctlOutcome {
    let &[pid] = args else {
        return SysctlOutcome::err(minix_types::EINVAL);
    };
    if check_data_namelen(args.len() as u32).is_err() {
        return SysctlOutcome::err(minix_types::EINVAL);
    }
    let now = match ctx.kernel.getticks() {
        Ok(t) => t,
        Err(code) => return SysctlOutcome::err(code),
    };
    if !ctx.tables.update(now, ctx.kernel, ctx.svc) {
        return SysctlOutcome::err(minix_types::EINVAL);
    }
    let pm = PmRows::new(&ctx.tables.pm_tab);

    // ProcFS slot semantics — proc.c:1240-1251.
    let (mslot, kslot) = if is_task_pid(pid) {
        let kslot = match resolve_task_slot(pid, minix_types::NR_TASKS as i32) {
            Ok(k) => k,
            Err(code) => return SysctlOutcome::err(code),
        };
        (None, kslot)
    } else {
        let (hash, next, pids) = build_pid_hash(&pm);
        let mslot = chain_lookup(
            &hash,
            &next,
            &pids,
            pid,
            hash_slots(minix_types::NR_PROCS as u32),
        );
        if mslot == NO_SLOT {
            return SysctlOutcome::err(minix_types::ESRCH);
        }
        (Some(mslot), minix_types::NR_TASKS as i32 + mslot)
    };

    let size = core::mem::size_of::<MinixProcData>() as u64;
    if oldp.is_none() {
        return SysctlOutcome::Done(size);
    }

    let kp: ProcInfoStruct = ctx
        .tables
        .kernel_tab
        .get(kslot as usize * core::mem::size_of::<ProcInfoStruct>()
            ..(kslot as usize + 1) * core::mem::size_of::<ProcInfoStruct>())
        .map(|w| {
            // SAFETY: `ProcInfoStruct` is `#[repr(C)]` Copy POD; the window
            // is exactly one row.
            unsafe { core::ptr::read_unaligned(w.as_ptr() as *const _) }
        })
        .unwrap_or_default();

    // Tasks read no management flags (proc.c:1261) and their name comes
    // from the kernel row (proc.c:1280-1285).
    let mflags = match mslot {
        Some(m) => mflags_for(pid, pm.row(m as usize).map(|r| r.mp_flags).unwrap_or(0)),
        None => 0,
    };
    let state = if mflags & (mp_flags::TRACE_ZOMBIE | mp_flags::ZOMBIE) != 0 {
        ProcState::Zombie
    } else if mflags & mp_flags::TRACE_STOPPED != 0 || KernelRows::is_p_stopped(&kp) {
        ProcState::Stopped
    } else if KernelRows::is_runnable(&kp) {
        ProcState::Runnable
    } else {
        ProcState::Other
    };

    let mut mpd = MinixProcData {
        mpd_endpoint: kp.p_endpoint,
        mpd_flags: data_flags(mflags & mp_flags::PRIV_PROC != 0, state),
        mpd_blocked_on: KernelRows::blocked_on(&kp),
        mpd_priority: kp.p_priority as u32,
        mpd_user_time: kp.p_user_time as u32,
        mpd_sys_time: kp.p_sys_time as u32,
        mpd_cycles: kp.p_cycles,
        // A-7: the kernel row lacks these two counters until C-22's first
        // half (edge1's producer); they answer 0, matching a zeroed row.
        mpd_kipc_cycles: 0,
        mpd_kcall_cycles: 0,
        mpd_nice: 0,
        mpd_name: [0; 16],
    };
    // C: `if (kslot >= NR_TASKS)` — user rows take nice+name from PM,
    // tasks from the kernel row (proc.c:1280-1285).
    if name_source(kslot, minix_types::NR_TASKS as i32) == NameSource::User {
        if let Some(r) = mslot.and_then(|m| pm.row(m as usize)) {
            mpd.mpd_nice = r.mp_nice as u32;
            put_name(&mut mpd.mpd_name, &r.mp_name);
        }
    } else {
        put_name(&mut mpd.mpd_name, &kp.p_name);
    }

    // SAFETY: `MinixProcData` is `#[repr(C)]` Copy POD; the byte view feeds
    // the copy seam only.
    let bytes =
        unsafe { core::slice::from_raw_parts(&mpd as *const MinixProcData as *const u8, size as usize) };
    match oldp.unwrap().copyout(ctx.kernel, 0, bytes) {
        Ok(_) => SysctlOutcome::Done(size),
        Err(code) => SysctlOutcome::err(code),
    }
}

/// `strlcpy` a NUL-terminated name into a fixed buffer (shared shape with
/// 17/18's fills).
fn put_name(dst: &mut [u8], src: &[u8]) {
    let end = src.iter().position(|b| *b == 0).unwrap_or(src.len());
    let keep = end.min(dst.len() - 1);
    dst[..keep].copy_from_slice(&src[..keep]);
    dst[keep] = 0;
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::CallAuth;
    use crate::heap::MibBudget;
    use crate::io::copy::Oldp;
    use crate::proc::test_mocks::{put_row, FakeKernel, FakeServices};
    use crate::tree::arena::MibTree;
    use crate::walker::{self, Request};
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::RefCell;
    use minix_types::{
        Endpoint, MPLF_IN_USE, MPLF_ZOMBIE, MPDF_RUNNABLE, MPDF_SYSTEM, MPDF_ZOMBIE,
        MProcSnap as Row, ProcInfoStruct,
    };
    use crate::proc::Tables;

    /// 场景：内核行 0 = KERNEL 任务（可运行）；PM 槽 1 = "init"（IN_USE、
    /// PRIV_PROC），槽 2 = 僵尸 "dead"，槽 3 = pid 0 的空壳（被 list 过滤）。
    fn padded(name: &[u8]) -> [u8; 16] {
        let mut n = [0u8; 16];
        n[..name.len()].copy_from_slice(name);
        n
    }
    fn kernel_name() -> [u8; 16] { padded(b"kernel") }
    fn init_name() -> [u8; 16] { padded(b"init") }
    fn dead_name() -> [u8; 16] { padded(b"dead") }

    fn fixture() -> (FakeKernel, FakeServices) {
        let krow = core::mem::size_of::<ProcInfoStruct>();
        let mut proctab = vec![0u8; (minix_types::NR_TASKS + minix_types::NR_PROCS) * krow];
        // pid -4（KERNEL 任务）→ kslot = -4 + NR_TASKS = 1（proc.c:1247）。
        put_row(
            &mut proctab,
            1,
            &ProcInfoStruct {
                p_name: kernel_name(),
                p_endpoint: -4,
                ..ProcInfoStruct::default()
            },
        );
        put_row(
            &mut proctab,
            minix_types::NR_TASKS + 1,
            &ProcInfoStruct {
                p_nr: 1,
                p_endpoint: 1,
                p_name: init_name(),
                ..ProcInfoStruct::default()
            },
        );
        let prow = core::mem::size_of::<Row>();
        let mut pm = vec![0u8; minix_types::NR_PROCS * prow];
        let put_m = |v: &mut Vec<u8>, mslot: usize, row: &Row| {
            // SAFETY(test): repr(C) POD 写进槽窗口，索引在界内。
            unsafe {
                core::ptr::write_unaligned(v.as_mut_ptr().add(mslot * prow) as *mut Row, *row);
            }
        };
        put_m(&mut pm, 1, &Row {
            mp_pid: 1,
            mp_procgrp: 1,
            mp_flags: mp_flags::IN_USE | mp_flags::PRIV_PROC,
            mp_name: init_name(),
            mp_started: 100,
            mp_nice: 5,
            ..Row::default()
        });
        put_m(&mut pm, 2, &Row {
            mp_pid: 99,
            mp_flags: mp_flags::IN_USE | mp_flags::ZOMBIE,
            mp_name: dead_name(),
            ..Row::default()
        });
        put_m(&mut pm, 3, &Row {
            mp_pid: 0, // pid 0：list 过滤（proc.c:1200 的 `pid <= 0`）
            mp_flags: mp_flags::IN_USE,
            ..Row::default()
        });
        (
            FakeKernel {
                proctab,
                ticks: 1000,
                hz: 50,
                boot: 0,
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

    /// PROC_LIST：整表 256 行全量拷出，两行在用——init（uid/gid/无僵尸
    /// 位）与 dead（僵尸位）；pid 0 空壳被过滤（proc.c:1200）。
    #[test]
    fn test_proc_list_end_to_end() {
        let (mut k, mut s) = fixture();
        let mut tables = Tables::new();
        let total = minix_types::NR_PROCS * core::mem::size_of::<MinixProcList>();
        {
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = walker::sysctl(
                &mut c,
                &mut Request {
                    name: &[minix_types::CTL_MINIX, minix_types::MINIX_PROC, minix_types::PROC_LIST],
                    oldp: Some(Oldp { endpt: Endpoint::PM, addr: 0, left: total as u64 }),
                    newp: None,
                },
            );
            assert_eq!(out, SysctlOutcome::Done(total as u64));
            let sink = c.kernel.sink.borrow();
            let snapshot: Vec<u8> = sink.clone();
            drop(sink);
            drop(c);
            let row = |i: usize| -> MinixProcList {
                // SAFETY(test): repr(C) POD 读，i 在拷出范围内。
                unsafe {
                    core::ptr::read_unaligned(
                        snapshot.as_ptr().add(i * core::mem::size_of::<MinixProcList>())
                            as *const MinixProcList,
                    )
                }
            };
        // 槽 0 未用：全零。
        assert_eq!(row(0).mpl_flags, 0);
        // 槽 1 = init：IN_USE、无僵尸位。
        let init = row(1);
        assert_eq!(init.mpl_flags, MPLF_IN_USE);
        assert_eq!(init.mpl_pid, 1);
        // 槽 2 = dead：IN_USE | ZOMBIE。
        let dead = row(2);
        assert_eq!(dead.mpl_flags, MPLF_IN_USE | MPLF_ZOMBIE);
        assert_eq!(dead.mpl_pid, 99);
        // 槽 3 = pid 0 空壳：被过滤，全零。
        assert_eq!(row(3).mpl_flags, 0);
        }

        // 大小估算（oldp=NULL）。
        {
            let mut tables = Tables::new();
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = walker::sysctl(
                &mut c,
                &mut Request { name: &[minix_types::CTL_MINIX, minix_types::MINIX_PROC, minix_types::PROC_LIST], oldp: None, newp: None },
            );
            assert_eq!(out, SysctlOutcome::Done(total as u64));
        }
    }

    /// PROC_DATA：用户行（init——SYSTEM 位来自 PRIV_PROC，RUNNABLE 来自
    /// 全零内核行）与任务行（pid=-4 → KERNEL 槽，名字取内核行）。
    #[test]
    fn test_proc_data_end_to_end() {
        let (mut k, mut s) = fixture();
        let mut tables = Tables::new();
        let size = core::mem::size_of::<MinixProcData>() as u64;

        // 用户行 pid 1。
        {
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = walker::sysctl(
                &mut c,
                &mut Request {
                    name: &[minix_types::CTL_MINIX, minix_types::MINIX_PROC, minix_types::PROC_DATA, 1],
                    oldp: Some(Oldp { endpt: Endpoint::PM, addr: 0, left: size }),
                    newp: None,
                },
            );
            assert_eq!(out, SysctlOutcome::Done(size));
            let sink = c.kernel.sink.borrow();
            // SAFETY(test): repr(C) POD 读。
            let mpd = unsafe {
                core::ptr::read_unaligned(sink.as_ptr() as *const MinixProcData)
            };
            assert_eq!(mpd.mpd_endpoint, 1);
            assert_eq!(
                mpd.mpd_flags,
                MPDF_SYSTEM | MPDF_RUNNABLE,
                "PRIV_PROC→SYSTEM；全零内核行→RUNNABLE"
            );
            assert_eq!(mpd.mpd_priority, 0);
            assert_eq!(mpd.mpd_nice, 5);
            assert_eq!(&mpd.mpd_name[..5], b"init\0");
        }

        // 任务行 pid = -4（KERNEL 槽）：名字取内核行，nice 恒 0。
        {
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = walker::sysctl(
                &mut c,
                &mut Request {
                    name: &[minix_types::CTL_MINIX, minix_types::MINIX_PROC, minix_types::PROC_DATA, -4],
                    oldp: Some(Oldp { endpt: Endpoint::PM, addr: 0, left: size }),
                    newp: None,
                },
            );
            assert_eq!(out, SysctlOutcome::Done(size));
            let sink = c.kernel.sink.borrow();
            // SAFETY(test): repr(C) POD 读。
            let mpd = unsafe {
                core::ptr::read_unaligned(sink.as_ptr() as *const MinixProcData)
            };
            assert_eq!(mpd.mpd_endpoint, -4);
            assert_eq!(&mpd.mpd_name[..7], b"kernel\0");
            assert_eq!(mpd.mpd_nice, 0, "任务行不读 mproc 的 nice");
            assert_eq!(mpd.mpd_flags & MPDF_ZOMBIE, 0);
        }

        // 门的门：namelen != 1 → EINVAL；pid 0 与越界负 pid → ESRCH。
        {
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = walker::sysctl(
                &mut c,
                &mut Request {
                    name: &[minix_types::CTL_MINIX, minix_types::MINIX_PROC, minix_types::PROC_DATA],
                    oldp: Some(Oldp { endpt: Endpoint::PM, addr: 0, left: size }),
                    newp: None,
                },
            );
            assert_eq!(out, SysctlOutcome::err(minix_types::EINVAL));
        }
        {
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = walker::sysctl(
                &mut c,
                &mut Request {
                    name: &[minix_types::CTL_MINIX, minix_types::MINIX_PROC, minix_types::PROC_DATA, 0],
                    oldp: Some(Oldp { endpt: Endpoint::PM, addr: 0, left: size }),
                    newp: None,
                },
            );
            assert_eq!(out, SysctlOutcome::err(minix_types::ESRCH));
        }
        {
            let mut c = e2e_ctx(&mut k, &mut s, &mut tables);
            let out = walker::sysctl(
                &mut c,
                &mut Request {
                    name: &[minix_types::CTL_MINIX, minix_types::MINIX_PROC, minix_types::PROC_DATA, -6],
                    oldp: Some(Oldp { endpt: Endpoint::PM, addr: 0, left: size }),
                    newp: None,
                },
            );
            assert_eq!(out, SysctlOutcome::err(minix_types::ESRCH));
        }
    }
}
