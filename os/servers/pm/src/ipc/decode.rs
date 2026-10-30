//! Inbound message decoding, single site — the only channel from the wire union to handler parameters.
//!
//! `MessageUnion` is a `#[repr(C)]` mirror of C's `message` union
//! (`minix3/minix/include/minix/ipc.h`), and selecting a member by `m_type` is
//! inherently unsafe. This module concentrates that unsafety into one decoding
//! function per call: the unsafe surface is limited to reading a single Copy
//! struct by value, with a SAFETY argument and an ipc.h anchor at each site;
//! `calls.rs`'s dispatch arms consume only the safe functions here and no longer
//! perform bare union reads.
//!
//! The C counterpart is the `m->m_lc_pm_xxx` field selection at the top of each
//! handler (as in `forkexit.c`, `signal.c`, `trace.c`); decoding is the Rust
//! counterpart of C's "take this call's parameters out of the message union"
//! step.

use minix_types::{
    MessLcPmExit, MessLcPmKill, MessLcPmPtrace, MessLcPmReboot, MessLcPmWait4,
    MessLsysPmProceventmask, MessLsysPmSrvFork, MessRsPmSrvKill, Message,
};
// The three credential-family wires (MessLcPmSetid/Groups/Getsid) have no
// dedicated MessageUnion arm, so decoding reads the first 24 bytes byte-wise
// (the raw-read precedent from the vm.rs wrapper).

// Layout assertions: the wire structs involved must stay inside the IPC payload
// (the C counterpart is `_ASSERT_MSG_SIZE` after each struct in ipc.h).
const _: () = assert!(size_of::<MessLsysPmSrvFork>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLcPmExit>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLcPmWait4>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLcPmKill>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessRsPmSrvKill>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLsysPmProceventmask>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLcPmPtrace>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLcPmReboot>() <= minix_types::MESSAGE_PAYLOAD_SIZE);


/// srv_fork parameters (uid, gid). RS → PM.
///
/// C: `mess_lsys_pm_srv_fork` — ipc.h:1422-1427 (`do_srv_fork` consumes uid/gid,
/// forkexit.c:206-211).
pub(crate) fn srv_fork(msg: &Message) -> (u32, u32) {
    // SAFETY: the calling contract is guaranteed by dispatch — m_type must have
    // matched SRV_FORK to reach this function, and the sender writes the same
    // member. The member is a fully-Copy `#[repr(C)]` struct, so reading it by
    // value creates no reference and carries no aliasing risk.
    let pl = unsafe { msg.m_u.m_lsys_pm_srv_fork };
    (pl.uid, pl.gid)
}

/// exit status. user → PM.
///
/// C: `mess_lc_pm_exit` — ipc.h:446-450 (`do_exit` consumes status,
/// forkexit.c:259).
pub(crate) fn exit(msg: &Message) -> i32 {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; Copy read by value.
    let pl = unsafe { msg.m_u.m_lc_pm_exit };
    pl.status
}

/// wait4 parameters (pid, options, addr). user → PM.
///
/// C: `mess_lc_pm_wait4` — ipc.h:585-591 (`do_wait` consumes pid and friends,
/// forkexit.c:490); `addr` is the VirBytes view of the user-space `struct rusage*`.
pub(crate) fn wait4(msg: &Message) -> (i32, i32, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; Copy read by value.
    let pl = unsafe { msg.m_u.m_lc_pm_wait4 };
    (pl.pid, pl.options, pl.addr)
}

/// kill parameters (pid, signo). user → PM.
///
/// C has no separate kill member — `do_kill` reads `m_in.m_lc_pm_sig` directly
/// (signal.c:201, sharing `mess_lc_pm_sig` with sigaction, ipc.h:531-540). The
/// Rust wire gives kill its own `MessLcPmKill` (an internal modeling choice):
/// the first 8 bytes (pid, signo) sit at the same offsets and types as
/// `mess_lc_pm_sig`'s (pid, nr), so the read is equivalent.
pub(crate) fn kill(msg: &Message) -> (i32, i32) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; Copy read by value.
    let pl = unsafe { msg.m_u.m_lc_pm_kill };
    (pl.pid, pl.signo)
}

/// srv_kill parameters (pid, signo). RS → PM.
///
/// C: `mess_rs_pm_srv_kill` — ipc.h:1879-1884 (`do_srv_kill` consumes pid/nr,
/// signal.c:219).
pub(crate) fn srv_kill(msg: &Message) -> (i32, i32) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; Copy read by value.
    let pl = unsafe { msg.m_u.m_rs_pm_srv_kill };
    (pl.pid, pl.signo)
}

