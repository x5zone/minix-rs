//! The two wires SCHED talks on: messages with the world, calls with the kernel.
//!
//! The loop (server.rs, 02) needs seven verbs — receive, send, get machine,
//! read the clock rate, register a takeover, send scheduling parameters, arm
//! the balance bell (`main.c:39,103,130` + `schedule.c:218,321,340`). This
//! module owns the seams and nothing else: which verbs exist, what they carry,
//! and how the real ends are assembled. The policy that decides when to call
//! them lives in server.rs (02); the wire packing half of the kernel calls
//! lives beside its siblings (schedctl.rs, schedule.rs).
//!
//! Two traits, not one, because C itself talks on two channels (`ipc_send`
//! versus `sys_*`, `minix3/minix/lib/libsys/`): messages come from peers,
//! calls go to the kernel, and a test double for one need not script the
//! other. The same split is the house style — VM pairs an IPC transport with
//! a kernel gateway (`os/servers/vm/src/ipc/transport.rs:120`), RS folds all
//! kernel calls into one `KernelApi` (`os/servers/rs/src/boot.rs:69`); this
//! module keeps both halves narrow enough that neither shape is missed.
//!
//! The real ends are final-shape but honest: every call is packed exactly as
//! production will send it and delegated to minix-sys's direct transports.
//! Until the user-space trap layer lands, those report `EIO` — the shape is
//! committed, the power comes later (following the VM transport precedent in
//! 02-stage-vm/todo.md).

use crate::kernel_api::schedctl::SchedctlCall;
use crate::kernel_api::schedule::Fanout;
use crate::sef::MachineInfo;
use minix_sys::ipc::IpcTransport as SysIpcTransport;
use minix_sys::ipc::{DirectTrapTransport, IpcStatus, TrapStatus};
use minix_sys::syscall::{DirectKernelCallTransport, perform_kernel_call};
use minix_types::{
    Endpoint, GET_HZ, GET_MACHINE, MessLsysKrnSchedule, MessLsysKrnSysGetinfo,
    MessLsysKrnSysSetalarm, Message, MessageUnion, SYS_GETINFO, SYS_SCHEDCTL, SYS_SCHEDULE,
    SYS_SETALARM,
};

/// Whether a receive status names a notification (C: `is_ipc_notify`,
/// com.h:92 — the call field equals `NOTIFY`; `CALL_NOTIFY` is 4,
/// minix-sys ipc.rs:53). minix-sys exposes the call number but not this
/// predicate, so it lives here where the loop (02) reads it.
pub const fn is_notify(status: IpcStatus) -> bool {
    status.call() == minix_sys::ipc::CALL_NOTIFY
}

/// The message wire: receive anything, send back.
///
/// Narrow on purpose — SCHED uses two of minix-sys's seven primitives
/// (`main.c:39` receives, `main.c:103` sends; no sendrec, no notify from
/// this side). Errors are Minix3 errno values (`i32`), the crate-wide
/// convention (table.rs:38's `SlotVerdict::errno` is the same contract).
pub trait IpcTransport {
    /// Wait for a message from anyone (C: `sef_receive_status(ANY, ...)`,
    /// `main.c:39`). Returns the message with its decoded status word —
    /// the status is how notifications and the kernel seal are recognized
    /// (02's `classify` and `noquantum_trust` consume the two halves).
    fn receive(&self) -> Result<(Message, IpcStatus), i32>;

    /// Send a message (C: `ipc_send`, `main.c:103` — the reply path).
    fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32>;

    /// Non-blocking send (C: `ipc_sendnb`) — replies must be non-blocking to avoid
    /// the PM↔sched mutual block of two blocking sends (confirmed with ELOCKED).
    fn sendnb(&self, to: Endpoint, message: &Message) -> Result<(), i32>;
}

/// Maps a trap failure to the crate's errno convention.
///
/// minix-sys reports trap failures as positive errno statuses
/// (`Err(TrapStatus(EIO))`, ipc.rs:531); the seam surfaces the same number
/// as an `i32`. The mapping lives in one place so the trap layer can refine it
/// without touching callers.
const fn trap_errno(status: TrapStatus) -> i32 {
    status.0
}

/// A zeroed message with a neutral source, ready for a receive or a call.
fn blank_message() -> Message {
    Message {
        m_source: Endpoint::ANY,
        m_type: 0,
        m_u: MessageUnion::zeroed(),
    }
}

