//! Shared test doubles (E1 — todo §13).
//!
//! `MockKernelApi` is the single recording external-boundary double. It
//! implements the five domain faces ([`SysApi`]/[`SchedApi`]/[`PmApi`]/[`VmApi`]
//! /[`IpcApi`]) and is a `KernelApi` through the composite blanket impl. The
//! boot shell tests use it today; the 19 wiring shell (main-loop dispatch /
//! permission checks) will reuse it. Pure decision modules (access, ipc_mask,
//! sched — T5) test with plain data and need no mock, so this is the *only*
//! external-boundary test double in the crate.

use crate::boot::{IpcApi, Machine, PmApi, SchedApi, SysApi, VmApi, VmRsMemReq};
use crate::privilege::{CallMask, PrivCtlOp, Privilege};
use crate::sched::SchedulerConfig;
use crate::service_slot::Label;
use alloc::vec::Vec;
use minix_types::{GrantId, Clock, Endpoint, Errno, Pid};

/// Deterministic xorshift64* PRNG for property-style tests (E-9).
///
/// `proptest` is not available in this workspace (no registry access); the
/// E-9 properties only need reproducible pseudo-random inputs, so this
/// tiny generator keeps the crate dependency-free while preserving the
/// property-test essentials: generated (not hand-picked) inputs, explicit
/// invariants, and a fixed seed so a failure replays exactly.
pub struct XorShift(u64);

impl XorShift {
    /// `seed | 1` — the generator never leaves the zero state.
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform value in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// Fills `buf` with bytes drawn from `alphabet`.
    pub fn fill(&mut self, buf: &mut [u8], alphabet: &[u8]) {
        for b in buf.iter_mut() {
            *b = alphabet[self.below(alphabet.len())];
        }
    }
}

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
    SysKill(Endpoint, i32),
    SysUpdate(Endpoint, Endpoint),
    SchedStop(Endpoint, Endpoint),
    SetUid(u32),
    Reply(Endpoint, i32),
    Notify(Endpoint),
    Asynsend(Endpoint, i32),
    DiagctlStacktrace(Endpoint),
    DiagWrite(String),
}

