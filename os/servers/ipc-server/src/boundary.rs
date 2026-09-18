//! 生产 [`IpcBoundary`] 与 [`EventLoopTransport`]——trap 后端实现。
//!
//! C 对应:`minix3/minix/servers/ipc/main.c` 的 libsys 调用面
//! (`clock_time`/`getnuid`/`proceventmask`/`ipc_sendnb`)、`sem.c`/
//! `shm.c` 的 `sys_datacopy` 搬运与 VM 消息(`vm_remap`/`vm_getphys`/
//! `vm_getrefcount`/`vm_unmap`)、`mmap.c:113-118` 的匿名 backing。
//!
//! # 错误约定
//!
//! trait 的 `i32` 错误是**正 errno**(reply 的 `m_type` 直接取负发回,
//! 与 Errno 常量同域);minix-sys 的 `sys_*` wrapper 返回负 errno,
//! 边界统一 `-e` 折正。VM/PM 的 `*_via` 返回 `Errno`,取 `to_i32()`。
//!
//! # MIB 面状态
//!
//! [`IpcBoundary::mib_process`] 的 RMIB 协议走查器(libsys `rmib.c`
//! 的服务端半)是 S26 余件:决策层已备(`mib_tree.rs` 的
//! `KernIpcChild`/`InfoRoute` + sem/shm 的两个 info 装配器),协议半
//! 待接——当前按 fail-closed 回 `EOPNOTSUPP`,与 C 的未知请求出口
//! 同型。

use alloc::vec::Vec;
use minix_sys::ipc::{
    AsyncSlot, AsyncSlotFlags, DirectTrapTransport, IpcTransport as _,
};
use minix_sys::syscall::{self, DirectKernelCallTransport};
use minix_sys::vm as sys_vm;
use minix_types::{
    Endpoint, Message, PM_GETEPINFO, PROC_EVENT_EXIT, PROC_EVENT_SIGNAL,
    ipc::{IpcPerm, MessLcVmShmUnmap, MessLsysPmGetepinfo, SemidDs, ShmidDs},
};

use crate::perms::{Identity, SetOptions};
use crate::sem::ctl::{self, SemInfo};
use crate::sem::op::SemOp;
use crate::server::{EventLoopTransport, IpcStatus, TransportError};
use crate::service::{Credentials, IpcBoundary};
use crate::shm::segment::Backing;
use crate::shm::ShmIdView;
use crate::shm::attach::{ShmInfoAgg, ShmSummary};

/// `semop` 数组的元素字节长(C `struct sembuf`:三个 short)。
const SEMBUF_SIZE: usize = 6;

fn positive(e: i32) -> i32 {
    if e < 0 {
        -e
    } else {
        e
    }
}

/// 生产边界:IPC 腿(PEER taskcall 与异步发送)+ 内核调用腿(safecopy、
/// 时间与凭证)。两腿结构与 rs 的 `TrapKernelApi`、input 的 serve 同型。
pub struct SysBoundary {
    ipc: DirectTrapTransport,
    kernel: DirectKernelCallTransport,
}

impl SysBoundary {
    pub const fn new() -> Self {
        Self {
            ipc: DirectTrapTransport,
            kernel: DirectKernelCallTransport,
        }
    }
}

impl Default for SysBoundary {
    fn default() -> Self {
        Self::new()
    }
}

impl SysBoundary {
    /// Caller → 本地缓冲的 `sys_datacopy`(sem.c:680-682 形状)。
    fn copy_in_raw(&self, from: Endpoint, ptr: u32, buf: &mut [u8]) -> Result<(), i32> {
        syscall::sys_datacopy(
            &self.kernel,
            from.0,
            ptr as u64,
            syscall::SELF,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
        .map_err(positive)
    }

    /// 本地缓冲 → caller 地址空间的 `sys_datacopy`。
    fn copy_out_raw(&self, to: Endpoint, ptr: u32, buf: &[u8]) -> Result<(), i32> {
        syscall::sys_datacopy(
            &self.kernel,
            syscall::SELF,
            buf.as_ptr() as u64,
            to.0,
            ptr as u64,
            buf.len() as u64,
        )
        .map_err(positive)
    }
}

impl IpcBoundary for SysBoundary {
    fn now(&self) -> u64 {
        // C clock_time(NULL)(clock_time.c):wall 秒 = boottime +
        // realtime_ticks / hz。错误时 0(时钟未初始化的时间戳,与 C 的
        // "bad, but what's better" 分支同型——时间戳不应让调用方死)。
        let ticks = syscall::sys_times(&self.kernel, Endpoint::SELF.0);
        let hz = syscall::sys_get_hz(&self.kernel);
        match (ticks, hz) {
            (Ok(t), Ok(h)) => {
                let snapshot = minix_sys::misc::ClockSnapshot {
                    uptime_ticks: t.boot_ticks,
                    realtime_ticks: t.real_ticks,
                    boottime_seconds: t.boot_time,
                    ticks_per_second: h as u64,
                };
                minix_sys::misc::wall_clock_time(snapshot).0
            }
            _ => 0,
        }
    }

