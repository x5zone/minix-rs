//! SEF (System Event Framework) receive loop — the Rust rewrite of the
//! interception core of C libsys `sef.c` (`sef_receive_status`,
//! sef.c:149-260) and the ping responder (`sef_ping.c:21-38`).
//!
//! C contract: `sef_receive_status(src, m_ptr, status_ptr)` loops on
//! `ipc_receive` and classifies every notification by source — SYSTEM
//! becomes a signal request (`SEF_SIGNAL_REQUEST_TYPE` =
//! `SIGS_SIGNAL_RECEIVED`, sef.h:263), RS becomes a ping request
//! (`SEF_PING_REQUEST_TYPE` = `NOTIFY_MESSAGE`, sef.h:122) — and
//! intercepts each class with a registered callback; only ordinary
//! messages reach the caller. `do_sef_ping_request` (sef_ping.c:21-38)
//! replies with `ipc_notify(source)` — the pong — and returns OK so the
//! message is swallowed (sef.c:208-214).
//!
//! E-ISWIRE (1): the IPC verbs are injected via [`SefIpc`] (production =
//! minix-sys `IpcTransport`; tests = scripted fakes), and the signal /
//! init events are surfaced as [`SefEvent`] values so each server maps
//! them onto its own callback set (IS `SefCallbacks`, devman `SefHooks`,
//! MIB `sef_receive_status` shape).

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use minix_sys::ipc::CALL_NOTIFY;
use minix_types::{Endpoint, Message, NOTIFY_MESSAGE, RS_INIT, SIGS_SIGNAL_RECEIVED};

/// Kernel/system pseudo-endpoint. C: `SYSTEM` — endpoint.h.
pub const SYSTEM_ENDPOINT: Endpoint = Endpoint(-2);
/// Reincarnation Server endpoint. C: `RS_PROC_NR` — com.h:61.
pub const RS_ENDPOINT: Endpoint = Endpoint(2);

/// IPC verbs the SEF loop needs (C: `ipc_receive` / `ipc_notify`).
pub trait SefIpc {
    /// Blocking receive from `src`; returns the decoded status word.
    /// C: `ipc_receive(src, m_ptr, &status)`.
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32>;
    /// Notify `dest` (the SEF ping pong). C: `ipc_notify(source)` —
    /// sef_ping.c:61.
    fn notify(&mut self, dest: Endpoint) -> Result<(), i32>;
}

/// C: `is_ipc_notify(status)` — com.h:93: the status call field equals
/// `NOTIFY` (`CALL_NOTIFY = 4`, minix-sys ipc.rs:53). The status word
/// packs the call number in the low 16 bits.
pub const fn is_ipc_notify(status: i32) -> bool {
    (status & 0xffff) == CALL_NOTIFY as i32
}

/// C: `NOTIFY_MESSAGE` — com.h:90. Notification band base in `m_type`.
pub const SEF_PING_REQUEST_TYPE: i32 = NOTIFY_MESSAGE;
/// C: `SEF_SIGNAL_REQUEST_TYPE` = `SIGS_SIGNAL_RECEIVED` — sef.h:263.
pub const SEF_SIGNAL_REQUEST_TYPE: i32 = SIGS_SIGNAL_RECEIVED;
/// C: `SEF_INIT_REQUEST_TYPE` = `RS_INIT` — sef.h:32.
pub const SEF_INIT_REQUEST_TYPE: i32 = RS_INIT;

/// What one `sef_receive_status` pass decided about the last received
/// message. Intercepted pings never surface; signal/init events surface
/// for the owning server to dispatch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SefEvent {
    /// An ordinary message for the server loop (the vast majority).
    Call(i32),
    /// SYSTEM notification → signal request (C sef.c:222-226; the server
    /// dispatches its registered signal handler).
    Signal(i32),
    /// RS notification with `SEF_INIT` marker (C sef.c:196-206). Carries
    /// the init type: 0 = FRESH, 1 = LU, 2 = RESTART (sef.h:93-95).
    Init(i32),
    /// RS notification that is not a valid ping: C's `IS_SEF_PING_REQUEST`
    /// failed and the message falls through the switch (sef.c:208-214
    /// `break` path).
    PingInvalid,
}

/// Outcome of one `sef_receive_status` pass.
#[derive(Debug, Clone, Copy)]
pub struct SefReceive {
    /// Sender endpoint of the delivered message.
    pub source: Endpoint,
    /// The delivered message (already classified as non-intercepted).
    pub message: Message,
    /// The decoded status word (C: the `*status_ptr` writeback).
    pub status: i32,
    /// The classification decision.
    pub event: SefEvent,
}

