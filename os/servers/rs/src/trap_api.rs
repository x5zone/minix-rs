//! 生产 `KernelApi`——RS 外部边界的 trap 后端实现。
//!
//! [`crate::boot::KernelApi`] 五域面的唯一生产实现:每方法委托 minix-sys
//! 的真实 wrapper(E2/E6/E9 已闭环),通电形态即最终形态(real-trap
//! feature 由镜像构建链统一开启,宿主构建双腿回答 ±EIO——失败显式
//! 可观察,不 panic)。
//!
//! # 两条传输腿
//!
//! minix-sys 有两条独立传输通道,五域面横跨两者:
//!
//! - **IPC 腿** `DirectTrapTransport`(SEF/PM/VM/SCHED 消息)——
//!   `pm.rs`/`vm.rs` 的 `*_via` wrapper 与 `IpcApi` 走这里;
//! - **内核调用腿** `DirectKernelCallTransport`(`sys_*` 调用)——
//!   `syscall.rs` 的 `sys_*` wrapper 全部吃 `&impl KernelCallTransport`。
//!
//! # 三条错误约定(见 19-rs-external-interfaces.md)
//!
//! 1. `*_via` taskcall wrapper:已折叠,返回即 `Result<_, Errno>`;
//! 2. `sys_*` 内核调用 wrapper:裸 i32,负 = errno → `Errno::from_i32(-r)`;
//! 3. IPC 腿 `TrapStatus`:正 errno → `Errno::from_i32(s.0)`。
//!
//! C 对应:`minix3/minix/servers/rs/main.c` 的 libsys 调用面
//! (sys_getmachine/sys_privctl/sched_start/...) 与
//! `minix3/minix/servers/rs/manager.c` 的 `_taskcall(PM/VM)` 面。

use minix_sys::ipc::{AsyncSlot, AsyncSlotFlags, IpcTransport as _};
use minix_sys::syscall::{self, DirectKernelCallTransport};
use minix_sys::{pm as sys_pm, vm as sys_vm};
use minix_types::{Clock, Endpoint, Errno, Message, Pid};

use crate::boot::{IpcApi, Machine, PmApi, SchedApi, SysApi, VmApi, VmRsMemReq};
use crate::privilege::{CallMask, PrivCtlOp, PrivFlags, PrivId, Privilege, SysMap, TrapMask};
use crate::sched::SchedulerConfig;

// ── privctl SetSys 的 wire 镜像 ─────────────────────────────────────

/// `IoRange` 的 wire 镜像(内核 kpriv.rs:21-24,repr(C),8 字节)。
#[repr(C)]
#[derive(Clone, Copy)]
struct WireIoRange {
    base: u32,
    limit: u32,
}

/// `MemRange` 的 wire 镜像(内核 kpriv.rs:39-42,repr(C),16 字节)。
#[repr(C)]
#[derive(Clone, Copy)]
struct WireMemRange {
    base: u64,
    limit: u64,
}

/// `SYS_PRIV_SET_SYS`/`UPDATE_SYS` 的载荷镜像。
///
/// 内核侧权威布局:`os/kernel/src/kpriv.rs` `PrivUpdateRequest`
/// (repr(C);内核注释明确"独立 struct,只需与 RS 服务端载荷布局内部
/// 自洽")。本镜像逐域对应 rs 的 [`Privilege`](crate::privilege::Privilege)
/// (privilege.rs:362-395 注释"逐域可比")——字段顺序与内核镜像严格
/// 一致,repr(C) 布局由相同字段序自然对齐。GET_PRIV 的线上 wire
/// (`PrivInfoStruct`,56 字节)拿不回 init_flags/io/irq/mem,故
/// SetSys 全量下发、getpriv 增量回读是既定分工。
#[repr(C)]
struct WirePrivUpdate {
    s_id: i32,
    s_flags: u16,
    s_init_flags: i32,
    s_sig_mgr: i32,
    s_bak_sig_mgr: i32,
    s_trap_mask: u16,
    s_ipc_to: u64,
    s_k_call_mask: [u32; 2],
    s_nr_io_range: i32,
    s_io_tab: [WireIoRange; 64],
    s_nr_irq: i32,
    s_irq_tab: [i32; 16],
    s_nr_mem_range: i32,
    s_mem_tab: [WireMemRange; 20],
}

