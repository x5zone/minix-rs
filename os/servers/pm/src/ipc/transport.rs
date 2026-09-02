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

use minix_types::{Endpoint, IpcError, Message};

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
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcError>;

    /// 发送消息。镜像 C `ipc_send(dest, &msg)`。
    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcError>;

    /// 发送并等待回复（同步调用）。镜像 C `ipc_sendrec(dest, &msg)`。
    ///
    /// C 语义：回复写入同一消息缓冲（`m_type` 被回复值覆盖）。
    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcError>;
}

// ── 生产实现：内核 IPC ──

/// 生产 IPC 传输。
///
/// **状态（2026-08-16）**：内核 IPC 原语未落地（`minix-sys` stub），
/// 本实现 `unimplemented!()` 失败，错误信息自说明——而非旧的
/// 隐晦 `Err(())`。
pub struct KernelIpcTransport;

impl KernelIpcTransport {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for KernelIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcTransport for KernelIpcTransport {
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcError> {
        // C: sef_receive_status(ANY, &msg, &rcv_sts) — libsys.a
        unimplemented!("KernelIpcTransport::receive — 等待内核 IPC 核心落地（minix-sys）")
    }

    fn send(&mut self, _dest: Endpoint, _msg: &Message) -> Result<(), IpcError> {
        // C: ipc_send(dest, &msg) — libsys.a
        unimplemented!("KernelIpcTransport::send — 等待内核 IPC 核心落地（minix-sys）")
    }

    fn sendrec(&mut self, _dest: Endpoint, _msg: &mut Message) -> Result<(), IpcError> {
        // C: ipc_sendrec(dest, &msg) — libsys.a
        unimplemented!("KernelIpcTransport::sendrec — 等待内核 IPC 核心落地（minix-sys）")
    }
}

// ── 测试实现：内存 mock ──

/// 测试专用 IPC 传输。
///
/// 记录每次 `send` 的目标与消息（供断言），并为 `sendrec` 预置回复
/// `m_type`（默认 OK=0，模拟 VFS 确认）。
#[derive(Debug)]
pub struct TestIpcTransport {
    /// 所有 `send` 调用记录（按序）。
    sent: alloc::vec::Vec<(Endpoint, Message)>,
    /// `sendrec` 写入消息的回复 `m_type`。
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
            reply_type: 0,
            next_receive: None,
        }
    }

    /// 设置 `sendrec` 的回复 `m_type`（默认 0 = OK）。
    pub fn set_reply_type(&mut self, reply: i32) {
        self.reply_type = reply;
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
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcError> {
        self.next_receive.take().ok_or(IpcError::WouldBlock)
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcError> {
        self.sent.push((dest, *msg));
        Ok(())
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcError> {
        self.send(dest, msg)?;
        // C: 回复覆盖 m_type（main.c:246-249 检查 `mess.m_type != OK`）。
        msg.m_type = self.reply_type;
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
        let mut msg = Message::default();
        msg.m_type = 0x900; // VFS_PM_INIT
        t.send(Endpoint::VFS, &msg).unwrap();
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
        let mut msg = Message::default();
        msg.m_type = 0x900; // VFS_PM_INIT
        msg.m_source = Endpoint::VFS;
        t.queue_receive(msg, IpcStatus { flags: 0 });
        let (got, sts) = t.receive().unwrap();
        assert_eq!(got.m_type, 0x900);
        assert_eq!(got.m_source, Endpoint::VFS);
        assert!(!sts.is_notify());
        // 队列一次性消费；第二次 receive 无消息（WouldBlock）。
        assert!(matches!(t.receive(), Err(IpcError::WouldBlock)));
    }

    #[test]
    fn test_mock_sendrec_overwrites_type() {
        let mut t = TestIpcTransport::new();
        let mut msg = Message::default();
        msg.m_type = 0x900;
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
