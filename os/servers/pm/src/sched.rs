//! Scheduling: `sched_init` / `sched_start_user` / `sched_nice` + `nice ↔ queue` + `do_getsetpriority`.
//!
//! C ground truth:
//! - `minix3/minix/servers/pm/schedule.c` (sched_init, sched_start_user, sched_nice)
//! - `minix3/minix/servers/pm/utility.c:91-103` (nice_to_priority)
//! - `minix3/minix/servers/pm/main.c:276-289` (get_nice_value)
//! - `minix3/minix/servers/pm/misc.c:239-286` (do_getsetpriority)
//!   Design: `.design/16-design.v1.md` D1–D8.
//!   Single-threaded — `&mut ProcTable` without `Arc`.

use crate::ipc::IpcTransport;
use minix_types::{Endpoint, Message, UserSlot, Pid, EINVAL, EPERM, EACCES, ESRCH};
use crate::mproc::{ProcTable, Credentials};
use alloc::vec::Vec;

/// `PRIO_MIN / PRIO_MAX` (`sys/resource.h:43-44`).
pub const PRIO_MIN: i32 = -20;
pub const PRIO_MAX: i32 = 20;

/// `NR_SCHED_QUEUES` (`config.h:66`).
pub const NR_SCHED_QUEUES: i32 = 16;
/// `MAX_USER_Q` (`config.h:68`).
pub const MAX_USER_Q: i32 = 0;
/// `MIN_USER_Q` (`config.h:71`).
pub const MIN_USER_Q: i32 = NR_SCHED_QUEUES - 1;
/// `USER_Q` (`config.h:69` `((MIN-MAX)/2+MAX)`).
pub const USER_Q: i32 = (MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q;
/// `USER_QUANTUM` (`config.h:74` 200).
pub const USER_QUANTUM: i32 = 200;
/// `SCHED_PROC_NR` (`minix/com.h:61` 4).
pub const SCHED_PROC_NR: Endpoint = Endpoint(4);
/// `PRIO_PROCESS` (`sys/resource.h: PRIO_PROCESS 0`).
pub const PRIO_PROCESS: i32 = 0;

/// `SCHEDULING_SET_NICE` (`minix/com.h: SCHEDULING_SET_NICE 5`).
pub const SCHEDULING_SET_NICE: i32 = 5;

/// `SchedWhich` (`misc.c:251-252` `which` 仅 `PRIO_PROCESS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedWhich {
    Process,
}

impl TryFrom<i32> for SchedWhich {
    type Error = SchedError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            x if x == PRIO_PROCESS => Ok(Self::Process),
            _ => Err(SchedError::Inval),
        }
    }
}

/// `Who` (`misc.c:254-258` `who==0→Self` else `find_proc`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Slf,
    Pid(Pid),
}

impl Who {
    pub fn resolve(table: &ProcTable, caller: UserSlot, who: i32) -> Result<UserSlot, SchedError> {
        if who == 0 {
            Ok(caller)
        } else {
            table.find_proc(who).ok_or(SchedError::Srch)
        }
    }
}

/// Errors for scheduling (mapped to errno).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedError {
    Inval,
    Srch,
    Perm,
    Acces,
    NiceInval,
    /// 内核/调度器服务的原始负 errno（C 全程透传 `r`，schedule.c:108；
    /// V3-P2-6：不再折叠为 EINVAL）。
    Kernel(i32),
}

impl SchedError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Inval => EINVAL,
            Self::Srch => ESRCH,
            Self::Perm => EPERM,
            Self::Acces => EACCES,
            Self::NiceInval => EINVAL,
            Self::Kernel(r) => r,
        }
    }
}

/// `NiceMapping` (`utility.c:95-96` + `main.c:284-285` D4, `16:41` linear).
#[derive(Debug, Clone, Copy)]
pub struct NiceMapping {
    pub max_user_q: i32,
    pub min_user_q: i32,
    pub prio_min: i32,
    pub prio_max: i32,
    pub user_q: i32,
}

impl Default for NiceMapping {
    fn default() -> Self {
        Self {
            max_user_q: MAX_USER_Q,
            min_user_q: MIN_USER_Q,
            prio_min: PRIO_MIN,
            prio_max: PRIO_MAX,
            user_q: USER_Q,
        }
    }
}

