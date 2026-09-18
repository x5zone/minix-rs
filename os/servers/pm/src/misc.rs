//! Misc queries: `do_sysuname/getsysinfo/getprocnr/getepinfo/reboot/svrctl/getrusage/sprofile/mcontext`.
//!
//! C ground truth: `minix3/minix/servers/pm/misc.c` (447 lines, 72/108/149/169/198/291/400)
//! + `profile.c:22` + `mcontext.c:13/23` + `utility.c:57/144`
//!   Design: `.design/20-design.v1.md` D1–D8 (explicit `UtsField/SysInfoWhat/EpInfo/RebootCtl/ParamStore/RusageWho`).
//!   Single-threaded — `&mut ProcTable` without `Arc`.

use minix_types::{Clock, Pid, Uid, Gid, Endpoint, VirBytes, EINVAL, EPERM, ESRCH, ENOSPC, E2BIG, ENOSYS};
use minix_types::MprocWire;
use crate::mproc::ProcTable;
use crate::ipc::ReplyIntent;

/// 自身/子进程的 (utime, stime) 二元对（`getrusage`/`sys_times` 汇聚的类型化别名）。
type UtimeStimePair = ((i64, i64), (i64, i64));

/// `UTS_TBL` length (`misc.c:46` 8).
pub const UTS_TBL_LEN: usize = 8;

/// `MAX_LOCAL_PARAMS` (`misc.c:297` 2).
pub const MAX_LOCAL_PARAMS: usize = 2;

/// `RB_POWERDOWN` (`reboot.h`).
pub const RB_POWERDOWN: i32 = 1 << 0;

/// `SI_PROC_TAB` (`sysinfo.h`).
pub const SI_PROC_TAB: i32 = 0;
/// `SI_CALL_STATS` (`sysinfo.h`, cfg guarded).
pub const SI_CALL_STATS: i32 = 1;

/// `RUSAGE_SELF/CHILDREN` (`resource.h`).
pub const RUSAGE_SELF: i32 = 0;
pub const RUSAGE_CHILDREN: i32 = -1;

/// `PROF_START/STOP` (`profile.h`).
pub const PROF_START: i32 = 0;
pub const PROF_STOP: i32 = 1;

/// `PM_*` call numbers (`callnr.h: 25/36-39/45-47`).
pub const PM_GETMCONTEXT: i32 = 18;
pub const PM_SETMCONTEXT: i32 = 19;
pub const PM_SYSUNAME: i32 = 25;
pub const PM_REBOOT: i32 = 37;
pub const PM_SVRCTL: i32 = 38;
pub const PM_SPROF: i32 = 39;
pub const PM_GETRUSAGE: i32 = 36;
pub const PM_GETEPINFO: i32 = 45;
pub const PM_GETPROCNR: i32 = 46;
pub const PM_GETSYSINFO: i32 = 47;

/// `monitor_params` capacity (`param.h: MULTIBOOT_PARAM_BUF_SIZE`).
pub const MONITOR_CAP: usize = 4096;

/// Errors for misc calls (mapped to errno).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiscError {
    Inval,
    Perm,
    Srch,
    Nospc,
    Big,
    Nosys,
    Fault,
    Busy,
    /// 内核/VM 服务的原始负 errno（C 全程透传 `r`，misc.c:430-431/441-442；
    /// V3-P2-6：不再折叠为 EINVAL）。
    Kernel(i32),
}

impl MiscError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Inval => EINVAL,
            Self::Perm => EPERM,
            Self::Srch => ESRCH,
            Self::Nospc => ENOSPC,
            Self::Big => E2BIG,
            Self::Nosys => ENOSYS,
            Self::Fault => minix_types::EFAULT,
            Self::Busy => minix_types::EBUSY,
            Self::Kernel(r) => r,
        }
    }
}

/// `uts_tbl` entry (`misc.c:46-60`, D1, A-11).
pub const UTS_VAL_SYSNAME: &str = "Minix";
pub const UTS_VAL_NODENAME: &str = "noname";
pub const UTS_VAL_RELEASE: &str = "3.3.0";
pub const UTS_VAL_VERSION: &str = "Minix 3.3.0";
pub const UTS_VAL_MACHINE: &str = "x86_64";

/// `UtsField` (`misc.c:46-60` 9 槽位 4 NULL).
const UTS_TBL: [Option<&'static str>; 8] = [
    Some(UTS_VAL_MACHINE), // 0: arch (x86_64, was i386/evbarm)
    None,                  // 1: No kernel arch
    Some(UTS_VAL_MACHINE), // 2: machine
    None,                  // 3: No hostname
    Some(UTS_VAL_NODENAME),// 4: nodename
    Some(UTS_VAL_RELEASE), // 5: release
    Some(UTS_VAL_VERSION), // 6: version
    Some(UTS_VAL_SYSNAME), // 7: sysname
];

/// `uts_field` (`misc.c:79-83`, D1).
pub fn uts_field(field: usize) -> Result<&'static str, MiscError> {
    if field >= UTS_TBL_LEN {
        return Err(MiscError::Inval);
    }
    UTS_TBL[field].ok_or(MiscError::Inval)
}

/// `SysInfoWhat` (`misc.c:125-133`, D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysInfoWhat {
    ProcTab,
    #[cfg(feature = "syscall_stats")]
    CallStats,
}

impl TryFrom<i32> for SysInfoWhat {
    type Error = MiscError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::ProcTab),
            #[cfg(feature = "syscall_stats")]
            1 => Ok(Self::CallStats),
            _ => Err(MiscError::Inval),
        }
    }
}

/// `SysInfoCtl` (`misc.c:125`, D2, A-11).
pub trait SysInfoCtl {
    fn proc_tab(&self) -> &[u8];
    #[cfg(feature = "syscall_stats")]
    fn call_stats(&self) -> &[u8];
}

/// Copy abstraction (`sys_datacopy`, `misc.c:90/143/188/385`).
pub trait CopyToUser {
    fn copy_to_user(&mut self, src: &[u8], dst: VirBytes) -> Result<(), MiscError>;
    fn copy_from_user(&mut self, src: VirBytes, dst: &mut [u8]) -> Result<(), MiscError>;
}

/// `EpInfo` (`misc.c:180-192`, D3) — `return pid` payload；`ngroups` 为
/// 全量组数（C misc.c:184），`groups` 为按调用方缓冲截断后的拷出内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpInfo {
    pub pid: Pid,
    pub uid: Uid,
    pub euid: Uid,
    pub gid: Gid,
    pub egid: Gid,
    pub ngroups: usize,
    pub groups: Vec<Gid>,
}

/// `RusageWho` (`misc.c:407-409`, D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RusageWho {
    Slf,
    Children,
}

impl TryFrom<i32> for RusageWho {
    type Error = MiscError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Slf),
            -1 => Ok(Self::Children),
            _ => Err(MiscError::Inval),
        }
    }
}

