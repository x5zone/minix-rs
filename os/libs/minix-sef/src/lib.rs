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
    /// RS birth request with `RS_INIT` (C sef.c:196-206, `IS_SEF_INIT_REQUEST`
    /// sef.h:33). Carries the init type: 0 = FRESH, 1 = LU, 2 = RESTART
    /// (sef.h:93-95). The server runs its own init callback and answers with
    /// [`sef_init_reply`].
    Init(i32),
    /// RS notification that is not a valid ping: C's `do_sef_ping_request`
    /// did not swallow it and the message falls through the switch
    /// (sef.c:208-214 `break` path) for the server to deliver.
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
/// notifications by source; intercept ping (reply pong, swallow); surface
/// the birth request as [`SefEvent::Init`] and signal requests as
/// [`SefEvent::Signal`]; return ordinary messages.
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
        let source = msg.m_source;

        // Birth request (E-BIRTHFACE library root): C `IS_SEF_INIT_REQUEST`
        // (sef.h:33) keys on `m_type == RS_INIT && m_source == RS` alone —
        // delivery-independent, so the async `RS_INIT` an RS boot sends
        // arrives as a plain (non-notify) request. Surface the init type
        // (sef_init.c:202 `do_sef_init_request`) so the server dispatches
        // its own fresh/LU/restart callback and answers via
        // [`sef_init_reply`]; SEF cannot run the server-specific callback.
        if m_type == SEF_INIT_REQUEST_TYPE && source == RS_ENDPOINT {
            let init_type = msg.rs_init_type().unwrap_or(0); // SEF_INIT_FRESH
            return Ok(SefReceive {
                source,
                message: *msg,
                status,
                event: SefEvent::Init(init_type),
            });
        }

        // C sef.c:174-191 — notification classification by source.
        if is_ipc_notify(status) {
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
            if source == RS_ENDPOINT {
                if m_type == SEF_PING_REQUEST_TYPE {
                    // C sef.c:208-214 + sef_ping.c:21-38 — valid ping: reply
                    // pong and continue (never returned to the caller).
                    pong_via_ipc(ipc, source);
                    continue;
                }
                // RS notification that is not a valid ping (sef.c:208-214
                // `break` path): C's `do_sef_ping_request` did not swallow it,
                // so it falls through for the server to deliver.
                return Ok(SefReceive {
                    source,
                    message: *msg,
                    status,
                    event: SefEvent::PingInvalid,
                });
            }
        }

        // Ordinary message: return to the server loop (C sef.c:252-258).
        return Ok(SefReceive {
            source,
            message: *msg,
            status,
            event: SefEvent::Call(m_type),
        });
    }
}

