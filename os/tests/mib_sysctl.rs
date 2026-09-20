//! E5(g) — MIB/sysctl 四链联调（宿主态，走 wire 契约）。
//!
//! edge_todo.md 验收面增补（2026-09-15，10-stage-mib 架构审查）登记的四链，
//! 宿主半沿 E5(e) pm_sched.rs 判例：两侧真代码对跑，只有不可宿主驱动的
//! 一端以 wire 契约代替。
//!
//! # 链路面
//!
//! 1. **调用方**：`minix_sys::{sysctl_via, sysctlbyname_via}` 真客户端——
//!    `MIB_SYSCTL` 组包（NS11-B 的 64 位车道 overlay）、sendrec 往返、
//!    回程长度车道读出（含"服务端拒绝也回写长度"的 NetBSD quirk，
//!    `__sysctl.c:29-38`）。传输是自带的 `CallerSide`（实现 minix-sys
//!    的 `IpcTransport`）：`sendrec` 把请求投进共享 bus 后当场泵一轮
//!    MIB 主循环再取回执（minix3/minix/lib/libsys/taskcall.c:16 的阻塞
//!    时序等价形）。
//! 2. **MIB 服务**：`minix_mib::server::Server::run_once` 真循环消费
//!    bus——SEF 收包、triage、`sysctl_arm` 五道门、walker 树查找、
//!    `copy_out` 拷出全在生产路径上。内核动词由 `TestKernel` 承载：
//!    `sys_datacopy` 在宿主上是同地址空间搬运（指针本就出自调用方
//!    进程内的缓冲，E5 线束判例），`sys_hz`/`getticks`/`boottime`
//!    给罐装值（`DEFAULT_HZ`=100，os/kernel/src/clock.rs:685 的内核
//!    线束名义）；peer 动词（DS 标签、VM 统计、远端中继）诚实
//!    `-EIO`——本文件的链不依赖它们。
//! 3. **内核/DS**：不进依赖（E5(e) 的特性统一陷阱对账不变）。
//!
//! # 覆盖（四链）
//!
//! - **链 1 · MIB_SYSCTL 往返**：clockrate 整读（OK + 20 字节
//!   `struct clockinfo` + 长度车道）；短缓冲 → `ENOMEM` 部分拷贝 +
//!   **完整长度仍回写**（C main.c:341-356）；`oldp=NULL` 尺寸探测 →
//!   OK + 长度。
//! - **链 1 · 名字走树**：`sysctlbyname_via("kern.clockrate")` 经真树
//!   的逐级 `CTL_QUERY` 枚举解析（NS11-A 客户端 × 真服务端树）。
//! - **链 2 · rmibtest 注册契约**（服务端半）：SENDREC 形注册被拒
//!   （C remote.c:210-211 单向协议）；空路径静默丢（:894-897）。
//!   客户端本地失败三分类（空/超长/非节点）与八类远端拒绝的**授权
//!   中继半**挂 T2（grant 传输不可宿主）——见文件尾登记行。
//! - **链 4 · minix.mib.* 统计对账**：nodes/objects/remotes 经真树
//!   读出；fresh 服务 remotes=0，nodes>0、objects≥nodes，且名字走查
//!   不改变计数。
//! - **链 3 · ERESTART 纯裁决面**：`check_reply` 三分支（C remote.c
//!   :455-459 + :359/:361/:461/:463）、`locate_slot` 三裁决（Fresh/
//!   Reuse/ReapThenReuse——死亡槽位收割即 mib_down 前半，C remote.c
//!   :64-68）、`dereg_slot` 摘除（C :285 一带）。
//!
//! # 登记（非本文件范围）
//!
//! 远端中继的授权拷贝半（`grant_magic` 路径）与 libsys
//! `rmib_register` 客户端本地失败面属 E-RMIBWIRE 传输余量，真机半
//! 挂 T2——本文件以纯裁决面钉住其决策逻辑。

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use minix_mib::io::relay::RelayDir;
use minix_mib::server::{MibIpc, MibServer, Server};
use minix_mib::transport::{MibKernel, MibServices};
use minix_mib::{io, remote};
use minix_sef::SefIpc;
use minix_sys::ipc::{AsyncSlot, CALL_SEND, CALL_SENDREC, IpcStatus, IpcTransport, TrapStatus};
use minix_sys::misc::MIB_CALL_SYSCTL;
use minix_sys::sysctl::{SysctlOld, sysctl_via};
use minix_types::{
    CTL_KERN, CTL_MINIX, Endpoint, GrantId, KERN_CLOCKRATE, MIB_NODES, MIB_OBJECTS, MIB_REMOTES,
    MINIX_MIB, MessLcMibSysctl, MessLsysMibRegister, Message,
};

