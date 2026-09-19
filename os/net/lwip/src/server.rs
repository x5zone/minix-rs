//! lwip service main loop: SEF interception, startup gate, road dispatch.
//!
//! C correspondence: `main` (`minix3/minix/net/lwip/lwip.c:293-382`).
//! Before the loop, the startup chain runs to completion; inside the loop,
//! a clock notification expires timers, a data-store notification
//! re-checks card drivers, MIB requests take the management road, virtual
//! file system requests split into the socket-device road and the packet
//! filter road, network driver replies take the device road, and anything
//! else is unexpected. Replies leave as non-blocking sends; `EINTR` from
//! the receive (`sef_cancel` was called) just falls back into the loop.
//!
//! The loop has two seams and no business logic: [`ServerIpc`] is the
//! kernel-facing transport (SEF receive plus non-blocking replies), and
//! [`NetHandler`] is every road's work. Production wires
//! [`KernelIpc`] over `minix-sys` traps; tests script both seams (the
//! pattern of the IPC server's event loop, servers/ipc-server/src/server.rs).

use crate::startup::Startup;
use alloc::vec::Vec;

use minix_netdriver::socktable::SockTable;
use minix_sef::{sef_receive_status, SefIpc};
use minix_types::{Endpoint, Message};

/// The reply verb, split off from [`SefIpc`] because only request roads
/// need it: SEF receive and the ping pong are already covered there.
pub trait ReplyIpc {
    /// Non-blocking reply send (C: `ipc_sendnb`).
    fn send_reply(&mut self, dest: Endpoint, msg: &Message) -> Result<(), i32>;
}

/// Per-road business work. Every method is the extension point where the
/// matching document's implementation lands; the loop calls them at
/// exactly the point where C calls the matching function.
pub trait NetHandler {
    /// Perform one startup-chain step (`lwip.c:203-263`, one of the
    /// [`Startup`] stages). False stops the service: the chain failed.
    fn startup_step(&mut self) -> bool;

    /// Keep the main loop running (C: `while (running)`; the live-update
    /// side of this flag is RS protocol, see E-NETSTART).
    fn keep_running(&mut self) -> bool;

    /// Clock tick: expire the timer list, then let the stack poll.
    fn notify_clock(&mut self, table: &mut SockTable, tick: &Message);

    /// Data-store notice: card drivers went up and/or down.
    fn notify_dev_mgr(&mut self);

    /// Management information base request. Returning a message sends it
    /// to the MIB server as the reply.
    fn management(&mut self, msg: &Message) -> Option<Message>;

    /// Socket-device request: decode, run or suspend it through the table,
    /// reply by the request's reply shape.
    fn socket_device(&mut self, table: &mut SockTable, msg: &Message) -> Option<Message>;

    /// Packet-filter device request (character or block road).
    fn bpf_device(&mut self, msg: &Message) -> Option<Message>;

    /// Network device driver reply or status report.
    fn net_device_reply(&mut self, msg: &Message) -> Option<Message>;

    /// Unexpected arrival: C prints and drops (`lwip.c:341`, `:376`), so
    /// no reply leaves for it.
    fn unexpected(&mut self, msg: &Message, is_notify: bool);

    /// 排空挂起续答的待发回执（C 的 `reply` 可在事件处理的任意点发出
    /// ——定时器到点、套接字事件唤醒；本模型的每条路只回一条，挂起
    /// 续答的回执由此口在每趟循环尾统一发出）。实现方返回自上次排空
    /// 以来积累的 `(调用方, 回复)` 对。
    fn take_wake_replies(&mut self) -> Vec<(Endpoint, Message)>;
}

/// How many consecutive transport failures the loop tolerates before it
/// gives up. A broken channel looks the same whatever the cause, so the
/// bound is the only honest failure handling inside the loop.
pub const BROKEN_BUDGET: u32 = 3;