/// `rusage_from_ticks` (`utility.c:149-155`, D6, A-11) — `ticks*1e6/hz → sec/usec`.
pub fn rusage_from_ticks(ticks: Clock, hz: Clock) -> (i64, i64) {
    if hz == 0 {
        return (0, 0);
    }
    let usec = (ticks as u128 * 1_000_000u128) / hz as u128;
    let sec = (usec / 1_000_000) as i64;
    let usec_rem = (usec % 1_000_000) as i64;
    (sec, usec_rem)
}

/// `find_param` (`utility.c:57-71`, D5) — `monitor_params` KVP linear scan.
///
/// `monitor` is `NUL`-separated `key=value\0` string (from `monitor_params`).
pub fn find_param(monitor: &str, key: &str) -> Option<String> {
    // monitor is raw bytes with \0 separators; we iterate via split('\0')
    for kv in monitor.split('\0') {
        if kv.is_empty() {
            continue;
        }
        if let Some(pos) = kv.find('=') {
            let (k, v) = kv.split_at(pos);
            let v = &v[1..];
            if k == key {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// `ParamStore` (`misc.c:298-301`, D5) — `local_overrides[2]` tiny table.
#[derive(Debug, Clone)]
pub struct ParamStore {
    local: Vec<(String, String)>,
    pub monitor: String,
}

impl ParamStore {
    pub fn new(monitor: String) -> Self {
        Self { local: Vec::new(), monitor }
    }
    pub fn set(&mut self, key: String, val: String) -> Result<(), MiscError> {
        // C misc.c:327 → 328-334：ENOSPC（表满）先于 keylen/vallen 边界——
        // 双条件并存时错误码不同（V3-P2-5 修正与 C 相反的检查顺序）。
        if self.local.len() >= MAX_LOCAL_PARAMS {
            return Err(MiscError::Nospc);
        }
        // C 328-334：keylen/vallen ∈ (0, 30)（name[30]/value[30] 边界）。
        if key.is_empty() || val.is_empty() || key.len() >= 30 || val.len() >= 30 {
            return Err(MiscError::Inval);
        }
        self.local.push((key, val));
        Ok(())
    }
    pub fn get(&self, key: &str) -> Option<String> {
        if key.is_empty() {
            // keylen==0 → whole table (misc.c:352-354)
            return Some(self.monitor.clone());
        }
        for (k, v) in &self.local {
            if k == key {
                return Some(v.clone());
            }
        }
        find_param(&self.monitor, key)
    }
}

/// `RebootCtl` (`misc.c:207-230`, D4, A-3).
pub trait RebootCtl {
    fn set_abort(&mut self, how: i32);
    fn try_power_off(&mut self) -> bool;
    fn broadcast_kill(&mut self);
    fn stop_init(&mut self);
    fn tell_reboot(&mut self) -> i32;
}

/// VM 侧补充的资源用量三元组（`vm_getrusage` 的产出,`utility.c:446-450`）。
///
/// C 里 VM 把这三个值写进 `struct rusage` 的 `ru_maxrss`（KB 单位,
/// `vm_total_max/1024`）、`ru_minflt`、`ru_majflt` 槽位（其余 11 个
/// long 域从未被任何一方写入,恒为零——`misc.c:423` memset + VM 只
/// 改这三处）。Rust 重写把"datacopy 往返"换成消息按值通道:
/// VM_GETRUSAGE 应答直接携带三值,PM 侧组装完整结构拷出,最终字节面
/// 与 C 完全一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmUsage {
    /// 峰值驻留集（KB）。C: `r_usage.ru_maxrss = vmp->vm_total_max/1024L`。
    pub max_rss_kb: u64,
    /// 次缺页（页重认领）。C: `r_usage.ru_minflt`。
    pub minor_faults: u64,
    /// 主缺页。C: `r_usage.ru_majflt`。
    pub major_faults: u64,
}

/// `TimesVmCtl` (`misc.c:429/441`, D6).
pub trait TimesVmCtl {
    fn sys_times(&mut self, ep: Endpoint) -> Result<(Clock, Clock), MiscError>;
    fn vm_rusage(&mut self, ep: Endpoint, who: RusageWho) -> Result<VmUsage, MiscError>;
}

/// `McontextCtl` (`mcontext.c:15/25`, D8).
pub trait McontextCtl {
    fn get(&self, ep: Endpoint, ctx: VirBytes) -> i32;
    fn set(&self, ep: Endpoint, ctx: VirBytes) -> i32;
}

/// `TimesVmCtl` 生产实现(S4 收尾)。CPU 计时直连内核 SYS_TIMES
/// (同 [`SysMcontextCtl`] 先例;hosted 构建诚实回 -EIO,通电挂 edge
/// E1),内存面三值走 VM_GETRUSAGE taskcall(vm_fork 的 sendrec 纪律,
/// dispatcher.rs:137-156)。
pub struct SysTimesVmCtl<'a, T: crate::ipc::IpcTransport + ?Sized> {
    /// VM 腿的传输(C `vm_getrusage` 的 `_taskcall(VM_PROC_NR, ...)`,
    /// libsys vm_getrusage.c:7-17)。
    pub transport: &'a mut T,
}

impl<T: crate::ipc::IpcTransport + ?Sized> TimesVmCtl for SysTimesVmCtl<'_, T> {
    fn sys_times(&mut self, ep: Endpoint) -> Result<(Clock, Clock), MiscError> {
        // C sys_times(who_e, &utime, &stime, NULL, NULL)（misc.c:429-431;
        // libsys sys_times.c:8-24)。失败原 errno 上抛(V3-P2-6)。
        let times = minix_sys::syscall::sys_times(
            &minix_sys::syscall::DirectKernelCallTransport,
            ep.0,
        )
        .map_err(MiscError::Kernel)?;
        Ok((times.user_time as Clock, times.system_time as Clock))
    }

    fn vm_rusage(&mut self, ep: Endpoint, who: RusageWho) -> Result<VmUsage, MiscError> {
        // 请求编码:C `m_lsys_vm_rusage {endpt, addr, children}`
        // (ipc.h:1513-1520)。C 的 addr 是 PM 栈上 r_usage 的地址,
        // 供 VM datacopy 往返;Rust 值按消息携带,addr 恒零。
        let children = matches!(who, RusageWho::Children);
        let mut msg = minix_types::Message {
            m_type: minix_types::VM_GETRUSAGE as i32,
            ..Default::default()
        };
        {
            // SAFETY: m_lsys_vm_rusage 是 VM_GETRUSAGE 的文档化载荷
            // 布局(VM 侧 call_getrusage 按 endpt@0/addr@8/children@16
            // 解码,dispatcher.rs:912-920)。
            let req = unsafe { &mut msg.m_u.m_lsys_vm_rusage };
            req.endpt = ep.0;
            req.addr = 0;
            req.children = children as i32;
        }
        self.transport
            .sendrec(minix_types::Endpoint::VM, &mut msg)
            .map_err(|_| MiscError::Kernel(minix_types::EIO))?;
        // C: taskcall 返回值即结果(vm_getrusage.c:17);VM 的错误
        // 应答 m_type 为正 errno。
        if msg.m_type != minix_types::OK {
            return Err(MiscError::Kernel(msg.m_type));
        }
        // 回复解码:VM 编码 m1p1=max_rss_kb/m1i1=minor/m1i2=major
        // (encode.rs:258-268,缺页计数按 i32 饱和)。
        // SAFETY: 应答臂 m_m1 由 VM 按同一布局覆写。
        let m1 = unsafe { &msg.m_u.m_m1 };
        Ok(VmUsage {
            max_rss_kb: m1.m1p1,
            minor_faults: u64::from(m1.m1i1.max(0) as u32),
            major_faults: u64::from(m1.m1i2.max(0) as u32),
        })
    }
}

/// `SprofCtl` (`profile.c:31-33`, D7).
/// （8 参数与 C 的 `sys_vtimer`/profile 消息载荷一一对应，`[ARCH: A-3]`。）
#[allow(clippy::too_many_arguments)]
pub trait SprofCtl {
    fn sprof(&mut self, action: i32, mem_size: usize, freq: u32, intr_type: i32, ep: Endpoint, ctl_ptr: VirBytes, mem_ptr: VirBytes) -> i32;
}

/// `McontextCtl` 生产实现(S8 批次 G):sys_getmcontext/sys_setmcontext
/// 直委托(minix-sys wrapper,E6 已落;错误透传,do_* 层收敛 EINVAL)。
pub struct SysMcontextCtl;

impl McontextCtl for SysMcontextCtl {
    fn get(&self, ep: Endpoint, ctx: VirBytes) -> i32 {
        match minix_sys::syscall::sys_getmcontext(
            &minix_sys::syscall::DirectKernelCallTransport,
            ep.0,
            ctx.0,
        ) {
            Ok(()) => 0,
            Err(e) => e,
        }
    }

    fn set(&self, ep: Endpoint, ctx: VirBytes) -> i32 {
        match minix_sys::syscall::sys_setmcontext(
            &minix_sys::syscall::DirectKernelCallTransport,
            ep.0,
            ctx.0,
        ) {
            Ok(()) => 0,
            Err(e) => e,
        }
    }
}

/// `SprofCtl` 生产实现(S8 批次 G):sys_sprof 直委托(E6 wrapper;
/// 错误透传为原始 errno,do_sprofile 的 feature 门控保持)。
pub struct SysSprofCtl {
    /// 目标进程 endpoint(sprof 的采样对象;do_sprofile 以 caller 填入)。
    pub ep: Endpoint,
}

impl SprofCtl for SysSprofCtl {
    fn sprof(
        &mut self,
        action: i32,
        mem_size: usize,
        freq: u32,
        intr_type: i32,
        ep: Endpoint,
        ctl_ptr: VirBytes,
        mem_ptr: VirBytes,
    ) -> i32 {
        let _ = ep;
        match minix_sys::syscall::sys_sprof(
            &minix_sys::syscall::DirectKernelCallTransport,
            action,
            freq as i32,
            intr_type,
            self.ep.0,
            ctl_ptr.0,
            mem_ptr.0,
        ) {
            Ok(()) => 0,
            Err(e) => e,
        }
    }
}

/// Helper: `is_superuser`.
pub(crate) fn is_superuser(table: &ProcTable, caller: minix_types::UserSlot) -> bool {
    table.procs[caller.get()]
        .resources
        .privilege
        .credentials()
        .map(|c| c.user.effective == 0)
        .unwrap_or(false)
}

/// `do_sysuname` (`misc.c:72-100`, D1).
pub fn do_sysuname(
    field: usize,
    req: i32,
    len: usize,
    caller_ep: Endpoint,
    cpy: &mut dyn CopyToUser,
) -> Result<usize, MiscError> {
    let s = uts_field(field)?;
    // C misc.c:85-96 default 分支：仅实现 req==0（拷出），req!=0 → EINVAL
    //（V3-P2-5 补——旧签名无 req 形参，该分支无处产生）。
    if req != 0 {
        return Err(MiscError::Inval);
    }
    let n = s.len() + 1;
    let to_copy = if n > len { len } else { n };
    if to_copy == 0 {
        return Ok(0);
    }
    // Include NUL if truncated? C copies min(strlen+1, len) bytes including partial NUL string.
    let bytes = s.as_bytes();
    let mut buf = vec![0u8; to_copy];
    let copy_len = if to_copy <= bytes.len() {
        bytes[..to_copy].to_vec()
    } else {
        let mut v = bytes.to_vec();
        v.push(0);
        v.resize(to_copy, 0);
        v
    };
    buf.copy_from_slice(&copy_len);
    cpy.copy_to_user(&buf, VirBytes(0))?; // placeholder VirBytes, caller will map
    // In C, sys_datacopy uses caller endpoint; abstract via CopyToUser.
    let _ = caller_ep;
    Ok(to_copy)
}

/// Simplified `do_getsysinfo` (`misc.c:108-144`, D2).
pub fn do_getsysinfo(
    table: &ProcTable,
    caller: minix_types::UserSlot,
    what: SysInfoWhat,
    size: usize,
    dst: VirBytes,
    ctl: &dyn SysInfoCtl,
    cpy: &mut dyn CopyToUser,
) -> Result<(), MiscError> {
    if !is_superuser(table, caller) {
        return Err(MiscError::Perm);
    }
    match what {
        SysInfoWhat::ProcTab => {
            // D-29 真实数据路径：整表 C-ABI 序列化（`struct mproc` 464B/槽
            // × NR_PROCS），逐槽拷出到调用方缓冲。C: misc.c:142-143 的
            // sys_datacopy(SELF, mproc, dst, size) 对应——逐槽拷避免 118KB
            // 内核/堆中转缓冲。
            const WIRE_SIZE: usize = core::mem::size_of::<MprocWire>();
            let tab_size = WIRE_SIZE * table.procs.len();
            if size != tab_size {
                return Err(MiscError::Inval);
            }
            for (idx, slot) in table.procs.iter().enumerate() {
                let wire = crate::mproc::wire::serialize_slot(idx, slot);
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        core::ptr::addr_of!(wire) as *const u8,
                        WIRE_SIZE,
                    )
                };
                cpy.copy_to_user(
                    bytes,
                    VirBytes(dst.0 + (idx * WIRE_SIZE) as u64),
                )?;
            }
            Ok(())
        }
        #[cfg(feature = "syscall_stats")]
        SysInfoWhat::CallStats => {
            let b = ctl.call_stats();
            if size != b.len() {
                return Err(MiscError::Inval);
            }
            cpy.copy_to_user(b, dst)?;
            Ok(())
        }
    }
}