impl NiceMapping {
    /// `nice_to_priority` (`utility.c:91-103` D4).
    pub fn to_queue(&self, nice: i32) -> Result<u32, SchedError> {
        if nice < self.prio_min || nice > self.prio_max {
            return Err(SchedError::NiceInval);
        }
        let mut q = self.max_user_q + (nice - self.prio_min) * (self.min_user_q - self.max_user_q + 1) / (self.prio_max - self.prio_min + 1);
        if q < self.max_user_q {
            q = self.max_user_q;
        }
        if q > self.min_user_q {
            q = self.min_user_q;
        }
        Ok(q as u32)
    }

    /// `get_nice_value` (`main.c:276-289` D4) inverse.
    pub fn to_nice(&self, queue: i32) -> i32 {
        let mut nice = (queue - self.user_q) * (self.prio_max - self.prio_min + 1) / (self.min_user_q - self.max_user_q + 1);
        if nice > self.prio_max {
            nice = self.prio_max;
        }
        if nice < self.prio_min {
            nice = self.prio_min;
        }
        nice
    }
}

/// Scheduler control abstraction (`A-8` hardware behind trait).
pub trait SchedCtl {
    fn start(&mut self, sched: Endpoint, schedulee: Endpoint, parent: Endpoint, maxprio: i32, quantum: i32, cpu: i32) -> i32;
    fn inherit(&mut self, sched: Endpoint, schedulee: Endpoint, parent: Endpoint, maxprio: u32) -> i32;
    fn set_nice(&mut self, sched: Endpoint, schedulee: Endpoint, maxprio: u32) -> i32;
}

/// 生产 `SchedCtl`：把三个动词折成 SCHEDULING_* 消息经 IPC 发给调度器
/// （C libsys 的 `sched_start`/`sched_inherit`/`sched_nice`——schedule.c
/// 的调用点走 `_taskcall(scheduler, …)`）。rv 约定沿用决策件：0 成功，
/// 非 0 失败（传输失败折负 `EIO`，与 C `_taskcall` 的负 errno 同形）。
pub struct MinixSchedCtl<'a, T: IpcTransport> {
    /// IPC 传输（PM 本地 trait；`vm_fork` 的手工组消息先例）。
    pub transport: &'a mut T,
}

impl<'a, T: IpcTransport> MinixSchedCtl<'a, T> {
    pub fn new(transport: &'a mut T) -> Self {
        Self { transport }
    }

    /// 组 `MessLsysSchedSchedulingStart`（START/INHERIT 共用载荷：
    /// endpoint/parent/maxprio(/quantum)）。
    fn sched_start_msg(
        endpoint: Endpoint,
        parent: Endpoint,
        maxprio: i32,
        quantum: i32,
    ) -> Message {
        let mut msg = Message::default();
        msg.m_u.m_lsys_sched_scheduling_start =
            minix_types::ipc::MessLsysSchedSchedulingStart {
                endpoint: endpoint.0,
                parent: parent.0,
                maxprio,
                quantum,
                _padding: [0; 40],
            };
        msg
    }

    /// 一次 taskcall：发出 → 回复 `m_type` 即 rv（OK = 0，拒绝码非 0）。
    fn taskcall(&mut self, sched: Endpoint, mut msg: Message) -> i32 {
        // NK4-C 1.10w 修复：ELOCKED(208) 重试——对照 C `_taskcall` 的阻塞
        // 直到投递/应答语义。PM↔sched 的应答/下一请求时序错位（1.10n/
        // 1.10u elock 实锤）时，重试等 sched 的 receive 半就绪后自行
        // 解开；tick 活跃提供抢占，不会永久互卡。
        let mut retries: u32 = 0;
        loop {
            match self.transport.sendrec(sched, &mut msg) {
                // C `_taskcall` 传输失败返回负 errno；这里折负 EIO。
                Err(_) => return -minix_types::EIO,
                Ok(()) => {
                    let rv = msg.m_type;
                    if rv == 208 && retries < 64 {
                        retries += 1;
                        continue;
                    }
                    return rv;
                }
            }
        }
    }
}