/// The real message end: final call shape through minix-sys's direct trap
/// transport (`EIO` until the trap bodies land, ipc.rs:523-525).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelIpcTransport {
    inner: DirectTrapTransport,
}

impl KernelIpcTransport {
    /// Assemble the production end. No fields today; the type exists so the
    /// binary names its intent (this is the transport a real process gets).
    pub const fn new() -> Self {
        Self {
            inner: DirectTrapTransport,
        }
    }
}

impl Default for KernelIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcTransport for KernelIpcTransport {
    fn receive(&self) -> Result<(Message, IpcStatus), i32> {
        // C: sef_receive_status(ANY, &m_in, &ipc_status) — main.c:39. The
        // raw trap is only the innermost leg: the SEF layer on top answers
        // RS pings in place (pong, swallow — sef.c:208-214) so a liveness
        // probe never lands in the dispatch table, and swallows SYSTEM
        // signal requests (SCHED registers no handler, main.c:118; the
        // no-handler default is OK, so sef.c:232-237 `continue`s). A
        // surfaced signal would earn a no_sys reply C never sends.
        //
        // The send leg of the birth reply: C's default
        // `sef_cb_init_response_rs_reply` is `ipc_sendrec(RS_PROC_NR, &m)`
        // (sef_init.c:458-466, SEF_CB_INIT_RESPONSE_DEFAULT=sef.h:90) — not a plain
        // send. sendrec has two-phase semantics: after sending RS_INIT it parks in
        // the receive(RS) half (the kernel sets REPLY_PEND, getfrom=RS) and waits to
        // be woken by the OK that RS's catch exit replies with. The old
        // implementation used a blocking send, so REPLY_PEND was never set, RS's
        // reply was unclaimed → a stray service consumed it → the echo went back to
        // RS → step3 catch received a non-RS_INIT → boot.rs:1254 panic.
        let mut sef = SefIpcAdapter { inner: self.inner };
        let mut send_reply =
            |m: &mut Message| self.inner.sendrec(Endpoint::RS, m).map_err(trap_errno);
        // Birth protocol phase one: the real wiring of C `sys_statectl`'s trap direct call.
        let mut statectl = |req: i32, addr: u64, len: i32| {
            let r = minix_sys::syscall::sys_statectl(
                &minix_sys::syscall::DirectKernelCallTransport,
                req,
                addr,
                len,
            );
            if r == minix_types::OK { Ok(r) } else { Err(r) }
        };
        sef_filtered_receive(&mut sef, &mut send_reply, &mut statectl)
    }

    fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
        // C: ipc_send(who_e, m_ptr) — main.c:103.
        self.inner.send(to, message).map_err(trap_errno)
    }

    fn sendnb(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
        // C: ipc_sendnb (non-blocking reply semantics).
        self.inner.sendnb(to, message).map_err(trap_errno)
    }
}

/// Verb adapter for `minix-sef` (trap direct): `receive` returns the status word
/// (both notifications and the kernel seal are read from it), and `notify` is the
/// ping reply channel (the `ipc_notify` of C `do_sef_ping_request`, sef_ping.c:21-38).
struct SefIpcAdapter {
    inner: DirectTrapTransport,
}

impl minix_sef::SefIpc for SefIpcAdapter {
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        let sts = self.inner.receive(src, msg).map_err(trap_errno)?;
        Ok(sts.0 as i32)
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        self.inner.notify(dest).map_err(trap_errno)
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        // The birth reply leg (C `ipc_sendrec(RS, &m)`, sef_init.c:458-466):
        // the reply overwrites msg in place; C's sendrec answers through the
        // reply's m_type, so a delivered frame is success here.
        self.inner.sendrec(dest, msg).map(|_| 0).map_err(trap_errno)
    }
}