impl WirePrivUpdate {
    /// [`Privilege`] → wire 全量编码(SetSys/UpdateSys 载荷)。
    fn encode(priv_: &Privilege) -> Self {
        Self {
            s_id: priv_.id.0,
            s_flags: priv_.flags.bits(),
            s_init_flags: priv_.init_flags as i32,
            s_sig_mgr: priv_.sig_mgr.0,
            s_bak_sig_mgr: priv_.bak_sig_mgr.0,
            s_trap_mask: priv_.trap_mask.bits(),
            s_ipc_to: priv_.ipc_to.0,
            s_k_call_mask: [
                priv_.k_call_mask.0 as u32,
                (priv_.k_call_mask.0 >> 32) as u32,
            ],
            s_nr_io_range: priv_.nr_io_range,
            s_io_tab: core::array::from_fn(|i| {
                let r = &priv_.io_ranges[i];
                // rs 侧语义是"数量 len";wire 是"末端口 limit"(C io_range,
                // 内核 kpriv 按原值比较)。len==0 → 保持全零(未用项)。
                let limit = if r.len == 0 { 0 } else { r.base + r.len - 1 };
                WireIoRange { base: r.base, limit }
            }),
            s_nr_irq: priv_.nr_irq,
            s_irq_tab: priv_.irqs,
            s_nr_mem_range: priv_.nr_mem_range,
            s_mem_tab: core::array::from_fn(|i| {
                let r = &priv_.mem_ranges[i];
                let limit = if r.len == 0 { 0 } else { r.base + r.len - 1 };
                WireMemRange { base: r.base, limit }
            }),
        }
    }
}

// ── 生产结构 ────────────────────────────────────────────────────────

/// RS 外部边界的生产实现(两腿 + privctl/mask 的 scratch 载荷)。
///
/// scratch 字段存在的理由:`sys_privctl(SetSys)` 与 `vm_rs_set_priv` 都
/// 传 **RS 自有内存的地址**(内核/VM 在同步调用期间 safecopy/datacopy
/// 该地址),结构体字段保证调用期间地址稳定——栈临时值的地址同样可以,
/// 但字段让"载荷存活期 ≥ 调用期"成为类型事实。
pub struct TrapKernelApi {
    /// IPC 腿(pm/vm/sched 的 `*_via` 与 [`IpcApi`])。
    ipc: minix_sys::ipc::DirectTrapTransport,
    /// 内核调用腿(`sys_*` wrapper)。
    kernel: DirectKernelCallTransport,
    /// `sys_privctl(SetSys/UpdateSys)` 的稳定载荷。
    priv_wire: WirePrivUpdate,
    /// `vm_rs_set_priv` 的掩码缓冲(VM datacopy 回读 8 字节,2×u32 LE)。
    mask_wire: [u8; 8],
}

impl TrapKernelApi {
    pub const fn new() -> Self {
        Self {
            ipc: minix_sys::ipc::DirectTrapTransport,
            kernel: DirectKernelCallTransport,
            priv_wire: WirePrivUpdate {
                s_id: 0,
                s_flags: 0,
                s_init_flags: 0,
                s_sig_mgr: 0,
                s_bak_sig_mgr: 0,
                s_trap_mask: 0,
                s_ipc_to: 0,
                s_k_call_mask: [0; 2],
                s_nr_io_range: 0,
                s_io_tab: [WireIoRange { base: 0, limit: 0 }; 64],
                s_nr_irq: 0,
                s_irq_tab: [0; 16],
                s_nr_mem_range: 0,
                s_mem_tab: [WireMemRange { base: 0, limit: 0 }; 20],
            },
            mask_wire: [0; 8],
        }
    }
}

impl Default for TrapKernelApi {
    fn default() -> Self {
        Self::new()
    }
}