impl Call {
    /// Variant-level equality — payloads ignored (fail-injection matching).
    ///
    /// `Call::SetAlarm(0)` in `MockKernelApi::fail_calls` makes every
    /// `setalarm` fail regardless of the requested delay.
    fn same_variant(&self, other: &Call) -> bool {
        matches!(
            (self, other),
            (Call::GetMachine, Call::GetMachine)
                | (Call::GetHz, Call::GetHz)
                | (Call::GetTicks, Call::GetTicks)
                | (Call::PrivCtl(..), Call::PrivCtl(..))
                | (Call::GetPriv(..), Call::GetPriv(..))
                | (Call::SchedInitProc(..), Call::SchedInitProc(..))
                | (Call::GetNuid(..), Call::GetNuid(..))
                | (Call::GetNpid(..), Call::GetNpid(..))
                | (Call::SetAlarm(_), Call::SetAlarm(_))
                | (Call::SrvExecve(..), Call::SrvExecve(..))
                | (Call::SrvKill(..), Call::SrvKill(..))
                | (Call::SysKill(..), Call::SysKill(..))
                | (Call::SysUpdate(..), Call::SysUpdate(..))
                | (Call::SchedStop(..), Call::SchedStop(..))
                | (Call::SetUid(_), Call::SetUid(_))
                | (Call::Reply(..), Call::Reply(..))
                | (Call::Notify(..), Call::Notify(..))
                | (Call::Asynsend(..), Call::Asynsend(..))
                | (Call::DiagctlStacktrace(..), Call::DiagctlStacktrace(..))
                | (Call::DiagWrite(..), Call::DiagWrite(..))
        )
    }
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
    /// Stack of `waitpid` results (LIFO); empty → `None` (no children).
    pub children: Vec<Pid>,
    /// Privilege structures pushed by `privctl(SetSys)`, per endpoint.
    /// `getpriv` echoes them back — mirroring C, where `sys_getpriv` reads
    /// the kernel copy of what RS just set for that process
    /// (manager.c:604-605). Per-endpoint storage because boot Step 1 pushes
    /// 12 different structures in sequence.
    pub set_privs: Vec<(Endpoint, Privilege)>,
    /// Kernel-side privilege structures that exist *before* RS sets
    /// anything — boot processes (RS/VM) already have one, since the kernel
    /// builds it from its own boot-image processing (C `sys_getpriv`
    /// succeeds for RS/VM at main.c:293-296 even though they skip
    /// `SYS_PRIV_SET_SYS`, main.c:285-291). `getpriv` falls back to this
    /// table when no `SetSys` entry exists; empty → `vacant` (previous
    /// behavior, kept for non-boot endpoints).
    pub kernel_privs: Vec<(Endpoint, Privilege)>,
    /// Canned `safecopy_from` payload: a flat caller-address-space image
    /// based at 0 (R4 — reads at the requested address, EFAULT out of
    /// range); `receive`-style tests stage the
    /// request bytes here; `None` → `Err(ENOSYS)` (fail-closed default).
    pub payload: Option<Vec<u8>>,
    /// Bytes handed to `safecopy_to`, in order (R7 copy-out assertions).
    pub sent_copies: Vec<(Endpoint, usize, Vec<u8>)>,
    /// grant_read 发放序号(下一个 gid)。
    pub grant_seq: u32,
    /// 最近一次 grant_read 的 (gid, addr, len)。
    pub last_grant: Option<(u32, u64, u64)>,
    /// 已 revoke 的 gid。
    pub revoked_grants: Vec<GrantId>,
    /// sendrec_to 记录(dest + 消息快照)。
    pub sent_rec: Vec<(Endpoint, minix_types::Message)>,
    /// lu_request_prepare 调用记录。
    pub prepare_requests: Vec<Endpoint>,
    /// Canned receive queue (E-10/06 wiring tests): `receive` pops the front
    /// entry; empty queue → `Err(ENOSYS)` (the loop ends, T2 semantics).
    pub inbox: Vec<(minix_types::Message, crate::dispatch::IpcStatus, Clock)>,
    /// Messages handed to `asynsend`, in order (I2 boot init tests decode
    /// these to assert the RS_INIT payload — utility.c:62).
    pub sent: Vec<(Endpoint, minix_types::Message)>,
    /// Reply payloads, in order (I3a payload replies — RS_LOOKUP's endpoint
    /// rides in `m_rs_req.endpoint`, request.c:1174).
    pub replies: Vec<(Endpoint, i32, minix_types::Message)>,
    /// Calls that must fail with `ENOSYS` (fail-injection, R34.18/E-10):
    /// matching is by variant, payloads ignored — `Call::SetAlarm(0)` fails
    /// every `setalarm`. Plain recording methods honor this; the methods
    /// with their own canned-result switches (`fork_pid`/`child_endpoint`/
    /// `vm_ok`/`execve_ok`/`kill_ok`) stay on those switches.
    pub fail_calls: Vec<Call>,
}

impl MockKernelApi {
    /// Whether this call's variant is injected to fail.
    fn failing(&self, c: &Call) -> bool {
        self.fail_calls.iter().any(|f| f.same_variant(c))
    }
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
            children: Vec::new(),
            set_privs: Vec::new(),
            kernel_privs: Vec::new(),
            fail_calls: Vec::new(),
            payload: None,
            sent_copies: Vec::new(),
            grant_seq: 0,
            last_grant: None,
            revoked_grants: Vec::new(),
            sent_rec: Vec::new(),
            prepare_requests: Vec::new(),
            inbox: Vec::new(),
            sent: Vec::new(),
            replies: Vec::new(),
        }
    }
}

