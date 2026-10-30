//! PM process table -> `SI_PROC_TAB` snapshot rows (`[ARCH: A-4]` single
//! authority).
//!
//! C ground truth: `servers/pm/mproc.h` `mproc[NR_PROCS]`; the wire row is
//! [`minix_types::MProcSnap`] (200 bytes per slot, with the layout witness in its
//! `layout` tests). Consumers (IS `dump_pm`'s two dumps, MIB `proc/tables`'s
//! table-fetch half) read fields by name — C's per-row `MP_MAGIC` drift check is
//! replaced by the shared type (see that struct's documentation).
//!
//! Mapping discipline: C fields map one by one to typed PM counterparts; fields
//! with no counterpart are filled with 0.
//! Difference table against the C snapshot (the A-4 subset decision's landing
//! list):
//!
//! | C field | Treatment | Notes |
//! |---|---|---|
//! | mp_pid/name/procgrp/parent/tracer | direct map | `NO_TRACER = 0` (const.h:11) |
//! | mp_realuid/effuid/realgid/effgid | credentials | both variants serialize uniformly: boot system processes are all zero (C initial values), srv_fork children carry injected values |
//! | mp_nice | resources.nice | direct map |
//! | mp_flags | bit composition ([`flags_for`]) | an in-use slot is one with IN_USE set |
//! | mp_ignore/catch/sigmask/sigpending `__bits[0]` | signals | low word; dumps print only this word |
//! | mp_timer_exp | resources.timer | `tmr_exp_time` (0 when there is no timer) |
//! | mp_started | resources.started | uptime set at fork (MIB swtime consumes it) |
//! | mp_svuid/svgid | credentials' IdSet saved fields | KERN_PROC2's p_svuid/svgid |
//! | mp_child_utime/stime | resources.child_* | KERN_PROC2's p_uctime |
//! | mp_ngroups/sgroups | credentials' ngroups/supplemental_groups | KERN_PROC2's p_groups |
//! | mp_endpoint | identity.endpoint | address carrier KERN_PROC_ARGS uses to read the target process's pages |
//! | mp_frame_addr/len | ipc.frame_addr/frame_len | argument frame base and length (the upper bound KERN_PROC_ARGS estimates) |
//! | remaining C fields (mp_reply[64], mp_sigact, mp_sgroups, mp_wpid...) | not in the snapshot | the A-4 decision: nobody reads them, or they are meaningless across the wire |
//!
//! The authority for `mp_flags` bit values is `minix_types::mp_flags`; this crate
//! re-exports it so existing reference sites stay untouched, and the pin-value test
//! at the end of the file prevents the two copies from diverging.

use crate::mproc::{ProcTable, Process};
use minix_types::MProcSnap;

/// The flags bits from C mproc.h:86-104 — the authority is `minix_types::mp_flags`.
pub use minix_types::mp_flags;

#[cfg(test)]
mod flag_authority_pin {
    /// Pin values for the authoritative bits (C mproc.h:86-104) — prevents a
    /// divergence regression.
    #[test]
    fn pm_flags_match_minix_types() {
        use super::mp_flags as a;
        assert_eq!(a::IN_USE, 0x00001);
        assert_eq!(a::WAITING, 0x00002);
        assert_eq!(a::ZOMBIE, 0x00004);
        assert_eq!(a::PROC_STOPPED, 0x00008);
        assert_eq!(a::ALARM_ON, 0x00010);
        assert_eq!(a::EXITING, 0x00020);
        assert_eq!(a::TOLD_PARENT, 0x00040);
        assert_eq!(a::TRACE_STOPPED, 0x00080);
        assert_eq!(a::SIGSUSPENDED, 0x00100);
        assert_eq!(a::VFS_CALL, 0x00400);
        assert_eq!(a::NEW_PARENT, 0x00800);
        assert_eq!(a::UNPAUSED, 0x01000);
        assert_eq!(a::PRIV_PROC, 0x02000);
        assert_eq!(a::PARTIAL_EXEC, 0x04000);
        assert_eq!(a::TRACE_EXIT, 0x08000);
        assert_eq!(a::TRACE_ZOMBIE, 0x10000);
        assert_eq!(a::DELAY_CALL, 0x20000);
        assert_eq!(a::TAINTED, 0x40000);
        assert_eq!(a::EVENT_CALL, 0x80000);
    }
}

