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
// Mechanical gate for the silent-error-swallowing family (fifth crate, same
// program as minix-sys/rt/fs/kernel): in the SEF framework `ipc.notify` is the
// fire-and-forget pong of C's default callback, and a failure leaves no reply
// to consume. From now on an unjustified new `let _ =` reports an extra
// warning on the spot.
#![warn(clippy::let_underscore_must_use)]

extern crate alloc;

use alloc::vec::Vec;

use minix_sys::ipc::CALL_NOTIFY;
use minix_types::{
    Endpoint, INIT_PROC_NR, KERNEL_SIGNAL_FIRST, KERNEL_SIGNAL_LAST, Message, NOTIFY_MESSAGE,
    RS_INIT, SIGS_SIGNAL_RECEIVED, SigSetBits, sigset_contains,
};

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
    /// Send-and-receive (C: `ipc_sendrec(dest, m)`): the birth reply leg —
    /// C `sef_cb_init_response_rs_reply` sends the RS_INIT report with a
    /// sendrec to RS (`sef_init.c:458-466`), whose receive half doubles as
    /// the next blocking point.
    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<i32, i32>;
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
    /// SYSTEM notification → signal request (C classifies a notification by
    /// source at sef.c:183-193 and intercepts it at sef.c:230-238; the server
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
    //[SD19-gate] notify pong is fire-and-forget: C's default SEF callback is a
    // fire-and-forget notify, with no reply to consume
    #[allow(clippy::let_underscore_must_use)]
    let _ = ipc.notify(source);
}

/// The receiving-loop cancel token — C `sef_cancel()`'s flag, made
/// explicit instead of a library global (`sef.c:291-297` sets
/// `sef_self_receiving = FALSE`; the loop top at `sef.c:161-162` answers
/// with `EINTR`).
///
/// A signal callback that decides the service should stop calls
/// [`SefCancel::cancel`]; [`sef_receive_status`] then stops blocking on new
/// frames and returns `Err(EINTR)`, so the service loop can check its own
/// stop flag and leave (C's documented use: "exit from the main receive
/// loop when a signal handler causes the process to want to shut down").
/// One token per service loop; [`SefIpc`] stays untouched by it.
pub struct SefCancel(core::cell::Cell<bool>);

impl Default for SefCancel {
    fn default() -> Self {
        Self::new()
    }
}

impl SefCancel {
    /// Not cancelled — the state every loop starts in.
    pub const fn new() -> Self {
        Self(core::cell::Cell::new(false))
    }

    /// C: `sef_cancel()` — request that the pending/next receive return
    /// `EINTR` instead of delivering a frame. Callable from inside a signal
    /// callback (single-threaded event loop; `Cell` needs no `UnsafeSync`).
    pub fn cancel(&self) {
        self.0.set(true);
    }

    /// Whether [`Self::cancel`] has been called.
    pub fn is_cancelled(&self) -> bool {
        self.0.get()
    }
}

/// What a service answers when one delivered signal number asks for a
/// decision — the shared three-state vocabulary of the signal axis (defined
/// once here; consumers keep their own trait shapes, this is the common
/// semantics).
///
/// C's handlers all reduce to one of these three answers: keep serving (a
/// handler that returns — or the library default `sef_cb_signal_handler_null`
/// for services that register nothing, sef.h:287), stop now
/// (`fsdriver_terminate()`, fsdriver.c:68-74 — `running = FALSE` +
/// `sef_cancel`), or flush-then-stop (`fs_sync()` before terminate,
/// mfs/main.c:75-77 — the flush belongs to whichever layer owns the mounted
/// state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalAction {
    /// Keep serving.
    Ignore,
    /// Leave the receive loop now.
    Terminate,
    /// Flush whatever state this service owns, then leave. The flush belongs
    /// to the layer holding that state (a file server's `synchronized` hook),
    /// so the loop performs it when it sees this answer.
    SyncThenTerminate,
}

/// C `do_sef_signal_request`'s kernel-notification half
/// (`sef_signal.c:94-113`): walk `SIGK_FIRST..=SIGK_LAST` in ascending order
/// and call the handler once per pending signal.
///
/// A bitmap holding no kernel-signal bit calls the handler once with
/// [`SEF_SIGNAL_REQUEST_TYPE`] instead of a signal number — the documented
/// wake-up fallback described under [`sef_receive_status`]'s contract (the
/// kernel keeps pending signals in a 64-bit `SigSet`, so bits 70..=73 cannot
/// be produced yet).
///
/// This is public because a server's wiring layer sits between the library
/// and its own hook: the file-server runtime's scripted transport has to hand
/// its signal decision the same numbers the library would, and repeating the
/// walk (or the fallback) there would let the two drift.
pub fn dispatch_kernel_notify(set: SigSetBits, on_signal: &mut dyn FnMut(i32)) {
    let mut pending = false;
    for signo in KERNEL_SIGNAL_FIRST..=KERNEL_SIGNAL_LAST {
        if sigset_contains(set, signo as u32) {
            on_signal(signo);
            pending = true;
        }
    }
    if !pending {
        // Zero hits in the window is today's production path; see the fallback
        // note on `sef_receive_status`.
        on_signal(SEF_SIGNAL_REQUEST_TYPE);
    }
}