    fn credentials_of(&self, endpoint: Endpoint) -> Option<Credentials> {
        // C main.c:265-268 的 getnuid/getngid + getnpid:一次
        // PM_GETEPINFO 全带回(taskcall 返回值即 pid)。真实 uid/gid 在
        // 应答臂的 uid/gid 域(minix-sys 的 GetepInfo 只露 euid/egid,
        // 权限判定需要真实身份,故此处读完整臂)。
        let mut msg = Message::default();
        msg.m_u.m_lsys_pm_getepinfo = MessLsysPmGetepinfo {
            endpt: endpoint.0,
            ..MessLsysPmGetepinfo::default()
        };
        let reply = syscall::perform_taskcall(
            &self.ipc,
            Endpoint::PM,
            PM_GETEPINFO,
            &mut msg,
        )
        .ok()?;
        if reply < 0 {
            return None;
        }
        // SAFETY: 应答臂在成功 taskcall 后由 PM 填写(ipc.h mess_pm_lsys_getepinfo)。
        let arm = unsafe { &msg.m_u.m_pm_lsys_getepinfo };
        Some(Credentials {
            identity: Identity {
                uid: arm.uid as u32,
                gid: arm.gid as u32,
            },
            pid: reply,
        })
    }

    fn copy_in_ops(&self, from: Endpoint, ptr: u32, count: usize) -> Result<Vec<SemOp>, i32> {
        let mut raw = alloc::vec![0u8; count * SEMBUF_SIZE];
        self.copy_in_raw(from, ptr, &mut raw)?;
        Ok(raw
            .chunks_exact(SEMBUF_SIZE)
            .map(|b| SemOp {
                num: u16::from_le_bytes([b[0], b[1]]),
                op: i16::from_le_bytes([b[2], b[3]]),
                flag: u16::from_le_bytes([b[4], b[5]]),
            })
            .collect())
    }

    fn copy_in_setall(&self, from: Endpoint, ptr: u32, count: usize) -> Result<Vec<u16>, i32> {
        let mut raw = alloc::vec![0u8; count * 2];
        self.copy_in_raw(from, ptr, &mut raw)?;
        Ok(raw
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect())
    }

    fn copy_out_getall(
        &self,
        to: Endpoint,
        ptr: u32,
        values: &[u16],
        count: usize,
    ) -> Result<(), i32> {
        let n = values.len().min(count);
        let mut raw = alloc::vec![0u8; count * 2];
        for (i, v) in values[..n].iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }
        self.copy_out_raw(to, ptr, &raw)
    }

    fn copy_out_sem_stat(
        &self,
        to: Endpoint,
        ptr: u32,
        view: &ctl::SemIdView,
    ) -> Result<(), i32> {
        let ds = SemidDs {
            sem_perm: IpcPerm {
                uid: view.perm.uid,
                gid: view.perm.gid,
                cuid: view.perm.creator_uid,
                cgid: view.perm.creator_gid,
                mode: view.perm.mode,
                ..IpcPerm::default()
            },
            sem_nsems: view.count,
            sem_otime: view.op_time,
            sem_ctime: view.change_time,
            ..SemidDs::default()
        };
        let bytes =
            unsafe { core::slice::from_raw_parts((&raw const ds) as *const u8, core::mem::size_of::<SemidDs>()) };
        self.copy_out_raw(to, ptr, bytes)
    }

    fn copy_out_seminfo(&self, to: Endpoint, ptr: u32, info: &SemInfo) -> Result<(), i32> {
        // C `struct seminfo`(sys/sem.h:69-79):十个 int,按 C 域序。
        let fields = [
            info.map,
            info.identifiers,
            info.total,
            info.undo_structures,
            info.per_id,
            info.max_ops,
            info.undo_entries,
            info.live_or_undo,
            info.max_value,
            info.allocated_or_exit,
        ];
        let mut raw = alloc::vec![0u8; fields.len() * 4];
        for (i, v) in fields.iter().enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        self.copy_out_raw(to, ptr, &raw)
    }

