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
use minix_sef::{SefIpc, sef_receive_status};
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

    /// Whether clients are mid-request right now (C `uds_in_use > 0`,
    /// uds.c:1358). The escape hatch consults this at signal time: C's
    /// `uds_signal` cancels the pending receive only when nothing is in
    /// flight — with a client mid-request the loop keeps serving it and
    /// leaves through the drain condition instead (`sef_cancel` would
    /// otherwise stall the drain on `EINTR` forever).
    fn draining(&self) -> bool;

    /// Clock tick: the service's only notification road.
    fn notify_clock(&mut self, table: &mut SockTable, tick: &Message);

    /// Socket-device request from the virtual file system: decode, run or
    /// suspend it through the table, reply by the request's reply shape.
    fn socket_device(&mut self, table: &mut SockTable, msg: &Message) -> Option<Message>;

    /// Termination signal (SIGTERM，`uds.c:1349-1365` 的 `uds_signal`——置
    /// running=0；排水规则随后决定是立即退出还是清完套接字再退)。
    ///
    /// 送达渠道是信号管理器的转发消息（C `pm/signal.c:470-473`、
    /// `rs/main.c:699-701` 的 `SIGS_SIGNAL_RECEIVED`）而不是内核通知：
    /// `uds_signal` 对非 SIGTERM 直接返回，所以内核唤醒不触发本路。
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
    // 逃生门（PD-27，C `sef_cancel()` — uds.c:1360 + sef.c:161-162）：终止
    // 决定在回调里取消挂起/下一次收信，库返 `EINTR`，下方 `Err(EINTR) =>
    // continue` 走到循环顶的 signalled 检查——管理器形信号帧被库吞掉后
    // 不再「吞完继续阻塞在下一收」。C 在 `uds_in_use == 0` 时才 cancel
    //（uds.c:1359-1360），排水语义由 `keep_running` 承担。
    let cancel = minix_sef::SefCancel::new();
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
        let mut on_signal = |signo: i32| {
            // C `uds_signal`（`minix3/minix/net/uds/uds.c:1349-1356`）：
            // "Only check for the termination signal. Ignore anything else."
            // ——非 SIGTERM 一律直接返回。不筛号码的话，一条内核唤醒通知
            // （今天递来的是 `SEF_SIGNAL_REQUEST_TYPE`，将来是 71..=74）
            // 就会把服务打掉，而 C 在这种情况下仍然继续服务。
            if signo == minix_types::SIGNAL_TERMINATE {
                signalled = true;
                // C uds.c:1357-1360 — `uds_running = FALSE; if (uds_in_use ==
                // 0) sef_cancel();`：无人用时取消挂起收信立即走，有人在途
                // 时继续收着排空（靠 while 条件退出，不靠逃生门——逃生门
                // 的 EINTR 会让排空永远等不到下一帧）。
                if !handler.draining() {
                    cancel.cancel();
                }
            }
        };
        let received =
            match sef_receive_status(ipc, Endpoint::ANY, &mut msg, &mut on_signal, &cancel) {
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
    let cancel = minix_sef::SefCancel::new(); // 握手期无信号退出路
    loop {
        let mut on_signal = |_: i32| {};
        let received =
            match sef_receive_status(ipc, Endpoint::ANY, &mut msg, &mut on_signal, &cancel) {
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
                &Message {
                    m_type: minix_types::OK,
                    ..Message::default()
                },
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
        self.transport
            .receive(src, msg)
            .map(|status| status.0 as i32)
            .map_err(|t| t.0)
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
    use minix_sef::CannedSefIpc;

    const CALL_NOTIFY: i32 = 4;
    const RS: Endpoint = minix_sef::RS_ENDPOINT;
    const SYSTEM: Endpoint = minix_sef::SYSTEM_ENDPOINT;
    const RS_INIT: i32 = minix_sef::SEF_INIT_REQUEST_TYPE;

    /// 脚本化传输（lwip 循环的同名夹具）：接收走 `CannedSefIpc`，回复只计数
    /// （uds 的库是纯 `no_std` 无 allocator，测试里不引 `Vec`）。
    /// 每帧可带一个 `m_pm_lsys_sigs_signal.num`——信号管理器转发的 SIGTERM
    /// 就是那个形状（C `pm/signal.c:470-473`、`rs/main.c:699-701`）。
    /// 脚本耗尽后收信回 `EINTR`（与生产一致：收信失败不累计到
    /// BROKEN_BUDGET），否则 `CannedSefIpc::new()` 的默认 EFAULT 会把
    /// 「循环正常结束」的本意伪装成错误退出（lwip 夹具同款处置）。
    struct ScriptedIpc {
        sef: CannedSefIpc,
        replies: u32,
    }

    impl ScriptedIpc {
        fn new(script: &[(i32, Endpoint, i32, Option<i32>)]) -> Self {
            let mut sef = CannedSefIpc::new();
            sef.empty_error = minix_types::EINTR;
            // CannedSefIpc 从队尾弹出，所以按到达顺序的反序压栈。
            for (status, source, m_type, signo) in script.iter().rev() {
                let mut m = Message {
                    m_source: *source,
                    m_type: *m_type,
                    ..Message::default()
                };
                if let Some(num) = signo {
                    m.m_u.m_pm_lsys_sigs_signal = minix_types::MessPmLsysSigsSignal {
                        num: *num,
                        _padding: [0; 52],
                    };
                }
                sef.push(*status, m);
            }
            Self { sef, replies: 0 }
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
        fn send_reply(&mut self, _dest: Endpoint, _msg: &Message) -> Result<(), i32> {
            self.replies += 1;
            Ok(())
        }
    }

    /// 记录型处理器：`served` 到 `stop_after` 就交还主循环；`terminations`
    /// 记录终止路被走过的次数。
    #[derive(Default)]
    struct RecordingHandler {
        clocks: u32,
        unexpected: u32,
        served: u32,
        stop_after: u32,
        terminations: u32,
        /// C `uds_in_use > 0` 的测试形：终止时有无在途客户端。
        in_flight: bool,
        /// C `uds_running` 的测试形：终止路已把 running 清零。
        terminated: bool,
    }

    impl UdsHandler for RecordingHandler {
        fn keep_running(&mut self) -> bool {
            // `terminated` 对位 C `uds_running=FALSE`（终止后不再续收），
            // 配额对位「脚本还有几帧要服务」的测试界。
            !self.terminated && self.served < self.stop_after
        }

        fn draining(&self) -> bool {
            self.in_flight
        }
        fn notify_clock(&mut self, _table: &mut SockTable, _tick: &Message) {
            self.clocks += 1;
            self.served += 1;
        }
        fn socket_device(&mut self, _table: &mut SockTable, _msg: &Message) -> Option<Message> {
            self.served += 1;
            None
        }
        fn on_terminate(&mut self) {
            self.terminations += 1;
            self.terminated = true;
        }
        fn unexpected(&mut self, _msg: &Message, _is_notify: bool) {
            self.unexpected += 1;
            self.served += 1;
        }
    }

    #[test]
    fn test_kernel_notify_does_not_terminate_the_service() {
        // C `uds_signal`（uds.c:1349-1356）第一句是「非 SIGTERM 直接返回」。
        // SYSTEM 通知今天递来的是唤醒兜底值，所以终止路一次都不该被走到，
        // 后面的时钟 tick 仍照常处理。旧形状（不筛号码）会在这里把服务打掉。
        let mut ipc = ScriptedIpc::new(&[
            (CALL_NOTIFY, RS, RS_INIT, None),
            (CALL_NOTIFY, SYSTEM, 0, None),
            (CALL_NOTIFY, Endpoint::CLOCK, 0, None),
        ]);
        // 额度 2：通知帧本身走意外路也要占一次（它仍是“服务过一趟”），
        // 所以留给时钟 tick 的余额得算上它。
        let mut handler = RecordingHandler {
            stop_after: 2,
            ..Default::default()
        };
        let mut table = SockTable::new();
        assert!(run(&mut ipc, &mut handler, &mut table).is_ok());
        assert_eq!(
            handler.terminations, 0,
            "内核唤醒通知不得触发 uds_signal 的终止路"
        );
        assert_eq!(handler.unexpected, 1, "通知本身走意外路（C 的 default 面）");
        assert_eq!(handler.clocks, 1, "服务必须还活着处理下一帧");
    }

    #[test]
    fn test_manager_sigterm_idle_cancels_the_pending_receive() {
        // 终止的真形状（闲）：管理器转发的 `SIGS_SIGNAL_RECEIVED` 携
        // num=SIGTERM（C pm/signal.c:470-473、rs/main.c:699-701）。C
        // `uds_signal`（uds.c:1357-1360）：`uds_running = FALSE; if
        // (uds_in_use == 0) sef_cancel();`——无人用套接字时取消挂起收信，
        // 库在循环顶返 EINTR（sef.c:161-162），循环立即离开，一帧都不再
        // 服务。PD-27 落地后本树与该 C 终态同形：回调置终止位并 cancel，
        // `Err(EINTR) => continue` 到循环顶走终止路、keep_running 因
        // terminated 而假、退出。若逃生门或 draining 门任一失效，库会吞帧
        // 续收 CLOCK（clocks 变 1），本断言即红。
        let mut ipc = ScriptedIpc::new(&[
            (CALL_NOTIFY, RS, RS_INIT, None),
            (
                0,
                Endpoint::PM,
                minix_types::SIGS_SIGNAL_RECEIVED,
                Some(minix_types::SIGNAL_TERMINATE),
            ),
            (CALL_NOTIFY, Endpoint::CLOCK, 0, None),
        ]);
        let mut handler = RecordingHandler {
            stop_after: 1,
            in_flight: false, // 闲：uds_in_use == 0
            ..Default::default()
        };
        let mut table = SockTable::new();
        assert!(run(&mut ipc, &mut handler, &mut table).is_ok());
        assert_eq!(handler.clocks, 0, "闲时终止：取消挂起收信，后续帧不再服务");
        assert_eq!(handler.terminations, 1, "终止路恰一次");
    }

    #[test]
    fn test_manager_sigterm_with_client_in_flight_drains_first() {
        // 终止的真形状（忙）：有在途客户端时 C 不调 sef_cancel
        // （uds.c:1358-1360 的 `uds_in_use == 0` 门）——循环继续收着把在途
        // 的排完，靠 while 条件退出。本树同形：不取消 → 库吞信号帧后送来
        // 下一帧（时钟 tick）照常服务，随后循环顶走终止路退出。
        let mut ipc = ScriptedIpc::new(&[
            (CALL_NOTIFY, RS, RS_INIT, None),
            (
                0,
                Endpoint::PM,
                minix_types::SIGS_SIGNAL_RECEIVED,
                Some(minix_types::SIGNAL_TERMINATE),
            ),
            (CALL_NOTIFY, Endpoint::CLOCK, 0, None),
        ]);
        let mut handler = RecordingHandler {
            stop_after: 1,
            in_flight: true, // 忙：uds_in_use > 0
            ..Default::default()
        };
        let mut table = SockTable::new();
        assert!(run(&mut ipc, &mut handler, &mut table).is_ok());
        assert_eq!(handler.clocks, 1, "忙时终止：在途帧照常排空");
        assert_eq!(handler.terminations, 1, "终止路恰一次");
    }
}
