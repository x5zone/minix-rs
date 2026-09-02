//! Exec: `do_exec` / `do_newexec` / `exec_restart` / `do_execrestart`.
//!
//! C ground truth: `minix3/minix/servers/pm/exec.c` (200 lines)
//! Design: `.design/17-design.v1.md` D1–D8 (explicit `ExecRequest`/`ExecState`/`FrameRegion`/`TracerExec`).
//! Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{Endpoint, UserSlot, Pid, VirBytes, EPERM, EINVAL, ESRCH, OK};
use crate::mproc::{ProcTable, Lifecycle, FrameRegion, ExecState, RemainingFlags};
use crate::ipc::ReplyIntent;

/// `VFS_PM_EXEC` message encoding (6 fields, `ipc.h:469`).
#[derive(Debug, Clone, Copy)]
pub struct ExecRequest {
    pub caller: UserSlot,
    pub endpoint: Endpoint,
    pub path: VirBytes,
    pub path_len: usize,
    pub frame: VirBytes,
    pub frame_len: usize,
    pub ps_str: VirBytes,
}

/// `exec_info` from VFS (`minix/vm.h: exec_info`, `exec.c:67` `args`).
#[derive(Debug, Clone)]
pub struct ExecInfo {
    pub allow_setuid: bool,
    pub new_uid: u32,
    pub new_gid: u32,
    pub progname: [u8; 16],
    pub stack_high: VirBytes,
    pub frame_len: usize,
}

/// `m_rs_pm_exec_restart` info (`exec.c:144-146`).
#[derive(Debug, Clone, Copy)]
pub struct ExecRestartInfo {
    pub endpoint: Endpoint,
    pub result: i32,
    pub pc: VirBytes,
    pub ps_str: VirBytes,
}

/// Errors for exec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecError {
    Perm,
    BadEndpoint,
    Fault,
    Inval,
}

impl ExecError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Perm => EPERM,
            Self::BadEndpoint => ESRCH,
            Self::Fault => 14, // EFAULT
            Self::Inval => EINVAL,
        }
    }
}

/// VFS exec forwarder (`tell_vfs(VFS_PM_EXEC)` → `SUSPEND`, `exec.c:52`, D1, A-4).
pub trait VfsExec {
    fn forward_exec(&mut self, req: ExecRequest) -> Result<ReplyIntent, ExecError>;
}

/// Kernel exec (`sys_exec`, `exec.c:197`, D7, A-3).
pub trait KernelExec {
    fn exec(&mut self, ep: Endpoint, sp: VirBytes, pc: VirBytes, ps_str: VirBytes, name: &[u8]) -> i32;
    fn kill(&mut self, ep: Endpoint, sig: i32);
    fn reply(&mut self, slot: UserSlot, code: i32);
}

/// Tracer signal for `exec` (`check_sig`, `exec.c:193`, D5).
pub trait TracerSig {
    fn send(&mut self, pid: Pid, sig: i32);
}

/// `do_exec` (`exec.c:38-56`, D1) — forward to VFS.
pub fn do_exec(
    table: &mut ProcTable,
    caller: UserSlot,
    req: ExecRequest,
    vfs: &mut dyn VfsExec,
) -> Result<ReplyIntent, ExecError> {
    // In C, `tell_vfs(mp, &m)` sets VFS_CALL and returns SUSPEND.
    // Here we delegate to VfsExec which encodes VFS_PM_EXEC and does tell_vfs internally.
    vfs.forward_exec(req)?;
    // Mark PARTIAL? No, do_exec does not set PARTIAL_EXEC; do_newexec does.
    let _ = table;
    let _ = caller;
    Ok(ReplyIntent::ReplyLater)
}