    fn copy_in_set_options(&self, from: Endpoint, ptr: u32) -> Result<SetOptions, i32> {
        // C sem.c:551-553:IPC_SET 拷入的是 perm 草稿半(uid/gid/mode,
        // 即 24 字节 `ipc_perm` 的前三个域口径)。
        let mut raw = [0u8; core::mem::size_of::<IpcPerm>()];
        self.copy_in_raw(from, ptr, &mut raw)?;
        Ok(SetOptions {
            uid: u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
            gid: u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]),
            mode: u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]),
        })
    }

    fn copy_out_shm_stat(
        &self,
        to: Endpoint,
        ptr: u32,
        view: &ShmIdView,
    ) -> Result<(), i32> {
        let ds = ShmidDs {
            shm_perm: IpcPerm {
                uid: view.perm.uid,
                gid: view.perm.gid,
                cuid: view.perm.creator_uid,
                cgid: view.perm.creator_gid,
                mode: view.perm.mode,
                ..IpcPerm::default()
            },
            shm_segsz: view.size,
            shm_lpid: view.last_pid,
            shm_cpid: view.creator_pid,
            shm_nattch: view.attached as u32,
            shm_atime: view.attach_time,
            shm_dtime: view.detach_time,
            shm_ctime: view.change_time,
            ..ShmidDs::default()
        };
        let bytes =
            unsafe { core::slice::from_raw_parts((&raw const ds) as *const u8, core::mem::size_of::<ShmidDs>()) };
        self.copy_out_raw(to, ptr, bytes)
    }

    fn copy_out_shminfo(
        &self,
        to: Endpoint,
        ptr: u32,
        summary: &ShmSummary,
    ) -> Result<(), i32> {
        // C shm.c:349-352:shminfo 全常量块(summarize 的裁决面)。
        // sys/shm.h shminfo 域序:shmmax/shmmin/shmmni/shmseg/shmall。
        let fields: [u32; 5] = [
            summary.max as u32,
            summary.min,
            summary.identifiers,
            summary.identifiers,
            (summary.pages as u32) * (summary.max as u32 / 4096).max(1),
        ];
        let mut raw = alloc::vec![0u8; fields.len() * 4];
        for (i, v) in fields.iter().enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        self.copy_out_raw(to, ptr, &raw)
    }

    fn copy_out_shmagg(
        &self,
        to: Endpoint,
        ptr: u32,
        agg: &ShmInfoAgg,
    ) -> Result<(), i32> {
        // C shm.c:354-364:shm_info 聚合块(sys/shm.h:208-216 域序)。
        let fields = [
            agg.used_ids,
            (agg.total_pages & 0x7fff_ffff) as i32,
            agg.resident_pages as i32,
            agg.swapped_pages as i32,
            agg.swap_attempts as i32,
            agg.swap_successes as i32,
        ];
        let mut raw = alloc::vec![0u8; fields.len() * 4];
        for (i, v) in fields.iter().enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        self.copy_out_raw(to, ptr, &raw)
    }

    fn set_proceventmask(&self, subscribe: bool) {
        // C main.c:163-168:订阅 = EXIT|SIGNAL,退订 = 0。best-effort
        // (C 不查 proceventmask 的失败)。
        let mask = if subscribe {
            PROC_EVENT_EXIT | PROC_EVENT_SIGNAL
        } else {
            0
        };
        let _ = minix_sys::pm::proceventmask_via(&self.ipc, mask);
    }

    fn send_wakeup(&self, endpoint: Endpoint, code: i32) {
        // C send_reply(sem.c:207-215):NO_REPLY 吞掉,其余清空消息后
        // ipc_sendnb,m_type = code。
        if code == crate::sem::NO_REPLY {
            return;
        }
        let msg = Message { m_type: code, ..Message::default() };
        let _ = self.ipc.sendnb(endpoint, &msg);
    }

    fn mib_process(&mut self, msg: &mut Message) {
        // RMIB 协议走查器(libsys rmib.c 服务端半)是 S26 余件:决策层
        // 已备(mib_tree 的 KernIpcChild/InfoRoute + sem/shm 两个 info
        // 装配器),协议走查半待接。fail-closed 回 EOPNOTSUPP——与 C 的
        // 未知请求出口同型,MIB 侧收到错误码可见。
        msg.m_type = minix_types::EOPNOTSUPP;
    }

    fn back_segment(&self, bytes: u64) -> Result<Backing, i32> {
        // C shm.c:113-118:mmap(MAP_ANON) 匿名段 + vm_getphys 取物理快照。
        use minix_sys::vm::MapRequest;
        let request = MapRequest {
            beneficiary: Endpoint::SELF,
            address: minix_types::VirBytes(0),
            length: minix_types::VirBytes(bytes),
            protection: sys_vm::MAP_PROTECTION_READ | sys_vm::MAP_PROTECTION_WRITE,
            flags: 0,
            file: -1,
            offset: 0,
        };
        // mmap_via(caller=SELF):受益人即调用者自己(MapRequest 内已带
        // beneficiary,caller 域是协议的"谁在发起"半)。
        let local = sys_vm::mmap_via(&self.ipc, Endpoint::SELF, request)
            .map_err(|e| e.to_i32())?;
        let phys = sys_vm::physical_address_via(&self.ipc, Endpoint::SELF, local)
            .map_err(|e| e.to_i32())?;
        Ok(Backing {
            local: local.0,
            phys: phys.0,
        })
    }

    fn remap(&self, caller: Endpoint, addr: u64, local: u64, bytes: u64) -> Option<u64> {
        // C shm.c:158-160:vm_remap(caller, addr, local, bytes) 段映射进
        // 调用方空间;MAP_FAILED = None。
        let placed = sys_vm::remap_via(
            &self.ipc,
            sys_vm::VM_CALL_REMAP,
            caller,
            Endpoint::SELF,
            minix_types::VirBytes(addr),
            minix_types::VirBytes(local),
            minix_types::VirBytes(bytes),
        )
        .ok()?;
        Some(placed.0)
    }

    fn phys_of(&self, caller: Endpoint, addr: u64) -> Option<u64> {
        // C shm.c:215-216:vm_getphys;零哨兵 = EINVAL(调用方口径)。
        let phys = sys_vm::physical_address_via(&self.ipc, caller, minix_types::VirBytes(addr))
            .ok()?;
        (phys.0 != 0).then_some(phys.0)
    }

    fn unmap(&self, caller: Endpoint, addr: u64) {
        // C shm.c:232:vm_unmap(caller, addr) —— VM_SHM_UNMAP
        // (libc mmap.c:131-142:forwhom + addr 两域)。
        let mut msg = Message::default();
        msg.m_u.m_lc_vm_shm_unmap = MessLcVmShmUnmap {
            forwhom: caller.0,
            _pad: 0,
            addr,
            _padding: [0; 44],
        };
        let _ = syscall::perform_taskcall(
            &self.ipc,
            Endpoint::VM,
            minix_types::VM_SHM_UNMAP as i32,
            &mut msg,
        );
    }

    fn refcount_of(&self, local: u64) -> Option<u8> {
        // 段在我们自己的空间里:vm_getrefcount(SELF, local)。
        sys_vm::reference_count_via(&self.ipc, Endpoint::SELF, minix_types::VirBytes(local)).ok()
    }

    fn release_mapping(&self, addr: u64, len: u64) {
        // 段销毁的本地半:退掉自己的匿名映射(C munmap 语义);错误
        // 无可恢复处,静默(与 C 忽略 munmap 返回同型)。
        let _ = sys_vm::munmap_via(
            &self.ipc,
            minix_types::VirBytes(addr),
            minix_types::VirBytes(len),
        );
    }
}