/// C: `sef_receive_status` — sef.c:149-260. Loop on receive; classify
/// notifications by source; intercept ping (reply pong, swallow); surface
/// the birth request as [`SefEvent::Init`] and signal requests as
/// [`SefEvent::Signal`]; return ordinary messages.
///
/// Live-update/state-transfer interception (`INTERCEPT_SEF_LU_REQUESTS`
/// / `__sef_st_before_receive`) is not modeled: those paths depend on
/// live-update/state-transfer support that has no user-side implementation
/// yet (C keeps them behind `#if INTERCEPT_SEF_LU_REQUESTS`).
///
/// # The `on_signal` contract
///
/// The closure plays the role of C's registered `sef_cb_signal_handler`
/// (`sef.h:269`), so its argument is a signal number. C's
/// `IS_SEF_SIGNAL_REQUEST` (`sef.h:264-266`) has two halves and each one
/// feeds the callback differently:
///
/// | C half | Shape | Callback argument | Message outcome |
/// |---|---|---|---|
/// | `is_ipc_notify && m_source == SYSTEM` (`sef.h:266`) | kernel notification, payload `m_notify.sigset` | one call per pending signal in `SIGK_FIRST..=SIGK_LAST`, ascending (`sef_signal.c:97-102`) | surfaced as [`SefEvent::Signal`] (a Rust-side choice — see the fallback note) |
/// | `m_type == SIGS_SIGNAL_RECEIVED && m_source < INIT_PROC_NR` (`sef.h:265`) | ordinary message from a signal manager, payload `m_pm_lsys_sigs_signal.num` | one call with that signal number (`sef_signal.c:117-128`) | swallowed, like C's `continue` (`sef.c:233-236`) |
///
/// # The wake-up fallback
///
/// A `SYSTEM` notification whose bitmap holds no kernel-signal bit calls the
/// callback once with [`SEF_SIGNAL_REQUEST_TYPE`] instead of a signal number.
/// That is a documented deviation, not a slip: the kernel keeps pending
/// signals in a 64-bit `SigSet`, so bits 70..=73 — the whole kernel-signal
/// window — cannot be produced yet (its own registered limitation,
/// `os/kernel/src/syscall_signal.rs:88-94`), while consumers act on the
/// wake-up half of C's `send_sig`: VM decides on `pending_signo.is_some()`
/// (`os/servers/vm/src/vm_server.rs:1522-1530`) and `uds`/`lwip` latch on
/// any notification arriving. Once the bitmap width lands, real signal
/// numbers flow through the same callback and the fallback stops firing.
///
/// # The escape hatch (`cancel`)
///
/// `cancel` plays the role of C's `sef_cancel()` (`sef.c:291-297`, which
/// sets `sef_self_receiving = FALSE`) and the loop-top check
/// (`sef.c:161-162`, `if (!sef_self_receiving) return EINTR`). A callback
/// that decides the service should stop calls [`SefCancel::cancel`]; the
/// library then returns `Err(EINTR)` instead of delivering another frame
/// — the manager-shaped arm reaches that through its `continue` exactly
/// like C, and the notify-shaped arm checks right after the callback so a
/// cancelled frame is swallowed, not surfaced. Service loops keep C's
/// shape: `Err(EINTR)` → loop top → check the stop flag → leave
/// (`uds.c:1393-1396`, `fsdriver.c:88-90`).
pub fn sef_receive_status(
    ipc: &mut impl SefIpc,
    src: Endpoint,
    msg: &mut Message,
    on_signal: &mut impl FnMut(i32),
    cancel: &SefCancel,
) -> Result<SefReceive, i32> {
    loop {
        // C sef.c:161-162 — the loop top honours a cancel from any earlier
        // callback before blocking again: `if (!sef_self_receiving)
        // return EINTR`. This is the escape hatch's whole mechanism (the
        // manager arm below merely `continue`s, exactly like C's
        // sef.c:233-236, and this check is what turns that into EINTR).
        if cancel.is_cancelled() {
            return Err(minix_types::EINTR);
        }
        // C: ipc_receive(src, m_ptr, &status) — sef.c:168-172.
        let status = ipc.receive(src, msg)?;
        let m_type = msg.m_type;
        let source = msg.m_source;

        // Birth request (the library root of the birth protocol): C
        // `IS_SEF_INIT_REQUEST` (sef.h:33) keys on `m_type == RS_INIT &&
        // m_source == RS` alone — delivery-independent, so the async `RS_INIT`
        // an RS boot sends arrives as a plain (non-notify) request. Surface the
        // init type (sef_init.c:202 `do_sef_init_request`) so the server
        // dispatches its own fresh/LU/restart callback and answers via
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

        // C sef.c:183-193 — notification classification by source.
        if is_ipc_notify(status) {
            if source == SYSTEM_ENDPOINT {
                // The SYSTEM arm of C sef.c:183-193 (notifications rewrite
                // m_type by source) plus sef.c:230-238 (`IS_SEF_SIGNAL_REQUEST`
                // hit hands off to `do_sef_signal_request`): the payload bitmap
                // walks the kernel-signal window in ascending order, one
                // callback call per set bit (sef_signal.c:96-113).
                dispatch_kernel_notify(msg.notify_sigset(), on_signal);
                // If the callback requested cancellation, C swallows the frame
                // inside the library and the loop top returns EINTR, so the
                // frame never surfaces (sef.c:233-236 → :161-162) — do the
                // same here; without cancellation, keep this library's
                // contract of surfacing a Signal event.
                if cancel.is_cancelled() {
                    return Err(minix_types::EINTR);
                }
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

        // The first half of C `IS_SEF_SIGNAL_REQUEST` (sef.h:265): non-notify
        // shape, `m_type == SIGS_SIGNAL_RECEIVED`, and the sender is a boot
        // module (endpoint number < INIT_PROC_NR) — that is, the ordinary
        // message a signal manager produces from a non-terminating signal (PM
        // encodes it at pm/signal.c:470-473, RS at rs/main.c:699-701). The
        // criterion compares the raw endpoint value, the same shape as C's
        // `m_source < INIT_PROC_NR`: a recycled endpoint with a generation bit
        // (generation > 0) is likewise not < 11 in C, so switching to `slot()`
        // would only loosen the check. The position is after the notification
        // band and before the Call fallback: an ordinary Call pays just one
        // extra integer comparison, which also matches C's case order
        // (ping/lu before signal).
        if m_type == SEF_SIGNAL_REQUEST_TYPE && source.get() < INIT_PROC_NR {
            // C sef_signal.c:115-128: the manager-shaped payload is a single
            // number, handed straight to the callback; after
            // `do_sef_signal_request` returns OK, sef.c:233-236 `continue`s —
            // the framework consumes this message and it never reaches the
            // service's dispatch table. Not surfacing an event both matches C
            // and avoids a trap: `SefEvent::Signal` consumers read `m_notify`
            // (e.g. the bitmap access in os/fs/fs-rt/src/ipc.rs), whereas the
            // manager shape's active arm is `m_pm_lsys_sigs_signal`.
            // `sigs_signal_num` is always `Some` under the `m_type` guard; the
            // `if let` just hands that fact to the type system instead of
            // inventing a zero signal number.
            //
            // Escape hatch (in place): a C handler that needs to exit calls
            // `sef_cancel()` (e.g. uds.c:1360, netdriver.c:949, fsdriver.c:73)
            // and then returns normally, and the loop-top check
            // (sef.c:161-162) turns this `continue` into `EINTR` — the service
            // main loop sees the interruption, checks its exit flag and leaves.
            // On the Rust side the callback calls [`SefCancel::cancel`], and
            // this arm's `continue` reaches the same check at the loop top;
            // without cancellation the behavior is word-for-word the same as
            // C's message swallowing.
            if let Some(signo) = msg.sigs_signal_num() {
                on_signal(signo);
            }
            continue;
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
/// Settled after verification: C's `process_init` reply fills
/// `m.m_source = sef_self_endpoint` (libsys/sef_init.c:114), but that is a
/// **defensive shape** — the kernel stamps `m_source` with the sender endpoint
/// on every delivery path (C proc.c:1071-1075; this tree's ipc.rs:1588
/// sendrec, :1986 senda, each pinned by a host test:
/// `test_sendrec_to_blocked_receiver_stamps_source_endpoint`,
/// `test_senda_delivers_to_receiving_target`). The m_source in the sender's
/// user buffer is always overwritten ⇒ leaving m_source=0 here is a **zero
/// difference in production behavior** from C; the difference exists only in
/// the shape-fidelity surface of mock transports, and a mock transport, by
/// definition simulating the kernel, should stamp too. Hence no self_ep
/// parameter (the IPC server's own endpoint is injected by RS and unnamed by
/// the explicit-endpoint design; hard-coding it would violate that). The
/// builder actually leaves `m_source = Endpoint::NONE` (derived Default) —
/// safer than C's memset 0: 0 is a valid PM endpoint, while NONE is explicitly
/// invalid, so a frame escaping onto an unstamped path is exposed at once.
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

/// The birth-protocol orchestration — C `process_init`
/// (`sef_init.c:42-143`) as a sequence owner, not a monolith (the linear
/// kernel calls belong to `minix-sys`, the service's own init semantics stay
/// with the consumer, the RS report is orchestrated here).
///
/// What this face owns, in C's six-paragraph order:
///
/// 1. **Clear the IPC filters** (`sef_init.c:59-60`,
///    `sys_statectl(SYS_STATE_CLEAR_IPC_FILTERS, 0, 0)`) — issued through
///    the `statectl` verb the consumer supplies (production wires
///    `minix_sys::syscall::sys_statectl`; tests script it). Failure is the
///    C `assert(r == OK)` — returned as `Err`, never swallowed.
/// 2. *Registered slot* — the state-transfer grant
///    (`cpf_grant_direct(SELF, 0, ULONG_MAX, CPF_READ)` with the
///    `SEF_STATE_TRANSFER_GID == 0` first-grant convention,
///    sef_init.c:62-67). Not modeled: no consumer of state transfer exists
///    yet; inventing the grant semantics here would be exactly the kind of
///    fabrication this split forbids.
/// 3. *Registered slot* — the debug-init-flags short circuit
///    (`SEF_INIT_CRASH/FAIL/TIMEOUT`, sef_init.c:69-89; the fault-injection
///    family). Not modeled for the same reason.
/// 4. **Dispatch by type** — the consumer's `init` callback answers the
///    result code (fresh/restart/LU semantics are its own; the library
///    only carries the number).
/// 5. **The RS report** — built here with [`sef_init_reply`] and returned;
///    the consumer transmits it over its own reply leg (C
///    `sef_cb_init_response`, sef_init.c:110-117).
/// 6. *Registered slots* — init-buffer `munmap`, `cpf_reload`,
///    `senda_reload`, `SYS_STATE_SET_STATE_TABLE` (sef_init.c:119-143):
///    grant/senda registration lives with the services that own those
///    tables today; the state-table call has no user-side wrapper yet.
///    Same no-invention rule as slots 2 and 3.
///
/// Returns the reply message to transmit, or the kernel error from step 1.
pub fn process_init(
    statectl: &mut dyn FnMut(i32, u64, i32) -> Result<i32, i32>,
    init_type: i32,
    init: &mut dyn FnMut(i32) -> i32,
) -> Result<Message, i32> {
    // Paragraph one (sef_init.c:59-60): C `assert(r == OK)` — failure means
    // the birth failed; return the kernel errno so the consumer fails closed
    // instead of swallowing it.
    let r = statectl(minix_types::SYS_STATE_CLEAR_IPC_FILTERS, 0, 0)?;
    if r != minix_types::OK {
        return Err(r);
    }
    // Paragraph four (:91-108): dispatch by type — consumer semantics; the
    // library only carries the number and the result.
    let result = init(init_type);
    // Paragraph five (:110-117): reply orchestration — the message is built
    // here (m_type = RS_INIT, m_rs_init.result) and sent over the consumer's
    // own reply leg (sendrec stops at the receive(RS) half).
    Ok(sef_init_reply(result))
}

/// C `sef_startup`'s birth conversation (`sef.c:125-141` +
/// `do_sef_init_request`, `sef_init.c:188-215`): block on RS, discard
/// spurious messages that are not `RS_INIT` requests, run the init
/// callback through [`process_init`], and answer RS with the report over a
/// sendrec (`sef_init.c:458-466`). A receive failure propagates — C panics
/// ("unable to ipc_receive from RS"), and this build's callers fail closed
/// the same way.
pub fn sef_await_init<I: SefIpc>(
    ipc: &mut I,
    statectl: &mut dyn FnMut(i32, u64, i32) -> Result<i32, i32>,
    init: &mut dyn FnMut(i32) -> i32,
) -> Result<(), i32> {
    let mut request = Message::zeroed();
    loop {
        let status = ipc.receive(Endpoint::RS, &mut request)?;
        let _ = status;
        // C `IS_SEF_INIT_REQUEST` (`sef.h:33`): `RS_INIT` from RS only —
        // spurious deliveries (an old instance's update requests) are
        // discarded and the block continues.
        if request.m_type == RS_INIT && request.m_source == Endpoint::RS {
            break;
        }
    }
    // C unpacks the parameters from `m_rs_init` (`sef_init.c:193-202`) and
    // hands only the type down; this build's init callbacks key on the type
    // alone.
    let init_type = unsafe { request.m_u.m_rs_init.type_ };
    let mut reply = process_init(statectl, init_type, init)?;
    ipc.sendrec(Endpoint::RS, &mut reply)?;
    Ok(())
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
        cancel: &SefCancel,
    ) -> Result<SefReceive, i32> {
        sef_receive_status(&mut self.ipc, src, msg, on_signal, cancel)
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
    /// Recorded birth replies: `(destination, message)` in send order.
    pub reply_sendrecs: Vec<(Endpoint, Message)>,
}

impl CannedSefIpc {
    /// Empty inbox, failures on EFAULT.
    pub fn new() -> Self {
        Self {
            inbox: Vec::new(),
            empty_error: minix_types::EFAULT,
            pongs: Vec::new(),
            reply_sendrecs: Vec::new(),
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

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        self.reply_sendrecs.push((dest, *msg));
        Ok(0)
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

    /// C sef.c:208-214 — RS ping interception: reply pong (notify) and
    /// continue, so the message never reaches the caller. The script is two
    /// pings plus one ordinary message; the ordinary message is returned in
    /// the end and the pong is recorded twice.
    #[test]
    fn sef_await_init_discards_spurious_then_runs_the_birth() {
        let mut ipc = CannedSefIpc::new();
        // A spurious delivery first (C's loop discards anything that is not
        // an RS_INIT request from RS, sef.c:127-139).
        let mut stale = Message::zeroed();
        stale.m_type = 1234;
        stale.m_source = Endpoint::RS;
        ipc.push(0, stale);
        // The real birth request: RS_INIT, fresh (type 0).
        let mut birth = Message::zeroed();
        birth.m_type = RS_INIT;
        birth.m_source = Endpoint::RS;
        // SAFETY: the m_rs_init arm is the documented payload for RS_INIT.
        unsafe { birth.m_u.m_rs_init.type_ = 0 };
        ipc.push(0, birth);

        let mut statectl_calls = 0usize;
        let mut seen_type: Option<i32> = None;
        let r = sef_await_init(
            &mut ipc,
            &mut |_req, _addr, _len| {
                statectl_calls += 1;
                Ok(0)
            },
            &mut |t| {
                seen_type = Some(t);
                0
            },
        );
        assert_eq!(r, Ok(()));
        assert_eq!(statectl_calls, 1, "process_init 的清过滤腿走过一次");
        assert_eq!(seen_type, Some(0));
        // The reply: one sendrec to RS carrying RS_INIT + result.
        assert_eq!(ipc.reply_sendrecs.len(), 1);
        let (dest, reply) = &ipc.reply_sendrecs[0];
        assert_eq!(*dest, Endpoint::RS);
        assert_eq!(reply.m_type, RS_INIT);
        // SAFETY: the reply's active arm is m_rs_init.
        let result = unsafe { reply.m_u.m_rs_init.result };
        assert_eq!(result, 0);
    }

    #[test]
    fn sef_await_init_propagates_the_receive_failure() {
        let mut ipc = CannedSefIpc::new();
        ipc.empty_error = minix_types::EIO;
        let r = sef_await_init(&mut ipc, &mut |_, _, _| Ok(0), &mut |_| 0);
        assert_eq!(r, Err(minix_types::EIO));
        assert!(ipc.reply_sendrecs.is_empty());
    }

    #[test]
    fn test_ping_intercepted_and_swallowed() {
        let mut ipc = CannedSefIpc::new();
        // C ipc_receive status: for a notify, the low 16 bits = CALL_NOTIFY(4);
        // a ping's source is RS (sef.h:122 classifies by m_source). The inbox
        // is LIFO: whatever is pushed first is received last, so push the
        // ordinary message first and the two pings after it.
        ipc.push(4 | 1, call_msg(11)); // non-notify: status carries another call.
        let mut ping2 = notify_msg();
        ping2.m_source = RS_ENDPOINT;
        ipc.push(4, ping2);
        let mut ping = notify_msg();
        ping.m_source = RS_ENDPOINT;
        ipc.push(4, ping);

        let mut pong_count = 0;
        let mut out = Message::default();
        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut out,
            &mut |_| {
                pong_count += 1;
            },
            &SefCancel::new(),
        )
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

    /// C sef.c:183-193 + sef.c:230-238 — a SYSTEM notification is a signal
    /// request and surfaces as a Signal event (the server dispatches its own
    /// signal handler).
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
            &SefCancel::new(),
        )
        .unwrap();
        assert_eq!(recv.event, SefEvent::Signal(SEF_SIGNAL_REQUEST_TYPE));
        assert_eq!(recv.source, SYSTEM_ENDPOINT);
        assert!(signaled);
    }

    /// The notify-shaped payload: `m_notify.sigset` (C's read at
    /// `sef_signal.c:96`).
    fn system_notify_msg(sigset: minix_types::SigSetBits) -> Message {
        let mut m = notify_msg();
        m.m_source = SYSTEM_ENDPOINT;
        m.m_u.m_notify = minix_types::MessNotify::new(0, 0, sigset);
        m
    }

    /// The manager message shape: an ordinary (non-notify)
    /// `SIGS_SIGNAL_RECEIVED` whose payload is a single number (C
    /// `pm/signal.c:470-473`, `rs/main.c:699-701`).
    fn manager_signal_msg(source: Endpoint, num: i32) -> Message {
        let mut m = Message {
            m_source: source,
            m_type: SIGS_SIGNAL_RECEIVED,
            ..Message::default()
        };
        m.m_u.m_pm_lsys_sigs_signal = minix_types::MessPmLsysSigsSignal {
            num,
            _padding: [0; 52],
        };
        m
    }

    /// C `sef_signal.c:97-102` — the SYSTEM bitmap is walked in ascending
    /// `SIGK_FIRST..SIGK_LAST` order, one callback call per set bit, and the
    /// argument is the signal number rather than the request type. The bit base
    /// is `signo-1` (`sigtypes.h:67-68`), so 71 lands on bit 6 of `bits[2]` and
    /// 74 on bit 9. The event shape is unchanged (consumers such as fs-rt still
    /// read the bitmap from the message themselves).
    #[test]
    fn test_notify_sigset_walks_kernel_signals_ascending() {
        let mut ipc = CannedSefIpc::new();
        ipc.push(4, system_notify_msg([0, 0, (1 << 6) | (1 << 9), 0]));

        let mut seen = Vec::new();
        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |signo| seen.push(signo),
            &SefCancel::new(),
        )
        .unwrap();
        assert_eq!(seen, vec![71, 74], "两位命中＝两次回调，升序");
        assert_eq!(
            recv.event,
            SefEvent::Signal(SEF_SIGNAL_REQUEST_TYPE),
            "事件形状不随位集变化"
        );
    }

    /// Outside the window nothing unfolds: with the low 64 bits all set,
    /// user/system signals 1..=64 are all pending, but they are not the kernel
    /// signal family (C's walk covers only 71..=74), so the callback can only
    /// receive the fallback request type, once.
    #[test]
    fn test_notify_low_word_bits_are_outside_the_kernel_window() {
        let mut ipc = CannedSefIpc::new();
        ipc.push(4, system_notify_msg([u32::MAX, u32::MAX, 0, 0]));

        let mut seen = Vec::new();
        sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |signo| seen.push(signo),
            &SefCancel::new(),
        )
        .unwrap();
        assert_eq!(
            seen,
            vec![SEF_SIGNAL_REQUEST_TYPE],
            "窗口零命中→只走一次兜底回调"
        );
    }

    /// Pins the shape of today's production path: the kernel `SigSet` is still
    /// 64 bits (bits 70..=73 cannot be produced), so every SYSTEM notification
    /// today is an empty bitmap → exactly one callback with the request type as
    /// its argument. This fallback is what VM's wake-up decision and the
    /// uds/lwip termination latch depend on (the "The wake-up fallback" note in
    /// the function header); deleting it silently cuts those two off.
    #[test]
    fn test_notify_empty_sigset_wakes_the_handler_once() {
        let mut ipc = CannedSefIpc::new();
        ipc.push(4, system_notify_msg([0; 4]));

        let mut seen = Vec::new();
        sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |signo| seen.push(signo),
            &SefCancel::new(),
        )
        .unwrap();
        assert_eq!(seen, vec![SEF_SIGNAL_REQUEST_TYPE]);
    }

    /// The first half of C `sef.h:265` plus `sef_signal.c:115-128`: the
    /// manager-shaped number goes to the callback, and the message itself is
    /// swallowed by the framework (`sef.c:233-236`'s `continue`) — so the
    /// service sees not this message but the ordinary one received in the next
    /// round; the pong is unrelated to it.
    #[test]
    fn test_manager_signal_request_is_swallowed_after_the_callback() {
        let mut ipc = CannedSefIpc::new();
        // The inbox is LIFO: push the ordinary message first so it is the one
        // finally returned.
        ipc.push(1, call_msg(11));
        ipc.push(1, manager_signal_msg(Endpoint::PM, 15));

        let mut seen = Vec::new();
        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |signo| seen.push(signo),
            &SefCancel::new(),
        )
        .unwrap();
        assert_eq!(seen, vec![15], "管理器形把载荷号码交给回调");
        assert_eq!(
            recv.event,
            SefEvent::Call(11),
            "被吞的管理器形不上浮，循环取下一条"
        );
        assert!(ipc.pongs.is_empty(), "信号请求不是 ping，不 pong");
    }

    /// The direction sentinel: the same number and message type, but when the
    /// sender is not a boot module (init slot 11 vs user slot 20) it must not be
    /// intercepted — otherwise a legal 0xE00 call a user process sends to the
    /// service would be silently swallowed by SEF (writing
    /// `source >= INIT_PROC_NR` would do exactly that).
    /// The orchestration face (C process_init — sef_init.c:42-143): paragraph
    /// one clears the filters, then dispatch happens by type, and the reply
    /// message is built by the library with the result passed through.
    #[test]
    fn test_process_init_fresh_clears_filters_and_builds_reply() {
        let calls = core::cell::RefCell::new(alloc::vec::Vec::new());
        let reply = process_init(
            &mut |req, addr, len| {
                calls.borrow_mut().push((req, addr, len));
                Ok(minix_types::OK)
            },
            0, // SEF_INIT_FRESH
            &mut |t| {
                assert_eq!(t, 0);
                minix_types::OK
            },
        )
        .unwrap();
        assert_eq!(
            calls.borrow().as_slice(),
            &[(minix_types::SYS_STATE_CLEAR_IPC_FILTERS, 0, 0)],
            "段一恰一次：SYS_STATE_CLEAR_IPC_FILTERS(0,0)"
        );
        assert_eq!(reply.m_type, minix_types::RS_INIT);
        assert_eq!(reply.rs_init_result(), Some(minix_types::OK));
    }

    #[test]
    fn test_process_init_lu_refusal_travels_to_the_reply() {
        // Consumer semantics (LU not modeled → ENOSYS) travel into the reply
        // unchanged — the library does not answer on its behalf.
        let reply = process_init(
            &mut |_req, _addr, _len| Ok(minix_types::OK),
            1, // SEF_INIT_LU
            &mut |_t| minix_types::ENOSYS,
        )
        .unwrap();
        assert_eq!(reply.rs_init_result(), Some(minix_types::ENOSYS));
    }

    #[test]
    fn test_process_init_filter_failure_fails_the_birth() {
        // C paragraph one `assert(r == OK)` (sef_init.c:60): failing to clear
        // the filters means the birth failed — the init callback must not be
        // called (an order proof), and the kernel errno propagates unchanged.
        let init_called = core::cell::Cell::new(false);
        let r = process_init(
            &mut |_req, _addr, _len| Err(minix_types::EPERM),
            0,
            &mut |_t| {
                init_called.set(true);
                minix_types::OK
            },
        );
        assert_eq!(r.unwrap_err(), minix_types::EPERM);
        assert!(!init_called.get(), "段一失败则分派不得到达");
    }

    /// The escape hatch (C `sef_cancel` — sef.c:291-297 + sef.c:161-162): when
    /// a callback inside manager-shaped interception requests cancellation, the
    /// library's `continue` reaches the loop top and closes out as
    /// `Err(EINTR)`, so the frame does not surface; the loop no longer blocks on
    /// the next receive — the deadlock this guard was registered against is
    /// defused.
    #[test]
    fn test_manager_signal_with_cancel_returns_eintr() {
        let mut ipc = CannedSefIpc::new();
        // The fixture is LIFO (`inbox.pop()`), so the frame pushed first
        // arrives later: enqueue the ordinary message first, and the signal
        // frame is the one that arrives on the first receive.
        ipc.push(0, call_msg(11));
        let mut m = call_msg(SIGS_SIGNAL_RECEIVED);
        m.m_source = Endpoint(0); // PM
        m.m_u.m_pm_lsys_sigs_signal.num = 15;
        ipc.push(0, m);

        let cancel = SefCancel::new();
        let seen = core::cell::Cell::new(0);
        let r = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {
                seen.set(seen.get() + 1);
                cancel.cancel();
            },
            &cancel,
        );
        assert_eq!(r.unwrap_err(), minix_types::EINTR);
        assert_eq!(seen.get(), 1, "回调恰被叫一次");
        assert_eq!(ipc.inbox.len(), 1, "取消发生在下一帧之前——普通消息未被消费");
    }

    /// The notify half of the escape hatch: cancellation inside a SYSTEM
    /// notification's callback means the frame is swallowed inside the library
    /// (C's intercepted frame never surfaces, sef.c:233-236 → :161-162) and
    /// `EINTR` is returned; without cancellation the existing contract of
    /// surfacing a Signal event holds.
    #[test]
    fn test_notify_signal_with_cancel_swallows_frame_and_returns_eintr() {
        let mut ipc = CannedSefIpc::new();
        ipc.push(0, call_msg(11)); // LIFO: the last pushed arrives first
        let mut m = notify_msg();
        m.m_source = SYSTEM_ENDPOINT;
        ipc.push(4, m);

        let cancel = SefCancel::new();
        let r = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| cancel.cancel(),
            &cancel,
        );
        assert_eq!(r.unwrap_err(), minix_types::EINTR);
    }

    /// The loop-top check comes before the blocking receive (the positional
    /// semantics of C sef.c:161-162): a cancellation left by the previous
    /// round's callback returns `EINTR` at this round's entry, without asking
    /// the IPC face for a message.
    #[test]
    fn test_pre_cancelled_loop_returns_eintr_without_receiving() {
        let mut ipc = CannedSefIpc::new();
        ipc.empty_error = minix_types::EPERM; // if the library dares to receive, it must fail visibly
        let cancel = SefCancel::new();
        cancel.cancel();
        let r = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
            &cancel,
        );
        assert_eq!(r.unwrap_err(), minix_types::EINTR);
    }

    #[test]
    fn test_signal_request_from_init_slot_or_above_is_a_plain_call() {
        for source in [Endpoint::INIT, Endpoint(20)] {
            let mut ipc = CannedSefIpc::new();
            ipc.push(1, manager_signal_msg(source, 15));

            let mut seen = Vec::new();
            let recv = sef_receive_status(
                &mut ipc,
                Endpoint::ANY,
                &mut Message::default(),
                &mut |signo| seen.push(signo),
                &SefCancel::new(),
            )
            .unwrap();
            assert!(seen.is_empty(), "{source:?} 不是信号管理器");
            assert_eq!(
                recv.event,
                SefEvent::Call(SIGS_SIGNAL_RECEIVED),
                "{source:?} 发的 0xE00 是普通调用，原样上浮"
            );
        }
    }

    /// A non-notify RS message: `is_ipc_notify` is false, so it skips the ping
    /// classification and surfaces unchanged.
    #[test]
    fn test_rs_non_notification_is_plain_call() {
        let mut ipc = CannedSefIpc::new();
        ipc.push(1 | 2, call_msg(33));

        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
            &SefCancel::new(),
        )
        .unwrap();
        assert_eq!(recv.event, SefEvent::Call(33));
        assert!(ipc.pongs.is_empty());
    }

    /// Canned receive failure passes straight through (C sef.c:173-174 —
    /// `if (r != OK) return r`).
    #[test]
    fn test_receive_error_passthrough() {
        let mut ipc = CannedSefIpc::new();
        ipc.empty_error = minix_types::EINTR;
        let r = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
            &SefCancel::new(),
        );
        assert_eq!(r, Err(minix_types::EINTR));
    }

    /// Library root of the birth protocol: RS's `RS_INIT` birth request (an
    /// async delivery, not a notify) surfaces as `SefEvent::Init` carrying
    /// m_rs_init.type (C decides with sef.h:33 and reads type at sef_init.c:202).
    /// Previously the library never constructed Init — the servers' Init arm
    /// was dead code.
    #[test]
    fn test_rs_init_request_surfaces_init_event() {
        let mut ipc = CannedSefIpc::new();
        ipc.push(1, rs_init_msg(1)); // status 1 = non-notify; type 1 = SEF_INIT_LU
        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
            &SefCancel::new(),
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

    /// The birth test keys on type+source and is independent of the delivery
    /// mode (C's IS_SEF_INIT_REQUEST has no notify condition): even when the
    /// status falls in the notification band, Init is still decided first and
    /// the ping branch is not entered by mistake.
    #[test]
    fn test_rs_init_wins_over_notify_band() {
        let mut ipc = CannedSefIpc::new();
        ipc.push(4, rs_init_msg(0)); // status in the notify band (CALL_NOTIFY=4); SEF_INIT_FRESH
        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
            &SefCancel::new(),
        )
        .unwrap();
        assert_eq!(recv.event, SefEvent::Init(0));
        assert!(ipc.pongs.is_empty());
    }

    /// An RS notification whose m_type is not NOTIFY_MESSAGE (not a valid ping)
    /// surfaces as `SefEvent::PingInvalid` (C sef.c:208-214 break path);
    /// previously nothing constructed this variant.
    #[test]
    fn test_rs_non_ping_notification_is_invalid() {
        let mut ipc = CannedSefIpc::new();
        let mut m = call_msg(999); // RS notification but not a ping message
        m.m_source = RS_ENDPOINT;
        ipc.push(4, m); // status in the notify band (CALL_NOTIFY=4)
        let recv = sef_receive_status(
            &mut ipc,
            Endpoint::ANY,
            &mut Message::default(),
            &mut |_| {},
            &SefCancel::new(),
        )
        .unwrap();
        assert_eq!(recv.event, SefEvent::PingInvalid);
        assert!(ipc.pongs.is_empty(), "无效 ping 不pong");
    }

    /// The shared reply helper: `sef_init_reply` builds the `RS_INIT`+result
    /// message sent back to RS (C process_init tail, sef_init.c:113-117, sent
    /// via ipc_sendrec(RS)).
    #[test]
    fn test_sef_init_reply_carries_result() {
        // Shape anchor: leaving m_source at 0 is not a defect — the kernel
        // stamps on delivery (ipc.rs:1588 sendrec, :1986 senda), always setting
        // the delivered value to the sender endpoint, the same shape as C
        // proc.c:1071-1075; C's sef_init.c:114 filling sef_self_endpoint is a
        // defensive shape that gets overwritten. Read the settled note above
        // before changing this assertion.
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