/// Apply `TAINTED` double and `allow_setuid` (helper for `do_newexec`, D2).
fn apply_tainted_and_creds(
    table: &mut ProcTable,
    target: UserSlot,
    allow_setuid: bool,
    info: &ExecInfo,
) {
    let proc = &mut table.procs[target.get()];
    // 83-84 default clear
    // allow_setuid already false; tainted cleared before call

    // 86-89 tracer check
    let tracer = proc.tracer();
    let mut allow = false;
    if tracer.is_none() {
        allow = true;
    }
    let effective_allow = allow && info.allow_setuid;

    if effective_allow {
        // 91-94 set eff uid/gid to new
        if let Some(creds) = proc.resources.privilege.credentials_mut() {
            creds.user.effective = info.new_uid;
            creds.group.effective = info.new_gid;
        }
    }
    // 97-98 svuid = eff
    if let Some(creds) = proc.resources.privilege.credentials_mut() {
        creds.user.saved = creds.user.effective;
        creds.group.saved = creds.group.effective;
    }

    // 100-109 TAINTED double
    let mut tainted = false;
    if effective_allow && info.allow_setuid {
        tainted = true;
    } else if let Some(creds) = proc.resources.privilege.credentials() {
        if creds.user.effective != creds.user.real || creds.group.effective != creds.group.real {
            tainted = true;
        }
    }
    proc.resources.tainted = tainted;
    if tainted {
        proc.resources.flags.insert(RemainingFlags::TAINTED);
    } else {
        proc.resources.flags.remove(RemainingFlags::TAINTED);
    }
}

/// `do_newexec` (`exec.c:62-125`, D2/D3/D6).
pub fn do_newexec(
    table: &mut ProcTable,
    who: Endpoint,
    endpoint: Endpoint,
    info: ExecInfo,
) -> Result<bool, ExecError> {
    if who != Endpoint::VFS && who != Endpoint::RS {
        return Err(ExecError::Perm);
    }
    let slot = table.pm_isokendpt(endpoint).map_err(|_| ExecError::BadEndpoint)?;
    // Simulate sys_datacopy success (info already provided)
    let target = slot;

    // 83-84
    table.procs[target.get()].resources.tainted = false;
    table.procs[target.get()].resources.flags.remove(RemainingFlags::TAINTED);

    // 86-109 apply creds and tainted
    apply_tainted_and_creds(table, target, false, &info);

    // For test we need to handle allow flag correctly – we already did via apply
    // But note apply_tainted_and_creds's allow logic uses tracer; we need to ensure it respects info.allow_setuid
    // The above already does.

    // 111-113 progname
    let mut name = [0u8; 16];
    let len = info.progname.len().min(15);
    name[..len].copy_from_slice(&info.progname[..len]);
    table.procs[target.get()].identity.name = name;

    // 116-117 frame
    let frame = FrameRegion::from_high_len(info.stack_high, info.frame_len);
    table.procs[target.get()].ipc.frame_addr = frame.base;
    table.procs[target.get()].ipc.frame_len = frame.len;
    table.procs[target.get()].resources.exec_state = ExecState::Partial { frame };

    // 120 PARTIAL_EXEC already via exec_state

    // reply.suid = allow_setuid && args.allow_setuid (122)
    let allow = table.procs[target.get()].tracer().is_none() && info.allow_setuid;
    Ok(allow)
}

/// `do_execrestart` (`exec.c:130-151`, D7).
pub fn do_execrestart(
    table: &mut ProcTable,
    who: Endpoint,
    info: ExecRestartInfo,
    kern: &mut dyn KernelExec,
    tracer: &mut dyn TracerSig,
) -> Result<(), ExecError> {
    if who != Endpoint::RS {
        return Err(ExecError::Perm);
    }
    let slot = table.pm_isokendpt(info.endpoint).map_err(|_| ExecError::BadEndpoint)?;
    exec_restart(table, slot, info.result, info.pc, table.procs[slot.get()].ipc.frame_addr, info.ps_str, kern, tracer);
    Ok(())
}