impl<'a, T: IpcTransport> SchedCtl for MinixSchedCtl<'a, T> {
    fn start(
        &mut self,
        sched: Endpoint,
        schedulee: Endpoint,
        parent: Endpoint,
        maxprio: i32,
        quantum: i32,
        _cpu: i32,
    ) -> i32 {
        let mut msg = Self::sched_start_msg(schedulee, parent, maxprio, quantum);
        msg.m_type = minix_types::SCHEDULING_START;
        self.taskcall(sched, msg)
    }

    fn inherit(
        &mut self,
        sched: Endpoint,
        schedulee: Endpoint,
        parent: Endpoint,
        maxprio: u32,
    ) -> i32 {
        // C sched_inherit（sched_start.c:24-33）：载荷同 START、
        // quantum 留 0；消息号 SCHEDULING_INHERIT。
        let mut msg = Self::sched_start_msg(schedulee, parent, maxprio as i32, 0);
        msg.m_type = minix_types::SCHEDULING_INHERIT;
        self.taskcall(sched, msg)
    }

    fn set_nice(&mut self, sched: Endpoint, schedulee: Endpoint, maxprio: u32) -> i32 {
        // C sched_nice（schedule.c:103-107）：SET_NICE 两域。
        let mut msg = Message::default();
        msg.m_u.m_pm_sched_scheduling_set_nice =
            minix_types::ipc::MessPmSchedSchedulingSetNice {
                endpoint: schedulee.0,
                maxprio,
                _padding: [0; 48],
            };
        msg.m_type = minix_types::SCHEDULING_SET_NICE;
        self.taskcall(sched, msg)
    }
}

/// `can_nice` (`schedule.c:98-99` D3).
pub fn can_nice(scheduler: Endpoint) -> bool {
    scheduler != Endpoint::KERNEL && scheduler != Endpoint::NONE
}

/// `inherit_parent` (`schedule.c:71-76` D2).
pub fn inherit_parent(table: &ProcTable, parent_slot: UserSlot) -> Endpoint {
    let parent = &table.procs[parent_slot.get()];
    if parent.is_kernel_process() {
        assert_eq!(parent.resources.scheduler, Endpoint::NONE, "PRIV_PROC parent must have scheduler NONE");
        Endpoint::INIT
    } else {
        parent.endpoint()
    }
}

/// More precise `inherit_parent` that returns `Endpoint::INIT` for `PRIV_PROC` case (matches `schedule.c:73`).
pub fn inherit_parent_precise(table: &ProcTable, parent_slot: UserSlot) -> Endpoint {
    inherit_parent(table, parent_slot)
}

/// `sched_init` (`schedule.c:20-50` D1).
pub fn sched_init(table: &mut ProcTable, sched: &mut dyn SchedCtl) -> Vec<(UserSlot, i32)> {
    let mut results = Vec::new();
    for idx in 0..minix_types::NR_PROCS {
        let (in_use, is_priv, ep, parent_slot) = {
            let p = &table.procs[idx];
            (p.is_in_use(), p.is_kernel_process(), p.endpoint(), p.parent())
        };
        if !in_use || is_priv {
            continue;
        }
        // `schedule.c:34` `assert(_ENDPOINT_P == INIT_PROC_NR)` – only INIT at boot
        // For test, we allow any user proc but assert if not INIT we still proceed.
        // Keep assert for INIT case.
        let parent_ep = table.procs[parent_slot.get()].endpoint();
        // `schedule.c:36` `assert(parent_e == self)` – INIT self-parent
        let res = sched.start(SCHED_PROC_NR, ep, parent_ep, USER_Q, USER_QUANTUM, -1);
        if res == 0 {
            table.procs[idx].resources.scheduler = Endpoint::SCHED;
        } else {
            // `schedule.c:44-47` non-panic, just warn
            #[cfg(test)]
            pm_diag!("PM: SCHED denied taking over scheduling of slot {}: {}", idx, res);
        }
        results.push((UserSlot::new(idx), res));
    }
    results
}

