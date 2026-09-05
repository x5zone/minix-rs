//! Shared test doubles (E1 — todo §13).
//!
//! `MockKernelApi` is the single recording `KernelApi` implementation. The
//! boot shell tests use it today; the 19 wiring shell (main-loop dispatch /
//! permission checks) will reuse it. Pure decision modules (access, ipc_mask,
//! sched — T5) test with plain data and need no mock, so this is the *only*
//! `KernelApi` test double in the crate.

use crate::boot::{KernelApi, Machine, VmRsMemReq};
use crate::privilege::{CallMask, PrivCtlOp, Privilege};
use crate::sched::SchedulerConfig;
use crate::service_slot::Label;
use alloc::vec::Vec;
use minix_types::{Clock, Endpoint, Errno, Pid};

/// Records the kernel calls made through the mock, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    GetMachine,
    GetHz,
    GetTicks,
    PrivCtl(Endpoint, PrivCtlOp),
    GetPriv(Endpoint),
    SchedInitProc(Endpoint),
    GetNuid(Endpoint),
    GetNpid(Endpoint),
    SetAlarm(u32),
    SrvExecve(Endpoint),
    SrvKill(Pid, i32),
    SchedStop(Endpoint, Endpoint),
    SetUid(u32),
    Reply(Endpoint, i32),
}

/// Recording `KernelApi` mock with configurable canned results.
///
/// Fields are `pub` (test-only module): tests set `hz`/`ticks`/`pids` before
/// driving the shell and assert on `calls` afterwards.
#[derive(Default)]
pub struct MockKernelApi {
    /// Kernel calls observed, in order.
    pub calls: Vec<Call>,
    /// `get_hz` result.
    pub hz: u32,
    /// `get_ticks` result.
    pub ticks: Clock,
    /// Stack of `getnpid` results (LIFO); defaults to 100 when empty.
    pub pids: Vec<i32>,
    /// `srv_fork` result: `None` → `ENOSYS` (fail-closed default).
    pub fork_pid: Option<Pid>,
    /// `getprocnr` result: `None` → `ENOSYS`.
    pub child_endpoint: Option<Endpoint>,
    /// `vm_memctl`/`vm_set_priv` success switch (create_service paths).
    pub vm_ok: bool,
    /// `srv_execve` success switch.
    pub execve_ok: bool,
    /// `srv_kill`/`sched_stop` success switch (cleanup phase 2).
    pub kill_ok: bool,
    /// Privilege structures pushed by `privctl(SetSys)`, per endpoint.
    /// `getpriv` echoes them back — mirroring C, where `sys_getpriv` reads
    /// the kernel copy of what RS just set for that process
    /// (manager.c:604-605). Per-endpoint storage because boot Step 1 pushes
    /// 12 different structures in sequence.
    pub set_privs: Vec<(Endpoint, Privilege)>,
}

impl MockKernelApi {
    /// Builds a mock whose `get_hz` returns `hz`.
    pub fn new(hz: u32) -> Self {
        Self {
            calls: Vec::new(),
            hz,
            ticks: 0,
            pids: Vec::new(),
            fork_pid: None,
            child_endpoint: None,
            vm_ok: false,
            execve_ok: false,
            kill_ok: false,
            set_privs: Vec::new(),
        }
    }
}

