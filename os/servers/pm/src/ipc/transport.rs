//! PM IPC transport — strategy trait for kernel IPC boundary.
//!
//! C 对应: `minix3/minix/lib/libsys/ipc_send.c` / `ipc_sendrec`（libsys.a，
//!         单 OS 构建在链接期选择内核 IPC 实现）。
//!
//! 与 VM 的 `os/servers/vm/src/ipc/transport.rs` 同型：用 trait 把"内核 IPC
//! 原语"抽象出来，生产路径（`KernelIpcTransport`）与测试路径
//! （`TestIpcTransport` mock）共享同一套调用代码。
//!
//! 使用方：01 的 `vfs_init_sync`（VFS_PM_INIT 同步）、04 主循环（收消息 +
//! 分发 + 回复）、05 的 VFS 异步回复。

use minix_sys::ipc::IpcTransport as _;
use minix_types::{Endpoint, Message};

/// IPC 接收状态字，镜像 C `sef_receive_status` 的 `rcv_sts` 输出参数。
///
/// 内核填入描述消息的标志位（如 notification vs 普通 IPC）。
/// Rust 侧目前只需知道消息是否为 notification（`is_ipc_notify`）。
///
/// C: `IPC_STATUS_CALL(status) == NOTIFY`（minix/com.h:92；
///     minix/ipcconst.h:10/16：`NOTIFY = 4`，`IPC_STATUS_CALL_MASK = 0x3F`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IpcStatus {
    /// 内核原始状态位。
    pub flags: u32,
}

impl IpcStatus {
    /// 是否为异步通知（notification）。
    ///
    /// 通知是异步信号（内核中断 / 时钟 tick），不是请求消息；主循环在
    /// endpoint 验证前必须跳过（main.c:65-71）。
    pub fn is_notify(&self) -> bool {
        // C: IPC_STATUS_CALL(status) = (status >> 0) & 0x3F（ipcconst.h:22-24）。
        (self.flags & 0x3F) == 4 // NOTIFY
    }
}

/// IPC 传输错误——PM 本地类型,镜像 C `ipc_*` 的原始 errno 返回约定。
///
/// 与 `minix_types::IpcError`(四变体、无原始载荷)的区别:V3-P2-6 错误
/// 保真规约要求内核返回的原始负 errno 可达不折叠(EIO/EINVAL 等在
/// minix3 IPC 错误面里真实存在),本类型以 `Kernel(i32)` 保真携带;
/// 具名变体仅保留测试 mock 与常见路径用的高频值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcTransportError {
    /// `EAGAIN`/`EWOULDBLOCK`——非阻塞模式下无消息。
    WouldBlock,
    /// `EPERM`——无权限向目标发送。
    NoPerm,
    /// 其余内核原始负 errno 原样携带(EIO/EINVAL/EINTR/...)。
    Kernel(i32),
}

impl IpcTransportError {
    /// 从内核原始状态字构造(保真:高频值取具名,其余原样)。
    pub fn from_errno(code: i32) -> Self {
        match code {
            minix_types::EAGAIN => Self::WouldBlock,
            minix_types::EPERM => Self::NoPerm,
            other => Self::Kernel(other),
        }
    }

    /// errno 数值(调用方透传给用户态时用)。
    pub fn errno(&self) -> i32 {
        match self {
            Self::WouldBlock => minix_types::EAGAIN,
            Self::NoPerm => minix_types::EPERM,
            Self::Kernel(code) => *code,
        }
    }
}

/// IPC 传输策略 trait。
///
/// # 为什么用 trait（而不是自由函数）
///
/// C 中 `ipc_send`/`ipc_sendrec` 链接 libsys.a；Rust 侧内核 IPC 尚未落地，
/// 用 trait 允许 `#[cfg(test)]` 替换为 mock，生产实现保持同一接口
/// （trait 质量准则：≥2 个行为不同的实现 + 被泛型约束使用）。
pub trait IpcTransport {
    /// 从任意来源接收一条 IPC 消息。镜像 C
    /// `sef_receive_status(ANY, &msg, &rcv_sts)`（main.c:61）。
    ///
    /// 阻塞直到消息到达（C 语义）；`IpcStatus` 供主循环判 notification。
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcTransportError>;