/// C `sef_startup` 的握手半：等 RS 的 `SEF_INIT` 通知（`CALL_NOTIFY`
/// 加 `RS_INIT` 号，sef.h:32/:122 的通知分类）到达，回 OK 给 RS，服务
/// 才算加载完成。握手期间的其他到达按协议不该发生——照主循环的意外
/// 路丢弃（打印后不回复）。
fn wait_for_init<I: SefIpc + ReplyIpc, H: NetHandler>(
    ipc: &mut I,
    handler: &mut H,
) -> Result<(), i32> {
    let mut msg = Message::default();
    let mut broken = 0u32;
    loop {
        let mut on_signal = |_: i32| {};
        let received = match sef_receive_status(ipc, Endpoint::ANY, &mut msg, &mut on_signal) {
            Ok(received) => received,
            Err(code) if code == minix_types::EINTR => continue,
            Err(code) => {
                broken += 1;
                if broken >= BROKEN_BUDGET {
                    return Err(code);
                }
                continue;
            }
        };
        broken = 0;
        let is_notify = minix_sef::is_ipc_notify(received.status);
        if is_notify
            && received.source == minix_sef::RS_ENDPOINT
            && received.message.m_type == minix_sef::SEF_INIT_REQUEST_TYPE
        {
            return ipc.send_reply(
                received.source,
                &Message { m_type: minix_types::OK, ..Message::default() },
            );
        }
        handler.unexpected(&received.message, is_notify);
    }
}

/// The service event loop. Runs the startup chain to [`Startup::running`],
/// then dispatches arrivals until [`NetHandler::keep_running`] says stop.
/// `Err` means the transport or the startup chain failed terminally.
pub fn run<I: SefIpc + ReplyIpc, H: NetHandler>(
    ipc: &mut I,
    handler: &mut H,
    table: &mut SockTable,
    startup: &mut Startup,
) -> Result<(), i32> {
    // C `sef_startup`：RS 的 `SEF_INIT` 到达、OK 回给 RS 之后，服务才算
    // 加载完成——初始化链与主循环都在握手之后。
    wait_for_init(ipc, handler)?;

    while !startup.is_running() {
        if !handler.startup_step() {
            return Err(crate::util::ERR_GENERIC);
        }
        startup.note_done();
    }

    let mut msg = Message::default();
    let mut broken = 0u32;
    let mut terminated = false;
    loop {
        if terminated || !handler.keep_running() {
            return Ok(());
        }
        let mut on_signal = |_: i32| terminated = true;
        let received = match sef_receive_status(ipc, Endpoint::ANY, &mut msg, &mut on_signal) {
            Ok(received) => received,
            Err(code) if code == minix_types::EINTR => continue,
            Err(code) => {
                broken += 1;
                if broken >= BROKEN_BUDGET {
                    return Err(code);
                }
                continue;
            }
        };
        broken = 0;

        let is_notify = minix_sef::is_ipc_notify(received.status);
        let source = received.message.m_source;
        if is_notify {
            if source == Endpoint::CLOCK {
                handler.notify_clock(table, &received.message);
            } else if source == Endpoint::DS {
                handler.notify_dev_mgr();
            } else {
                // C 的通知 default 分支就是意外路径（printf 后丢弃，
                // `lwip.c:336-343`），不另设路。
                handler.unexpected(&received.message, true);
            }
            continue;
        }

        let from_vfs_devices = source == Endpoint::VFS
            && crate::bpfdev::is_filter_request(received.message.m_type);
        let m_type = received.message.m_type;
        let reply = if source == Endpoint::MIB {
            handler.management(&received.message)
        } else if source == Endpoint::VFS {
            if minix_sockdriver::sdev::is_sdev_request(m_type as u32) {
                handler.socket_device(table, &received.message)
            } else if from_vfs_devices {
                handler.bpf_device(&received.message)
            } else {
                handler.unexpected(&received.message, false);
                None
            }
        } else if minix_netdriver::protocol::is_net_reply(m_type) {
            handler.net_device_reply(&received.message)
        } else {
            handler.unexpected(&received.message, false);
            None
        };
        if let Some(reply) = reply {
            ipc.send_reply(source, &reply)?;
        }
        // 挂起续答的回执在每趟循环尾发出（C 的 `reply` 非阻塞发送，
        // 失败仅 printf 后继续——对端已死不是服务的错误面）。
        for (dest, wake_reply) in handler.take_wake_replies() {
            let _ = ipc.send_reply(dest, &wake_reply);
        }
    }
}

/// Production transport: kernel traps behind the SEF and reply verbs.
///
/// `minix-sys` owns the trap instructions; this adapter translates their
/// result types into the flat `i32` codes the SEF loop speaks.
pub struct KernelIpc<T: minix_sys::ipc::IpcTransport> {
    pub transport: T,
}

