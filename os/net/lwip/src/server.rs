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
use minix_sef::{SefIpc, sef_receive_status};
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

    /// Drain the queued receipts of suspended-answer requests (C's `reply` can
    /// go out at any point in event handling—when a timer expires, when a socket
    /// event wakes the server; each road in this model answers at most once, so
    /// the receipts for suspended answers leave through this outlet at the tail
    /// of every loop turn). The implementor returns the `(caller, reply)` pairs
    /// accumulated since the last drain.
    fn take_wake_replies(&mut self) -> Vec<(Endpoint, Message)>;
}

/// How many consecutive transport failures the loop tolerates before it
/// gives up. A broken channel looks the same whatever the cause, so the
/// bound is the only honest failure handling inside the loop.
pub const BROKEN_BUDGET: u32 = 3;

/// The handshake half of C `sef_startup`: wait for RS's `SEF_INIT` notification
/// (a `CALL_NOTIFY` with the `RS_INIT` number, the notification taxonomy of
/// sef.h:32/:122), answer OK to RS, and only then is the service loaded. Other
/// arrivals during the handshake should not happen per protocol—they take the
/// main loop's unexpected road (print, then no reply).
fn wait_for_init<I: SefIpc + ReplyIpc, H: NetHandler>(
    ipc: &mut I,
    handler: &mut H,
) -> Result<(), i32> {
    let mut msg = Message::default();
    let mut broken = 0u32;
    let cancel = minix_sef::SefCancel::new(); // no signal-driven exit during the handshake
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

/// The service event loop. Runs the startup chain to [`Startup::running`],
/// then dispatches arrivals until [`NetHandler::keep_running`] says stop.
/// `Err` means the transport or the startup chain failed terminally.
pub fn run<I: SefIpc + ReplyIpc, H: NetHandler>(
    ipc: &mut I,
    handler: &mut H,
    table: &mut SockTable,
    startup: &mut Startup,
) -> Result<(), i32> {
    // C `sef_startup`: RS's `SEF_INIT` arrives, OK goes back to RS, and only
    // then is the service loaded—both the init chain and the main loop run after
    // the handshake.
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
    // Escape hatch (C `sef_cancel()` — netdriver.c:949 + sef.c:161-162): the
    // termination decision cancels the parked/next receive from inside the
    // callback, the library returns `EINTR`, and the `Err(EINTR) => continue`
    // below reaches the terminated check at the top of the loop—so even when the
    // library swallows a manager-shaped signal frame, the loop no longer
    // "swallows it and blocks in the next receive".
    let cancel = minix_sef::SefCancel::new();
    loop {
        if terminated || !handler.keep_running() {
            return Ok(());
        }
        let mut on_signal = |signo: i32| {
            // C `got_signal` (`minix3/minix/lib/libnetdriver/netdriver.c:956-963`):
            // `if (sig != SIGTERM) return;` and only then `netdriver_terminate()`.
            // Without filtering the number, a kernel wake-up notification (today
            // delivered as `SEF_SIGNAL_REQUEST_TYPE`, later 71..=74) would knock
            // the network service down, whereas C keeps serving in that case.
            if signo == minix_types::SIGNAL_TERMINATE {
                terminated = true;
                cancel.cancel();
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
            } else if source == Endpoint::DS {
                handler.notify_dev_mgr();
            } else {
                // C's notification default branch *is* the unexpected path
                // (printf, then drop, `lwip.c:336-343`); no separate road.
                handler.unexpected(&received.message, true);
            }
            continue;
        }

        let from_vfs_devices =
            source == Endpoint::VFS && crate::bpfdev::is_filter_request(received.message.m_type);
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
        // Receipts for suspended answers leave at the tail of every loop turn
        // (C's `reply` is a non-blocking send; on failure it only printfs and
        // continues—a dead peer is not an error face of the service).
        for (dest, wake_reply) in handler.take_wake_replies() {
            //[SD19-gate] wake_reply is a fire-and-forget notification
            #[allow(clippy::let_underscore_must_use)]
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
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_sef::CannedSefIpc;

    const CALL_NOTIFY: i32 = 4;
    const VFS: Endpoint = Endpoint::VFS;
    const MIB: Endpoint = Endpoint::MIB;
    const RS: Endpoint = minix_sef::RS_ENDPOINT;
    const SYSTEM: Endpoint = minix_sef::SYSTEM_ENDPOINT;
    const RS_INIT: i32 = minix_sef::SEF_INIT_REQUEST_TYPE;

    /// Scripted transport: receives go through CannedSefIpc (pings swallowed),
    /// replies are recorded.
    struct ScriptedIpc {
        sef: CannedSefIpc,
        replies: Vec<(Endpoint, i32)>,
    }

    impl ScriptedIpc {
        /// Scripts are given in arrival order; CannedSefIpc pops from the tail
        /// (`Vec::pop`), so reverse once here and tests can write in arrival
        /// order.
        fn script(script: &[(i32, Endpoint, i32)]) -> ScriptedIpc {
            ScriptedIpc::script_signalled(
                &script
                    .iter()
                    .map(|(s, e, t)| (*s, *e, *t, None))
                    .collect::<Vec<_>>(),
            )
        }

        /// Same as above, but each frame may carry an
        /// `m_pm_lsys_sigs_signal.num`—a SIGTERM forwarded by the signal
        /// manager has that shape (C `pm/signal.c:470-473`, `rs/main.c:699-701`);
        /// without it only the kernel-notification shape can be tested.
        fn script_signalled(script: &[(i32, Endpoint, i32, Option<i32>)]) -> ScriptedIpc {
            ScriptedIpc {
                sef: CannedSefIpc {
                    inbox: script
                        .iter()
                        .rev()
                        .map(|(status, source, m_type, signo)| {
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
                            (*status, m)
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

    /// Recording handler: records one entry per road visit; the startup chain
    /// runs all seven steps.
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
            Some(Message {
                m_source: Endpoint::NONE,
                m_type: 0x1A,
                ..Message::default()
            })
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
            self.roads.push(if is_notify {
                "unexpected-notify"
            } else {
                "unexpected"
            });
            self.served += 1;
        }

        fn take_wake_replies(&mut self) -> Vec<(Endpoint, Message)> {
            Vec::new()
        }
    }

    #[test]
    fn test_init_handshake_replies_ok_to_rs() {
        // RS's SEF_INIT arrives → OK goes back to RS (the answering face of
        // sef_startup).
        let mut ipc = ScriptedIpc::script(&[(CALL_NOTIFY, RS, RS_INIT)]);
        let mut handler = RecordingHandler {
            stop_after: 0,
            ..Default::default()
        };
        let mut table = SockTable::new();
        let mut startup = Startup::new();
        let result = run(&mut ipc, &mut handler, &mut table, &mut startup);
        assert!(result.is_ok());
        assert_eq!(ipc.replies[0], (RS, minix_types::OK), "握手 OK 回给 RS");
        assert_eq!(handler.startup_steps, 7, "握手之后初始化链照走");
    }

    #[test]
    fn test_kernel_notify_does_not_terminate_the_loop() {
        // The first sentence of C `got_signal` is `if (sig != SIGTERM) return;`.
        // A SYSTEM notification today delivers the wake-up fallback value (not
        // 15), so the loop must stay alive and serve the following requests as
        // usual—the old shape (no number filtering) would exit right here.
        let mut ipc = ScriptedIpc::script(&[
            (CALL_NOTIFY, RS, RS_INIT),
            (CALL_NOTIFY, SYSTEM, 0),
            (0, MIB, 0x1234),
        ]);
        let mut handler = RecordingHandler {
            stop_after: 2,
            ..Default::default()
        };
        let mut table = SockTable::new();
        let mut startup = Startup::new();
        let result = run(&mut ipc, &mut handler, &mut table, &mut startup);
        assert!(result.is_ok());
        assert_eq!(
            handler.roads,
            ["unexpected-notify", "management"],
            "内核唤醒通知不得终止服务：通知帧走意外路后，后面的 MIB 请求必须照常被服务"
        );
    }

    #[test]
    fn test_manager_sigterm_terminates_the_loop() {
        // The real shape of termination: the signal manager forwards
        // `SIGS_SIGNAL_RECEIVED` carrying num=SIGTERM (C pm/signal.c:470-473,
        // rs/main.c:699-701). C's `got_signal` then calls
        // `netdriver_terminate()` to set running=FALSE and unconditionally calls
        // `sef_cancel()` (netdriver.c:947-949); the library sees that mark at the
        // top of the receive loop and returns EINTR (sef.c:161-162)—so that turn
        // never receives the next frame. With the escape hatch in place this
        // tree matches C's final state: the main-loop closure sets terminated on
        // TERM and calls `cancel.cancel()`, the library returns EINTR, and the
        // loop top exits directly—not one frame is served after SIGTERM (roads
        // stays empty). If the escape hatch were removed (cancel had no effect),
        // the library would swallow the frame and receive a 4th frame, roads
        // would become ["management"], and this assertion would go red.
        let mut ipc = ScriptedIpc::script_signalled(&[
            (CALL_NOTIFY, RS, RS_INIT, None),
            (
                0,
                Endpoint::PM,
                minix_types::SIGS_SIGNAL_RECEIVED,
                Some(minix_types::SIGNAL_TERMINATE),
            ),
            (0, Endpoint::MIB, 0x1234, None),
            (0, Endpoint::MIB, 0x1234, None),
        ]);
        let mut handler = RecordingHandler {
            stop_after: 2,
            ..Default::default()
        };
        let mut table = SockTable::new();
        let mut startup = Startup::new();
        let result = run(&mut ipc, &mut handler, &mut table, &mut startup);
        assert!(result.is_ok());
        assert!(
            handler.roads.is_empty(),
            "SIGTERM 之后一帧都不再服务（逃生门生效，与 C 终态同形）：{:?}",
            handler.roads
        );
    }

    #[test]
    fn test_startup_gate_runs_before_first_arrival() {
        let mut ipc = ScriptedIpc::script(&[(CALL_NOTIFY, RS, RS_INIT)]);
        let mut handler = RecordingHandler {
            stop_after: 0,
            ..Default::default()
        };
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
            (CALL_NOTIFY, RS, RS_INIT),        // handshake
            (0, VFS, 0x1234),                  // unexpected (unknown VFS number)
            (CALL_NOTIFY, Endpoint::CLOCK, 0), // clock
            (CALL_NOTIFY, Endpoint::DS, 0),    // device up/down
            (0, MIB, 0x501),                   // management
            (0, VFS, 0x1900),                  // SDEV create (within is_sdev_request)
            (CALL_NOTIFY, Endpoint::PM, 0),    // unexpected notification
        ];
        let mut ipc = ScriptedIpc::script(&script);
        let mut handler = RecordingHandler {
            stop_after: 6,
            ..Default::default()
        };
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
        // VFS source + an SDEV range number → socket road; the same number from
        // a non-VFS source → device reply road or unexpected, decided by the
        // guard. This pins the range decision itself.
        assert!(minix_sockdriver::sdev::is_sdev_request(0x1900));
        assert!(minix_netdriver::protocol::is_net_reply(0x1A80));
        assert!(!minix_netdriver::protocol::is_net_reply(0x1900));
        assert!(
            crate::bpfdev::is_filter_request(0x403),
            "字符范围进过滤器路"
        );
        assert!(crate::bpfdev::is_filter_request(0x503), "块范围进过滤器路");
        assert!(
            !crate::bpfdev::is_filter_request(0x1900),
            "SDEV 号不属于过滤器路"
        );
    }
}