impl KernelApi for MockKernelApi {
    fn get_machine(&mut self) -> Result<Machine, Errno> {
        self.calls.push(Call::GetMachine);
        Ok(Machine::default())
    }
    fn get_hz(&mut self) -> Result<u32, Errno> {
        self.calls.push(Call::GetHz);
        Ok(self.hz)
    }
    fn get_ticks(&mut self) -> Result<Clock, Errno> {
        self.calls.push(Call::GetTicks);
        Ok(self.ticks)
    }
    fn privctl(
        &mut self,
        proc: Endpoint,
        op: PrivCtlOp,
        priv_: Option<&Privilege>,
    ) -> Result<(), Errno> {
        self.calls.push(Call::PrivCtl(proc, op));
        if op == PrivCtlOp::SetSys {
            if let Some(p) = priv_ {
                // Per-endpoint echo storage: the last push per endpoint wins
                // (matches the kernel's one-priv-structure-per-process).
                self.set_privs.retain(|(e, _)| *e != proc);
                self.set_privs.push((proc, p.clone()));
            }
        }
        Ok(())
    }
    fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, Errno> {
        self.calls.push(Call::GetPriv(proc));
        Ok(self
            .set_privs
            .iter()
            .rev()
            .find(|(e, _)| *e == proc)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(Privilege::vacant))
    }
    fn sched_init_proc(&mut self, cfg: &SchedulerConfig) -> Result<Endpoint, Errno> {
        self.calls.push(Call::SchedInitProc(cfg.endpoint));
        Ok(cfg.scheduler)
    }
    fn getnuid(&mut self, proc: Endpoint) -> Result<u32, Errno> {
        self.calls.push(Call::GetNuid(proc));
        Ok(0) // root for boot tests
    }
    fn getnpid(&mut self, proc: Endpoint) -> Result<i32, Errno> {
        self.calls.push(Call::GetNpid(proc));
        Ok(self.pids.pop().unwrap_or(100))
    }
    fn setalarm(&mut self, delay_ticks: u32) -> Result<(), Errno> {
        self.calls.push(Call::SetAlarm(delay_ticks));
        Ok(())
    }
    fn srv_fork(&mut self, _uid: u32, _gid: u32) -> Result<Pid, Errno> {
        match self.fork_pid {
            Some(pid) => Ok(pid),
            // Fail-closed default: the 19 wiring shell may stub canned
            // results here once it lands.
            None => Err(Errno::ENOSYS),
        }
    }
    fn getprocnr(&mut self, _pid: Pid) -> Result<Endpoint, Errno> {
        match self.child_endpoint {
            Some(ep) => Ok(ep),
            None => Err(Errno::ENOSYS),
        }
    }
    fn vm_memctl(
        &mut self,
        _proc: Endpoint,
        _req: VmRsMemReq,
        _a: usize,
        _b: usize,
    ) -> Result<(), Errno> {
        if self.vm_ok {
            Ok(())
        } else {
            Err(Errno::ENOSYS)
        }
    }
    fn vm_set_priv(
        &mut self,
        _proc: Endpoint,
        _vm_call_mask: CallMask,
        _allow: bool,
    ) -> Result<(), Errno> {
        // Configurable (create_service paths): `vm_ok` gates the result.
        if self.vm_ok {
            Ok(())
        } else {
            Err(Errno::ENOSYS)
        }
    }
    fn srv_execve(
        &mut self,
        proc: Endpoint,
        _exec: &[u8],
        _progname: &Label,
        _args: &[u8],
        _argc: usize,
    ) -> Result<(), Errno> {
        self.calls.push(Call::SrvExecve(proc));
        if self.execve_ok {
            Ok(())
        } else {
            Err(Errno::ENOSYS)
        }
    }
    fn srv_kill(&mut self, pid: Pid, signo: i32) -> Result<(), Errno> {
        self.calls.push(Call::SrvKill(pid, signo));
        if self.kill_ok {
            Ok(())
        } else {
            Err(Errno::ENOSYS)
        }
    }
    fn sched_stop(&mut self, scheduler: Endpoint, proc: Endpoint) -> Result<(), Errno> {
        self.calls.push(Call::SchedStop(scheduler, proc));
        if self.kill_ok {
            Ok(())
        } else {
            Err(Errno::ENOSYS)
        }
    }
    fn setuid(&mut self, uid: u32) -> Result<(), Errno> {
        self.calls.push(Call::SetUid(uid));
        Ok(())
    }
    fn reply(&mut self, target: Endpoint, result: i32) -> Result<(), Errno> {
        self.calls.push(Call::Reply(target, result));
        Ok(())
    }
}