/// proceventmask mask. subscriber → PM.
///
/// C: `mess_lsys_pm_proceventmask` — ipc.h:1415-1419 (`do_proceventmask`
/// consumes mask, event.c:179).
pub(crate) fn proceventmask(msg: &Message) -> u32 {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; Copy read by value.
    let pl = unsafe { msg.m_u.m_lsys_pm_proceventmask };
    pl.mask
}

/// ptrace parameters (pid, req, addr, data). tracer → PM.
///
/// C: `mess_lc_pm_ptrace` — ipc.h:493-500 (`do_trace` consumes them, trace.c).
pub(crate) fn ptrace(msg: &Message) -> (i32, i32, u64, i64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; Copy read by value.
    let pl = unsafe { msg.m_u.m_lc_pm_ptrace };
    (pl.pid, pl.req, pl.addr, pl.data)
}

/// reboot's how bit set. user → PM.
///
/// C: `mess_lc_pm_reboot` — ipc.h:503-507 (`do_reboot` consumes how, misc.c:205;
/// once read in it becomes `abort_flag`, glo.h:26).
pub(crate) fn reboot(msg: &Message) -> i32 {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; Copy read by value.
    let pl = unsafe { msg.m_u.m_lc_pm_reboot };
    pl.how
}

/// exec parameters (path pointer, path_len, frame pointer, framelen, ps_str).
/// user/VFS → PM.
///
/// C: `mess_lc_pm_exec` — ipc.h:435-443(name@0/namelen@8/frame@16/
/// framelen@24/ps_str@32, each field 8 bytes under LP64).
pub(crate) fn exec(msg: &Message) -> (u64, u64, u64, u64, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; the five fields are 8 bytes each.
    let raw = unsafe { msg.m_u.raw };
    let u64_at = |off: usize| u64::from_le_bytes(raw[off..off + 8].try_into().unwrap());
    (u64_at(0), u64_at(8), u64_at(16), u64_at(24), u64_at(32))
}

/// EXEC_NEW parameters (endpt, exec_info pointer). VFS/RS → PM.
///
/// C: `mess_lexec_pm_exec_new` — ipc.h:966-973(endpt@0/ptr@8); ptr points into
/// VFS memory at a `struct exec_info` (copy size in calls.rs' EXEC_INFO_COPY_SIZE).
pub(crate) fn exec_new(msg: &Message) -> (i32, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; endpt@0/ptr@8.
    let raw = unsafe { msg.m_u.raw };
    let endpt = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let ptr = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    (endpt, ptr)
}

/// EXEC_RESTART parameters (endpt, result, pc, ps_str). RS → PM.
///
/// C: `mess_rs_pm_exec_restart` (minix-types `MessRsPmExecRestart` —
/// endpt@0/result@4/pc@8/ps_str@16, ipc.h:665-673).
pub(crate) fn exec_restart(msg: &Message) -> (i32, i32, u64, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; Copy read by value.
    let pl = unsafe { msg.m_u.m_rs_pm_exec_restart };
    (pl.endpt, pl.result, pl.pc, pl.ps_str)
}

/// sysuname parameters (req, field, len, value pointer). user → PM.
///
/// C: `mess_lc_pm_sysuname` — ipc.h:565-571(req@0/field@4/len@8/
/// value@16; raw byte read, no dedicated arm).
pub(crate) fn sysuname(msg: &Message) -> (i32, i32, u64, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type.
    let raw = unsafe { msg.m_u.raw };
    let req = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let field = i32::from_le_bytes(raw[4..8].try_into().unwrap());
    let len = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    let value = u64::from_le_bytes(raw[16..24].try_into().unwrap());
    (req, field, len, value)
}

/// svrctl parameters (request, arg). user → PM.
///
/// C: `mess_lc_svrctl` — ipc.h:603-608(request@0/arg@8).
pub(crate) fn svrctl(msg: &Message) -> (u64, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type.
    let raw = unsafe { msg.m_u.raw };
    let request = u64::from_le_bytes(raw[0..8].try_into().unwrap());
    let arg = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    (request, arg)
}