/// `exec_restart` (`exec.c:156-199`, D3/D4/D5/D7).
pub fn exec_restart(
    table: &mut ProcTable,
    target: UserSlot,
    result: i32,
    pc: VirBytes,
    sp: VirBytes,
    ps_str: VirBytes,
    kern: &mut dyn KernelExec,
    tracer: &mut dyn TracerSig,
) {
    if result != OK {
        // 161-167 PARTIAL_EXEC → SIGKILL else reply
        let is_partial = matches!(table.procs[target.get()].resources.exec_state, ExecState::Partial { .. });
        if is_partial {
            kern.kill(table.procs[target.get()].endpoint(), 9); // SIGKILL
            return;
        }
        kern.reply(target, result);
        return;
    }

    // 173 clear PARTIAL
    table.procs[target.get()].resources.exec_state = ExecState::Idle;
    table.procs[target.get()].resources.flags.remove(RemainingFlags::PARTIAL_EXEC);

    // 178-184 reset caught
    table.procs[target.get()].resources.signals.reset_caught_for_exec();

    // 189-194 tracer signal before sys_exec
    let tracer_slot = table.procs[target.get()].tracer();
    let flags = table.procs[target.get()].state.guardianship.trace_options().bits();
    if tracer_slot.is_some() {
        const TO_NOEXEC: u32 = 0x1;
        const TO_ALTEXEC: u32 = 0x2;
        if (flags & TO_NOEXEC) == 0 {
            let sig = if (flags & TO_ALTEXEC) != 0 { 17 } else { 5 }; // SIGSTOP vs SIGTRAP
            let pid = table.procs[target.get()].identity.id.pid;
            tracer.send(pid, sig);
        }
    }

    // 197 sys_exec
    let ep = table.procs[target.get()].endpoint();
    let name = table.procs[target.get()].identity.name;
    let r = kern.exec(ep, sp, pc, ps_str, &name);
    if r != OK {
        panic!("sys_exec failed: {}", r);
    }
}

// Helper trait for credentials_mut (for Privilege)
trait PrivExt {
    fn credentials_mut(&mut self) -> Option<&mut crate::mproc::Credentials>;
}
impl PrivExt for crate::mproc::Privilege {
    fn credentials_mut(&mut self) -> Option<&mut crate::mproc::Credentials> {
        match self {
            crate::mproc::Privilege::User(c) => Some(c),
            crate::mproc::Privilege::Kernel => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials, RemainingFlags};
    use minix_types::{Endpoint, UserSlot, VirBytes};

    fn mk_proc(table: &mut ProcTable, slot: usize) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        table.procs[slot].resources.flags = RemainingFlags::empty();
        table.procs[slot].resources.tainted = false;
        table.procs[slot].resources.exec_state = ExecState::Idle;
        table.procs[slot].state.guardianship = crate::mproc::Guardianship::Normal { parent: UserSlot::new(0) };
    }

    struct NopVfs;
    impl VfsExec for NopVfs {
        fn forward_exec(&mut self, _req: ExecRequest) -> Result<ReplyIntent, ExecError> { Ok(ReplyIntent::ReplyLater) }
    }
    struct TestKern { pub killed: Option<Endpoint>, pub replied: Option<(UserSlot,i32)>, pub execed: Option<Endpoint> }
    impl KernelExec for TestKern {
        fn exec(&mut self, ep: Endpoint, _sp: VirBytes, _pc: VirBytes, _ps: VirBytes, _name: &[u8]) -> i32 { self.execed = Some(ep); 0 }
        fn kill(&mut self, ep: Endpoint, _sig: i32) { self.killed = Some(ep); }
        fn reply(&mut self, slot: UserSlot, code: i32) { self.replied = Some((slot, code)); }
    }
    struct NopTracer;
    impl TracerSig for NopTracer {
        fn send(&mut self, _pid: Pid, _sig: i32) {}
    }
    struct RecTracer { pub sig: Option<i32> }
    impl TracerSig for RecTracer {
        fn send(&mut self, _pid: Pid, sig: i32) { self.sig = Some(sig); }
    }

