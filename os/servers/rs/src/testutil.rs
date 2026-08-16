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
}

impl MockKernelApi {
    /// Builds a mock whose `get_hz` returns `hz`.
    pub fn new(hz: u32) -> Self {
        Self {
            calls: Vec::new(),
            hz,
            ticks: 0,
            pids: Vec::new(),
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
        _priv_: Option<&Privilege>,
    ) -> Result<(), Errno> {
        self.calls.push(Call::PrivCtl(proc, op));
        Ok(())
    }
    fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, Errno> {
        self.calls.push(Call::GetPriv(proc));
        Ok(Privilege::vacant())
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
        // Fail-closed (matches the production `UnimplementedKernelApi`): the
        // 19 wiring shell may stub canned results here once it lands.
        Err(Errno::ENOSYS)
    }
    fn getprocnr(&mut self, _pid: Pid) -> Result<Endpoint, Errno> {
        // Fail-closed (matches the production `UnimplementedKernelApi`): the
        // 19 wiring shell may stub canned results here once it lands.
        Err(Errno::ENOSYS)
    }
    fn vm_memctl(
        &mut self,
        _proc: Endpoint,
        _req: VmRsMemReq,
        _a: usize,
        _b: usize,
    ) -> Result<(), Errno> {
        // Fail-closed (matches the production `UnimplementedKernelApi`): the
        // 19 wiring shell may stub canned results here once it lands.
        Err(Errno::ENOSYS)
    }
    fn vm_set_priv(
        &mut self,
        _proc: Endpoint,
        _vm_call_mask: CallMask,
        _allow: bool,
    ) -> Result<(), Errno> {
        // Fail-closed (matches the production `UnimplementedKernelApi`): the
        // 19 wiring shell may stub canned results here once it lands.
        Err(Errno::ENOSYS)
    }
}