/// sprof parameters (action, freq, intr_type, ctl_ptr, mem_ptr, mem_size).
/// user → PM.
///
/// C: `mess_lc_pm_sprof` — ipc.h:550-558(six fields under LP64: action@0/freq@4/
/// intr_type@8/ctl_ptr@16/mem_ptr@24/mem_size@32).
pub(crate) fn sprof(msg: &Message) -> (i32, i32, i32, u64, u64, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type.
    let raw = unsafe { msg.m_u.raw };
    let i32_at = |o: usize| i32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
    let u64_at = |o: usize| u64::from_le_bytes(raw[o..o + 8].try_into().unwrap());
    (
        i32_at(0),
        i32_at(4),
        i32_at(8),
        u64_at(16),
        u64_at(24),
        u64_at(32),
    )
}

/// getsysinfo parameters (what, where, size). RS → PM.
///
/// C: `mess_lsys_getsysinfo` — ipc.h:1062-1070(what@0/where@8/size@16).
pub(crate) fn getsysinfo(msg: &Message) -> (i32, u64, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type.
    let raw = unsafe { msg.m_u.raw };
    let what = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let where_ = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    let size = u64::from_le_bytes(raw[16..24].try_into().unwrap());
    (what, where_, size)
}

/// mcontext parameters (endpt, ctx user-space pointer). user → PM.
///
/// C: `mess_lc_pm_mcontext` (minix-types `MessLcPmMcontext` —
/// endpt@0/ctx@8; MessageUnion has no dedicated arm, so raw byte read).
pub(crate) fn mcontext(msg: &Message) -> (i32, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; endpt@0/ctx@8.
    let raw = unsafe { msg.m_u.raw };
    let endpt = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let ctx = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    (endpt, ctx)
}

/// getprocnr parameters (pid). RS → PM.
///
/// C: `mess_lc_pm_getprocnr` (pid@0, raw byte read, no dedicated arm).
pub(crate) fn getprocnr(msg: &Message) -> i32 {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; pid@0.
    let raw = unsafe { msg.m_u.raw };
    i32::from_le_bytes(raw[0..4].try_into().unwrap())
}

/// getepinfo parameters (endpt, caller-group buffer pointer, caller-group capacity). RS → PM.
///
/// C: `mess_lsys_pm_getepinfo` — ipc.h:503-510(endpt@0/groups@8/
/// ngroups@16, LP64).
pub(crate) fn getepinfo(msg: &Message) -> (i32, u64, i32) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; endpt@0/
    // groups@8/ngroups@16.
    let raw = unsafe { msg.m_u.raw };
    let endpt = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let groups = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    let ngroups = i32::from_le_bytes(raw[16..20].try_into().unwrap());
    (endpt, groups, ngroups)
}

/// Time-family parameters (clk_id, now, sec, nsec). user → PM.
///
/// C: `mess_lc_pm_time` (minix-types `MessLcPmTime` — sec@0/clk_id@8/
/// now@12/nsec@16; stime uses only sec, the clock family uses clk_id/now/sec/nsec).
pub(crate) fn time(msg: &Message) -> (i32, bool, u64, i64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; sec@0(u64)/
    // clk_id@8(i32)/now@12(i32)/nsec@16(i64), raw byte read (MessageUnion has no
    // dedicated arm).
    let raw = unsafe { msg.m_u.raw };
    let clk_id = i32::from_le_bytes(raw[8..12].try_into().unwrap());
    let now = i32::from_le_bytes(raw[12..16].try_into().unwrap());
    let sec = u64::from_le_bytes(raw[0..8].try_into().unwrap());
    let nsec = i64::from_le_bytes(raw[16..24].try_into().unwrap());
    (clk_id, now != 0, sec, nsec)
}

/// setuid/seteuid/setgid/setegid family parameters (id). user → PM.
///
/// C: `mess_lc_pm_setid` (minix-types `MessLcPmSetid`, ipc.h:528-533:
/// id u32@0). The four calls share this one wire format.
pub(crate) fn setid(msg: &Message) -> u32 {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; raw@0 = id(u32).
    let raw = unsafe { msg.m_u.raw };
    u32::from_le_bytes(raw[0..4].try_into().unwrap())
}

/// groups parameters (num, gid array pointer). user → PM.
///
/// C: `mess_lc_pm_groups` — ipc.h:459-465 (num@0/ptr@8, LP64 pad@4).
pub(crate) fn groups(msg: &Message) -> (i32, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; num@0(i32),
    // ptr@8(u64, LP64-aligned).
    let raw = unsafe { msg.m_u.raw };
    let num = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let ptr = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    (num, ptr)
}