impl<T: minix_sys::ipc::IpcTransport> SefIpc for KernelIpc<T> {
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        self.transport.receive(src, msg).map(|status| status.0 as i32).map_err(|t| t.0)
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        self.transport.notify(dest).map_err(|t| t.0)
    }
}

impl<T: minix_sys::ipc::IpcTransport> ReplyIpc for KernelIpc<T> {
    fn send_reply(&mut self, dest: Endpoint, msg: &Message) -> Result<(), i32> {
        self.transport.sendnb(dest, msg).map_err(|t| t.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_sef::CannedSefIpc;

    const CALL_NOTIFY: i32 = 4;
    const VFS: Endpoint = Endpoint::VFS;
    const MIB: Endpoint = Endpoint::MIB;
    const RS: Endpoint = minix_sef::RS_ENDPOINT;
    const SYSTEM: Endpoint = minix_sef::SYSTEM_ENDPOINT;
    const RS_INIT: i32 = minix_sef::SEF_INIT_REQUEST_TYPE;

    /// 脚本化传输：接收走 CannedSefIpc（含 ping 吞掉），回复入账。
    struct ScriptedIpc {
        sef: CannedSefIpc,
        replies: Vec<(Endpoint, i32)>,
    }

    impl ScriptedIpc {
        /// 按到达顺序给脚本；CannedSefIpc 从队尾弹出（`Vec::pop`），
        /// 这里先反转一次，测试里就能按到达顺序写。
        fn script(script: &[(i32, Endpoint, i32)]) -> ScriptedIpc {
            ScriptedIpc {
                sef: CannedSefIpc {
                    inbox: script
                        .iter()
                        .rev()
                        .map(|(status, source, m_type)| {
                            (*status, Message { m_source: *source, m_type: *m_type, ..Message::default() })
                        })
                        .collect(),
                    empty_error: minix_types::EINTR,
                    pongs: Vec::new(),
                },
                replies: Vec::new(),
            }
        }
    }

    impl SefIpc for ScriptedIpc {
        fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
            self.sef.receive(src, msg)
        }

        fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
            self.sef.notify(dest)
        }
    }

    impl ReplyIpc for ScriptedIpc {
        fn send_reply(&mut self, dest: Endpoint, msg: &Message) -> Result<(), i32> {
            self.replies.push((dest, msg.m_type));
            Ok(())
        }
    }

    /// 记录型处理器：每条路来一次记一笔；启动链走满七步。
    #[derive(Default)]
    struct RecordingHandler {
        roads: Vec<&'static str>,
        replies: Vec<i32>,
        startup_steps: u32,
        served: u32,
        stop_after: u32,
    }

    impl RecordingHandler {
        fn replyer(&mut self) -> Option<Message> {
            self.replies.push(0x1A);
            Some(Message { m_source: Endpoint::NONE, m_type: 0x1A, ..Message::default() })
        }
    }

    impl NetHandler for RecordingHandler {
        fn startup_step(&mut self) -> bool {
            self.startup_steps += 1;
            true
        }

        fn keep_running(&mut self) -> bool {
            self.served < self.stop_after
        }

        fn notify_clock(&mut self, _table: &mut SockTable, _tick: &Message) {
            self.roads.push("notify-clock");
            self.served += 1;
        }

        fn notify_dev_mgr(&mut self) {
            self.roads.push("notify-devmgr");
            self.served += 1;
        }

        fn management(&mut self, _msg: &Message) -> Option<Message> {
            self.roads.push("management");
            self.served += 1;
            self.replyer()
        }

        fn socket_device(&mut self, _table: &mut SockTable, _msg: &Message) -> Option<Message> {
            self.roads.push("socket-device");
            self.served += 1;
            self.replyer()
        }

        fn bpf_device(&mut self, _msg: &Message) -> Option<Message> {
            self.roads.push("bpf-device");
            self.served += 1;
            None
        }

        fn net_device_reply(&mut self, _msg: &Message) -> Option<Message> {
            self.roads.push("net-device-reply");
            self.served += 1;
            None
        }

        fn unexpected(&mut self, _msg: &Message, is_notify: bool) {
            self.roads.push(if is_notify { "unexpected-notify" } else { "unexpected" });
            self.served += 1;
        }

        fn take_wake_replies(&mut self) -> Vec<(Endpoint, Message)> {
            Vec::new()
        }
    }