/// Bit composition for C `mp_flags` (invariant: an in-use slot always carries
/// `IN_USE`).
///
/// C: the setters for the bits at mproc.h:86-104 are scattered across the
/// wait/exit/signal files; this function composes the same set **in one place**,
/// shared by `serialize_snap` and the empty-slot rule.
pub fn flags_for(p: &Process) -> u32 {
    let mut flags: u32 = mp_flags::IN_USE;
    flags |= match &p.state.lifecycle {
        crate::mproc::Lifecycle::Zombie { .. } | crate::mproc::Lifecycle::TraceZombie { .. } => {
            mp_flags::ZOMBIE
        }
        crate::mproc::Lifecycle::ToldParent { .. } => mp_flags::TOLD_PARENT,
        crate::mproc::Lifecycle::Exiting { .. } => mp_flags::EXITING,
        _ => 0,
    };
    if p.state.wait.waiting {
        flags |= mp_flags::WAITING;
    }
    if p.state.block.stopped {
        flags |= mp_flags::PROC_STOPPED;
    }
    if p.state.block.ipc_blocked.is_some() {
        flags |= mp_flags::VFS_CALL;
    }
    if p.state.block.unpaused {
        flags |= mp_flags::UNPAUSED;
    }
    if p.state.trace.stopped {
        flags |= mp_flags::TRACE_STOPPED;
    }
    if p.state.trace.exit_pending {
        flags |= mp_flags::TRACE_ZOMBIE;
    }
    if p.resources.flags.contains(crate::mproc::RemainingFlags::ALARM_ON) {
        flags |= mp_flags::ALARM_ON;
    }
    if p.resources.flags.contains(crate::mproc::RemainingFlags::PARTIAL_EXEC) {
        flags |= mp_flags::PARTIAL_EXEC;
    }
    if p.resources.flags.contains(crate::mproc::RemainingFlags::TAINTED) || p.resources.tainted {
        flags |= mp_flags::TAINTED;
    }
    if p.resources.signals.suspended {
        flags |= mp_flags::SIGSUSPENDED;
    }
    // C's `is_kernel_process() ⇔ mp_flags & PRIV_PROC` (mproc.h). Credentials of
    // both `Privilege` variants serialize uniformly in `serialize_snap`, and the
    // privilege bit is composed here: boot services and srv_fork children are both
    // `Kernel`, and MIB's table-fetch half decides SYSTEM from it
    // (`mflags & PRIV_PROC` in `minix_proc_exec.rs`).
    if p.resources.privilege.is_kernel() {
        flags |= mp_flags::PRIV_PROC;
    }
    flags
}

/// Serialize one PM slot into an `MProcSnap` row.
///
/// `idx` is the slot index (what `mp_parent`/`mp_tracer` reference). An unused slot
/// returns an all-zero row (`IN_USE` not set) — consumers filter empty slots by
/// `mp_flags & IN_USE`, the same rule as C's empty-slot image.
pub fn serialize_snap(idx: usize, p: &Process) -> MProcSnap {
    //[SD19-gate] PM send/notify/reply results are fire-and-forget
    #[allow(clippy::let_underscore_must_use)]
    let _ = idx; // C's mp_parent/mp_tracer are already slot indices; no conversion needed
    let mut w = MProcSnap::default();
    if !p.state.lifecycle.is_in_use() {
        return w;
    }

    // ── identity (mproc.h:28-45/80) ──
    w.mp_pid = p.identity.id.pid;
    w.mp_procgrp = p.identity.procgrp;
    w.mp_name = p.identity.name;

    // ── guardianship (mproc.h:33-34; NO_TRACER = 0) ──
    w.mp_parent = p.state.guardianship.parent().0 as i32;
    w.mp_tracer = p.state.guardianship.tracer().map_or(0, |t| t.0 as i32);

    // ── credentials (mproc.h:41-45) ──
    // Credentials always exist: User slots carry user credentials and Kernel
    // privilege slots carry all-zero credentials (C system processes start with
    // uid/gid 0, and consumers treat uid=0 as root). Both kinds serialize
    // uniformly.
    {
        let cred = p.resources.privilege.credentials();
        w.mp_realuid = cred.user.real;
        w.mp_effuid = cred.user.effective;
        w.mp_realgid = cred.group.real;
        w.mp_effgid = cred.group.effective;
    }

    // ── signals (mproc.h:53-57; low word of the u64 SigSet) ──
    let sig = &p.resources.signals;
    w.mp_ignore0 = sig.ignored as u32;
    w.mp_catch0 = sig.caught as u32;
    w.mp_sigmask0 = sig.mask as u32;
    w.mp_sigpending0 = sig.pending as u32;

    // ── resources: nice / timer expiry / started (mproc.h:62/75) ──
    w.mp_nice = p.resources.nice;
    if let Some(timer) = &p.resources.timer {
        w.mp_timer_exp = timer.expire_time as u32;
    }
    // C: `mp_started = getuptime()` (forkexit.c:114, the same instant noted in
    // forkexit.rs) — MIB's swtime (`uptime - mp_started`) consumes it.
    w.mp_started = p.resources.started as u64;

    // ── credential tail + child clocks (consumed by KERN_PROC2's
    // p_svuid/svgid, p_groups, p_uctime) ──
    {
        let cred = p.resources.privilege.credentials();
        w.mp_svuid = cred.user.saved;
        w.mp_svgid = cred.group.saved;
        w.mp_ngroups = cred.ngroups as u32;
        w.mp_sgroups = cred.supplemental_groups;
    }
    w.mp_child_utime = p.resources.child_utime as u64;
    w.mp_child_stime = p.resources.child_stime as u64;
    // ── page-fetch carrier for KERN_PROC_ARGS ──
    // C: `mp->mp_endpoint` (the datacopy target process at proc.c:963) and
    // `mp_frame_addr + mp_frame_len` (ps_strings sits at the frame's tail,
    // proc.c:964).
    w.mp_endpoint = p.identity.endpoint.0;
    w.mp_frame_addr = p.ipc.frame_addr.0;
    w.mp_frame_len = p.ipc.frame_len as u64;

    w.mp_flags = flags_for(p);
    w
}