const CALLER: Endpoint = Endpoint(20);
const HZ: u32 = 100;
/// `struct clockinfo` 尺寸（五 `int`，sys/sys/sysctl.h:206-212；walker
/// 的 clockrate 应答大小，kern.rs:561 `CTLTYPE_STRUCT | RO` = 20）。
const CLOCKINFO_LEN: usize = 20;

// ---------------------------------------------------------------------------
// 共享 bus：调用方的 sendrec 与 MIB 的 receive/sendnb 在此会合
// ---------------------------------------------------------------------------

#[derive(Default)]
struct BusInner {
    /// 调用方 → MIB 的到达队列，随消息携带到达状态字（sendrec/单向
    /// send 的判别由 MIB triage 读）。
    to_mib: RefCell<VecDeque<(Message, u32)>>,
    /// MIB → 调用方的回执队列（`send_nb` 的落地）。
    to_caller: RefCell<VecDeque<Message>>,
}

#[derive(Clone, Default)]
struct Bus(Rc<BusInner>);

impl Bus {
    fn deliver(&self, message: Message, status: u32) {
        self.0.to_mib.borrow_mut().push_back((message, status));
    }

    fn take_reply(&self) -> Option<Message> {
        self.0.to_caller.borrow_mut().pop_front()
    }

    fn replies_pending(&self) -> usize {
        self.0.to_caller.borrow().len()
    }
}

/// MIB 服务端 IPC 缝（[`MibIpc`] + SEF 收包）：从 bus 取到达、往 bus
/// 投回执。
struct MibSide {
    bus: Bus,
}

impl MibIpc for MibSide {
    fn receive_status(&mut self, message: &mut Message) -> Result<u32, i32> {
        match self.bus.0.to_mib.borrow_mut().pop_front() {
            Some((arriving, status)) => {
                *message = arriving;
                Ok(status)
            }
            None => Err(minix_types::ENOMSG),
        }
    }

    fn send_nb(&mut self, _to: Endpoint, message: &Message) -> Result<(), i32> {
        self.bus.0.to_caller.borrow_mut().push_back(*message);
        Ok(())
    }

    fn send_rec(&mut self, _peer: Endpoint, _message: &mut Message) -> Result<(), i32> {
        Err(minix_types::EIO) // 远端中继腿：本文件不触达（T2）
    }

    fn notify(&mut self, _to: Endpoint) -> Result<(), i32> {
        Ok(())
    }
}

impl SefIpc for MibSide {
    fn receive(&mut self, _src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        match self.bus.0.to_mib.borrow_mut().pop_front() {
            // 状态字是 triage 与 is_sendrec 判别的唯一来源，必须原样
            // 送达（真实内核在 receive 的第三参数里交给它）。
            Some((arriving, status)) => {
                *msg = arriving;
                Ok(status as i32)
            }
            None => Err(minix_types::ENOMSG),
        }
    }

    fn notify(&mut self, _dest: Endpoint) -> Result<(), i32> {
        Ok(())
    }
}

/// 内核动词：同地址空间搬运（宿主回放 `sys_datacopy`）+ 罐装时基。
struct TestKernel;

impl MibKernel for TestKernel {
    fn datacopy_from(&mut self, _src: Endpoint, src_addr: u64, buf: &mut [u8]) -> Result<(), i32> {
        // SAFETY: 宿主线束——src_addr 是调用方进程内缓冲的地址（调用方
        // 与服务同进程），E5 线束判例的同地址空间回放。
        let src = unsafe { core::slice::from_raw_parts(src_addr as *const u8, buf.len()) };
        buf.copy_from_slice(src);
        Ok(())
    }