impl SysApi for MockKernelApi {
    fn get_machine(&mut self) -> Result<Machine, Errno> {
        self.calls.push(Call::GetMachine);
        if self.failing(&Call::GetMachine) {
            return Err(Errno::ENOSYS);
        }
        Ok(Machine::default())
    }
    fn get_hz(&mut self) -> Result<u32, Errno> {
        self.calls.push(Call::GetHz);
        if self.failing(&Call::GetHz) {
            return Err(Errno::ENOSYS);
        }
        Ok(self.hz)
    }
    fn get_ticks(&mut self) -> Result<Clock, Errno> {
        self.calls.push(Call::GetTicks);
        if self.failing(&Call::GetTicks) {
            return Err(Errno::ENOSYS);
        }
        Ok(self.ticks)
    }
    fn privctl(
        &mut self,
        proc: Endpoint,
        op: PrivCtlOp,
        priv_: Option<&Privilege>,
    ) -> Result<(), Errno> {
        self.calls.push(Call::PrivCtl(proc, op));
        if self.failing(&Call::PrivCtl(proc, op)) {
            return Err(Errno::ENOSYS);
        }
        if op == PrivCtlOp::SetSys
            && let Some(p) = priv_
        {
            // Per-endpoint echo storage: the last push per endpoint wins
            // (matches the kernel's one-priv-structure-per-process).
            self.set_privs.retain(|(e, _)| *e != proc);
            self.set_privs.push((proc, p.clone()));
        }
        Ok(())
    }
    fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, Errno> {
        self.calls.push(Call::GetPriv(proc));
        if self.failing(&Call::GetPriv(proc)) {
            return Err(Errno::ENOSYS);
        }
        if let Some((_, p)) = self.set_privs.iter().rev().find(|(e, _)| *e == proc) {
            return Ok(p.clone());
        }
        if let Some((_, p)) = self.kernel_privs.iter().rev().find(|(e, _)| *e == proc) {
            return Ok(p.clone());
        }
        Ok(Privilege::vacant())
    }
    fn setalarm(&mut self, delay_ticks: u32) -> Result<(), Errno> {
        self.calls.push(Call::SetAlarm(delay_ticks));
        if self.failing(&Call::SetAlarm(delay_ticks)) {
            return Err(Errno::ENOSYS);
        }
        Ok(())
    }
    fn sys_kill(&mut self, proc: Endpoint, signo: i32) -> Result<(), Errno> {
        self.calls.push(Call::SysKill(proc, signo));
        if self.failing(&Call::SysKill(proc, signo)) {
            return Err(Errno::ENOSYS);
        }
        Ok(())
    }
    fn sys_update(
        &mut self,
        src: Endpoint,
        dst: Endpoint,
        _flags: crate::service_slot::SysFlags,
    ) -> Result<(), Errno> {
        self.calls.push(Call::SysUpdate(src, dst));
        if self.failing(&Call::SysUpdate(src, dst)) {
            return Err(Errno::ENOSYS);
        }
        Ok(())
    }
    fn diagctl_stacktrace(&mut self, target: Endpoint) -> Result<(), Errno> {
        if self.failing(&Call::DiagctlStacktrace(target)) {
            return Err(Errno::ENOSYS);
        }
        self.calls.push(Call::DiagctlStacktrace(target));
        Ok(())
    }
    fn diag_write(&mut self, text: &str) -> Result<(), Errno> {
        let call = Call::DiagWrite(text.to_owned());
        if self.failing(&call) {
            return Err(Errno::ENOSYS);
        }
        self.calls.push(call);
        Ok(())
    }
}

impl SchedApi for MockKernelApi {
    fn sched_init_proc(&mut self, cfg: &SchedulerConfig) -> Result<Endpoint, Errno> {
        self.calls.push(Call::SchedInitProc(cfg.endpoint));
        if self.failing(&Call::SchedInitProc(cfg.endpoint)) {
            return Err(Errno::ENOSYS);
        }
        Ok(cfg.scheduler)
    }
    fn sched_stop(&mut self, scheduler: Endpoint, proc: Endpoint) -> Result<(), Errno> {
        self.calls.push(Call::SchedStop(scheduler, proc));
        if self.kill_ok {
            Ok(())
        } else {
            Err(Errno::ENOSYS)
        }
    }
}

