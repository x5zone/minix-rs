//! uds service main loop: SEF interception and the socket-device road.
//!
//! C correspondence: `main` (`minix3/minix/net/uds/uds.c`): the loop runs
//! while the service is marked running or sockets remain in use
//! (`uds.c:1391`; the condition is [`crate::core::loop_keeps_running`]),
//! a clock tick is the only notification road, virtual file system
//! socket-device requests take the single working road, and anything else
//! is unexpected. Replies leave as non-blocking sends.
//!
//! Same two seams as the lwip loop: the transport ([`SefIpc`] plus
//! [`ReplyIpc`]) and the handler.

use minix_netdriver::socktable::SockTable;
use minix_sef::{sef_receive_status, SefIpc};
use minix_types::{Endpoint, Message};

/// The reply verb (C: `ipc_sendnb`).
pub trait ReplyIpc {
    fn send_reply(&mut self, dest: Endpoint, msg: &Message) -> Result<(), i32>;
}

/// Per-road work for the UNIX-domain service.
pub trait UdsHandler {
    /// Keep the loop running: the running flag (cleared by termination)
    /// or any socket still in use (`uds.c:1349-1365`, the drain rule).
    fn keep_running(&mut self) -> bool;

    /// Clock tick: the service's only notification road.
    fn notify_clock(&mut self, table: &mut SockTable, tick: &Message);

    /// Socket-device request from the virtual file system: decode, run or
    /// suspend it through the table, reply by the request's reply shape.
    fn socket_device(&mut self, table: &mut SockTable, msg: &Message) -> Option<Message>;

    /// Termination signal (SYSTEM 通知，`uds.c` 的 `uds_signal`——置
    /// running=0；排水规则随后决定是立即退出还是清完套接字再退)。
    fn on_terminate(&mut self);

    /// Unexpected arrival: dropped, no reply.
    fn unexpected(&mut self, msg: &Message, is_notify: bool);
}

/// Consecutive transport failures tolerated; lwip 循环同款界。
pub const BROKEN_BUDGET: u32 = 3;

/// The service event loop. Startup work belongs to the handler before the
/// first `keep_running` answer; the loop itself only dispatches.
pub fn run<I: SefIpc + ReplyIpc, H: UdsHandler>(
    ipc: &mut I,
    handler: &mut H,
    table: &mut SockTable,
) -> Result<(), i32> {
    // C `sef_startup`：RS 的 `SEF_INIT` 到达、OK 回给 RS 之后，主循环
    // 才开始（lwip 循环同款握手，`sef.h:32` 的 RS_INIT 通知）。
    wait_for_init(ipc, handler)?;

    let mut msg = Message::default();
    let mut broken = 0u32;
    let mut signalled = false;
    loop {
        if signalled {
            // `uds_signal`（`uds.c` 的终止信号）：置 running=0，排水规则
            // （`loop_keeps_running`）随后决定是清完再退还是立即退。
            handler.on_terminate();
            signalled = false;
        }
        if !handler.keep_running() {
            return Ok(());
        }
        let mut on_signal = |_: i32| signalled = true;
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
            } else {
                handler.unexpected(&received.message, true);
            }
            continue;
        }

        if source == Endpoint::VFS
            && minix_sockdriver::sdev::is_sdev_request(received.message.m_type as u32)
        {
            if let Some(reply) = handler.socket_device(table, &received.message) {
                ipc.send_reply(source, &reply)?;
            }
            continue;
        }
        handler.unexpected(&received.message, false);
    }
}

/// C `sef_startup` 的握手半（lwip 循环同款）：等 RS 的 `SEF_INIT`
/// 通知（`CALL_NOTIFY` 加 `RS_INIT` 号，sef.h:32/:122）到达，回 OK 给
/// RS。握手期间的其他到达照主循环的意外路丢弃。
fn wait_for_init<I: SefIpc + ReplyIpc, H: UdsHandler>(
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

/// Production transport: kernel traps behind the SEF and reply verbs
/// （lwip 循环的同名适配器，类型归属各自 crate 以免服务间互相依赖）。
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