/// Serialize the whole table (`NR_PROCS` slots x 76 bytes). Returns the slot count
/// (== the output slot count).
pub fn serialize_mproc_tab(table: &ProcTable, out: &mut [MProcSnap]) {
    assert_eq!(out.len(), table.procs.len(), "mproc tab slot count mismatch");
    for (idx, (row, proc)) in out.iter_mut().zip(table.procs.iter()).enumerate() {
        *row = serialize_snap(idx, proc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::ProcTable;

    /// Empty slot: an all-zero row (`IN_USE` not set). Same rule as C's empty
    /// `mproc` slot.
    #[test]
    fn test_empty_slot_is_all_zero() {
        let table = ProcTable::new();
        let mut out = vec![MProcSnap::default(); table.procs.len()];
        serialize_mproc_tab(&table, &mut out);
        assert_eq!(out[0].mp_flags & mp_flags::IN_USE, 0);
        assert_eq!(out[0], MProcSnap::default());
    }

    /// An in-use slot always carries IN_USE; the flag composition covers the
    /// wait/exit/signal bit groups.
    #[test]
    fn test_flags_for_in_use_and_waiting() {
        use crate::mproc::Lifecycle;
        let mut p = crate::mproc::Process::new(0, 1);
        p.state.lifecycle = Lifecycle::Running;
        p.state.wait.waiting = true;
        let flags = flags_for(&p);
        assert_ne!(flags & mp_flags::IN_USE, 0);
        assert_ne!(flags & mp_flags::WAITING, 0);
    }

    /// `PRIV_PROC` is composed along with `Privilege::Kernel`: boot services and
    /// srv_fork children are both Kernel, and MIB decides SYSTEM from it; User
    /// processes must never carry this bit.
    #[test]
    fn test_flags_for_priv_proc() {
        use crate::mproc::{Credentials, Privilege};
        let mut kernel = crate::mproc::Process::new(0, 1);
        kernel.resources.privilege = Privilege::Kernel(Credentials::default());
        assert_ne!(
            flags_for(&kernel) & mp_flags::PRIV_PROC,
            0,
            "Kernel 进程 mp_flags 必带 PRIV_PROC（C is_kernel_process 判据）"
        );
        let user = crate::mproc::Process::new(1, 2); // default User
        assert_eq!(
            flags_for(&user) & mp_flags::PRIV_PROC,
            0,
            "User 进程不得带 PRIV_PROC"
        );
    }

    /// Row width follows the minix-types authority (88, with `mp_started` as the
    /// tail slot), and the whole-table width follows it (the old 464-byte C-ABI row
    /// is retired).
    #[test]
    fn test_row_width_follows_shared_snapshot() {
        assert_eq!(core::mem::size_of::<MProcSnap>(), 200);
        assert_eq!(core::mem::size_of::<MProcSnap>() * 256, 51_200);
        assert_eq!(core::mem::offset_of!(MProcSnap, mp_name), 12);
        assert_eq!(core::mem::offset_of!(MProcSnap, mp_started), 80);
    }
}