    /// 非阻塞发送。镜像 C `ipc_sendnb(dest, &msg)`——SEF `reply()` 与
    /// asynsend 风格的一次性投递用（目标尚未 receive 时返回
    /// `ENOTREADY`，不挂起调用方）。PM 绝大多数 `send` 站点是回复
    /// （exit/trace/signal/fork）或异步 tell_vfs，C 里都不阻塞，映射本
    /// 方法；唯一需要阻塞的 `ipc_send` 见 `send_blocking`。
    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError>;

    /// 阻塞发送。镜像 C `ipc_send(dest, &msg)`（`SEND` 系统调用，
    /// main.c:226）——目标未进 receive 时挂起调用方直到投递完成，
    /// 用于吸收启动顺序竞态。仅 VFS_PM_INIT 握手用（PM 必须先于
    /// VFS 起、逐条 send 得等 VFS 进入 receive）；回复/异步发送
    /// 不得用本方法（会破坏 C 的非阻塞回复语义）。
    fn send_blocking(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError>;

    /// 发送并等待回复（同步调用）。镜像 C `ipc_sendrec(dest, &msg)`。
    ///
    /// C 语义：回复写入同一消息缓冲（`m_type` 被回复值覆盖）。
    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcTransportError>;
}

// ── 生产实现：内核 IPC ──

/// 生产 IPC 传输。
///
/// 委托 minix-sys 的用户态 trap 后端（`DirectTrapTransport`）：real-trap
/// feature 下执行真实 trap 指令序列（E1 已于 2026-09-16/17 通电），
/// 宿主构建回答 `-EIO`——失败显式可观察（返回错误，绝不 panic），经
/// 主循环的错误路径处理，与"内核链路慢/断"不可区分。
pub struct KernelIpcTransport {
    /// 用户态 trap 后端。调用形态即最终形态，通电无需再改本类型。
    inner: minix_sys::ipc::DirectTrapTransport,
}

impl KernelIpcTransport {
    pub const fn new() -> Self {
        Self {
            inner: minix_sys::ipc::DirectTrapTransport,
        }
    }
}

impl Default for KernelIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcTransport for KernelIpcTransport {
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcTransportError> {
        // C: ipc_receive(ANY, &msg, &rcv_sts)（libsys ipc_kern.c;
        // SEF 分类是主循环上层的独立缝）。minix-sys 的 IpcStatus(u32)
        // 就是本类型 flags 建模的原始状态字（NOTIFY = 低 6 位 == 4）。
        let mut msg = Message::default();
        match self.inner.receive(Endpoint::ANY, &mut msg) {
            Ok(sts) => Ok((msg, IpcStatus { flags: sts.0 })),
            Err(trap) => Err(IpcTransportError::from_errno(trap.0)),
        }
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError> {
        // C: ipc_sendnb(dest, &msg)（非阻塞，目标未 ready 返回 ENOTREADY）。
        // PM 回复与异步 tell_vfs 站点均对位此语义（SEF reply() 不阻塞）。
        self.inner
            .sendnb(dest, msg)
            .map_err(|trap| IpcTransportError::from_errno(trap.0))
    }

    fn send_blocking(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError> {
        // C: ipc_send(dest, &msg)（main.c:226，SEND 系统调用）——目标未进
        // receive 时挂起调用方。委托后端阻塞 send（SEND_NR），非 sendnb。
        self.inner
            .send(dest, msg)
            .map_err(|trap| IpcTransportError::from_errno(trap.0))
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcTransportError> {
        // C: ipc_sendrec(dest, &msg)——回复写入同一消息缓冲。
        self.inner
            .sendrec(dest, msg)
            .map_err(|trap| IpcTransportError::from_errno(trap.0))
    }
}

// ── 测试实现：内存 mock ──

/// 测试专用 IPC 传输。
///
/// 记录每次 `send` 的目标与消息（供断言），并为 `sendrec` 预置回复：
/// 有脚本（`queue_sendrec_reply`）时整条回复消息出队写入（含载荷
/// 字段，供 VM_FORK 这类"回复即载荷"的协议用）；无脚本时退回旧行为
/// ——仅覆盖回复 `m_type`（默认 OK=0，模拟 VFS 确认）。
///
/// ⚠️ **仅供测试与集成测试注入使用**。`send` 只把消息记进本地缓冲，
/// `receive` 立即返回 WouldBlock——生产代码构造本类型意味着该路径的
/// 对外通信全部丢失（模式 TSTL，见 todo.md §11.5；V2-P0-1 即实例）。
/// 不做 `#[cfg(test)]` 门控是因为集成测试（`tests/`）以普通依赖方式
/// 编译本 crate，看不到 cfg(test) 项。
#[derive(Debug)]
pub struct TestIpcTransport {
    /// 所有 `send` 调用记录（按序）。
    sent: alloc::vec::Vec<(Endpoint, Message)>,
    /// `sendrec` 脚本化回复队列（整条消息，按序出队）。
    sendrec_replies: alloc::collections::VecDeque<Message>,
    /// 无脚本时 `sendrec` 写入消息的回复 `m_type`。
    reply_type: i32,
    /// 下一次 `receive` 返回的消息（+ 状态字）。
    next_receive: Option<(Message, IpcStatus)>,
}

impl TestIpcTransport {
    /// 创建空 mock（`sendrec` 回复 OK；`receive` 无消息返回
    /// `Err(Unimplemented)`，模拟"无消息"）。
    pub fn new() -> Self {
        Self {
            sent: alloc::vec::Vec::new(),
            sendrec_replies: alloc::collections::VecDeque::new(),
            reply_type: 0,
            next_receive: None,
        }
    }

    /// 设置 `sendrec` 的回复 `m_type`（默认 0 = OK；仅无脚本时生效）。
    pub fn set_reply_type(&mut self, reply: i32) {
        self.reply_type = reply;
    }

    /// 入队一条 `sendrec` 脚本化回复（整条消息，按序出队）。
    ///
    /// 与 `set_reply_type` 的区别：脚本回复**整体覆盖**调用方的消息
    /// 缓冲——`m_type` 与 m1/m3 等载荷字段都来自脚本，供 `vm_fork`
    /// 这类"回复即载荷"的协议（VMF_CHILD_ENDPOINT 在 m1i3）构造
    /// 真实形态的应答；无脚本时的退回行为只覆盖 `m_type`。
    pub fn queue_sendrec_reply(&mut self, reply: Message) {
        self.sendrec_replies.push_back(reply);
    }

    /// 预置下一条 `receive` 返回的消息（主循环测试用）。
    pub fn queue_receive(&mut self, msg: Message, status: IpcStatus) {
        self.next_receive = Some((msg, status));
    }

    /// 已发送的 (目标, 消息) 列表（按序）。
    pub fn sent(&self) -> &[(Endpoint, Message)] {
        &self.sent
    }

    /// 最近一次 `send` 的目标 endpoint（`tell_vfs` 测试用）。
    pub fn last_sent_dest(&self) -> Option<Endpoint> {
        self.sent.last().map(|(dest, _)| *dest)
    }
}

impl Default for TestIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcTransport for TestIpcTransport {
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcTransportError> {
        self.next_receive.take().ok_or(IpcTransportError::WouldBlock)
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError> {
        self.sent.push((dest, *msg));
        Ok(())
    }

    fn send_blocking(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError> {
        // mock 不区分阻塞/非阻塞语义：只记录投递（send 一致）。
        self.sent.push((dest, *msg));
        Ok(())
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcTransportError> {
        self.send(dest, msg)?;
        // C: 回复写入同一消息缓冲（main.c:246-249 检查 `mess.m_type != OK`）。
        // 有脚本 → 整条覆盖；无脚本 → 仅覆盖 m_type。
        if let Some(reply) = self.sendrec_replies.pop_front() {
            *msg = reply;
        } else {
            msg.m_type = self.reply_type;
        }
        Ok(())
    }
}

// ── 选择器 ──

/// 选择当前构建的 IPC 传输。
///
/// `#[cfg(test)]` 返回 mock，生产返回内核实现（内核 IPC 落地前会
/// `unimplemented!()`）。
#[cfg(test)]
pub fn ipc_transport_for_build() -> TestIpcTransport {
    TestIpcTransport::new()
}

#[cfg(not(test))]
pub fn ipc_transport_for_build() -> KernelIpcTransport {
    KernelIpcTransport::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_records_send() {
        let mut t = TestIpcTransport::new();
        let msg = Message { m_type: 0x900, ..Message::default() }; // VFS_PM_INIT
        t.send(Endpoint::VFS, &msg).unwrap();
        assert_eq!(t.sent().len(), 1);
        assert_eq!(t.sent()[0].0, Endpoint::VFS);
        assert_eq!(t.sent()[0].1.m_type, 0x900);
    }

    #[test]
    fn test_mock_records_send_blocking() {
        // 阻塞发送（VFS_PM_INIT 对位 C ipc_send）与 send 一样记录投递。
        let mut t = TestIpcTransport::new();
        let msg = Message { m_type: 0x900, ..Message::default() }; // VFS_PM_INIT
        t.send_blocking(Endpoint::VFS, &msg).unwrap();
        assert_eq!(t.sent().len(), 1);
        assert_eq!(t.sent()[0].0, Endpoint::VFS);
        assert_eq!(t.sent()[0].1.m_type, 0x900);
    }

    #[test]
    fn ipc_status_notify_bit_matches_minix3() {
        // C: IPC_STATUS_CALL(status) = (status >> 0) & 0x3F；NOTIFY = 4
        // （ipcconst.h:10/16/22-24）。
        assert!(IpcStatus { flags: 4 }.is_notify());
        assert!(
            !IpcStatus {
                flags: 3 /* SENDREC */
            }
            .is_notify()
        );
        assert!(!IpcStatus { flags: 0 }.is_notify());
        // 高位标志（如 IPC_FLG_MSG_FROM_KERNEL，位 16）不影响低 6 位 CALL。
        assert!(
            IpcStatus {
                flags: 4 | (1 << 16)
            }
            .is_notify()
        );
    }

    #[test]
    fn test_mock_receive_returns_queued_message() {
        let mut t = TestIpcTransport::new();
        let mut msg = Message { m_type: 0x900, ..Message::default() }; // VFS_PM_INIT
        msg.m_source = Endpoint::VFS;
        t.queue_receive(msg, IpcStatus { flags: 0 });
        let (got, sts) = t.receive().unwrap();
        assert_eq!(got.m_type, 0x900);
        assert_eq!(got.m_source, Endpoint::VFS);
        assert!(!sts.is_notify());
        // 队列一次性消费；第二次 receive 无消息（WouldBlock）。
        assert!(matches!(t.receive(), Err(IpcTransportError::WouldBlock)));
    }

    #[test]
    fn test_mock_sendrec_overwrites_type() {
        let mut t = TestIpcTransport::new();
        let mut msg = Message { m_type: 0x900, ..Message::default() };
        t.sendrec(Endpoint::VFS, &mut msg).unwrap();
        // 默认回复 OK=0（模拟 VFS 确认）。
        assert_eq!(msg.m_type, 0);
    }

    #[test]
    fn test_mock_sendrec_custom_reply() {
        let mut t = TestIpcTransport::new();
        t.set_reply_type(5);
        let mut msg = Message::default();
        t.sendrec(Endpoint::VFS, &mut msg).unwrap();
        assert_eq!(msg.m_type, 5);
    }
}