/// Receive loop through the SEF layer (the main-loop contract of C
/// `sef_receive_status`): ordinary calls and notifications surface unchanged (the
/// status word rides along; 02's classify and `noquantum_trust` consume its two
/// halves); signals are swallowed and the loop waits for the next message; RS's
/// init request goes through the **birth face**.
///
/// `SefEvent::Signal` is swallowed here because SCHED registers no signal handler —
/// C's `sef_receive_status` likewise swallows them for a service without a handler
/// (handler result OK → `continue`). Evidence: grepping the whole
/// `minix3/minix/servers/sched/` directory for `sef_setcb_signal_handler` yields
/// zero hits; without registration the library default is the null function
/// `sef_cb_signal_handler_null` (sef.h:287 `SEF_CB_SIGNAL_HANDLER_DEFAULT`, an
/// empty body at sef_signal.c:156-158), so a SYSTEM notification is rewritten into
/// a signal request inside the library (sef.c:187), consumed by the null handler,
/// and the framework `continue`s (sef.c:231-235); the main loop has no observable
/// behavior on the signal face — the empty closure here is equivalent one for one.
///
/// **Birth face** (RS_INIT previously surfaced into the dispatch table, drew a
/// no_sys reply, and RS's boot-time catch blocked forever): C `sef_startup` blocks
/// waiting for RS's init request (sef.c:125-131, a do-while that leaves only on
/// `IS_SEF_INIT_REQUEST`; sef.h:33-34 decides by type+source regardless of delivery
/// mode; the main loop arm at sef.c:197-203 intercepts again), then replies
/// `RS_INIT+result` once the callback finishes (the tail of process_init,
/// sef_init.c:113-121). Rust-side sched init work — `sys_getmachine` +
/// `init_scheduling` — already completes before the loop in main (equivalent to
/// the completion semantics of C `sef_cb_init_fresh` main.c:126-136; devman's
/// "built during construction" has the same shape), so the reply happens on the
/// first received packet: fresh answers `RS_INIT+OK` then enters the main loop;
/// LU/RESTART are honestly refused with `ENOSYS` and the process stops (the same
/// family protocol as the devman/fs-rt birth faces, the fail-closed counterpart of
/// C panicking on init failure).
fn sef_filtered_receive<S: minix_sef::SefIpc>(
    sef: &mut S,
    send_reply: &mut dyn FnMut(&mut Message) -> Result<(), i32>,
    statectl: &mut dyn FnMut(i32, u64, i32) -> Result<i32, i32>,
) -> Result<(Message, IpcStatus), i32> {
    // SCHED registers no signal handler in C (anchored in the doc comment above);
    // the escape-hatch token exists only to satisfy the library contract — nobody
    // cancels it.
    let cancel = minix_sef::SefCancel::new();
    loop {
        let mut message = blank_message();
        let recv =
            minix_sef::sef_receive_status(sef, Endpoint::ANY, &mut message, &mut |_| {}, &cancel)?;
        // Birth interception precedes event classification: RS_INIT is decided by
        // type+source per sef.h:33-34 regardless of the event shape it arrives in
        // (minix-sef produces no Init event; asynchronous delivery uses the Call
        // shape).
        if recv.message.m_type == minix_types::RS_INIT && recv.source == Endpoint::RS {
            // SAFETY: the active union arm of a birth request is `m_rs_init`
            // (m_type == RS_INIT and source RS; both gates above passed).
            let kind = unsafe { recv.message.m_u.m_rs_init.type_ };
            // Birth protocol orchestration belongs to the library: phase one clears
            // the IPC filter (C `process_init`'s first phase, the `assert(r == OK)` at
            // sef_init.c:59-60; the statectl verb is injected by the caller), the reply
            // message is built by the library (equivalent to the old hand-built
            // `m.m_rs_init.result = result`); the dispatch semantics (sched's init work
            // already completed before the main loop ≡ `sef_cb_init_fresh` completion
            // semantics → fresh OK; LU/RESTART honestly refused with `ENOSYS`) stay
            // unchanged.
            // C's phase-one assert = fail-closed stop when birth fails.
            let mut reply = minix_sef::process_init(statectl, kind, &mut |t| {
                if t == 0 {
                    minix_types::OK
                } else {
                    minix_types::ENOSYS
                }
            })?;
            send_reply(&mut reply)?;
            if reply.rs_init_result() == Some(minix_types::OK) {
                continue; // birth answered; swallow this message and continue the main loop
            }
            return Err(minix_types::ENOSYS); // init refused: fail-closed stop
        }
        match recv.event {
            // Ordinary messages and "RS notifications that are not pings" (C's `break` path) surface as usual.
            minix_sef::SefEvent::Call(_) | minix_sef::SefEvent::PingInvalid => {
                return Ok((recv.message, IpcStatus(recv.status as u32)));
            }
            // Signal request without a handler: swallow and keep waiting (C sef.c:232-237).
            minix_sef::SefEvent::Signal(_) | minix_sef::SefEvent::Init(_) => continue,
        }
    }
}