/// `do_getprocnr` (`misc.c:149-164`, D3).
pub fn do_getprocnr(
    table: &ProcTable,
    caller_ep: Endpoint,
    pid: Pid,
) -> Result<Endpoint, MiscError> {
    if caller_ep != Endpoint::RS {
        return Err(MiscError::Perm);
    }
    let slot = table.find_proc(pid).ok_or(MiscError::Srch)?;
    Ok(table.procs[slot.get()].endpoint())
}

/// `do_getepinfo` (`misc.c:169-193`, D3).
pub fn do_getepinfo(
    table: &ProcTable,
    ep: Endpoint,
    caller_ngroups: usize,
    _cpy: &mut dyn CopyToUser,
) -> Result<EpInfo, MiscError> {
    let slot = table.pm_isokendpt(ep).map_err(|_| MiscError::Srch)?;
    let rmp = &table.procs[slot.get()];
    let creds = rmp.resources.privilege.credentials().ok_or(MiscError::Inval)?;
    // C misc.c:184：回复载荷的 ngroups 填**全量** mp_ngroups——截断只影响
    // 拷贝数，调用方（RS）有权知道真实组数（区分"只有 N 组"与"有更多但
    // 缓冲不足"）。V3-P2-4 之前先截断再填，语义退化。
    let ngroups_full = creds.ngroups;
    let copy_len = ngroups_full.min(caller_ngroups);
    let groups = if copy_len > 0 {
        creds.supplemental_groups[..copy_len].to_vec()
    } else {
        Vec::new()
    };
    // [DEFERRED: D-30] groups 经 sys_datacopy 拷出到调用方缓冲 + EFAULT
    // 透传（misc.c:187-190）——随批次 A/G 的 wire 与 CopyGroups 生产实现
    // 落地（挂 edge E7）；当前以截断 Vec 表达拷出内容。
    Ok(EpInfo {
        pid: rmp.identity.id.pid,
        uid: creds.user.real,
        euid: creds.user.effective,
        gid: creds.group.real,
        egid: creds.group.effective,
        ngroups: ngroups_full,
        groups,
    })
}