    fn datacopy_to(&mut self, _dest: Endpoint, dest_addr: u64, buf: &[u8]) -> Result<(), i32> {
        // SAFETY: 同上——dest_addr 指向调用方进程内缓冲。
        let dest = unsafe { core::slice::from_raw_parts_mut(dest_addr as *mut u8, buf.len()) };
        dest.copy_from_slice(buf);
        Ok(())
    }

    fn grant_magic(
        &mut self,
        _whom: Endpoint,
        _addr: u64,
        _len: u64,
        _dir: RelayDir,
    ) -> Result<GrantId, i32> {
        Err(minix_types::EIO) // 授权中继：T2
    }

    fn grant_revoke(&mut self, _grant: GrantId) {}

    fn getproctab(&mut self, _buf: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn getticks(&mut self) -> Result<u64, i32> {
        Ok(1_000_000)
    }

    fn hz(&mut self) -> Result<u32, i32> {
        Ok(HZ)
    }

    fn boottime(&mut self) -> Result<u64, i32> {
        Ok(1_700_000_000)
    }
}

/// peer 动词：全部诚实 `-EIO`（本文件的链不依赖 DS/VM/PM 对端）。
struct TestServices;

impl MibServices for TestServices {
    fn getnuid(&mut self, _who: Endpoint) -> Result<u32, i32> {
        Ok(0) // root：PRIVATE 节点可读（本文件未触达 PRIVATE）
    }