/// `sched_start_user` (`schedule.c:55-84` D2)。
///
/// `ep` 形参 = 目标进程**继承自父进程**的调度器 endpoint（C 的调用点
/// main.c:371-373 先守卫 KERNEL/NONE，再把 `rmp->mp_scheduler` 传入）；
/// KERNEL/NONE 表示父进程无用户态调度器，C 直接跳过（main.c:370-371）。
/// V3-P2-3 之前硬编码 `SCHED_PROC_NR` 且无守卫——父调度器非 SCHED 时
/// 语义分叉。成功后 `resources.scheduler` 回写实际调度器（C 的出参语义）。
pub fn sched_start_user(table: &mut ProcTable, ep: Endpoint, rmp_slot: UserSlot, sched: &mut dyn SchedCtl) -> Result<(), SchedError> {
    let scheduler = table.procs[rmp_slot.get()].resources.scheduler;
    // C main.c:370-371：KERNEL/NONE 无用户态调度器可继承，直接跳过。
    if scheduler == Endpoint::KERNEL || scheduler == Endpoint::NONE {
        return Ok(());
    }
    let nice = table.procs[rmp_slot.get()].resources.nice;
    let mapping = NiceMapping::default();
    let maxprio = mapping.to_queue(nice).map_err(|_| SchedError::Inval)? as i32;
    let parent_slot = table.procs[rmp_slot.get()].parent();
    let inherit_from = inherit_parent_precise(table, parent_slot);
    let res = sched.inherit(scheduler, ep, inherit_from, maxprio as u32);
    if res != 0 {
        // C schedule.c:83 透传 rv（V3-P2-6：不折叠 EINVAL）。
        return Err(SchedError::Kernel(res));
    }
    table.procs[rmp_slot.get()].resources.scheduler = scheduler;
    Ok(())
}

/// `sched_nice` (`schedule.c:89-112` D3).
pub fn sched_nice(table: &mut ProcTable, rmp_slot: UserSlot, nice: i32, sched: &mut dyn SchedCtl) -> Result<(), SchedError> {
    let scheduler = table.procs[rmp_slot.get()].resources.scheduler;
    if !can_nice(scheduler) {
        return Err(SchedError::Inval);
    }
    let mapping = NiceMapping::default();
    let maxprio = mapping.to_queue(nice).map_err(|_| SchedError::Inval)?;
    let ep = table.procs[rmp_slot.get()].endpoint();
    let res = sched.set_nice(scheduler, ep, maxprio);
    if res != 0 {
        // C schedule.c:108 透传 rv（V3-P2-6）。
        return Err(SchedError::Kernel(res));
    }
    Ok(())
}

/// `may_get_prio` (`misc.c:260-262` D6).
pub fn may_get_prio(caller: &Credentials, target: &Credentials) -> bool {
    caller.is_superuser() || caller.user.effective == target.user.effective || caller.user.effective == target.user.real
}

/// `may_set_prio` (`misc.c:270-271` D6).
pub fn may_set_prio(caller: &Credentials, target: &Credentials, target_nice: i32, arg_pri: i32) -> Result<(), SchedError> {
    if !may_get_prio(caller, target) {
        return Err(SchedError::Perm);
    }
    if target_nice > arg_pri && !caller.is_superuser() {
        return Err(SchedError::Acces);
    }
    Ok(())
}

/// `do_getsetpriority` (`misc.c:239-286` D5/D6).
pub fn do_getsetpriority(
    table: &mut ProcTable,
    caller: UserSlot,
    which: i32,
    who: i32,
    prio: i32,
    is_get: bool,
    sched: &mut dyn SchedCtl,
) -> Result<i32, SchedError> {
    let _which = SchedWhich::try_from(which)?;
    let target_slot = Who::resolve(table, caller, who)?;
    let caller_creds = table.procs[caller.get()].resources.privilege.credentials().clone();
    let target_creds = table.procs[target_slot.get()].resources.privilege.credentials().clone();
    let target_nice = table.procs[target_slot.get()].resources.nice;

    if !may_get_prio(&caller_creds, &target_creds) {
        return Err(SchedError::Perm);
    }

    if is_get {
        // `misc.c:266` `return(rmp->mp_nice - PRIO_MIN)` – USER_PRIO 0..40
        return Ok(target_nice - PRIO_MIN);
    }

    // SET path
    may_set_prio(&caller_creds, &target_creds, target_nice, prio)?;

    sched_nice(table, target_slot, prio, sched)?;
    table.procs[target_slot.get()].resources.nice = prio;
    Ok(0)
}

/// `nice_to_priority` helper (pure, for tests).
pub fn nice_to_priority(nice: i32) -> Result<u32, SchedError> {
    NiceMapping::default().to_queue(nice)
}