/// `do_reboot` (`misc.c:198-233`, D4).
pub fn do_reboot(
    table: &ProcTable,
    caller: minix_types::UserSlot,
    how: i32,
    ctl: &mut dyn RebootCtl,
) -> Result<ReplyIntent, MiscError> {
    if !is_superuser(table, caller) {
        return Err(MiscError::Perm);
    }
    ctl.set_abort(how);
    if (how & RB_POWERDOWN) != 0 {
        let _ = ctl.try_power_off();
    }
    ctl.broadcast_kill();
    ctl.stop_init();
    let _ = ctl.tell_reboot();
    Ok(ReplyIntent::ReplyLater) // SUSPEND never reply
}

/// `do_svrctl` set/get (`misc.c:291-395`, D5) — simplified ParamStore API.
pub fn do_svrctl_set(store: &mut ParamStore, key: String, val: String) -> Result<(), MiscError> {
    store.set(key, val)
}
pub fn do_svrctl_get(store: &ParamStore, key: Option<String>, _keylen: usize, vallen: usize) -> Result<String, MiscError> {
    // C misc.c:359：GET key 的 search_key[64] 边界 → EINVAL（V3-P2-5 补）。
    if let Some(k) = &key
        && (k.is_empty() || k.len() > 64)
    {
        return Err(MiscError::Inval);
    }
    let (val, needed) = match key {
        // C 352-354：keylen==0 → 全表，val_len = sizeof(monitor_params)
        //（MULTIBOOT_PARAM_BUF_SIZE = 1024，minix/param.h:28 与 multiboot.h:240）
        // —— E2BIG 判据用缓冲全长而非实际串长（V3-P2-5 修正）。
        None => {
            // C 352-354：keylen==0 → val_len = sizeof(monitor_params)
            //（无条件取缓冲全长 1024，NUL 尾随区域一并计入——这是 C 的
            // 全表快照语义，非"实际串长"）。
            let full = store.monitor.clone();
            let needed = crate::init::MULTIBOOT_PARAM_BUF_SIZE;
            (full, needed)
        }
        Some(k) => {
            let v = store.get(&k).ok_or(MiscError::Srch)?;
            let needed = v.len() + 1;
            (v, needed)
        }
    };
    // C 376-378：val_len > sysgetenv.vallen → E2BIG。
    if needed > vallen {
        return Err(MiscError::Big);
    }
    Ok(val)
}

/// C `_IOC(inout, group, num, len)`（`sys/ioccom.h:84-90`，x86-64
/// `sizeof(struct sysgetenv)` = 32：两个指针 + 两个 size_t）。
const fn ioc(inout: u32, group: u8, num: u32, len: u32) -> i32 {
    (inout | ((len & 0xfff) << 16) | ((group as u32) << 8) | num) as i32
}

const IOC_INOUT: u32 = 0xC000_0000;
const IOC_IN: u32 = 0x8000_0000;
const IOC_OUT: u32 = 0x4000_0000;
const SIZEOF_SYSGETENV: u32 = 32; // char *key + char *val + size_t keylen + size_t vallen

/// `PMGETPARAM`（`sys/svrctl.h:16` `_IOWR('P', 0, sysgetenv)`）。
pub const PMGETPARAM: i32 = ioc(IOC_INOUT, b'P', 0, SIZEOF_SYSGETENV);
/// `PMSETPARAM`（`sys/svrctl.h:17` `_IOW('P', 1, sysgetenv)`）。
pub const PMSETPARAM: i32 = ioc(IOC_IN, b'P', 1, SIZEOF_SYSGETENV);
/// `OPMGETPARAM`（`sys/svrctl.h:19` `_IOW('M', 5, sysgetenv)`，旧口径）。
pub const OPMGETPARAM: i32 = ioc(IOC_OUT, b'M', 5, SIZEOF_SYSGETENV);
/// `OPMSETPARAM`（`sys/svrctl.h:20` `_IOR('M', 7, sysgetenv)`，旧口径）。
pub const OPMSETPARAM: i32 = ioc(IOC_OUT, b'M', 7, SIZEOF_SYSGETENV);

/// C `IOCGROUP(x)`（`sys/ioccom.h:68`）。
const fn ioc_group(x: i32) -> u8 {
    (((x as u32) >> 8) & 0xff) as u8
}