/// 内核调用 wrapper 的裸 i32 折返(负 = errno;pm/src/exit.rs 先例)。
fn sys_result(r: i32) -> Result<(), Errno> {
    if r < 0 {
        Err(Errno::from_i32(-r))
    } else {
        Ok(())
    }
}

impl SysApi for TrapKernelApi {
    fn get_machine(&mut self) -> Result<Machine, Errno> {
        // 内核 getinfo_machine 拷 MachineStruct(32 字节;kernel/misc.rs:301):
        // processors_count u32@0, bsp_id u32@4。
        let mut buf = [0u8; 32];
        syscall::sys_get_machine(&self.kernel, &mut buf).map_err(|e| Errno::from_i32(e))?;
        Ok(Machine {
            processors_count: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            bsp_id: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
        })
    }

    fn get_hz(&mut self) -> Result<u32, Errno> {
        let hz = syscall::sys_get_hz(&self.kernel).map_err(|e| Errno::from_i32(e))?;
        Ok(hz as u32)
    }

    fn get_ticks(&mut self) -> Result<Clock, Errno> {
        // SYS_TIMES 即载体(E9;C getticks 的内核请求)——SELF 由内核替换
        // 为调用者。RS 只消费 boot_ticks( syscall.rs:1798 注)。
        let times = syscall::sys_times(&self.kernel, Endpoint::SELF.0)
            .map_err(|e| Errno::from_i32(e))?;
        Ok(times.boot_ticks as Clock)
    }

    fn privctl(
        &mut self,
        proc: Endpoint,
        op: PrivCtlOp,
        priv_: Option<&Privilege>,
    ) -> Result<(), Errno> {
        // SetSys/UpdateSys:内核从 arg_ptr safecopy 整个 WirePrivUpdate
        // (kernel/syscall.rs privctl_set_sys 的 arg_ptr != 0 分支);
        // 其余 opcode 只传 opcode+endpoint,arg_ptr=0。
        let arg_ptr = match (op, priv_) {
            (PrivCtlOp::SetSys | PrivCtlOp::UpdateSys, Some(p)) => {
                self.priv_wire = WirePrivUpdate::encode(p);
                (&raw const self.priv_wire) as u64
            }
            _ => 0,
        };
        syscall::sys_privctl(&self.kernel, proc.0, op as i32, arg_ptr)
            .map_err(|e| Errno::from_i32(e))
    }

    fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, Errno> {
        // 线上 wire 是 PrivInfoStruct(56 字节;kernel/misc.rs:807):无
        // init_flags/bak_sig_mgr/io/irq/mem——增量回读是既定分工(见
        // WirePrivUpdate 注)。
        let mut buf = [0u8; 56];
        syscall::sys_get_priv(&self.kernel, proc.0, &mut buf).map_err(|e| Errno::from_i32(e))?;
        let rd32 = |off: usize| u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        let rd64 = |off: usize| u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
        let k_mask = [rd32(48), rd32(52)];
        Ok(Privilege {
            id: PrivId(rd32(4) as i32),
            flags: PrivFlags::from_bits_truncate(rd32(8) as u16),
            init_flags: 0,
            sig_mgr: Endpoint(rd32(20) as i32),
            bak_sig_mgr: Endpoint::NONE,
            trap_mask: TrapMask::from_bits_retain(rd32(12) as u16),
            ipc_to: SysMap(rd64(40)),
            k_call_mask: CallMask(k_mask[0] as u64 | ((k_mask[1] as u64) << 32)),
            ..Privilege::vacant()
        })
    }

    fn setalarm(&mut self, delay_ticks: u32) -> Result<(), Errno> {
        // C main.c:433 不查旧闹钟——忽略返回的 (time_left, uptime)。
        syscall::sys_setalarm(&self.kernel, delay_ticks as u64, false)
            .map(|_| ())
            .map_err(|e| Errno::from_i32(e))
    }

    fn sys_kill(&mut self, proc: Endpoint, signo: i32) -> Result<(), Errno> {
        sys_result(syscall::sys_kill(&self.kernel, proc.0, signo))
    }