    #[test]
    fn test_init_handshake_replies_ok_to_rs() {
        // RS 的 SEF_INIT 到达 → OK 回给 RS（sef_startup 的应答面）。
        let mut ipc = ScriptedIpc::script(&[(CALL_NOTIFY, RS, RS_INIT)]);
        let mut handler = RecordingHandler { stop_after: 0, ..Default::default() };
        let mut table = SockTable::new();
        let mut startup = Startup::new();
        let result = run(&mut ipc, &mut handler, &mut table, &mut startup);
        assert!(result.is_ok());
        assert_eq!(ipc.replies[0], (RS, minix_types::OK), "握手 OK 回给 RS");
        assert_eq!(handler.startup_steps, 7, "握手之后初始化链照走");
    }

    #[test]
    fn test_terminating_signal_stops_the_loop() {
        // SYSTEM 通知 = 信号请求：循环停止（C 的 running=0 语义）。
        let mut ipc = ScriptedIpc::script(&[
            (CALL_NOTIFY, RS, RS_INIT),
            (CALL_NOTIFY, SYSTEM, 0),
            (0, VFS, 0x1234), // 信号之后不再分发
        ]);
        let mut handler = RecordingHandler { stop_after: 0, ..Default::default() };
        let mut table = SockTable::new();
        let mut startup = Startup::new();
        let result = run(&mut ipc, &mut handler, &mut table, &mut startup);
        assert!(result.is_ok());
        assert!(handler.roads.is_empty(), "信号后不分发");
    }

    #[test]
    fn test_startup_gate_runs_before_first_arrival() {
        let mut ipc = ScriptedIpc::script(&[(CALL_NOTIFY, RS, RS_INIT)]);
        let mut handler = RecordingHandler { stop_after: 0, ..Default::default() };
        let mut table = SockTable::new();
        let mut startup = Startup::new();

        let result = run(&mut ipc, &mut handler, &mut table, &mut startup);
        assert!(result.is_ok());
        assert_eq!(handler.startup_steps, 7, "启动链七步走满才进循环");
        assert!(handler.roads.is_empty(), "启动完成前不分发任何消息");
    }

    #[test]
    fn test_roads_dispatch_and_replies_reach_sources() {
        let script = [
            (CALL_NOTIFY, RS, RS_INIT),  // 握手
            (0, VFS, 0x1234),            // 意外（VFS 陌生号）
            (CALL_NOTIFY, Endpoint::CLOCK, 0),  // 时钟
            (CALL_NOTIFY, Endpoint::DS, 0),     // 设备上下线
            (0, MIB, 0x501),             // 管理
            (0, VFS, 0x1900),            // SDEV 建户（is_sdev_request 判定范围）
            (CALL_NOTIFY, Endpoint::PM, 0),     // 意外通知
        ];
        let mut ipc = ScriptedIpc::script(&script);
        let mut handler = RecordingHandler { stop_after: 6, ..Default::default() };
        let mut table = SockTable::new();
        let mut startup = Startup::new();

        let result = run(&mut ipc, &mut handler, &mut table, &mut startup);
        assert!(result.is_ok());
        assert_eq!(
            handler.roads,
            vec![
                "unexpected",
                "notify-clock",
                "notify-devmgr",
                "management",
                "socket-device",
                "unexpected-notify",
            ]
        );
        assert_eq!(ipc.replies.len(), 3, "握手 OK、管理路、套接字路");
        assert_eq!(ipc.replies[0], (RS, minix_types::OK));
        assert_eq!(ipc.replies[1].0, MIB);
        assert_eq!(ipc.replies[2].0, VFS);
        assert_eq!(ipc.sef.pongs.len(), 0, "无 ping 则无 pong");
        assert_eq!(handler.startup_steps, 7);
    }

    #[test]
    fn test_sdev_request_is_routed_by_range_not_sender_guess() {
        // VFS 来源 + SDEV 范围号 → 套接字路；同号非 VFS 来源 → 设备回复路
        // 或意外，由守卫决定。这里锁住范围判定本身。
        assert!(minix_sockdriver::sdev::is_sdev_request(0x1900));
        assert!(minix_netdriver::protocol::is_net_reply(0x1A80));
        assert!(!minix_netdriver::protocol::is_net_reply(0x1900));
        assert!(crate::bpfdev::is_filter_request(0x403), "字符范围进过滤器路");
        assert!(crate::bpfdev::is_filter_request(0x503), "块范围进过滤器路");
        assert!(!crate::bpfdev::is_filter_request(0x1900), "SDEV 号不属于过滤器路");
    }
}