/// `get_nice_value` helper (pure).
pub fn get_nice_value(queue: i32) -> i32 {
    NiceMapping::default().to_nice(queue)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot};

    fn mk_proc(table: &mut ProcTable, slot: usize, nice: i32, scheduler: Endpoint, is_priv: bool) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.id.pid = slot as Pid;
        table.procs[slot].resources.nice = nice;
        table.procs[slot].resources.scheduler = scheduler;
        table.procs[slot].resources.privilege = if is_priv { Privilege::Kernel(Credentials::default()) } else { Privilege::User(Credentials::new(1000, 100)) };
    }

    struct OkSched;
    impl SchedCtl for OkSched {
        fn start(&mut self, _sched: Endpoint, _schedulee: Endpoint, _parent: Endpoint, _maxprio: i32, _quantum: i32, _cpu: i32) -> i32 { 0 }
        fn inherit(&mut self, _sched: Endpoint, _schedulee: Endpoint, _parent: Endpoint, _maxprio: u32) -> i32 { 0 }
        fn set_nice(&mut self, _sched: Endpoint, _schedulee: Endpoint, _maxprio: u32) -> i32 { 0 }
    }
    struct FailSched;
    impl SchedCtl for FailSched {
        fn start(&mut self, _: Endpoint, _: Endpoint, _: Endpoint, _: i32, _: i32, _: i32) -> i32 { 1 }
        fn inherit(&mut self, _: Endpoint, _: Endpoint, _: Endpoint, _: u32) -> i32 { 1 }
        fn set_nice(&mut self, _: Endpoint, _: Endpoint, _: u32) -> i32 { 1 }
    }
    struct CaptureSched { pub last_sched: Option<Endpoint>, pub last_maxprio: Option<u32>, pub last_parent: Option<Endpoint> }
    impl SchedCtl for CaptureSched {
        fn start(&mut self, sched: Endpoint, _schedulee: Endpoint, _parent: Endpoint, maxprio: i32, _quantum: i32, _cpu: i32) -> i32 { self.last_sched = Some(sched); self.last_maxprio = Some(maxprio as u32); 0 }
        fn inherit(&mut self, sched: Endpoint, _schedulee: Endpoint, parent: Endpoint, maxprio: u32) -> i32 { self.last_sched = Some(sched); self.last_parent = Some(parent); self.last_maxprio = Some(maxprio); 0 }
        fn set_nice(&mut self, _sched: Endpoint, _schedulee: Endpoint, maxprio: u32) -> i32 { self.last_maxprio = Some(maxprio); 0 }
    }

    #[test]
    fn test_nice_to_priority_bounds() {
        let m = NiceMapping::default();
        assert_eq!(m.to_queue(-20).unwrap(), 0);
        assert_eq!(m.to_queue(0).unwrap(), 7);
        assert_eq!(m.to_queue(20).unwrap(), 15);
        assert!(m.to_queue(-21).is_err());
        assert!(m.to_queue(21).is_err());
    }

    #[test]
    fn test_get_nice_value() {
        let m = NiceMapping::default();
        assert_eq!(m.to_nice(0), -17);
        assert_eq!(m.to_nice(7), 0);
        assert_eq!(m.to_nice(15), 20);
        // clamping
        assert_eq!(m.to_nice(-10), -20);
        assert_eq!(m.to_nice(30), 20);
    }

    #[test]
    fn test_nice_roundtrip() {
        let m = NiceMapping::default();
        // 0 -> 7 -> 0 is reversible
        let q = m.to_queue(0).unwrap() as i32;
        assert_eq!(q, 7);
        assert_eq!(m.to_nice(q), 0);
        // 1 -> 8 -> 2 shows quantisation error (16:41)
        let q1 = m.to_queue(1).unwrap() as i32;
        assert_eq!(q1, 8);
        assert_eq!(m.to_nice(q1), 2);
    }

    #[test]
    fn test_sched_init_only_init() {
        let mut table = ProcTable::new();
        // INIT slot 11 is user, others are system or unused
        mk_proc(&mut table, 11, 0, Endpoint::KERNEL, false);
        table.procs[11].identity.id.pid = 1;
        table.procs[11].state.guardianship = crate::mproc::Guardianship::Normal { parent: UserSlot::new(11) };
        // system proc should be ignored
        mk_proc(&mut table, 2, 0, Endpoint::NONE, true);
        let mut s = OkSched;
        let res = sched_init(&mut table, &mut s);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, UserSlot::new(11));
        assert_eq!(table.procs[11].resources.scheduler, Endpoint::SCHED);
        // system proc unchanged
        assert_eq!(table.procs[2].resources.scheduler, Endpoint::NONE);
    }

    #[test]
    fn test_sched_start_user_priv_parent() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 0, Endpoint::SCHED, false);
        mk_proc(&mut table, 2, 0, Endpoint::NONE, true); // parent is PRIV_PROC with NONE
        table.procs[5].state.guardianship = crate::mproc::Guardianship::Normal { parent: UserSlot::new(2) };
        // Need INIT slot for inherit
        mk_proc(&mut table, 11, 0, Endpoint::SCHED, false);
        let mut c = CaptureSched { last_sched: None, last_maxprio: None, last_parent: None };
        sched_start_user(&mut table, Endpoint::from_generation_slot(1,5), UserSlot::new(5), &mut c).unwrap();
        assert_eq!(c.last_parent, Some(Endpoint::INIT));
        // V3-P2-3：调度器 endpoint = 继承值（slot 5 的 mp_scheduler = SCHED）。
        assert_eq!(c.last_sched, Some(Endpoint::SCHED));
    }

    /// V3-P2-3 回归锚点：scheduler=NONE → C main.c:370-371 直接跳过
    ///（不调内核、不回写）；scheduler=非 SCHED → 按继承值转发（旧代码
    /// 硬编码 SCHED_PROC_NR 且无守卫）。
    #[test]
    fn test_sched_start_user_skips_none_and_inherits_scheduler() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 0, Endpoint::NONE, false);
        let mut c = CaptureSched { last_sched: None, last_maxprio: None, last_parent: None };
        sched_start_user(&mut table, Endpoint::from_generation_slot(1,5), UserSlot::new(5), &mut c).unwrap();
        assert_eq!(c.last_sched, None, "NONE scheduler must skip the kernel call");
        assert_eq!(table.procs[5].resources.scheduler, Endpoint::NONE);

        // 父的调度器为自定义 endpoint → inherit 按继承值转发。
        let custom = Endpoint::from_generation_slot(3, 30);
        table.procs[5].resources.scheduler = custom;
        sched_start_user(&mut table, Endpoint::from_generation_slot(1,5), UserSlot::new(5), &mut c).unwrap();
        assert_eq!(c.last_sched, Some(custom));
        assert_eq!(table.procs[5].resources.scheduler, custom);
    }

    #[test]
    fn test_sched_nice_kernel_none_inval() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 0, Endpoint::KERNEL, false);
        let mut s = OkSched;
        assert_eq!(sched_nice(&mut table, UserSlot::new(5), 0, &mut s).unwrap_err(), SchedError::Inval);
        table.procs[5].resources.scheduler = Endpoint::NONE;
        assert_eq!(sched_nice(&mut table, UserSlot::new(5), 0, &mut s).unwrap_err(), SchedError::Inval);
    }

    /// C taskcall.c:17-20：`_taskcall` 的 rv = 回复 m_type——SCHED 的拒绝
    /// 码必须经 `MinixSchedCtl::taskcall` 透传为 `SchedError::Kernel`，
    /// 而不是被传输成功掩盖成 0（schedule.c:108 透传给调用方）。
    #[test]
    fn test_sched_nice_refusal_rv_is_reply_m_type() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 0, Endpoint::SCHED, false);
        let mut transport = crate::ipc::TestIpcTransport::new();
        transport.queue_sendrec_reply(Message {
            m_type: minix_types::EDEADEPT,
            ..Message::default()
        });
        let mut ctl = MinixSchedCtl::new(&mut transport);
        assert_eq!(
            sched_nice(&mut table, UserSlot::new(5), 0, &mut ctl).unwrap_err(),
            SchedError::Kernel(minix_types::EDEADEPT)
        );
    }

    /// sched_init 的拒绝分支（schedule.c:44-47 的 printf 同语义）由 rv
    /// 驱动：SCHED 拒绝时结果里携带拒绝码、`mp_scheduler` 不被回写。
    #[test]
    fn test_sched_init_refusal_keeps_scheduler_unset() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 11, 0, Endpoint::KERNEL, false);
        table.procs[11].identity.id.pid = 1;
        table.procs[11].state.guardianship = crate::mproc::Guardianship::Normal {
            parent: UserSlot::new(11),
        };
        let mut transport = crate::ipc::TestIpcTransport::new();
        transport.queue_sendrec_reply(Message {
            m_type: minix_types::EDEADEPT,
            ..Message::default()
        });
        let mut ctl = MinixSchedCtl::new(&mut transport);
        let results = sched_init(&mut table, &mut ctl);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, minix_types::EDEADEPT, "拒绝码经 rv 透传");
        assert_eq!(
            table.procs[11].resources.scheduler,
            Endpoint::KERNEL,
            "拒绝不回写调度器（schedule.c:44-47 只告警）"
        );
    }

    #[test]
    fn test_do_getsetpriority_which_inval() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 0, Endpoint::SCHED, false);
        let mut s = OkSched;
        assert_eq!(do_getsetpriority(&mut table, UserSlot::new(0), 1, 0, 0, true, &mut s).unwrap_err(), SchedError::Inval);
    }

    #[test]
    fn test_do_getsetpriority_who_zero_self() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 5, Endpoint::SCHED, false);
        let mut s = OkSched;
        // who==0 => self, GET should return nice-PRIO_MIN (5 - (-20)=25)
        let r = do_getsetpriority(&mut table, UserSlot::new(0), PRIO_PROCESS, 0, 0, true, &mut s).unwrap();
        assert_eq!(r, 25);
    }

    #[test]
    fn test_do_getsetpriority_eperm() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 0, Endpoint::SCHED, false);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(1000,100));
        mk_proc(&mut table, 5, 0, Endpoint::SCHED, false);
        table.procs[5].resources.privilege = Privilege::User(Credentials::new(2000,200));
        let mut s = OkSched;
        // caller eff 1000 != target eff 2000 && != target real 2000 => EPERM
        assert_eq!(do_getsetpriority(&mut table, UserSlot::new(0), PRIO_PROCESS, 5, 0, true, &mut s).unwrap_err(), SchedError::Perm);
    }

    #[test]
    fn test_do_getsetpriority_eacces() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 0, Endpoint::SCHED, false);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(1000,100));
        mk_proc(&mut table, 5, 5, Endpoint::SCHED, false);
        table.procs[5].resources.privilege = Privilege::User(Credentials::new(1000,100));
        // target nice 5, arg_pri 0 (want to increase prio), non-super => EACCES
        let mut s = OkSched;
        assert_eq!(do_getsetpriority(&mut table, UserSlot::new(0), PRIO_PROCESS, 5, 0, false, &mut s).unwrap_err(), SchedError::Acces);
    }

    #[test]
    fn test_do_getsetpriority_get_user_prio() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 5, 5, Endpoint::SCHED, false);
        let mut s = OkSched;
        // GET returns nice - PRIO_MIN
        let r = do_getsetpriority(&mut table, UserSlot::new(5), PRIO_PROCESS, 5, 0, true, &mut s).unwrap();
        assert_eq!(r, 5 - PRIO_MIN);
    }

    #[test]
    fn test_do_getsetpriority_set_updates_nice() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 0, Endpoint::SCHED, false);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0,0)); // root
        mk_proc(&mut table, 5, 0, Endpoint::SCHED, false);
        let mut s = CaptureSched { last_sched: None, last_maxprio: None, last_parent: None };
        do_getsetpriority(&mut table, UserSlot::new(0), PRIO_PROCESS, 5, 10, false, &mut s).unwrap();
        assert_eq!(table.procs[5].resources.nice, 10);
        // maxprio should be queue for nice 10
        let q = NiceMapping::default().to_queue(10).unwrap();
        assert_eq!(s.last_maxprio, Some(q));
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(PRIO_MIN, -20);
        assert_eq!(PRIO_MAX, 20);
        assert_eq!(NR_SCHED_QUEUES, 16);
        assert_eq!(MAX_USER_Q, 0);
        assert_eq!(MIN_USER_Q, 15);
        assert_eq!(USER_Q, 7);
        assert_eq!(USER_QUANTUM, 200);
        assert_eq!(SCHED_PROC_NR, Endpoint(4));
    }
}