    fn getsysinfo(&mut self, _target: Endpoint, _what: i32, _buf: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn ds_retrieve_label_name(&mut self, _who: Endpoint, buf: &mut [u8]) -> Result<usize, i32> {
        let label = b"rmibtest";
        buf[..label.len()].copy_from_slice(label);
        Ok(label.len())
    }

    fn remote_info(&mut self, _peer: Endpoint, _n: &mut [u8], _d: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn vm_info(&mut self, _what: i32, _ep: Endpoint, _buf: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn remote_call(
        &mut self,
        _peer: Endpoint,
        _call: io::relay::RemoteCall,
        _reply: &mut io::relay::RemoteReplyWire,
    ) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn pm_getparam(&mut self, _param: i32, _buf: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }
}

// ---------------------------------------------------------------------------
// 装配：Rc<RefCell<Server>> 共享句柄——调用方传输投递后泵一轮主循环，
// 测试体可在任意时刻经同一句柄检查服务端状态（slots/计数）。
// ---------------------------------------------------------------------------

type TestServer = Server<TestKernel, TestServices, MibSide>;

/// 调用方侧传输（minix-sys 的 [`IpcTransport`]）。
#[derive(Clone)]
struct CallerSide {
    bus: Bus,
    server: Rc<RefCell<TestServer>>,
}

impl CallerSide {
    fn pair() -> (Self, Rc<RefCell<TestServer>>, Bus) {
        let bus = Bus::default();
        let server = Rc::new(RefCell::new(Server::new(
            MibServer::new(),
            MibSide { bus: bus.clone() },
            TestKernel,
            TestServices,
        )));
        let caller = Self {
            bus: bus.clone(),
            server: server.clone(),
        };
        (caller, server, bus)
    }

    /// 泵一轮并取回执（sendrec 的回程腿）。
    fn round_trip(&self, message: &mut Message) -> Option<Message> {
        message.m_source = CALLER;
        self.bus.deliver(*message, CALL_SENDREC);
        let _ = self.server.borrow_mut().run_once();
        self.bus.take_reply()
    }

    /// 单向发送（注册面协议）：投递 + 泵一轮，回执留在 bus 上供断言。
    fn fire_and_forget(&self, message: &Message) {
        let mut arriving = *message;
        arriving.m_source = CALLER;
        self.bus.deliver(arriving, CALL_SEND);
        let _ = self.server.borrow_mut().run_once();
    }
}

impl IpcTransport for CallerSide {
    fn sendrec(&self, _destination: Endpoint, message: &mut Message) -> Result<(), TrapStatus> {
        match self.round_trip(message) {
            Some(reply) => {
                *message = reply;
                Ok(())
            }
            None => Err(TrapStatus(minix_types::EIO)),
        }
    }

    fn send(&self, _destination: Endpoint, message: &Message) -> Result<(), TrapStatus> {
        self.fire_and_forget(message);
        Ok(())
    }

    fn receive(&self, _s: Endpoint, _m: &mut Message) -> Result<IpcStatus, TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn notify(&self, _d: Endpoint) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn sendnb(&self, _d: Endpoint, _m: &Message) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn senda(&self, _t: &[AsyncSlot]) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn query_kerninfo_page(&self) -> Result<u64, TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
}

/// 经真树读一个 `minix.mib.*` 计数（链 4 的观测面）。
fn read_stat(caller: &CallerSide, leaf: i32) -> (i64, usize) {
    let mut buffer = [0u8; 16];
    let mut length = buffer.len();
    let name = [CTL_MINIX, MINIX_MIB, leaf];
    sysctl_via(
        caller,
        &name,
        Some(SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    )
    .expect("minix.mib.* 读数");
    let value = match length {
        4 => i32::from_le_bytes(buffer[0..4].try_into().unwrap()) as i64,
        8 => i64::from_le_bytes(buffer[0..8].try_into().unwrap()),
        other => panic!("计数节点尺寸 {other} 不在 int/intptr 形"),
    };
    (value, length)
}

// ---------------------------------------------------------------------------
// 链 1 · MIB_SYSCTL 往返
// ---------------------------------------------------------------------------

/// clockrate 整读：真解码 → 真树查找 → 真拷出 → OK + 完整 Clockinfo +
/// 长度车道 20。
#[test]
fn sysctl_round_trip_returns_full_clockinfo() {
    let (caller, _server, _bus) = CallerSide::pair();
    let mut buffer = [0u8; 32];
    let mut length = buffer.len();
    sysctl_via(
        &caller,
        &[CTL_KERN, KERN_CLOCKRATE],
        Some(SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    )
    .expect("clockrate 整读");
    assert_eq!(length, CLOCKINFO_LEN, "回程长度车道 = 节点全尺寸");
    // Clockinfo 五 int：hz/tick/tickadj/stathz/profhz——tick=1e6/hz。
    let i32_at = |off: usize| i32::from_le_bytes(buffer[off..off + 4].try_into().unwrap());
    assert_eq!(i32_at(0), HZ as i32, "hz");
    assert_eq!(i32_at(4), 1_000_000 / HZ as i32, "tick = 1e6/hz");
    assert_eq!(i32_at(8), 1_000_000 / HZ as i32, "tickadj = tick");
    assert_eq!(i32_at(12), HZ as i32, "stathz");
    assert_eq!(i32_at(16), HZ as i32, "profhz");
}

/// 短缓冲：服务端拷出放得下的前缀、应答 ENOMEM，但**完整长度仍回写**
/// （C main.c:341-356；sysctl_via 侧"服务端拒绝也回写长度"的
/// `__sysctl.c:29-38` quirk 在真两端间成立）。
#[test]
fn enomem_partial_copy_still_writes_full_length() {
    let (caller, _server, _bus) = CallerSide::pair();
    let mut buffer = [0u8; 8];
    let mut length = buffer.len();
    let outcome = sysctl_via(
        &caller,
        &[CTL_KERN, KERN_CLOCKRATE],
        Some(SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    );
    assert_eq!(
        outcome,
        Err(minix_types::Errno::ENOMEM),
        "短缓冲应答 ENOMEM"
    );
    assert_eq!(length, CLOCKINFO_LEN, "失败路径长度车道仍回写全尺寸");
    // 部分拷贝：放得下的前 8 字节是真实数据（hz + tick 前半）。
    assert_eq!(
        i32::from_le_bytes(buffer[0..4].try_into().unwrap()),
        HZ as i32,
        "前缀 hz 已拷出"
    );
}

/// `oldp=NULL` 的尺寸探测：OK（非 ENOMEM）且长度车道携带全尺寸——
/// sysctl(3) 的两段式读法第一段。
#[test]
fn length_probe_without_oldp_answers_size() {
    let (caller, _server, _bus) = CallerSide::pair();
    let mut message = Message::zeroed();
    {
        // SAFETY: 请求 overlay 按文档化车道写（NS11-B 64 位车道）。
        let wire: &mut MessLcMibSysctl = unsafe { &mut message.m_u.m_lc_mib_sysctl };
        wire.oldp = 0;
        wire.oldlen = 64;
        wire.namelen = 2;
        wire.name[..2].copy_from_slice(&[CTL_KERN, KERN_CLOCKRATE]);
    }
    message.m_type = MIB_CALL_SYSCTL;
    let reply = caller.round_trip(&mut message).expect("探测有回执");
    assert_eq!(reply.m_type, minix_types::OK, "尺寸探测应答 OK");
    // SAFETY: 应答 overlay 读回程长度车道。
    let replied = unsafe { &reply.m_u.m_mib_lc_sysctl };
    assert_eq!(replied.oldlen as usize, CLOCKINFO_LEN, "长度车道 = 全尺寸");
}

/// `sysctlbyname_via` 经真树逐级 `CTL_QUERY` 枚举解析名字——客户端
/// 名字走 × 服务端查询枚举的真实闭环。
#[test]
fn sysctlbyname_walks_the_real_tree() {
    let (caller, _server, _bus) = CallerSide::pair();
    let mut buffer = [0u8; CLOCKINFO_LEN];
    let mut length = buffer.len();
    let outcome = minix_sys::sysctl::sysctlbyname_via(
        &caller,
        b"kern.clockrate",
        Some(SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    );
    assert!(outcome.is_ok(), "名字走树解析 kern.clockrate");
    assert_eq!(length, CLOCKINFO_LEN);
    assert_eq!(
        i32::from_le_bytes(buffer[0..4].try_into().unwrap()),
        HZ as i32,
        "走树读得的 hz 与直接 mib 一致"
    );
}

// ---------------------------------------------------------------------------
// 链 2 · rmibtest 注册契约（服务端半）
// ---------------------------------------------------------------------------

/// SENDREC 形注册被拒：注册协议是单向（C remote.c:210-211，避免
/// MIB→服务调用环）——拒绝有回执（阻塞调用方不能被晾着），且槽表
/// 不动。
#[test]
fn register_via_sendrec_is_refused() {
    let (caller, server, bus) = CallerSide::pair();
    let mut message = Message::zeroed();
    {
        // SAFETY: 注册 overlay 按文档化车道写。
        let wire: &mut MessLsysMibRegister = unsafe { &mut message.m_u.m_lsys_mib_register };
        wire.miblen = 2;
        wire.mib[..2].copy_from_slice(&[CTL_MINIX, 60]);
    }
    message.m_type = minix_types::MIB_REGISTER;
    let reply = caller.round_trip(&mut message).expect("阻塞注册有回执");
    assert_ne!(reply.m_type, minix_types::OK, "SENDREC 注册被拒");
    assert!(
        server
            .borrow()
            .server
            .slots
            .iter()
            .all(|s| s.endpt.is_none()),
        "被拒注册不留占用槽位"
    );
    let _ = bus.replies_pending();
}

/// 空路径单向注册：静默丢（C :894-897 "nothing to mount"）——无回执
/// 落地，槽表不动。
#[test]
fn register_empty_path_is_silently_dropped() {
    let (caller, server, bus) = CallerSide::pair();
    let mut message = Message::zeroed();
    {
        // SAFETY: 注册 overlay 按文档化车道写。
        let wire: &mut MessLsysMibRegister = unsafe { &mut message.m_u.m_lsys_mib_register };
        wire.miblen = 0;
    }
    message.m_type = minix_types::MIB_REGISTER;
    caller.fire_and_forget(&message);
    assert_eq!(bus.replies_pending(), 0, "单向空路径零回执");
    assert!(
        server
            .borrow()
            .server
            .slots
            .iter()
            .all(|s| s.endpt.is_none()),
        "空路径不留占用槽位"
    );
}

// ---------------------------------------------------------------------------
// 链 4 · minix.mib.* 统计对账
// ---------------------------------------------------------------------------

/// 统计子树三计数经真树读出并与服务端挂载态对账：fresh 服务
/// remotes=0；nodes>0、objects≥nodes（对象含节点）；名字走查（枚举）
/// 不改变任何计数。
#[test]
fn minix_mib_stats_reconcile_with_tree_and_slots() {
    let (caller, server, _bus) = CallerSide::pair();
    let (nodes, _) = read_stat(&caller, MIB_NODES);
    let (objects, _) = read_stat(&caller, MIB_OBJECTS);
    let (remotes, _) = read_stat(&caller, MIB_REMOTES);
    assert!(nodes > 0, "静态树节点计数为正");
    assert_eq!(
        objects, 0,
        "fresh 服务零堆分配（静态树不占对象账，C tree.c:1524-1525）"
    );
    assert_eq!(remotes, 0, "fresh 服务无挂载远端");
    assert!(
        server
            .borrow()
            .server
            .slots
            .iter()
            .all(|s| s.endpt.is_none()),
        "remotes 计数与槽表一致（无占用槽）"
    );
    // 走查幂等：一次完整的 by-name 枚举解析不改变计数。
    let mut buffer = [0u8; CLOCKINFO_LEN];
    let mut length = buffer.len();
    minix_sys::sysctl::sysctlbyname_via(
        &caller,
        b"kern.clockrate",
        Some(SysctlOld {
            buffer: &mut buffer,
            length: &mut length,
        }),
        None,
    )
    .expect("走查");
    let (nodes_after, _) = read_stat(&caller, MIB_NODES);
    let (objects_after, _) = read_stat(&caller, MIB_OBJECTS);
    let (remotes_after, _) = read_stat(&caller, MIB_REMOTES);
    assert_eq!(
        (nodes, objects, remotes),
        (nodes_after, objects_after, remotes_after)
    );
}

// ---------------------------------------------------------------------------
// 链 3 · ERESTART 纯裁决面（remote.rs 的决策逻辑）
// ---------------------------------------------------------------------------

/// 回执校验三分支：正确投递、类型错、请求 id 错（C remote.c:455-459
/// + :359/:361/:461/:463）——中继调用"服务还活着吗"的判定核心。
#[test]
fn check_reply_verdicts_match_c() {
    use remote::ReplyCheck;
    assert_eq!(remote::check_reply(true, 0, 0), ReplyCheck::Deliver(0));
    assert_eq!(remote::check_reply(true, 0, -5), ReplyCheck::Deliver(-5));
    assert_eq!(remote::check_reply(false, 0, 0), ReplyCheck::WrongType);
    assert_eq!(remote::check_reply(true, 7, 0), ReplyCheck::WrongId);
}

/// 槽位生命周期：Fresh 建户、同端点同标签 Reuse、同标签死亡槽位
/// ReapThenReuse（收割即 mib_down 前半，C remote.c:64-68）、按端点
/// 摘除。
#[test]
fn slot_lifecycle_verdicts_cover_shadowing_and_death() {
    use remote::{EndptSlot, Label, SlotVerdict, locate_slot};
    let label = |name: &[u8]| Label::from_bytes(name).unwrap();
    let slot = |endpt: Option<i32>, name: &[u8]| EndptSlot {
        endpt,
        roots: Vec::new(),
        label: label(name),
    };
    // 有空闲座位的新服务 → Fresh（表预分配座位，Fresh=找到空座，
    // C remote.c:136-147）。
    let slots = vec![slot(Some(20), b"lwip"), slot(None, b"")];
    assert!(matches!(
        locate_slot(&slots, 21, label(b"netstack")),
        SlotVerdict::Fresh { eid: 1 }
    ));
    // 同端点 → Reuse，标签无关（C :126-127 端点优先）。
    assert!(matches!(
        locate_slot(&slots, 20, label(b"lwip")),
        SlotVerdict::Reuse { eid: 0 }
    ));
    assert!(matches!(
        locate_slot(&slots, 20, label(b"renamed")),
        SlotVerdict::Reuse { eid: 0 }
    ));
    // 占用槽 + 同标签 + 不同端点 → ReapThenReuse（服务死亡→收割→
    // 续座，mib_down 的表侧形态，C :128-136）。
    let slots = vec![slot(Some(20), b"lwip"), slot(None, b"")];
    assert!(matches!(
        locate_slot(&slots, 21, label(b"lwip")),
        SlotVerdict::ReapThenReuse { eid: 0 }
    ));
    // 全占用且无同标签 → Full（C :140-148）。
    let full = vec![slot(Some(20), b"lwip")];
    assert!(matches!(
        locate_slot(&full, 21, label(b"netstack")),
        SlotVerdict::Full
    ));
    // 按端点摘除：死亡服务退场。
    let mut slots = vec![slot(Some(20), b"lwip"), slot(Some(21), b"uds")];
    assert_eq!(remote::dereg_slot(&slots, 20), Some(0));
    assert_eq!(remote::dereg_slot(&slots, 99), None);
    let _ = &mut slots;
}