/// getsid parameters (pid; 0 = self). user → PM.
///
/// C: `mess_lc_pm_getsid` (minix-types `MessLcPmGetsid`, pid@0).
pub(crate) fn getsid(msg: &Message) -> i32 {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; raw@0 = pid(i32).
    let raw = unsafe { msg.m_u.raw };
    i32::from_le_bytes(raw[0..4].try_into().unwrap())
}

/// get/setpriority parameters (which, who, prio). user → PM.
///
/// C: the three integers `m1i1/m1i2/m1i3` at `misc.c:244-247`.
pub fn getsetpriority(msg: &Message) -> (i32, i32, i32) {
    // SAFETY: three integers in the m1 payload (misc.c:244's job_m_in.m1_i1/i2/i3).
    let raw = unsafe { &msg.m_u.raw };
    let i32_at = |at: usize| i32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    (i32_at(0), i32_at(4), i32_at(8))
}

/// itimer parameters (which, value pointer, ovalue pointer). user → PM.
///
/// C: `mess_lc_pm_itimer` (minix-types `MessLcPmItimer`, repr(C) field order
/// which@0/value@8/ovalue@16; value/ovalue are user-space pointers to
/// `struct itimerval`, whose bytes move through the gateway copy seam).
/// MessageUnion has no dedicated arm — the first 24 bytes are read byte-wise (the
/// raw-read precedent from the vm.rs wrapper).
/// sigaction parameters (nr, act pointer, oact pointer, sigreturn stub). user → PM.
///
/// C: `mess_lc_pm_sig` (minix-types `MessLcPmSig` — field order
/// pid/nr/act/oact/ret; PM consumes only the three fields starting at nr plus
/// ret). act/oact are user-space `struct sigaction` pointers, and their bytes
/// move through the gateway copy seam.
pub(crate) fn sigaction(msg: &Message) -> (i32, u64, u64, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; Copy read by value.
    let pl = unsafe { msg.m_u.m_lc_pm_sig };
    (pl.nr, pl.act, pl.oact, pl.ret)
}

/// Signal-set family parameters (how, ctx stub, set mask). user → PM.
///
/// C: `mess_lc_pm_sigset` — how@0/ctx@8/set@16 (`sigset_t` = 4×u32,
/// sigtypes.h:57-62). Rust `SigSet` is u64: it joins the low two u32s into a u64
/// (bits for signal numbers >64 are the no-op half the kernel has already ruled
/// on in C; see proc_table.rs).
pub(crate) fn sigset(msg: &Message) -> (i32, u64, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; Copy read by value.
    let pl = unsafe { msg.m_u.m_lc_pm_sigset };
    let set = (pl.set[0] as u64) | ((pl.set[1] as u64) << 32);
    (pl.how, pl.ctx, set)
}

/// getrusage parameters (who, addr). user → PM.
///
/// C: `mess_lc_pm_rusage` — ipc.h:510-515 (who@endpoint_t@0/addr@8, raw byte
/// read, no dedicated union arm); `who` takes `RUSAGE_SELF(0)`/
/// `RUSAGE_CHILDREN(-1)` (sys/resource.h:54-55), and `addr` is the VirBytes view
/// of a user-space `struct rusage*` (the destination of do_getrusage's final
/// sys_datacopy, misc.c:446-447).
pub(crate) fn rusage(msg: &Message) -> (i32, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; who@0/addr@8,
    // read byte-wise with no type reinterpretation.
    let raw = unsafe { msg.m_u.raw };
    let who = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let addr = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    (who, addr)
}