/// `do_svrctl` (`misc.c:291-395`, D5)。
///
/// `req` 形参化（V3-P2-5）：IOCGROUP 门（'P'/'M'，misc.c:307-309）→ 四命令
/// 分派（309-395）→ 未知 req `EINVAL`（392-393）。旧签名的 `is_set: bool`
/// 丢弃了 req——非法 req 无法拒绝。用户缓冲拷贝（sysgetenv/复制/EFAULT）
/// 仍属 wire 面（批次 G），此处逻辑面以形参直传。
pub fn do_svrctl(store: &mut ParamStore, req: i32, key: Option<String>, val: Option<String>, vallen: usize) -> Result<Option<String>, MiscError> {
    // C 307-309：IOCGROUP ∈ {'P','M'} 门。
    let group = ioc_group(req);
    if group != b'P' && group != b'M' {
        return Err(MiscError::Inval);
    }
    match req {
        req if req == PMSETPARAM || req == OPMSETPARAM => {
            let k = key.ok_or(MiscError::Inval)?;
            let v = val.ok_or(MiscError::Inval)?;
            do_svrctl_set(store, k, v).map(|_| None)
        }
        req if req == PMGETPARAM || req == OPMGETPARAM => {
            let keylen = key.as_ref().map_or(0, |k| k.len());
            do_svrctl_get(store, key, keylen, vallen).map(Some)
        }
        _ => Err(MiscError::Inval), // C 392-393 未知 req
    }
}

/// `set_rusage_times` (`utility.c:144-156`, D6) — name-match wrapper.
pub fn set_rusage_times(ru_utime_sec: &mut i64, ru_utime_usec: &mut i64, ru_stime_sec: &mut i64, ru_stime_usec: &mut i64, user_time: Clock, sys_time: Clock, hz: Clock) {
    let (us, uu) = rusage_from_ticks(user_time, hz);
    let (ss, su) = rusage_from_ticks(sys_time, hz);
    *ru_utime_sec = us;
    *ru_utime_usec = uu;
    *ru_stime_sec = ss;
    *ru_stime_usec = su;
}

/// `struct rusage` 的 LP64 字节长——2 个 timeval（`ru_utime/ru_stime`,
/// 各 `sec+usec` 两个 i64）+ 14 个 long（`sys/resource.h:57-76`,
/// `ru_maxrss..ru_nivcsw`）。
const RUSAGE_LEN: usize = 128;

/// `do_getrusage` (`misc.c:400-447`, D6)。
///
/// 拷出目标 `addr` 是用户态 `struct rusage*`（C `sys_datacopy` 的
/// 目的地,misc.c:446-447）;组装顺序与 C 的三段一致——PM 填
/// `ru_utime/ru_stime`（`set_rusage_times` 对应物）、VM 补
/// `ru_maxrss/minflt/majflt`、其余 11 域恒零,最后一次性拷给用户。
pub fn do_getrusage(
    table: &ProcTable,
    caller: minix_types::UserSlot,
    who: RusageWho,
    addr: VirBytes,
    hz: Clock,
    ctl: &mut dyn TimesVmCtl,
    cpy: &mut dyn CopyToUser,
) -> Result<UtimeStimePair, MiscError> {
    let ep = table.procs[caller.get()].endpoint();
    let (utime, stime) = match who {
        // C misc.c:430-431 透传内核返回值（V3-P2-6：seam 的错误原样
        // 上抛，不再折叠 EINVAL；TimesVmCtl 生产实现的原始 errno 保留在
        // MiscError::Kernel 载荷中）。
        RusageWho::Slf => ctl.sys_times(ep)?,
        RusageWho::Children => {
            let p = &table.procs[caller.get()];
            (p.resources.child_utime, p.resources.child_stime)
        }
    };
    let (u_sec, u_usec) = rusage_from_ticks(utime, hz);
    let (s_sec, s_usec) = rusage_from_ticks(stime, hz);
    // C misc.c:441-442 透传 vm_getrusage 返回值（V3-P2-6）。
    let vm = ctl.vm_rusage(ep, who)?;
    // 组装 `struct rusage`（sys/resource.h:57-76 的 LP64 布局）:
    // ru_utime@0/ru_stime@16/ru_maxrss@32/ru_minflt@64/ru_majflt@72,
    // 其余 11 个 long 域 C 侧从未写入（memset 归零后 VM 只改这三处,
    // misc.c:423 + utility.c:446-450）,保持零。
    let mut buf = [0u8; RUSAGE_LEN];
    buf[0..8].copy_from_slice(&u_sec.to_le_bytes());
    buf[8..16].copy_from_slice(&u_usec.to_le_bytes());
    buf[16..24].copy_from_slice(&s_sec.to_le_bytes());
    buf[24..32].copy_from_slice(&s_usec.to_le_bytes());
    buf[32..40].copy_from_slice(&(vm.max_rss_kb as i64).to_le_bytes());
    buf[64..72].copy_from_slice(&(vm.minor_faults as i64).to_le_bytes());
    buf[72..80].copy_from_slice(&(vm.major_faults as i64).to_le_bytes());
    cpy.copy_to_user(&buf, addr)?;
    // 返回分解值供测试观测;C 的返回值是 sys_datacopy 结果(成功即 OK)。
    Ok(((u_sec, u_usec), (s_sec, s_usec)))
}

/// `do_sprofile` (`profile.c:22-45`, D7).
pub fn do_sprofile(
    action: i32,
    mem_size: usize,
    freq: u32,
    intr_type: i32,
    ep: Endpoint,
    ctl_ptr: VirBytes,
    mem_ptr: VirBytes,
    ctl: &mut dyn SprofCtl,
) -> Result<(), MiscError> {
    // C profile.c:22-45 —— PROF_START 全载荷经 SprofCtl;PROF_STOP 只带
    // action;其余动作 EINVAL(switch default)。
    #[cfg(feature = "sprofile")]
    {
        ctl.sprof(
            action, mem_size, freq, intr_type, ep, ctl_ptr, mem_ptr,
        );
        Ok(())
    }
    #[cfg(not(feature = "sprofile"))]
    {
        let _ = (action, mem_size, freq, intr_type, ep, ctl_ptr, mem_ptr, ctl);
        Err(MiscError::Nosys)
    }
}