    #[test]
    fn test_do_exec_forwards() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0);
        let req = ExecRequest { caller: UserSlot::new(0), endpoint: Endpoint::from_generation_slot(1,0), path: VirBytes(0x1000), path_len: 5, frame: VirBytes(0x2000), frame_len: 128, ps_str: VirBytes(0) };
        let mut vfs = NopVfs;
        let r = do_exec(&mut table, UserSlot::new(0), req, &mut vfs).unwrap();
        assert_eq!(r, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_do_newexec_perm_gate() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5);
        let info = ExecInfo { allow_setuid: true, new_uid: 0, new_gid: 0, progname: *b"init\0\0\0\0\0\0\0\0\0\0\0\0", stack_high: VirBytes(0x8000), frame_len: 128 };
        let res = do_newexec(&mut table, Endpoint::PM, Endpoint::from_generation_slot(1,5), info);
        assert_eq!(res.unwrap_err(), ExecError::Perm);
    }

    #[test]
    fn test_do_newexec_tainted_double() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5);
        table.procs[5].resources.privilege.credentials_mut().unwrap().user.effective = 1000;
        table.procs[5].resources.privilege.credentials_mut().unwrap().user.real = 1000;
        let info = ExecInfo { allow_setuid: true, new_uid: 0, new_gid: 0, progname: *b"prog\0\0\0\0\0\0\0\0\0\0\0\0", stack_high: VirBytes(0x8000), frame_len: 64 };
        // tracer none, allow_setuid true -> should set eff to 0 and tainted true
        let allow = do_newexec(&mut table, Endpoint::VFS, Endpoint::from_generation_slot(1,5), info).unwrap();
        assert!(allow);
        assert!(table.procs[5].resources.tainted);
        assert_eq!(table.procs[5].resources.privilege.credentials().unwrap().user.effective, 0);
        // frame saved
        assert!(matches!(table.procs[5].resources.exec_state, ExecState::Partial { .. }));
    }

    #[test]
    fn test_partial_exec_sentinel() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5);
        table.procs[5].resources.exec_state = ExecState::Partial { frame: FrameRegion { base: VirBytes(0x7000), len: 128 } };
        let mut kern = TestKern { killed: None, replied: None, execed: None };
        let mut tracer = NopTracer;
        exec_restart(&mut table, UserSlot::new(5), -5, VirBytes(0x1000), VirBytes(0x7000), VirBytes(0), &mut kern, &mut tracer);
        assert!(kern.killed.is_some());
        assert!(kern.replied.is_none());
    }

    #[test]
    fn test_exec_restart_resets_caught() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5);
        table.procs[5].resources.signals.caught = 1u64 << 2;
        table.procs[5].resources.signals.actions[2].sa_handler = 0x1000;
        let mut kern = TestKern { killed: None, replied: None, execed: None };
        let mut tracer = NopTracer;
        exec_restart(&mut table, UserSlot::new(5), 0, VirBytes(0x1000), VirBytes(0x7000), VirBytes(0), &mut kern, &mut tracer);
        assert_eq!(table.procs[5].resources.signals.caught & (1u64 << 2), 0);
        assert_eq!(table.procs[5].resources.signals.actions[2].sa_handler, 0);
        assert!(kern.execed.is_some());
    }

    #[test]
    fn test_tracer_signal() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5);
        table.procs[5].state.guardianship = crate::mproc::Guardianship::Traced { parent: UserSlot::new(0), tracer: UserSlot::new(1), trace_exit: false, trace_options: crate::mproc::TraceOptions::empty() };
        // TO_NOEXEC not set, so should send SIGTRAP (5)
        let mut kern = TestKern { killed: None, replied: None, execed: None };
        let mut tracer = RecTracer { sig: None };
        exec_restart(&mut table, UserSlot::new(5), 0, VirBytes(0x1000), VirBytes(0x7000), VirBytes(0), &mut kern, &mut tracer);
        assert_eq!(tracer.sig, Some(5));
    }

    #[test]
    fn test_frame_region() {
        let high = VirBytes(0x8000);
        let len = 128;
        let fr = FrameRegion::from_high_len(high, len);
        assert_eq!(fr.base, VirBytes(0x8000 - 128));
        assert_eq!(fr.len, 128);
    }

    #[test]
    fn test_allow_setuid_double() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5);
        // With tracer, allow_setuid false -> tainted should be via eff!=real
        table.procs[5].state.guardianship = crate::mproc::Guardianship::Traced { parent: UserSlot::new(0), tracer: UserSlot::new(1), trace_exit: false, trace_options: crate::mproc::TraceOptions::empty() };
        table.procs[5].resources.privilege.credentials_mut().unwrap().user.effective = 2000; // eff != real (1000)
        let info = ExecInfo { allow_setuid: true, new_uid: 0, new_gid: 0, progname: [0;16], stack_high: VirBytes(0x8000), frame_len: 64 };
        let _ = do_newexec(&mut table, Endpoint::VFS, Endpoint::from_generation_slot(1,5), info).unwrap();
        // tracer present => allow_setuid false, so not via first branch, but eff!=real => tainted true
        assert!(table.procs[5].resources.tainted);
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(crate::mproc::RemainingFlags::PARTIAL_EXEC.bits(), 0x4000);
        assert_eq!(crate::mproc::RemainingFlags::TAINTED.bits(), 0x40000);
    }
}