/// The kernel-call wire: the five calls SCHED makes downstairs.
///
/// One method per C call site, named for the C entry point. Errors are the
/// errno the kernel answered with (`i32`); a call that never got an answer
/// reports the transport's errno (`EIO` before the trap layer). The wire packing for
/// `schedctl` and `schedule` reuses the mirrors that already exist
/// (schedctl.rs:137, schedule.rs:122-154); `get_machine`/`get_hz` pack the
/// GETMINFO pointer contract here, where the buffer is a local.
pub trait KernelApi {
    /// Read the machine (C: `sys_getmachine(&machine)`, `main.c:130`).
    fn get_machine(&mut self) -> Result<MachineInfo, i32>;

    /// Read the clock rate (C: `sys_hz()`, `sysutil.h:60` — a cached
    /// GET_HZ query, `com.h:333`).
    fn get_hz(&mut self) -> Result<u32, i32>;

    /// Register a takeover (C: `sys_schedctl`, `schedule.c:218`).
    fn schedctl(&mut self, call: &SchedctlCall) -> Result<(), i32>;

    /// Send scheduling parameters (C: `sys_schedule`, `schedule.c:321`).
    fn schedule(&mut self, fanout: &Fanout) -> Result<(), i32>;

    /// Arm the balance bell (C: `sys_setalarm`, `schedule.c:340,367`).
    fn setalarm(&mut self, ticks: u32) -> Result<(), i32>;
}

/// The safecopy buffer for GET_MACHINE (C: `struct machine`, type.h:122-131;
/// the kernel's copy source is the same layout,
/// `os/kernel/src/misc.rs:294-308`). GETMINFO carries a pointer, not a
/// payload (`os/kernel/src/misc.rs:1049` safecopies into caller memory), so
/// the buffer must match the C struct byte for byte — SCHED reads the first
/// two fields and lets the rest ride.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct MachineInfoBuf {
    /// C: `unsigned processors_count` — type.h:123.
    processors_count: u32,
    /// C: `unsigned bsp_id` — type.h:124.
    bsp_id: u32,
    /// C: `int padding` (used to be protected) — type.h:126.
    padding: i32,
    /// C: `int apic_enabled` — type.h:127.
    apic_enabled: i32,
    /// C: `phys_bytes acpi_rsdp` — type.h:128.
    acpi_rsdp: u64,
    /// C: `unsigned int board_id` — type.h:130.
    board_id: u32,
}

/// The real kernel end: packs each call into its wire message and sends it
/// through minix-sys's kernel-call protocol (`perform_kernel_call`,
/// syscall.rs:201 — the ENOTREADY retry ring is C's `_kernel_call` verbatim).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysKernelApi {
    calls: DirectKernelCallTransport,
}

impl SysKernelApi {
    /// Assemble the production end.
    pub const fn new() -> Self {
        Self {
            calls: DirectKernelCallTransport,
        }
    }

    /// One kernel call: pack, send, decode the errno convention.
    ///
    /// `perform_kernel_call` returns the raw reply message type; the kernel
    /// writes failures as negative values into `m_type` (syscall.rs:119-124),
    /// so a negative reply is `-errno` and a non-negative one is `OK` —
    /// the same reading every minix-sys caller makes.
    fn call(&mut self, call_number: i32, pack: impl FnOnce(&mut Message)) -> Result<(), i32> {
        let mut message = blank_message();
        pack(&mut message);
        let reply = perform_kernel_call(&self.calls, call_number, &mut message, |_| {});
        if reply < 0 { Err(-reply) } else { Ok(()) }
    }
}

impl Default for SysKernelApi {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelApi for SysKernelApi {
    fn get_machine(&mut self) -> Result<MachineInfo, i32> {
        // C: sys_getmachine — getmachine.c packs GETMINFO(GET_MACHINE) with
        // a pointer to the caller's `machine` (kernel answers by safecopy,
        // misc.rs:1038-1050). The pointer cast itself needs no unsafe: the
        // write happens kernel-side, into this local, via the trap.
        let mut buf = MachineInfoBuf::default();
        let rv = self.call(SYS_GETINFO, |message| {
            message.m_u.m_lsys_krn_sys_getinfo = MessLsysKrnSysGetinfo {
                request: GET_MACHINE,
                endpt: 0,
                val_ptr: &mut buf as *mut MachineInfoBuf as u64,
                val_len: core::mem::size_of::<MachineInfoBuf>() as i32,
                val_ptr2: 0,
                val_len2_e: 0,
                _padding: [0; 20],
            };
        });
        // Read-back without unsafe: the kernel safecopied into `buf` before
        // the reply returned (misc.rs:1049); pre-E1 the call errs above and
        // the zeroed buffer is never read as data.
        rv.map(|()| MachineInfo {
            processors_count: buf.processors_count,
            bsp_id: buf.bsp_id,
        })
    }