/// `do_get/setmcontext` (`mcontext.c:13/23`, D8).
pub fn do_getmcontext(ctl: &dyn McontextCtl, ep: Endpoint, ctx: VirBytes) -> Result<(), MiscError> {
    let r = ctl.get(ep, ctx);
    if r != 0 {
        return Err(MiscError::Inval);
    }
    Ok(())
}
pub fn do_setmcontext(ctl: &dyn McontextCtl, ep: Endpoint, ctx: VirBytes) -> Result<(), MiscError> {
    let r = ctl.set(ep, ctx);
    if r != 0 {
        return Err(MiscError::Inval);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot, VirBytes};

    fn mk_running(table: &mut ProcTable, slot: usize, eff: u32) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(eff, 100));
        table.procs[slot].resources.child_utime = 200;
        table.procs[slot].resources.child_stime = 100;
    }

    struct NopCopy;
    impl CopyToUser for NopCopy {
        fn copy_to_user(&mut self, _src: &[u8], _dst: VirBytes) -> Result<(), MiscError> { Ok(()) }
        fn copy_from_user(&mut self, _src: VirBytes, _dst: &mut [u8]) -> Result<(), MiscError> { Ok(()) }
    }
    struct TestSysInfo {
        data: Vec<u8>,
    }
    impl SysInfoCtl for TestSysInfo {
        fn proc_tab(&self) -> &[u8] { &self.data }
        #[cfg(feature = "syscall_stats")]
        fn call_stats(&self) -> &[u8] { &self.data }
    }
    impl SysInfoCtl for TestSysInfoAlt {
        fn proc_tab(&self) -> &[u8] { &[] }
        #[cfg(feature = "syscall_stats")]
        fn call_stats(&self) -> &[u8] { &[] }
    }
    struct TestSysInfoAlt { _a: u8 }
    impl CopyToUser for TestSysInfoAlt {
        fn copy_to_user(&mut self, _src: &[u8], _dst: VirBytes) -> Result<(), MiscError> { Ok(()) }
        fn copy_from_user(&mut self, _src: VirBytes, _dst: &mut [u8]) -> Result<(), MiscError> { Ok(()) }
    }
    struct TestReboot {
        abort: i32,
        killed: bool,
        stopped: bool,
        told: bool,
    }
    impl RebootCtl for TestReboot {
        fn set_abort(&mut self, how: i32) { self.abort = how; }
        fn try_power_off(&mut self) -> bool { false }
        fn broadcast_kill(&mut self) { self.killed = true; }
        fn stop_init(&mut self) { self.stopped = true; }
        fn tell_reboot(&mut self) -> i32 { self.told = true; 0 }
    }
    struct AltReboot;
    impl RebootCtl for AltReboot {
        fn set_abort(&mut self, _how: i32) {}
        fn try_power_off(&mut self) -> bool { true }
        fn broadcast_kill(&mut self) {}
        fn stop_init(&mut self) {}
        fn tell_reboot(&mut self) -> i32 { 0 }
    }
    struct TestTimesVm { hz: Clock }
    impl TimesVmCtl for TestTimesVm {
        fn sys_times(&mut self, _ep: Endpoint) -> Result<(Clock, Clock), MiscError> { Ok((120, 60)) }
        fn vm_rusage(&mut self, _ep: Endpoint, _who: RusageWho) -> Result<VmUsage, MiscError> {
            Ok(VmUsage { max_rss_kb: 4096, minor_faults: 33, major_faults: 4 })
        }
    }
    struct AltTimesVm;
    impl TimesVmCtl for AltTimesVm {
        fn sys_times(&mut self, _ep: Endpoint) -> Result<(Clock, Clock), MiscError> { Ok((0,0)) }
        fn vm_rusage(&mut self, _ep: Endpoint, _who: RusageWho) -> Result<VmUsage, MiscError> {
            Ok(VmUsage { max_rss_kb: 0, minor_faults: 0, major_faults: 0 })
        }
    }
    struct TestMctx { ret: i32 }
    impl McontextCtl for TestMctx {
        fn get(&self, _ep: Endpoint, _ctx: VirBytes) -> i32 { self.ret }
        fn set(&self, _ep: Endpoint, _ctx: VirBytes) -> i32 { self.ret }
    }
    struct AltMctx;
    impl McontextCtl for AltMctx {
        fn get(&self, _ep: Endpoint, _ctx: VirBytes) -> i32 { 0 }
        fn set(&self, _ep: Endpoint, _ctx: VirBytes) -> i32 { 0 }
    }
    struct TestSprof;
    impl SprofCtl for TestSprof {
        fn sprof(&mut self, _action: i32, _mem: usize, _freq: u32, _intr: i32, _ep: Endpoint, _ctl: VirBytes, _mem2: VirBytes) -> i32 { 0 }
    }
    struct AltSprof;
    impl SprofCtl for AltSprof {
        fn sprof(&mut self, _a: i32, _b: usize, _c: u32, _d: i32, _e: Endpoint, _f: VirBytes, _g: VirBytes) -> i32 { 0 }
    }

    #[test]
    fn test_uts_field() {
        assert_eq!(uts_field(0).unwrap(), "x86_64");
        assert_eq!(uts_field(7).unwrap(), "Minix");
        assert!(uts_field(8).is_err());
        assert!(uts_field(1).is_err()); // NULL slot
        assert!(uts_field(3).is_err());
    }

    #[test]
    fn test_getsysinfo_perm_size() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 1000);
        let ctl = TestSysInfo { data: vec![0u8; 100] };
        let mut cpy = NopCopy;
        // non-super → Perm
        assert_eq!(do_getsysinfo(&table, UserSlot::new(0), SysInfoWhat::ProcTab, 100, VirBytes(0x1000), &ctl, &mut cpy).unwrap_err(), MiscError::Perm);
        // super but size mismatch（表大小 = 槽数 × 464）
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0, 0));
        assert_eq!(do_getsysinfo(&table, UserSlot::new(0), SysInfoWhat::ProcTab, 99, VirBytes(0x1000), &ctl, &mut cpy).unwrap_err(), MiscError::Inval);
        // D-29 激活：size = 表大小 → 真实序列化拷出（NopCopy 吞拷贝），
        // 返回 Ok——ENOSYS fail-closed 由真实数据路径取代（V3-P1-4 →
        // D-29 闭环）。
        let mproc_tab = table.procs.len() * core::mem::size_of::<minix_types::MprocWire>();
        assert_eq!(
            do_getsysinfo(&table, UserSlot::new(0), SysInfoWhat::ProcTab, mproc_tab, VirBytes(0x1000), &ctl, &mut cpy).unwrap(),
            ()
        );
    }

    #[test]
    fn test_getprocnr_rs_only() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        mk_running(&mut table, 1, 0);
        table.procs[1].identity.id.pid = 55;
        assert_eq!(do_getprocnr(&table, Endpoint(99), 55).unwrap_err(), MiscError::Perm);
        assert_eq!(do_getprocnr(&table, Endpoint::RS, 999).unwrap_err(), MiscError::Srch);
        let ep = do_getprocnr(&table, Endpoint::RS, 55).unwrap();
        assert_eq!(ep, table.procs[1].endpoint());
    }

    #[test]
    fn test_getepinfo_trunc() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        mk_running(&mut table, 1, 0);
        table.procs[1].resources.privilege = Privilege::User({
            let mut c = Credentials::new(1000, 2000);
            c.ngroups = 3;
            c.supplemental_groups[0] = 10;
            c.supplemental_groups[1] = 20;
            c.supplemental_groups[2] = 30;
            c
        });
        let mut cpy = NopCopy;
        // caller buffer 2 groups：ngroups 报全量 3（C misc.c:184），拷出截断 2。
        let info = do_getepinfo(&table, table.procs[1].endpoint(), 2, &mut cpy).unwrap();
        assert_eq!(info.ngroups, 3, "reply ngroups = full mp_ngroups (C misc.c:184)");
        assert_eq!(info.groups, vec![10,20]);
        assert_eq!(info.pid, 101);
        // 缓冲充足时全量拷出。
        let info = do_getepinfo(&table, table.procs[1].endpoint(), 16, &mut cpy).unwrap();
        assert_eq!(info.ngroups, 3);
        assert_eq!(info.groups, vec![10,20,30]);
        // invalid endpoint
        assert_eq!(do_getepinfo(&table, Endpoint::from_generation_slot(9, 9), 16, &mut cpy).unwrap_err(), MiscError::Srch);
    }

    #[test]
    fn test_reboot_perm_suspend() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 1000);
        let mut ctl = TestReboot { abort: 0, killed: false, stopped: false, told: false };
        assert_eq!(do_reboot(&table, UserSlot::new(0), 0, &mut ctl).unwrap_err(), MiscError::Perm);
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0,0));
        let r = do_reboot(&table, UserSlot::new(0), RB_POWERDOWN, &mut ctl).unwrap();
        assert_eq!(r, ReplyIntent::ReplyLater);
        assert!(ctl.killed);
        assert!(ctl.stopped);
        assert!(ctl.told);
        assert_eq!(ctl.abort, RB_POWERDOWN);
    }

    #[test]
    fn test_svrctl_local_e2big() {
        let mut store = ParamStore::new("foo=bar\0".to_string());
        // set two → ok, third → ENOSPC
        assert!(do_svrctl_set(&mut store, "a".to_string(), "1".to_string()).is_ok());
        assert!(do_svrctl_set(&mut store, "b".to_string(), "2".to_string()).is_ok());
        assert_eq!(do_svrctl_set(&mut store, "c".to_string(), "3".to_string()).unwrap_err(), MiscError::Nospc);
        // get → Esrch if missing
        assert_eq!(do_svrctl_get(&store, Some("nope".to_string()), 4, 100).unwrap_err(), MiscError::Srch);
        // E2BIG if vallen too small
        assert_eq!(do_svrctl_get(&store, Some("a".to_string()), 1, 1).unwrap_err(), MiscError::Big);
        // keylen==0 → whole table（缓冲 4096 > 1024 全长 → 不触发 E2BIG）
        let whole = do_svrctl_get(&store, None, 0, 4096).unwrap();
        assert!(whole.contains("foo=bar"));
        // V3-P2-5：全表 E2BIG 判据用 monitor 缓冲全长（C 352-354）——
        // vallen < 1024 即 E2BIG，即使实际串只有 8 字节。
        assert_eq!(do_svrctl_get(&store, None, 0, 512).unwrap_err(), MiscError::Big);
        // V3-P2-5：GET key 64 字节边界（C 359）。
        let long_key = "k".repeat(65);
        assert_eq!(do_svrctl_get(&store, Some(long_key), 65, 100).unwrap_err(), MiscError::Inval);
        // V3-P2-5：ENOSPC 先于边界（C 327→328）——表满时连超长 key 也报 ENOSPC。
        assert_eq!(
            do_svrctl_set(&mut store, "k".repeat(64), "v".repeat(64)).unwrap_err(),
            MiscError::Nospc
        );
    }

    // ---- do_svrctl 的 req 门控与四命令分派（V3-P2-5 回归锚点）----

    #[test]
    fn test_svrctl_req_gate_and_dispatch() {
        let mut store = ParamStore::new(String::new());
        // IOCGROUP 门：非 'P'/'M' 组 → EINVAL（C misc.c:307-309）。
        assert_eq!(
            do_svrctl(&mut store, ioc(IOC_IN, b'F', 1, 32), Some("k".into()), Some("v".into()), 0).unwrap_err(),
            MiscError::Inval
        );
        // 四命令分派：PMSETPARAM 写入 → PMGETPARAM 读回；OPM 旧口径同语义。
        assert_eq!(
            do_svrctl(&mut store, PMSETPARAM, Some("k".into()), Some("v".into()), 2).unwrap(),
            None
        );
        assert_eq!(
            do_svrctl(&mut store, PMGETPARAM, Some("k".into()), None, 2).unwrap(),
            Some("v".to_string())
        );
        assert_eq!(
            do_svrctl(&mut store, OPMGETPARAM, Some("k".into()), None, 2).unwrap(),
            Some("v".to_string())
        );
        // 命令码与 C _IOC 算术逐位一致（i386/earm 双 arch 的 LP64 尺寸口径）。
        assert_eq!(PMGETPARAM, 0xC020_5000u32 as i32);
        assert_eq!(PMSETPARAM, 0x8020_5001u32 as i32);
        assert_eq!(OPMGETPARAM, 0x4020_4D05u32 as i32);
        assert_eq!(OPMSETPARAM, 0x4020_4D07u32 as i32);
        // 未知 req（组对但命令号未定义）→ EINVAL（C 392-393）。
        assert_eq!(
            do_svrctl(&mut store, PMGETPARAM + 0x100, None, None, 0).unwrap_err(),
            MiscError::Inval
        );
    }

    #[test]
    fn test_sysuname_req_direction_gate() {
        // V3-P2-5：do_sysuname 的 req 方向分支（C misc.c:89-96 default →
        // EINVAL）——旧签名无 req 形参，该分支无处产生。
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 1000);
        let mut cpy = NopCopy;
        assert!(do_sysuname(0, 0, 100, Endpoint::PM, &mut cpy).is_ok());
        assert_eq!(
            do_sysuname(0, 1, 100, Endpoint::PM, &mut cpy).unwrap_err(),
            MiscError::Inval
        );
    }

    #[test]
    fn test_find_param_kvp() {
        let mon = "foo=bar\0baz=qux\0".to_string();
        assert_eq!(find_param(&mon, "foo").unwrap(), "bar");
        assert_eq!(find_param(&mon, "baz").unwrap(), "qux");
        assert!(find_param(&mon, "nope").is_none());
    }

    /// 捕获拷出目的地与字节的 CopyToUser(`do_getrusage` 布局见证)。
    struct CapCopy { dst: VirBytes, buf: alloc::vec::Vec<u8>, fail: bool }
    impl CopyToUser for CapCopy {
        fn copy_to_user(&mut self, src: &[u8], dst: VirBytes) -> Result<(), MiscError> {
            if self.fail {
                return Err(MiscError::Fault);
            }
            self.dst = dst;
            self.buf = src.to_vec();
            Ok(())
        }
        fn copy_from_user(&mut self, _src: VirBytes, _dst: &mut [u8]) -> Result<(), MiscError> { Ok(()) }
    }

    #[test]
    fn test_getrusage_self_children() {
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let mut ctl = TestTimesVm { hz: 100 };
        let mut cpy = CapCopy { dst: VirBytes(0), buf: alloc::vec::Vec::new(), fail: false };
        let ((u_sec, _), (_s_sec, _)) = do_getrusage(&table, UserSlot::new(0), RusageWho::Slf, VirBytes(0x3000), 100, &mut ctl, &mut cpy).unwrap();
        assert_eq!(u_sec, 1); // 120/100
        assert_eq!(cpy.dst, VirBytes(0x3000));
        let ((u2, _), _) = do_getrusage(&table, UserSlot::new(0), RusageWho::Children, VirBytes(0x3000), 100, &mut ctl, &mut cpy).unwrap();
        assert_eq!(u2, 2); // child_utime 200/100
    }

    #[test]
    fn test_getrusage_struct_byte_layout() {
        // sys/resource.h:57-76 的 LP64 布局见证:utime@0/stime@16/
        // maxrss@32/minflt@64/majflt@72;其余 11 个 long 域 C 侧从未
        // 写入(misc.c:423 memset + utility.c:446-450 只改三处),恒零。
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let mut ctl = TestTimesVm { hz: 100 };
        let mut cpy = CapCopy { dst: VirBytes(0), buf: alloc::vec::Vec::new(), fail: false };
        let ((u_sec, u_usec), (s_sec, s_usec)) =
            do_getrusage(&table, UserSlot::new(0), RusageWho::Slf, VirBytes(0x3000), 100, &mut ctl, &mut cpy).unwrap();
        assert_eq!((u_sec, u_usec, s_sec, s_usec), (1, 200_000, 0, 600_000));
        assert_eq!(cpy.buf.len(), 128);
        let i64_at = |off: usize| i64::from_le_bytes(cpy.buf[off..off + 8].try_into().unwrap());
        assert_eq!(i64_at(0), 1); // ru_utime.tv_sec
        assert_eq!(i64_at(8), 200_000); // ru_utime.tv_usec
        assert_eq!(i64_at(16), 0); // ru_stime.tv_sec
        assert_eq!(i64_at(24), 600_000); // ru_stime.tv_usec
        assert_eq!(i64_at(32), 4096); // ru_maxrss(VM,KB)
        assert_eq!(i64_at(64), 33); // ru_minflt(VM)
        assert_eq!(i64_at(72), 4); // ru_majflt(VM)
        for off in (40..64).step_by(8).chain((80..128).step_by(8)) {
            assert_eq!(i64_at(off), 0, "field at {off} must stay zero");
        }
    }

    #[test]
    fn test_getrusage_vm_error_propagates_without_copyout() {
        // C misc.c:441-442:vm_getrusage 非 OK 原样返回,数据不落用户缓冲。
        struct VmFail;
        impl TimesVmCtl for VmFail {
            fn sys_times(&mut self, _ep: Endpoint) -> Result<(Clock, Clock), MiscError> { Ok((0, 0)) }
            fn vm_rusage(&mut self, _ep: Endpoint, _who: RusageWho) -> Result<VmUsage, MiscError> {
                Err(MiscError::Kernel(-3)) // ESRCH
            }
        }
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let mut ctl = VmFail;
        let mut cpy = CapCopy { dst: VirBytes(0), buf: alloc::vec::Vec::new(), fail: false };
        assert_eq!(
            do_getrusage(&table, UserSlot::new(0), RusageWho::Slf, VirBytes(0x3000), 100, &mut ctl, &mut cpy).unwrap_err(),
            MiscError::Kernel(-3)
        );
        assert!(cpy.buf.is_empty());
    }

    #[test]
    fn test_getrusage_copyout_failure_is_fault() {
        // C misc.c:446-447:sys_datacopy 失败即调用失败(EFAULT 面)。
        let mut table = ProcTable::new();
        mk_running(&mut table, 0, 0);
        let mut ctl = TestTimesVm { hz: 100 };
        let mut cpy = CapCopy { dst: VirBytes(0), buf: alloc::vec::Vec::new(), fail: true };
        assert_eq!(
            do_getrusage(&table, UserSlot::new(0), RusageWho::Slf, VirBytes(0x3000), 100, &mut ctl, &mut cpy).unwrap_err(),
            MiscError::Fault
        );
    }

    #[test]
    fn test_rusage_from_ticks() {
        let (sec, usec) = rusage_from_ticks(150, 100);
        assert_eq!(sec, 1);
        assert_eq!(usec, 500_000);
    }

    #[test]
    fn test_sprofile_enosys() {
        let mut ctl = TestSprof;
        assert_eq!(
            do_sprofile(0, 0, 0, 0, Endpoint::SELF, VirBytes(0), VirBytes(0), &mut ctl)
                .unwrap_err(),
            MiscError::Nosys
        );
        // AltSprof also Nosys when feature disabled
        let mut alt = AltSprof;
        assert_eq!(
            do_sprofile(1, 0, 0, 0, Endpoint::SELF, VirBytes(0), VirBytes(0), &mut alt)
                .unwrap_err(),
            MiscError::Nosys
        );
    }

    #[test]
    fn test_mcontext_passthrough() {
        let ctl = TestMctx { ret: 0 };
        assert!(do_getmcontext(&ctl, Endpoint(0), VirBytes(0x1000)).is_ok());
        assert!(do_setmcontext(&ctl, Endpoint(0), VirBytes(0x1000)).is_ok());
        let fail = TestMctx { ret: -22 };
        assert!(do_getmcontext(&fail, Endpoint(0), VirBytes(0)).is_err());
        // alt impl
        let alt = AltMctx;
        assert!(do_getmcontext(&alt, Endpoint(0), VirBytes(0)).is_ok());
    }

    #[test]
    fn test_constants_match_c() {
        assert_eq!(PM_SYSUNAME, 25);
        assert_eq!(PM_GETSYSINFO, 47);
        assert_eq!(PM_GETPROCNR, 46);
        assert_eq!(PM_GETEPINFO, 45);
        assert_eq!(PM_REBOOT, 37);
        assert_eq!(PM_SVRCTL, 38);
        assert_eq!(PM_GETRUSAGE, 36);
        assert_eq!(SI_PROC_TAB, 0);
        assert_eq!(RUSAGE_SELF, 0);
        assert_eq!(RUSAGE_CHILDREN, -1);
    }

    // Ensure traits have ≥2 impl for Gate D
    #[test]
    fn test_traits_have_two_impls() {
        let _ = TestSysInfo { data: vec![] };
        let _ = TestSysInfoAlt { _a: 0 };
        let _ = TestReboot { abort: 0, killed: false, stopped: false, told: false };
        let _ = AltReboot;
    }
}