impl PartialEq for SefReceive {
    /// Structured equality on the fields tests assert on: source, status
    /// and the classification. `message` byte equality would drag `Message`
    /// into `Eq` (it is `Copy` + `Debug` only), and no test needs it.
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source && self.status == other.status && self.event == other.event
    }
}

/// C: `IS_SEF_PING_REQUEST` — sef.h:123: a notification from RS whose
/// `m_type` equals `NOTIFY_MESSAGE`.
pub fn is_sef_ping_request(source: Endpoint, m_type: i32, is_notify: bool) -> bool {
    is_notify && source == RS_ENDPOINT && m_type == SEF_PING_REQUEST_TYPE
}

/// The ping pong policy (C: `sef_cb_ping_reply_pong` — sef_ping.c:59-62:
/// `ipc_notify(source)`). Trait so tests can record pongs; production
/// forwards to the IPC notify verb.
pub trait SefPingReply {
    fn ping_reply(&mut self, source: Endpoint);
}

/// Notify-based pong (the C default callback).
pub fn pong_via_ipc(ipc: &mut impl SefIpc, source: Endpoint) {
    let _ = ipc.notify(source);
}

/// C: `sef_receive_status` — sef.c:149-260. Loop on receive; classify
/// notifications by source; intercept ping (reply pong, swallow) and
/// signal (surface as [`SefEvent::Signal`]); return ordinary messages.
///
/// Live-update/state-transfer interception (`INTERCEPT_SEF_LU_REQUESTS`
/// / `__sef_st_before_receive`) is not modeled: those paths depend on the
/// LU/ST campaigns and are absent from 03-stage-rs (C keeps them behind
/// `#if INTERCEPT_SEF_LU_REQUESTS`).
///
/// The `on_signal` closure receives the signal request type (always
/// `SEF_SIGNAL_REQUEST_TYPE` today) — C surfaces it to
/// `do_sef_signal_request` which invokes the registered signal handler.
pub fn sef_receive_status(
    ipc: &mut impl SefIpc,
    src: Endpoint,
    msg: &mut Message,
    on_signal: &mut impl FnMut(i32),
) -> Result<SefReceive, i32> {
    loop {
        // C: ipc_receive(src, m_ptr, &status) — sef.c:168-172.
        let status = ipc.receive(src, msg)?;
        let m_type = msg.m_type;

        // C sef.c:174-191 — notification classification by source.
        if is_ipc_notify(status) {
            let source = msg.m_source;
            if source == SYSTEM_ENDPOINT {
                // C sef.c:222-226 — SYSTEM → signal request.
                on_signal(SEF_SIGNAL_REQUEST_TYPE);
                return Ok(SefReceive {
                    source,
                    message: *msg,
                    status,
                    event: SefEvent::Signal(SEF_SIGNAL_REQUEST_TYPE),
                });
            }
            if source == RS_ENDPOINT && m_type == SEF_PING_REQUEST_TYPE {
                // C sef.c:208-214 + sef_ping.c:21-38 — ping: reply pong and
                // continue (never returned to the caller).
                pong_via_ipc(ipc, source);
                continue;
            }
        }

        // Ordinary message: return to the server loop (C sef.c:252-258).
        return Ok(SefReceive {
            source: msg.m_source,
            message: *msg,
            status,
            event: SefEvent::Call(m_type),
        });
    }
}

/// Buffered SEF receive: keeps the IPC behind a struct so tests script the
/// transcript.
pub struct SefLoop<I: SefIpc> {
    pub ipc: I,
}

impl<I: SefIpc> SefLoop<I> {
    /// One `sef_receive_status` pass (ping swallowed inside).
    pub fn receive_status(
        &mut self,
        src: Endpoint,
        msg: &mut Message,
        on_signal: &mut impl FnMut(i32),
    ) -> Result<SefReceive, i32> {
        sef_receive_status(&mut self.ipc, src, msg, on_signal)
    }
}

/// Canned SEF IPC for host tests: a script of incoming messages with
/// status words, plus a notify transcript.
pub struct CannedSefIpc {
    /// (status, message) pairs popped in order; empty = `empty_error`.
    pub inbox: Vec<(i32, Message)>,
    /// Error to return from receive when the inbox is empty (C: ipc_receive
    /// failure, sef.c:173-174 returns `r` directly).
    pub empty_error: i32,
    /// Recorded pong destinations.
    pub pongs: Vec<Endpoint>,
}

