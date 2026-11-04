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

    /// Termination signal (SIGTERM; `uds_signal` at `uds.c:1349-1365` — sets
    /// running=0; the drain rule then decides between exiting now and finishing
    /// the sockets first).
    ///
    /// The delivery path is the signal manager's forwarded message (C
    /// `SIGS_SIGNAL_RECEIVED` in `pm/signal.c:470-473` and `rs/main.c:699-701`),
    /// not a kernel notification: `uds_signal` returns immediately for non-SIGTERM,
    /// so a kernel wake-up does not trigger this path.
    fn on_terminate(&mut self);

    /// Unexpected arrival: dropped, no reply.
    fn unexpected(&mut self, msg: &Message, is_notify: bool);
}

/// Consecutive transport failures tolerated; the same bound as the lwip loop.
pub const BROKEN_BUDGET: u32 = 3;

/// The service event loop. Startup work belongs to the handler before the
/// first `keep_running` answer; the loop itself only dispatches.
pub fn run<I: SefIpc + ReplyIpc, H: UdsHandler>(
    ipc: &mut I,
    handler: &mut H,
    table: &mut SockTable,
) -> Result<(), i32> {
    // C `sef_startup`: the main loop starts only after RS's `SEF_INIT` arrives and
    // OK is sent back to RS (the same handshake as the lwip loop, the RS_INIT
    // notification of `sef.h:32`).
    wait_for_init(ipc, handler)?;

    let mut msg = Message::default();
    let mut broken = 0u32;
    let mut signalled = false;
    // Escape hatch (C `sef_cancel()` — uds.c:1360 + sef.c:161-162): a termination
    // decision cancels the pending/next receive in the callback, the library
    // returns `EINTR`, and the `Err(EINTR) => continue` below reaches the signalled
    // check at the top of the loop — after the library swallows a manager-shaped
    // signal frame, the loop no longer "swallows it and blocks on the next receive".
    // C cancels only when `uds_in_use == 0` (uds.c:1359-1360); the drain semantics
    // is carried by `keep_running`.
    let cancel = minix_sef::SefCancel::new();
    loop {
        if signalled {
            // `uds_signal` (the termination signal in `uds.c`): set running=0; the
            // drain rule (`loop_keeps_running`) then decides between draining first
            // and exiting immediately.
            handler.on_terminate();
            signalled = false;
        }
        if !handler.keep_running() {
            return Ok(());
        }
        let mut on_signal = |signo: i32| {
            // C `uds_signal` (`minix3/minix/net/uds/uds.c:1349-1356`):
            // "Only check for the termination signal. Ignore anything else."
            // — anything that is not SIGTERM returns immediately. Without the number
            // filter, a kernel wake-up notification (today it delivers
            // `SEF_SIGNAL_REQUEST_TYPE`, in the future 71..=74) would kill the
            // service here, while C keeps serving in that situation.
            if signo == minix_types::SIGNAL_TERMINATE {
                signalled = true;
                // C uds.c:1357-1360 — `uds_running = FALSE; if (uds_in_use ==
                // 0) sef_cancel();`: with nobody using the socket, cancel the pending
                // receive and leave at once; with clients in flight, keep receiving to
                // drain (exit via the while condition, not via the escape hatch — its
                // EINTR would leave the drain waiting forever for the next frame).
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

/// The handshake half of C `sef_startup` (the same as the lwip loop): wait for
/// RS's `SEF_INIT` notification (`CALL_NOTIFY` with the `RS_INIT` number,
/// sef.h:32/:122) to arrive, then reply OK to RS. Other arrivals during the
/// handshake are dropped via the main loop's unexpected path.
fn wait_for_init<I: SefIpc + ReplyIpc, H: UdsHandler>(
    ipc: &mut I,
    handler: &mut H,
) -> Result<(), i32> {
    let mut msg = Message::default();
    let mut broken = 0u32;
    let cancel = minix_sef::SefCancel::new(); // no signal-exit path during the handshake
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
/// (the same-named adapter as the lwip loop; the types belong to each crate so
/// services do not depend on each other).
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

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        // The birth reply rides the same transport (C `ipc_sendrec(RS, &m)`,
        // sef_init.c:458-466).
        self.transport
            .sendrec(dest, msg)
            .map(|_| 0)
            .map_err(|t| t.0)
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

    /// Scripted transport (the same-named fixture as the lwip loop): receives go
    /// through `CannedSefIpc`, replies are only counted (the uds library is pure
    /// `no_std` with no allocator, so tests do not pull in `Vec`).
    /// Each frame may carry an `m_pm_lsys_sigs_signal.num` — the shape of the
    /// SIGTERM forwarded by the signal manager (C `pm/signal.c:470-473`,
    /// `rs/main.c:699-701`).
    /// Once the script is exhausted, receive returns `EINTR` (consistent with
    /// production: receive failures do not accumulate towards BROKEN_BUDGET);
    /// otherwise the default EFAULT of `CannedSefIpc::new()` would disguise
    /// "the loop ended normally" as an error exit (the same treatment as the lwip
    /// fixture).
    struct ScriptedIpc {
        sef: CannedSefIpc,
        replies: u32,
    }

    impl ScriptedIpc {
        fn new(script: &[(i32, Endpoint, i32, Option<i32>)]) -> Self {
            let mut sef = CannedSefIpc::new();
            sef.empty_error = minix_types::EINTR;
            // CannedSefIpc pops from the back, so push in reverse arrival order.
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
        fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<i32, i32> {
            self.sef.sendrec(dest, msg)
        }
    }

    impl ReplyIpc for ScriptedIpc {
        fn send_reply(&mut self, _dest: Endpoint, _msg: &Message) -> Result<(), i32> {
            self.replies += 1;
            Ok(())
        }
    }

    /// Recording handler: hands control back to the main loop once `served`
    /// reaches `stop_after`; `terminations` counts how often the termination
    /// path ran.
    #[derive(Default)]
    struct RecordingHandler {
        clocks: u32,
        unexpected: u32,
        served: u32,
        stop_after: u32,
        terminations: u32,
        /// Test shape of C `uds_in_use > 0`: whether a client is in flight at
        /// termination.
        in_flight: bool,
        /// Test shape of C `uds_running`: the termination path has cleared running.
        terminated: bool,
    }

    impl UdsHandler for RecordingHandler {
        fn keep_running(&mut self) -> bool {
            // `terminated` mirrors C `uds_running=FALSE` (no more receiving after
            // termination); the quota mirrors the test bound "how many frames the
            // script still has to serve".
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
        // The first line of C `uds_signal` (uds.c:1349-1356) is "return immediately
        // for non-SIGTERM". A SYSTEM notification today delivers a wake-up fallback
        // value, so the termination path must never run and the following clock tick
        // is still handled as usual. The old shape (no number filter) would kill the
        // service here.
        let mut ipc = ScriptedIpc::new(&[
            (CALL_NOTIFY, RS, RS_INIT, None),
            (CALL_NOTIFY, SYSTEM, 0, None),
            (CALL_NOTIFY, Endpoint::CLOCK, 0, None),
        ]);
        // Quota 2: the notification frame itself spends one pass through the
        // unexpected path (it still counts as "served once"), so the budget left
        // for the clock tick must include it.
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
        // The real termination shape (idle): the manager forwards
        // `SIGS_SIGNAL_RECEIVED` with num=SIGTERM (C pm/signal.c:470-473,
        // rs/main.c:699-701). C `uds_signal` (uds.c:1357-1360):
        // `uds_running = FALSE; if (uds_in_use == 0) sef_cancel();` — with nobody
        // using the socket, cancel the pending receive,
        // the library returns EINTR at the top of the loop (sef.c:161-162), and the
        // loop leaves immediately without serving another frame.
        // This tree matches that C end state: the callback sets the termination bit
        // and cancels, `Err(EINTR) => continue` reaches the loop top, the termination
        // path runs, keep_running is false because of
        // terminated, and the loop exits. If either the escape hatch or the draining
        // gate fails, the library swallows the frame and goes on to receive CLOCK
        // (clocks becomes 1), turning this assertion red.
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
            in_flight: false, // idle: uds_in_use == 0
            ..Default::default()
        };
        let mut table = SockTable::new();
        assert!(run(&mut ipc, &mut handler, &mut table).is_ok());
        assert_eq!(handler.clocks, 0, "闲时终止：取消挂起收信，后续帧不再服务");
        assert_eq!(handler.terminations, 1, "终止路恰一次");
    }

    #[test]
    fn test_manager_sigterm_with_client_in_flight_drains_first() {
        // The real termination shape (busy): with clients in flight C does not call
        // sef_cancel (the `uds_in_use == 0` gate at uds.c:1358-1360) — the loop keeps
        // receiving and drains what is in flight, exiting via the while condition.
        // This tree matches: no cancel → after the library swallows the signal frame
        // it delivers the next frame (the clock tick), which is served as usual, and
        // the loop top then takes the termination path and exits.
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
            in_flight: true, // busy: uds_in_use > 0
            ..Default::default()
        };
        let mut table = SockTable::new();
        assert!(run(&mut ipc, &mut handler, &mut table).is_ok());
        assert_eq!(handler.clocks, 1, "忙时终止：在途帧照常排空");
        assert_eq!(handler.terminations, 1, "终止路恰一次");
    }
}