pub(crate) fn itimer(msg: &Message) -> (i32, u64, u64) {
    // SAFETY: same as srv_fork — dispatch has already routed by m_type; the raw arm
    // reads byte-wise with no type reinterpretation, and the field order matches
    // `MessLcPmItimer`'s repr(C) layout.
    let raw = unsafe { msg.m_u.raw };
    let u64_at = |off: usize| u64::from_le_bytes(raw[off..off + 8].try_into().unwrap());
    let which = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    (which, u64_at(8), u64_at(16))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a message carrying only the given member payload (m_type is decided by the dispatch contract and is irrelevant to decoding).
    fn msg_with(f: impl FnOnce(&mut Message)) -> Message {
        let mut m = Message::default();
        f(&mut m);
        m
    }

    #[test]
    fn srv_fork_decodes_uid_gid() {
        let m = msg_with(|m| {
            m.m_u.m_lsys_pm_srv_fork.uid = 1000;
            m.m_u.m_lsys_pm_srv_fork.gid = 100;
        });
        assert_eq!(srv_fork(&m), (1000, 100));
    }

    #[test]
    fn exit_decodes_status() {
        let m = msg_with(|m| m.m_u.m_lc_pm_exit.status = 7);
        assert_eq!(exit(&m), 7);
    }

    #[test]
    fn rusage_decodes_who_and_addr_roundtrip() {
        // ipc.h:510-515 layout witness: who@endpoint_t@0 (-1 = RUSAGE_CHILDREN
        // goes through the negative path), addr@8. The writing side packs in the
        // same field order, and the read-back agrees.
        for who in [0i32, -1i32, 7i32] {
            let m = msg_with(|m| {
                // SAFETY: constructing under test — write raw bytes in the decode field order.
                let raw = unsafe { &mut m.m_u.raw };
                raw[0..4].copy_from_slice(&who.to_le_bytes());
                raw[8..16].copy_from_slice(&0x2000u64.to_le_bytes());
            });
            assert_eq!(rusage(&m), (who, 0x2000));
        }
    }

    #[test]
    fn wait4_decodes_pid_options_addr() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_wait4.pid = -1;
            m.m_u.m_lc_pm_wait4.options = 0x3;
            m.m_u.m_lc_pm_wait4.addr = 0x7000;
        });
        assert_eq!(wait4(&m), (-1, 0x3, 0x7000));
    }

    #[test]
    fn kill_decodes_pid_signo() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_kill.pid = 42;
            m.m_u.m_lc_pm_kill.signo = 9;
        });
        assert_eq!(kill(&m), (42, 9));
    }

    #[test]
    fn srv_kill_decodes_pid_signo() {
        let m = msg_with(|m| {
            m.m_u.m_rs_pm_srv_kill.pid = 5;
            m.m_u.m_rs_pm_srv_kill.signo = 15;
        });
        assert_eq!(srv_kill(&m), (5, 15));
    }

    #[test]
    fn proceventmask_decodes_mask() {
        let m = msg_with(|m| m.m_u.m_lsys_pm_proceventmask.mask = 0b101);
        assert_eq!(proceventmask(&m), 0b101);
    }

    #[test]
    fn itimer_decodes_which_and_pointers() {
        let m = msg_with(|m| unsafe {
            // Write raw bytes in field order: which@0/value@8/ovalue@16 (repr(C) byte
            // layout). Slicing the union field is a read borrow, so it must be
            // wrapped in an unsafe block.
            m.m_u.raw[0..4].copy_from_slice(&0i32.to_le_bytes());
            m.m_u.raw[8..16].copy_from_slice(&0x5000u64.to_le_bytes());
            m.m_u.raw[16..24].copy_from_slice(&0x6000u64.to_le_bytes());
        });
        assert_eq!(itimer(&m), (0, 0x5000, 0x6000));
    }

    #[test]
    fn sigaction_decodes_nr_and_pointers() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_sig.nr = 9;
            m.m_u.m_lc_pm_sig.act = 0x5000;
            m.m_u.m_lc_pm_sig.oact = 0x6000;
            m.m_u.m_lc_pm_sig.ret = 0x7000;
        });
        assert_eq!(sigaction(&m), (9, 0x5000, 0x6000, 0x7000));
    }

    #[test]
    fn sigset_decodes_how_ctx_and_mask_low64() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_sigset.how = 2;
            m.m_u.m_lc_pm_sigset.ctx = 0x4000;
            m.m_u.m_lc_pm_sigset.set = [0x11, 0x22, 0x33, 0x44];
        });
        // The high 64 bits (signal numbers >64, the no-op half on the C side) are dropped.
        assert_eq!(sigset(&m), (2, 0x4000, 0x22_0000_0011));
    }

    #[test]
    fn ptrace_decodes_full_tuple() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_ptrace.pid = 3;
            m.m_u.m_lc_pm_ptrace.req = 2;
            m.m_u.m_lc_pm_ptrace.addr = 0x4000;
            m.m_u.m_lc_pm_ptrace.data = 0x1234_5678;
        });
        assert_eq!(ptrace(&m), (3, 2, 0x4000, 0x1234_5678));
    }

    #[test]
    fn reboot_decodes_how_bitmask() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_reboot.how = 0x0808;
        });
        assert_eq!(reboot(&m), 0x0808);
    }
}