impl CannedSefIpc {
    /// Empty inbox, failures on EFAULT.
    pub fn new() -> Self {
        Self { inbox: Vec::new(), empty_error: minix_types::EFAULT, pongs: Vec::new() }
    }

    /// Script one incoming message with its status word.
    pub fn push(&mut self, status: i32, msg: Message) {
        self.inbox.push((status, msg));
    }
}

impl Default for CannedSefIpc {
    fn default() -> Self {
        Self::new()
    }
}

impl SefIpc for CannedSefIpc {
    fn receive(&mut self, _src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        match self.inbox.pop() {
            Some((status, m)) => {
                *msg = m;
                Ok(status)
            }
            None => Err(self.empty_error),
        }
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        self.pongs.push(dest);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn notify_msg() -> Message {
        let mut m = Message::default();
        m.m_type = NOTIFY_MESSAGE;
        m
    }

    fn call_msg(m_type: i32) -> Message {
        let mut m = Message::default();
        m.m_type = m_type;
        m
    }

    /// C sef.c:208-214 — RS ping 拦截:回 pong(notify)后 continue,消息
    /// 不到达调用方。脚本两条 ping + 一条普通消息;普通消息最终返回,
    /// pong 记录两次。
    #[test]
    fn test_ping_intercepted_and_swallowed() {
        let mut ipc = CannedSefIpc::new();
        // C ipc_receive 的 status:notify 时低 16 位 = CALL_NOTIFY(4);
        // ping 的 source 是 RS(sef.h:122 分类按 m_source)。inbox 是 LIFO:
        // 先 push 的后收到,故普通消息先压栈、两个 ping 后压栈。
        ipc.push(4 | 1, call_msg(11)); // 非通知:status 携带别的 call。
        let mut ping2 = notify_msg();
        ping2.m_source = RS_ENDPOINT;
        ipc.push(4, ping2);
        let mut ping = notify_msg();
        ping.m_source = RS_ENDPOINT;
        ipc.push(4, ping);

        let mut pong_count = 0;
        let mut out = Message::default();
        let recv = sef_receive_status(&mut ipc, Endpoint::ANY, &mut out, &mut |_| {
            pong_count += 1;
        })
        .unwrap();

        assert_eq!(ipc.pongs, vec![Endpoint(2), Endpoint(2)], "RS ping → pong ×2");
        assert_eq!(recv.event, SefEvent::Call(11), "普通消息原样上浮");
        assert_eq!(recv.message.m_type, 11);
        let _ = pong_count;
    }

    /// C sef.c:222-226 — SYSTEM 通知 = 信号请求,作为 Signal 事件上浮
    /// (服务器分派自己的 signal handler)。
    #[test]
    fn test_system_notification_surfaces_signal() {
        let mut ipc = CannedSefIpc::new();
        let mut m = notify_msg();
        m.m_source = SYSTEM_ENDPOINT;
        ipc.push(4, m);

        let mut signaled = false;
        let recv = sef_receive_status(&mut ipc, Endpoint::ANY, &mut Message::default(), &mut |_| {
            signaled = true;
        })
        .unwrap();
        assert_eq!(recv.event, SefEvent::Signal(SEF_SIGNAL_REQUEST_TYPE));
        assert_eq!(recv.source, SYSTEM_ENDPOINT);
        assert!(signaled);
    }

    /// 非通知的 RS 消息:is_ipc_notify 为假,不走 ping 分类,原样上浮。
    #[test]
    fn test_rs_non_notification_is_plain_call() {
        let mut ipc = CannedSefIpc::new();
        ipc.push(1 | 2, call_msg(33));

        let recv = sef_receive_status(&mut ipc, Endpoint::ANY, &mut Message::default(), &mut |_| {})
            .unwrap();
        assert_eq!(recv.event, SefEvent::Call(33));
        assert!(ipc.pongs.is_empty());
    }

    /// Canned receive 失败直通(C sef.c:173-174 — `if (r != OK) return r`)。
    #[test]
    fn test_receive_error_passthrough() {
        let mut ipc = CannedSefIpc::new();
        ipc.empty_error = minix_types::EINTR;
        let r = sef_receive_status(&mut ipc, Endpoint::ANY, &mut Message::default(), &mut |_| {});
        assert_eq!(r, Err(minix_types::EINTR));
    }
}