// ── 生产事件循环传输 ────────────────────────────────────────────────

/// 生产 `EventLoopTransport`:receive = 内核接收(ANY),reply =
/// `ipc_sendnb`(main.c:273),async = 单槽 `senda` 的 `AMF_NOREPLY`
/// (main.c:207-208 的 asynsend3 形状,照 input serve.rs 先例)。
pub struct SysEventLoopTransport {
    inner: DirectTrapTransport,
}

impl SysEventLoopTransport {
    pub const fn new() -> Self {
        Self { inner: DirectTrapTransport }
    }
}

impl Default for SysEventLoopTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl EventLoopTransport for SysEventLoopTransport {
    fn receive(&mut self) -> Result<(Message, IpcStatus), TransportError> {
        let mut msg = Message::default();
        let sts = self
            .inner
            .receive(Endpoint::ANY, &mut msg)
            .map_err(|_| TransportError)?;
        let notify = (sts.0 & 0x3F) == 4; // NOTIFY(com.h:92)
        Ok((msg, IpcStatus { notify }))
    }

    fn send_reply(&mut self, dest: Endpoint, msg: &Message) -> Result<(), TransportError> {
        self.inner
            .sendnb(dest, msg)
            .map_err(|_| TransportError)
    }

    fn send_async(&mut self, dest: Endpoint, msg: &Message) -> Result<(), TransportError> {
        let slot = AsyncSlot {
            flags: AsyncSlotFlags(AsyncSlotFlags::VALID.0 | AsyncSlotFlags::NO_REPLY.0),
            destination: dest,
            message: *msg,
            result: 0,
        };
        self.inner.senda(&[slot]).map_err(|_| TransportError)
    }
}