/// Build the SEF birth reply a server sends back to RS once its init
/// callback has run — C: the `process_init` tail (sef_init.c:113-117),
/// which fills an `RS_INIT` message with `m_rs_init.result` and delivers it
/// to RS through `sef_cb_init_response_rs_reply` (`ipc_sendrec(RS_PROC_NR,
/// m)`, sef_init.c:458-466).
///
/// This is the shared counterpart of [`SefEvent::Init`]: `sef_receive_status`
/// surfaces the request, the server runs its own init work, then sends this
/// message (result `0`/`OK` on success) to release RS's boot step3. Kept a
/// pure builder so it needs no new [`SefIpc`] verb — each server already owns
/// a send round trip. Mirrors the `run_birth` reply in the driver/FS
/// runtimes (`minix-driver-rt`, `fs-rt`).
/// P-ALL-08 T6 定谳（[待验证] 已结）：C 的 `process_init` 应答填
/// `m.m_source = sef_self_endpoint`（libsys/sef_init.c:114），但那是
/// **防御性形状**——内核在每条投递路径上都会把 `m_source` 盖成发送者
/// 端点（C proc.c:1071-1075；本树 ipc.rs:1588 sendrec、:1986 senda，
/// 各有宿主测试钉住：`test_sendrec_to_blocked_receiver_stamps_source_
/// endpoint`、`test_senda_delivers_to_receiving_target`）。发送者用户
/// 缓冲里的 m_source 恒被覆盖 ⇒ 本 builder 留 m_source=0 与 C 在
/// **生产行为上零差异**；差异仅存在于 mock 传输的形状保真面，而 mock
/// 传输按定义模拟内核行为也应盖章。故不加 self_ep 参数（is/ipc-server
/// 的自身端点按 A-9 由 RS 注入不命名，硬编码会违反 A-9）。builder 实际
/// 留 `m_source = Endpoint::NONE`（derive Default）——比 C 的 memset 0 更
/// 防炸：0 是 PM 的有效端点，NONE 显式无效，混进未盖章路径立刻暴露。
pub fn sef_init_reply(result: i32) -> Message {
    let mut m = Message {
        m_type: SEF_INIT_REQUEST_TYPE,
        ..Message::default()
    };
    // Union field *write* is safe (only reads need the `m_type` tag guard);
    // the active arm is `m_rs_init`, the same one the reply carries in C.
    m.m_u.m_rs_init.result = result;
    m
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
        Self {
            inbox: Vec::new(),
            empty_error: minix_types::EFAULT,
            pongs: Vec::new(),
        }
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

    /// An `RS_INIT` birth request from RS carrying `init_type` in the
    /// `m_rs_init` arm (C: RS boot async-sends `m_type = RS_INIT`; the type
    /// field is what `IS_SEF_INIT_REQUEST` + `do_sef_init_request` read).
    fn rs_init_msg(init_type: i32) -> Message {
        let mut m = Message {
            m_source: RS_ENDPOINT,
            m_type: RS_INIT,
            ..Message::default()
        };
        // Union field write is safe; the `RS_INIT` tag selects `m_rs_init`.
        m.m_u.m_rs_init.type_ = init_type;
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

        assert_eq!(
            ipc.pongs,
            vec![Endpoint(2), Endpoint(2)],
            "RS ping → pong ×2"
        );
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
        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {
                signaled = true;
            },
        )
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

        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(recv.event, SefEvent::Call(33));
        assert!(ipc.pongs.is_empty());
    }

    /// Canned receive 失败直通(C sef.c:173-174 — `if (r != OK) return r`)。
    #[test]
    fn test_receive_error_passthrough() {
        let mut ipc = CannedSefIpc::new();
        ipc.empty_error = minix_types::EINTR;
        let r = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
        );
        assert_eq!(r, Err(minix_types::EINTR));
    }

    /// E-BIRTHFACE 库根：RS 的 `RS_INIT` 出生请求（非 notify 的异步投递）
    /// 上浮为 `SefEvent::Init`，携带 m_rs_init.type（C sef.h:33 判定 +
    /// sef_init.c:202 读 type）。此前库从不构造 Init——服务器 Init 臂死代码。
    #[test]
    fn test_rs_init_request_surfaces_init_event() {
        let mut ipc = CannedSefIpc::new();
        ipc.push(1, rs_init_msg(1)); // status 1 = 非通知；type 1 = SEF_INIT_LU
        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(
            recv.event,
            SefEvent::Init(1),
            "RS_INIT → Init(init_type) 上浮"
        );
        assert_eq!(recv.source, RS_ENDPOINT);
        assert!(
            ipc.pongs.is_empty(),
            "出生请求应答归服务器，SEF 不代答 pong"
        );
    }

    /// 出生判定按 type+source、与投递方式无关（C IS_SEF_INIT_REQUEST 无
    /// notify 条件）：即便 status 落在通知段，仍先判 Init，不误入 ping 分支。
    #[test]
    fn test_rs_init_wins_over_notify_band() {
        let mut ipc = CannedSefIpc::new();
        ipc.push(4, rs_init_msg(0)); // status 通知段 (CALL_NOTIFY=4)；SEF_INIT_FRESH
        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(recv.event, SefEvent::Init(0));
        assert!(ipc.pongs.is_empty());
    }

    /// H-22：RS 的通知但 m_type 非 NOTIFY_MESSAGE（不是有效 ping）上浮为
    /// `SefEvent::PingInvalid`（C sef.c:208-214 break 路径），此前无人构造。
    #[test]
    fn test_rs_non_ping_notification_is_invalid() {
        let mut ipc = CannedSefIpc::new();
        let mut m = call_msg(999); // RS 通知但非 ping 消息
        m.m_source = RS_ENDPOINT;
        ipc.push(4, m); // status 通知段 (CALL_NOTIFY=4)
        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(recv.event, SefEvent::PingInvalid);
        assert!(ipc.pongs.is_empty(), "无效 ping 不pong");
    }

    /// 共享应答助手：`sef_init_reply` 造出回 RS 的 `RS_INIT`+result 消息
    /// （C process_init 尾部 sef_init.c:113-117，经 ipc_sendrec(RS) 送出）。
    #[test]
    fn test_sef_init_reply_carries_result() {
        // T6 形状锚：m_source 留 0 不是缺陷——内核投递盖章（ipc.rs:1588
        // sendrec、:1986 senda）把送达值恒置为发送者端点，与 C proc.c:
        // 1071-1075 同形；C 的 sef_init.c:114 填 sef_self_endpoint 是
        // 被覆盖前的防御性形状。改动此断言前先读上方 T6 定谳注释。
        let ok = sef_init_reply(minix_types::OK);
        assert_eq!(
            ok.m_source,
            minix_types::Endpoint::NONE,
            "builder leaves m_source at the NONE sentinel; the kernel stamps the sender endpoint on delivery"
        );
        assert_eq!(ok.m_type, RS_INIT);
        assert_eq!(ok.rs_init_result(), Some(minix_types::OK));
        let refused = sef_init_reply(minix_types::ENOSYS);
        assert_eq!(refused.rs_init_result(), Some(minix_types::ENOSYS));
    }
}