impl PmApi for MockKernelApi {
    fn getnuid(&mut self, proc: Endpoint) -> Result<u32, Errno> {
        self.calls.push(Call::GetNuid(proc));
        if self.failing(&Call::GetNuid(proc)) {
            return Err(Errno::ENOSYS);
        }
        Ok(0) // root for boot tests
    }
    fn getnpid(&mut self, proc: Endpoint) -> Result<i32, Errno> {
        self.calls.push(Call::GetNpid(proc));
        if self.failing(&Call::GetNpid(proc)) {
            return Err(Errno::ENOSYS);
        }
        Ok(self.pids.pop().unwrap_or(100))
    }
    fn getprocnr(&mut self, _pid: Pid) -> Result<Endpoint, Errno> {
        match self.child_endpoint {
            Some(ep) => Ok(ep),
            None => Err(Errno::ENOSYS),
        }
    }
    fn srv_fork(&mut self, _uid: u32, _gid: u32) -> Result<Pid, Errno> {
        match self.fork_pid {
            Some(pid) => Ok(pid),
            // Fail-closed default: the 19 wiring shell may stub canned
            // results here once it lands.
            None => Err(Errno::ENOSYS),
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
    fn waitpid(&mut self) -> Option<Pid> {
        self.children.pop()
    }
    fn setuid(&mut self, uid: u32) -> Result<(), Errno> {
        self.calls.push(Call::SetUid(uid));
        if self.failing(&Call::SetUid(uid)) {
            return Err(Errno::ENOSYS);
        }
        Ok(())
    }
}

impl VmApi for MockKernelApi {
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
}

impl IpcApi for MockKernelApi {
    fn reply(
        &mut self,
        target: Endpoint,
        result: i32,
        payload: &minix_types::Message,
    ) -> Result<(), Errno> {
        self.calls.push(Call::Reply(target, result));
        self.replies.push((target, result, *payload));
        if self.failing(&Call::Reply(target, result)) {
            return Err(Errno::ENOSYS);
        }
        Ok(())
    }
    fn receive(
        &mut self,
        _endpoint: Endpoint,
    ) -> Result<(minix_types::Message, crate::dispatch::IpcStatus, Clock), Errno> {
        match self.inbox.first() {
            Some(_) => {
                let delivered = self.inbox.remove(0);
                Ok(delivered)
            }
            None => Err(Errno::ENOSYS),
        }
    }
    fn notify(&mut self, endpoint: Endpoint) -> Result<(), Errno> {
        if self.failing(&Call::Notify(endpoint)) {
            return Err(Errno::ENOSYS);
        }
        self.calls.push(Call::Notify(endpoint));
        Ok(())
    }
    fn asynsend(
        &mut self,
        endpoint: Endpoint,
        message: &minix_types::Message,
    ) -> Result<(), Errno> {
        if self.failing(&Call::Asynsend(endpoint, message.m_type)) {
            return Err(Errno::ENOSYS);
        }
        self.calls.push(Call::Asynsend(endpoint, message.m_type));
        self.sent.push((endpoint, *message));
        Ok(())
    }
    fn safecopy_from(
        &mut self,
        _source: Endpoint,
        addr: usize,
        buf: &mut [u8],
    ) -> Result<(), Errno> {
        // R4 mock-fidelity upgrade: the payload is a flat image of the
        // caller's address space based at 0 — `sys_datacopy` copies from
        // the *requested address* (manager.c:140-142), and do_up is the
        // first handler to fetch several buffers from different addresses
        // of one request. Out-of-range reads fail EFAULT like the kernel.
        match &self.payload {
            Some(bytes) => {
                let end = addr.checked_add(buf.len()).ok_or(Errno::EFAULT)?;
                if end > bytes.len() {
                    return Err(Errno::EFAULT);
                }
                buf.copy_from_slice(&bytes[addr..end]);
                Ok(())
            }
            None => Err(Errno::ENOSYS),
        }
    }
    fn safecopy_to(&mut self, dest: Endpoint, addr: usize, buf: &[u8]) -> Result<(), Errno> {
        self.sent_copies.push((dest, addr, buf.to_vec()));
        Ok(())
    }

    fn ds_lookup_by_label(&mut self, _label: &str) -> Option<Endpoint> {
        None
    }

    fn sendrec_to(&mut self, dest: Endpoint, msg: &mut minix_types::Message) -> Result<(), Errno> {
        self.sent_rec.push((dest, *msg));
        Ok(())
    }

    fn grant_read(&mut self, _dest: Endpoint, addr: u64, len: u64) -> Result<GrantId, Errno> {
        self.grant_seq += 1;
        self.last_grant = Some((self.grant_seq, addr, len));
        Ok(self.grant_seq as GrantId)
    }

    fn grant_revoke(&mut self, grant: GrantId) {
        self.revoked_grants.push(grant);
    }

    fn lu_request_prepare(
        &mut self,
        ep: Endpoint,
        _flags: i32,
        _state_data_gid: Option<GrantId>,
        _state: i32,
    ) -> Result<(), Errno> {
        self.prepare_requests.push(ep);
        Ok(())
    }

    fn vm_prepare(&mut self, _src: Endpoint, _dst: Endpoint, _flags: i32) -> Result<(), Errno> {
        Ok(())
    }
}

// ── Shared boot fixtures（R40：夹具族上移，shell 域测试随 handler 同模块）──────

use crate::boot::{BootTables, KernelApi, RinitState};
use crate::process_table::RProcTable;
use crate::service_slot::RFlags;
use crate::{RsServer, ServerState};

/// A server with a completed boot: table + one in-use VFS service with
/// pid 700, plus an exited child (pid 700's own child bookkeeping is the
/// sigchld target).
pub(crate) fn booted() -> RsServer {
    booted_with(alloc::boxed::Box::new(crate::boot::UnimplementedKernelApi))
}

/// Same fixture with an injectable kernel seam (E-10: the waitpid drain
/// and the second-init panic need a mock / a consumed boot machine).
pub(crate) fn booted_with(kernel: alloc::boxed::Box<dyn KernelApi>) -> RsServer {
    let mut server = RsServer::with_kernel(BootTables::placeholder(), kernel);
    let mut table = RProcTable::new();
    let id = table.alloc_slot().unwrap();
    {
        let s = table.get_mut(id);
        s.flags = RFlags::IN_USE | RFlags::ACTIVE;
        s.pub_.endpoint = Endpoint::VFS;
        s.pid = Some(700);
    }
    server.state = Some(ServerState {
        tables: BootTables::placeholder(),
        machine: Machine::default(),
        rinit: RinitState::default(),
        table,
        shutting_down: false,
        system_hz: 60,
        nr_uncaught_init_srvs: 0,
        update: crate::live_update::UpdateState::default(),
    });
    server
}

/// A booted server whose VFS slot carries the given label (system
/// privilege excluded) and whose safecopy seam serves the given payload
/// — the 14 wiring tests' fixture.
pub(crate) fn booted_vfs_labeled(label: &'static [u8], payload: &'static [u8]) -> RsServer {
    let mut mock = crate::testutil::MockKernelApi::new(60);
    mock.payload = Some(payload.to_vec());
    mock.ticks = 500;
    // create_service's VM, exec and fork faces succeed (13 label arms
    // clone through them); the boot-level default is fail-closed.
    mock.vm_ok = true;
    mock.execve_ok = true;
    mock.fork_pid = Some(701);
    mock.child_endpoint = Some(Endpoint::MEM);
    booted_vfs_kernel(alloc::boxed::Box::new(mock), label)
}

/// [`booted_vfs_labeled`]'s table setup with an injected kernel seam —
/// for tests that need seam-level failure injection (R37).
pub(crate) fn booted_vfs_kernel(
    kernel: alloc::boxed::Box<dyn KernelApi>,
    label: &'static [u8],
) -> RsServer {
    let mut server = booted_with(kernel);
    {
        let state = server.state.as_mut().unwrap();
        state
            .table
            .set_endpoint_index(Endpoint::VFS, Some(crate::service_slot::SlotId::new(0)));
        let s = state.table.get_mut(crate::service_slot::SlotId::new(0));
        s.flags = RFlags::IN_USE | RFlags::ACTIVE;
        s.pub_.in_use = true;
        s.pub_.endpoint = Endpoint::VFS;
        s.pub_.label = Label::from_bytes(label);
        // A launch command: create_service's preconditions
        // (manager.c:540-560) require SF_USE_COPY or a command.
        s.cmd[..8].copy_from_slice(b"/bin/vfs");
    }
    server
}

/// Schedules a two-entry chain (slot 0 = VFS, slot 1 = PM) and walks it
/// to `curr = entry 0` with `RS_UPDATING` armed — the state a batch
/// RS_UPDATE leaves behind while the first service is preparing. Both
/// slots get a cloned replica (`new_rp`), which start_update requires
/// (update.c:631).
pub(crate) fn two_entry_chain(server: &mut RsServer) {
    let id_pm = {
        let table = &mut server.state.as_mut().unwrap().table;
        let pid = table.alloc_slot().unwrap();
        let s = table.get_mut(pid);
        s.flags = RFlags::IN_USE | RFlags::ACTIVE;
        s.pub_.in_use = true;
        s.pub_.endpoint = Endpoint::PM;
        s.cmd[..7].copy_from_slice(b"/bin/pm");
        pid
    };
    let vfs = crate::service_slot::SlotId::new(0);
    let replica_vfs =
        crate::service_create::clone_slot(&mut server.state.as_mut().unwrap().table, vfs).unwrap();
    let replica_pm =
        crate::service_create::clone_slot(&mut server.state.as_mut().unwrap().table, id_pm)
            .unwrap();
    {
        let table = &mut server.state.as_mut().unwrap().table;
        table.get_mut(vfs).new_rp = Some(replica_vfs);
        table.get_mut(id_pm).new_rp = Some(replica_pm);
    }
    let state = server.state.as_mut().unwrap();
    state
        .update
        .chain
        .add(crate::live_update::UpdateEntry::new(vfs, Endpoint::VFS));
    state
        .update
        .chain
        .add(crate::live_update::UpdateEntry::new(id_pm, Endpoint::PM));
    let mut noop_req = |_: &crate::service_slot::ServiceSlot, _: i32| {};
    let mut noop_vm = |_: Endpoint, _: Endpoint, _: crate::service_slot::SysFlags| {};
    let mut noop_abort = |_: i32| {};
    let mut noop_end = |_: i32| {};
    state
        .update
        .start_update_prepare(
            &mut state.table,
            true,
            true,
            &mut noop_abort,
            &mut noop_end,
            &mut crate::live_update::PrepareEffects {
                request_prepare: &mut noop_req,
                vm_prepare: &mut noop_vm,
            },
        )
        .expect("prepare schedules the first entry");
}

/// An RS_INIT envelope from the VFS service with the given result.
/// Union-field *writes* are safe (bit stores); the tagged *read* goes
/// through `Message::rs_init_result` (minix-types, E-12 decode).
pub(crate) fn rs_init_envelope(result: i32) -> minix_types::Message {
    let mut m = minix_types::Message {
        m_source: Endpoint::VFS,
        m_type: minix_types::RS_INIT,
        m_u: Default::default(),
    };
    m.m_u.m_rs_init.result = result;
    m
}