    fn get_hz(&mut self) -> Result<u32, i32> {
        // C: sys_hz — sysutil.h:60 caches GETMINFO(GET_HZ) (kernel copies a
        // single i32, misc.rs:1033-1036). Fresh query each time: the cache
        // is libsys's convenience, not the contract (the balancer asks once
        // per boot, server.rs `init_scheduling`).
        let mut hz: u32 = 0;
        let rv = self.call(SYS_GETINFO, |message| {
            message.m_u.m_lsys_krn_sys_getinfo = MessLsysKrnSysGetinfo {
                request: GET_HZ,
                endpt: 0,
                val_ptr: &mut hz as *mut u32 as u64,
                val_len: core::mem::size_of::<u32>() as i32,
                val_ptr2: 0,
                val_len2_e: 0,
                _padding: [0; 20],
            };
        });
        rv.map(|()| hz)
    }

    fn schedctl(&mut self, call: &SchedctlCall) -> Result<(), i32> {
        // C: sys_schedctl — kernel_call.c packs the wire (the mirror's
        // order is the contract, schedctl.rs:131-149).
        self.call(SYS_SCHEDCTL, |message| {
            message.m_u.m_lsys_krn_schedctl = call.wire();
        })
    }

    fn schedule(&mut self, fanout: &Fanout) -> Result<(), i32> {
        // C: sys_schedule — sys_schedule.c packs endpoint, quantum,
        // priority, cpu, niced (the kernel reads them in that order,
        // doc 02 §2.7 row 6). The `KEEP` rendering lives on the Fanout.
        self.call(SYS_SCHEDULE, |message| {
            message.m_u.m_lsys_krn_schedule = MessLsysKrnSchedule {
                endpoint: fanout.endpoint.0,
                quantum: fanout.wire_quantum_ms(),
                priority: fanout.wire_priority(),
                cpu: fanout.wire_cpu(),
                niced: fanout.wire_niced(),
                _padding: [0; 36],
            };
        })
    }

    fn setalarm(&mut self, ticks: u32) -> Result<(), i32> {
        // C: sys_setalarm(balance_timeout, 0) — relative expiry
        // (`abs_time` 0), schedule.c:340. The reply's time_left/uptime
        // fields are news SCHED never reads.
        self.call(SYS_SETALARM, |message| {
            message.m_u.m_lsys_krn_sys_setalarm = MessLsysKrnSysSetalarm {
                exp_time: ticks as u64,
                time_left: 0,
                uptime: 0,
                abs_time: 0,
                _padding: [0; 28],
            };
        })
    }
}

#[cfg(test)]
mod sef_filter_tests {
    //! Interception contract of `sef_filtered_receive` (host-repeatable): a ping is
    //! answered in-layer (pong) and swallowed, signal requests without a handler are
    //! swallowed, and real calls and notifications surface unchanged. C counterpart:
    //! sef.c:208-214 (ping) and sef.c:232-237 (signals).

    use super::*;
    use core::cell::RefCell;

    /// Scripted SEF verbs: emit messages per the script and record every notify (the evidence of a pong).
    struct ScriptedSef {
        script: RefCell<std::vec::Vec<(i32, Message)>>,
        notified: RefCell<std::vec::Vec<Endpoint>>,
    }

    impl ScriptedSef {
        fn new(script: Vec<(i32, Message)>) -> Self {
            Self {
                script: RefCell::new(script),
                notified: RefCell::new(std::vec::Vec::new()),
            }
        }
    }

    impl minix_sef::SefIpc for ScriptedSef {
        fn receive(&mut self, _src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
            let mut script = self.script.borrow_mut();
            if script.is_empty() {
                return Err(minix_types::EIO);
            }
            let (status, m) = script.remove(0);
            *msg = m;
            Ok(status)
        }

        fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
            self.notified.borrow_mut().push(dest);
            Ok(())
        }

        fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<i32, i32> {
            // The birth reply rides the same script face; recorded in
            // `notified`-style bookkeeping is not needed for the existing
            // pins, so this answers delivered and hands the message back.
            self.notified.borrow_mut().push(dest);
            let _ = msg;
            Ok(0)
        }
    }

    fn arrival(source: Endpoint, m_type: i32, notify: bool) -> (i32, Message) {
        let mut m = Message {
            m_source: source,
            m_type,
            ..Message::default()
        };
        let _ = &mut m;
        let status = if notify {
            IpcStatus::from_call(minix_sys::ipc::CALL_NOTIFY)
        } else {
            IpcStatus::from_call(minix_sys::ipc::CALL_RECEIVE)
        };
        (status.0 as i32, m)
    }

    #[test]
    fn ping_is_ponged_and_swallowed_signal_swallowed_call_surfaces() {
        // Script: RS ping (RS probes liveness with it; no pong means the service is
        // dead) → SYSTEM signal request → real call. Only the third surfaces; exactly
        // one pong, sent to RS.
        let script = vec![
            arrival(Endpoint::RS, minix_sef::SEF_PING_REQUEST_TYPE, true),
            arrival(
                minix_sef::SYSTEM_ENDPOINT,
                minix_sef::SEF_SIGNAL_REQUEST_TYPE,
                true,
            ),
            arrival(
                Endpoint::from_generation_slot(1, 2),
                minix_types::SCHEDULING_START,
                false,
            ),
        ];
        let mut sef = ScriptedSef::new(script);
        let mut statectl = |_: i32, _: u64, _: i32| Ok(minix_types::OK);
        let (message, _status) =
            sef_filtered_receive(&mut sef, &mut |_: &mut Message| Ok(()), &mut statectl)
                .expect("真调用浮出");
        assert_eq!(message.m_type, minix_types::SCHEDULING_START);
        assert_eq!(
            sef.notified.borrow().as_slice(),
            [Endpoint::RS],
            "ping 在层内被 pong 一次，从不到达分发表"
        );
    }

    #[test]
    fn exhausted_script_reports_the_transport_error() {
        // Broken base link (host trap always EIO) → Err surfaces unchanged; the loop does not fake success.
        let mut sef = ScriptedSef::new(std::vec::Vec::new());
        assert!(
            matches!(
                sef_filtered_receive(
                    &mut sef,
                    &mut |_: &mut Message| Ok(()),
                    &mut |_: i32, _: u64, _: i32| Ok(minix_types::OK),
                ),
                Err(e) if e == minix_types::EIO
            ),
            "底座断链 → Err 原样上浮"
        );
    }

    /// Birth face: RS's fresh init request is answered `RS_INIT+OK` and then
    /// swallowed; the next business message surfaces normally — the transport-level
    /// round trip of "init sent → OK answered → service enters the main loop".
    #[test]
    fn birth_fresh_is_answered_ok_then_loop_serves_business() {
        let mut birth = Message {
            m_type: minix_types::RS_INIT,
            m_source: Endpoint::RS,
            ..Message::default()
        };
        // Union field writes are safe; the active arm is `m_rs_init` (type_=0 is
        // SEF_INIT_FRESH).
        birth.m_u.m_rs_init.type_ = 0;
        let business = arrival(
            Endpoint::from_generation_slot(1, 2),
            minix_types::SCHEDULING_START,
            false,
        );
        let script = vec![(0, birth), business];
        let mut sef = ScriptedSef::new(script);
        let replied: RefCell<std::vec::Vec<Message>> = RefCell::new(std::vec::Vec::new());
        let mut send = |m: &mut Message| {
            replied.borrow_mut().push(*m);
            Ok(())
        };
        let (message, _status) =
            sef_filtered_receive(&mut sef, &mut send, &mut |_: i32, _: u64, _: i32| {
                Ok(minix_types::OK)
            })
            .expect("业务消息浮出");
        assert_eq!(message.m_type, minix_types::SCHEDULING_START);
        let replies = replied.borrow();
        assert_eq!(replies.len(), 1, "出生回信恰好一条");
        assert_eq!(replies[0].m_type, minix_types::RS_INIT);
        // SAFETY(test): reply arm `m_rs_init.result`.
        unsafe {
            assert_eq!(replies[0].m_u.m_rs_init.result, minix_types::OK);
        }
    }