    fn sys_update(
        &mut self,
        src: Endpoint,
        dst: Endpoint,
        flags: crate::service_slot::SysFlags,
    ) -> Result<(), Errno> {
        syscall::sys_update(&self.kernel, src.0, dst.0, flags.bits() as i32)
            .map_err(|e| Errno::from_i32(e))
    }

    fn diagctl_stacktrace(&mut self, target: Endpoint) -> Result<(), Errno> {
        syscall::sys_diagctl_stacktrace(&self.kernel, target.0).map_err(|e| Errno::from_i32(e))
    }

    fn diag_write(&mut self, text: &str) -> Result<(), Errno> {
        syscall::sys_diagctl_write(&self.kernel, text).map_err(|e| Errno::from_i32(e))
    }
}

impl SchedApi for TrapKernelApi {
    fn sched_init_proc(&mut self, cfg: &SchedulerConfig) -> Result<Endpoint, Errno> {
        // C sched_start.c:46-88 的复合传输目标:scheduler 是内核 →
        // sys_schedctl;否则 SCHEDULING_START 消息发给调度器端点。
        if cfg.scheduler == Endpoint::KERNEL {
            syscall::sys_schedctl(
                &self.kernel,
                cfg.endpoint.0,
                cfg.priority,
                cfg.quantum,
                cfg.cpu,
            )
            .map_err(|e| Errno::from_i32(e))?;
        } else {
            sys_pm::sched_start_via(
                &self.ipc,
                cfg.scheduler,
                cfg.endpoint,
                cfg.parent,
                cfg.priority,
                cfg.quantum,
            )?;
        }
        Ok(cfg.scheduler)
    }

    fn sched_stop(&mut self, scheduler: Endpoint, proc: Endpoint) -> Result<(), Errno> {
        sys_pm::sched_stop_via(&self.ipc, scheduler, proc)
    }
}

impl PmApi for TrapKernelApi {
    fn getnuid(&mut self, proc: Endpoint) -> Result<u32, Errno> {
        let uid = sys_pm::getnuid_via(&self.ipc, proc)?;
        Ok(uid as u32)
    }

    fn getnpid(&mut self, proc: Endpoint) -> Result<i32, Errno> {
        sys_pm::getnpid_via(&self.ipc, proc)
    }

    fn getprocnr(&mut self, pid: Pid) -> Result<Endpoint, Errno> {
        sys_pm::getprocnr_via(&self.ipc, pid)
    }

    fn srv_fork(&mut self, uid: u32, gid: u32) -> Result<Pid, Errno> {
        sys_pm::service_fork_via(&self.ipc, uid, gid)
    }

    fn srv_execve(
        &mut self,
        _proc: Endpoint,
        _exec: &[u8],
        _progname: &crate::service_slot::Label,
        _args: &[u8],
        _argc: usize,
    ) -> Result<(), Errno> {
        // 复合操作(C rs/exec.c:21-64 在 RS 进程内运行):libexec 解 ELF →
        // 段分配 + 内核拷贝 → PM 接管 → exec_restart 握手。积木已备
        // (minix-elf 解析、sys_datacopy 拷贝、sys_pm::exec_restart_via
        // 收尾),段/栈镜像组装是 19 号 handler 接线半的工作——fail-closed
        // 待接,RS 建服务路径在接线前不可用(T2 门)。
        Err(Errno::ENOSYS)
    }

    fn srv_kill(&mut self, pid: Pid, signo: i32) -> Result<(), Errno> {
        sys_pm::service_kill_via(&self.ipc, pid, signo)
    }

    fn waitpid(&mut self) -> Option<Pid> {
        // C request.c:1063 do_sigchld 排空循环:waitpid(-1, &status,
        // WNOHANG)。trait 无错误通道:ECHILD(无更多子进程)正常终止;
        // 其余错误同样终止排空(与 C 的循环退出形状一致)。
        match sys_pm::waitpid_via(&self.ipc, -1, minix_sys::wait::WNOHANG, 0) {
            Ok((pid, _status)) => Some(pid),
            Err(_) => None,
        }
    }

    fn setuid(&mut self, uid: u32) -> Result<(), Errno> {
        sys_pm::setuid_via(&self.ipc, uid)
    }
}

impl VmApi for TrapKernelApi {
    fn vm_memctl(
        &mut self,
        proc: Endpoint,
        req: VmRsMemReq,
        a: usize,
        b: usize,
    ) -> Result<(), Errno> {
        // a/b 仅 HeapPrealloc/MapPrealloc(req 2/3)消费:addr 走 m1p1,
        // len 走 m1i3(VM 解码 dispatcher.rs:885-895)。
        sys_vm::vm_rs_memctl_via(&self.ipc, proc, req as i32, a as u64, b as u64)
    }

    fn vm_set_priv(
        &mut self,
        proc: Endpoint,
        vm_call_mask: CallMask,
        allow: bool,
    ) -> Result<(), Errno> {
        // VM datacopy 回读 8 字节 = 2×u32 bitchunk LE 拼 u64(vm
        // dispatcher.rs:805-846,NR_VM_CALLS=49 → 2 chunk)。
        self.mask_wire = vm_call_mask.0.to_le_bytes();
        sys_vm::vm_rs_set_priv_via(
            &self.ipc,
            proc,
            (&raw const self.mask_wire) as u64,
            allow as i32,
        )
    }
}

impl IpcApi for TrapKernelApi {
    fn receive(
        &mut self,
        endpoint: Endpoint,
    ) -> Result<(Message, crate::dispatch::IpcStatus, Clock), Errno> {
        let mut msg = Message::default();
        let sts = self.ipc.receive(endpoint, &mut msg).map_err(|t| Errno::from_i32(t.0))?;
        let status = crate::dispatch::IpcStatus { flags: sts.0 };
        let stamp = if status.is_notify() {
            // 通知载荷的内核时间戳(ipc.h:1715);union 臂仅成功接收且
            // 判定为通知后读取,与发送方填充的臂一致。
            (unsafe { msg.m_u.m_notify.timestamp }) as Clock
        } else {
            0
        };
        Ok((msg, status, stamp))
    }

    fn reply(
        &mut self,
        target: Endpoint,
        result: i32,
        payload: &Message,
    ) -> Result<(), Errno> {
        // C reply(utility.c:318-345)是阻塞 ipc_send——用 send 而非
        // sendnb,handler 变异后的请求消息带 result 型回传。
        let mut m = *payload;
        m.m_type = result;
        self.ipc.send(target, &m).map_err(|t| Errno::from_i32(t.0))
    }

    fn notify(&mut self, endpoint: Endpoint) -> Result<(), Errno> {
        self.ipc.notify(endpoint).map_err(|t| Errno::from_i32(t.0))
    }

    fn asynsend(&mut self, endpoint: Endpoint, message: &Message) -> Result<(), Errno> {
        // C rs_asynsend(utility.c:62)的一次性单槽形态(照抄 input
        // serve.rs 先例):flags 最后写 VALID;NO_REPLY = 不等回执。
        let slot = AsyncSlot {
            flags: AsyncSlotFlags(AsyncSlotFlags::VALID.0 | AsyncSlotFlags::NO_REPLY.0),
            destination: endpoint,
            message: *message,
            result: 0,
        };
        self.ipc.senda(&[slot]).map_err(|t| Errno::from_i32(t.0))
    }

    fn safecopy_from(&mut self, source: Endpoint, addr: usize, buf: &mut [u8]) -> Result<(), Errno> {
        // C manager.c:141/160 的 sys_datacopy 形状(SELF = -2,内核替换)。
        syscall::sys_datacopy(
            &self.kernel,
            source.0,
            addr as u64,
            syscall::SELF,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
        .map_err(|e| Errno::from_i32(e))
    }

    fn safecopy_to(&mut self, dest: Endpoint, addr: usize, buf: &[u8]) -> Result<(), Errno> {
        syscall::sys_datacopy(
            &self.kernel,
            syscall::SELF,
            buf.as_ptr() as u64,
            dest.0,
            addr as u64,
            buf.len() as u64,
        )
        .map_err(|e| Errno::from_i32(e))
    }
}