    /// Birth face: LU/RESTART are honestly refused with `ENOSYS` and the process
    /// stops (after replying ENOSYS an Err surfaces and the caller fails closed; C
    /// panics on init failure).
    #[test]
    fn birth_stateful_is_refused_enosys_and_stops() {
        let mut birth = Message {
            m_type: minix_types::RS_INIT,
            m_source: Endpoint::RS,
            ..Message::default()
        };
        // type_=1 is SEF_INIT_LU.
        birth.m_u.m_rs_init.type_ = 1;
        let mut sef = ScriptedSef::new(vec![(0, birth)]);
        let replied: RefCell<std::vec::Vec<Message>> = RefCell::new(std::vec::Vec::new());
        let mut send = |m: &mut Message| {
            replied.borrow_mut().push(*m);
            Ok(())
        };
        let outcome = sef_filtered_receive(&mut sef, &mut send, &mut |_: i32, _: u64, _: i32| {
            Ok(minix_types::OK)
        });
        assert!(
            matches!(outcome, Err(e) if e == minix_types::ENOSYS),
            "init 被拒 → Err(ENOSYS) 上浮停机"
        );
        let replies = replied.borrow();
        assert_eq!(replies.len(), 1);
        // SAFETY(test): reply arm `m_rs_init.result`.
        unsafe {
            assert_eq!(replies[0].m_u.m_rs_init.result, minix_types::ENOSYS);
        }
    }

    /// An RS_INIT from a non-RS source is not intercepted (sef.h:33-34's source
    /// gate) — it surfaces into the dispatch table as usual, with no reply.
    #[test]
    fn rs_init_from_other_source_surfaces_unanswered() {
        let mut impostor = Message {
            m_type: minix_types::RS_INIT,
            m_source: Endpoint::from_generation_slot(1, 2),
            ..Message::default()
        };
        impostor.m_u.m_rs_init.type_ = 0;
        let mut sef = ScriptedSef::new(vec![(0, impostor)]);
        let replied: RefCell<std::vec::Vec<Message>> = RefCell::new(std::vec::Vec::new());
        let mut send = |m: &mut Message| {
            replied.borrow_mut().push(*m);
            Ok(())
        };
        let (message, _status) =
            sef_filtered_receive(&mut sef, &mut send, &mut |_: i32, _: u64, _: i32| {
                Ok(minix_types::OK)
            })
            .expect("非 RS 源照常上浮");
        assert_eq!(message.m_type, minix_types::RS_INIT);
        assert!(replied.borrow().is_empty(), "非 RS 源不触发出生应答");
    }
}

#[cfg(test)]
pub(crate) mod mock {
    //! Test doubles for both wires: script the answers up front, inspect
    //! the recorded calls after. The loop tests (server.rs) drive whole
    //! turns through them without any kernel.

    use super::*;
    use core::cell::RefCell;

    /// Scripted message wire.
    ///
    /// Interior mutability because the trait takes shared `&self` (a real
    /// transport is stateless behind the trap); the cells are test-local
    /// and never cross threads (minix-sys's CannedTransport precedent,
    /// ipc.rs:562-566). An empty script receives `EIO`, so the loop's
    /// failure path is drivable without any setup.
    #[derive(Debug)]
    pub struct MockIpc {
        /// Receive script, handed out front-first.
        pub incoming: RefCell<std::vec::Vec<Result<(Message, IpcStatus), i32>>>,
        /// Every `send` lands here, in order.
        pub sent: RefCell<std::vec::Vec<(Endpoint, Message)>>,
        /// What `send` answers (default `Ok`; set `Err` to exercise the
        /// reply-failure rule, main.c:104-105).
        pub send_result: Result<(), i32>,
    }

    impl Default for MockIpc {
        fn default() -> Self {
            Self {
                incoming: RefCell::new(std::vec::Vec::new()),
                sent: RefCell::new(std::vec::Vec::new()),
                send_result: Ok(()),
            }
        }
    }

    impl MockIpc {
        /// Queue one incoming message with its status word.
        pub fn deliver(&self, message: Message, status: IpcStatus) {
            self.incoming.borrow_mut().push(Ok((message, status)));
        }
    }

    impl IpcTransport for MockIpc {
        fn receive(&self) -> Result<(Message, IpcStatus), i32> {
            // An empty script reads as a broken transport (EIO) — the
            // loop's failure path needs no setup. `remove(0)` hands out
            // the front answer by value (the entries are `Copy`).
            let mut script = self.incoming.borrow_mut();
            if script.is_empty() {
                return Err(minix_types::EIO);
            }
            script.remove(0)
        }

        fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
            self.sent.borrow_mut().push((to, *message));
            self.send_result
        }

        fn sendnb(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
            self.sent.borrow_mut().push((to, *message));
            self.send_result
        }
    }

    /// Scripted kernel wire: each call pops its answer (empty script
    /// answers `OK`) and records the packed arguments.
    #[derive(Debug)]
    pub struct MockKernel {
        /// `get_machine` answer (the buffer read-back pre-E1 is zeroed on
        /// the real end; the mock hands the real numbers directly).
        pub machine: MachineInfo,
        /// `get_hz` answer.
        pub hz: u32,
        /// `schedctl` answers (raw errno, `0` = `OK`), front-first;
        /// empty means `Ok`.
        pub schedctl_rvs: RefCell<std::vec::Vec<i32>>,
        /// `schedule` answers (raw errno, `0` = `OK`), front-first;
        /// empty means `Ok`.
        pub schedule_rvs: RefCell<std::vec::Vec<i32>>,
        /// `setalarm` answers (raw errno, `0` = `OK`), front-first;
        /// empty means `Ok`.
        pub setalarm_rvs: RefCell<std::vec::Vec<i32>>,
        /// Recorded `schedctl` targets, in order.
        pub schedctl_calls: RefCell<std::vec::Vec<Endpoint>>,
        /// Recorded `schedule` wires, in order (already rendered: `KEEP`
        /// is `-1`, `niced` 0/1).
        pub schedule_calls: RefCell<std::vec::Vec<MessLsysKrnSchedule>>,
        /// Recorded `setalarm` tick counts, in order.
        pub setalarm_calls: RefCell<std::vec::Vec<u32>>,
    }

    impl MockKernel {
        /// A machine with `count` CPUs booting on `bsp`.
        pub fn new(count: u32, bsp: u32) -> Self {
            Self {
                machine: MachineInfo {
                    processors_count: count,
                    bsp_id: bsp,
                },
                hz: 60,
                schedctl_rvs: RefCell::new(std::vec::Vec::new()),
                schedule_rvs: RefCell::new(std::vec::Vec::new()),
                setalarm_rvs: RefCell::new(std::vec::Vec::new()),
                schedctl_calls: RefCell::new(std::vec::Vec::new()),
                schedule_calls: RefCell::new(std::vec::Vec::new()),
                setalarm_calls: RefCell::new(std::vec::Vec::new()),
            }
        }

        /// Queue a `schedule` answer (front-first).
        pub fn fail_next_schedule(&self, errno: i32) {
            self.schedule_rvs.borrow_mut().push(errno);
        }
    }

    impl Default for MockKernel {
        fn default() -> Self {
            Self::new(1, 0)
        }
    }

    fn next<T: Default>(script: &RefCell<std::vec::Vec<T>>) -> T {
        let mut queue = script.borrow_mut();
        if queue.is_empty() {
            T::default()
        } else {
            queue.remove(0)
        }
    }

    /// `0` is `OK`, anything else is the errno the kernel answered with.
    fn decode_errno(rv: i32) -> Result<(), i32> {
        if rv == 0 { Ok(()) } else { Err(rv) }
    }

    impl KernelApi for MockKernel {
        fn get_machine(&mut self) -> Result<MachineInfo, i32> {
            Ok(self.machine)
        }

        fn get_hz(&mut self) -> Result<u32, i32> {
            Ok(self.hz)
        }

        fn schedctl(&mut self, call: &SchedctlCall) -> Result<(), i32> {
            self.schedctl_calls.borrow_mut().push(call.target);
            decode_errno(next(&self.schedctl_rvs))
        }

        fn schedule(&mut self, fanout: &Fanout) -> Result<(), i32> {
            self.schedule_calls.borrow_mut().push(MessLsysKrnSchedule {
                endpoint: fanout.endpoint.0,
                quantum: fanout.wire_quantum_ms(),
                priority: fanout.wire_priority(),
                cpu: fanout.wire_cpu(),
                niced: fanout.wire_niced(),
                _padding: [0; 36],
            });
            decode_errno(next(&self.schedule_rvs))
        }

        fn setalarm(&mut self, ticks: u32) -> Result<(), i32> {
            self.setalarm_calls.borrow_mut().push(ticks);
            decode_errno(next(&self.setalarm_rvs))
        }
    }
}
