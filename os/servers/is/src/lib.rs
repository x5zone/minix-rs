#![cfg_attr(not(test), no_std)]

//! Minix-RS Information Server (IS).
//!
//! Userspace debug-dump aggregator: idle until a TTY function-key
//! notification arrives, then dispatches to the owning dump routine.
//! Documented in `notes/rewrite/fork-syscall-rewrite/08-stage-is/
//! 01-is-init-main.md`.
//!
//! Execution model: single-threaded event loop (user-space server —
//! `!Send`/`!Sync` are correct; no cross-CPU sharing exists).
//!
//! C: `minix3/minix/servers/is/main.c` (148 lines).

pub mod acquire;
pub mod dispatch;
pub mod dump_ds;
pub mod dump_kernel;
pub mod dump_pm;
pub mod dump_rs;
pub mod dump_vfs;
pub mod dump_vm;
pub mod sef;
pub mod state;
pub mod tty_fkey;

pub use acquire::{
    DiagctlTransport, GetRequest, GetSysinfoTransport, KerninfoTransport, SiWhat,
    SysGetinfoTransport, UnimplementedAcquires, VmInfoTransport, getsysinfo_call,
};
pub use dispatch::{
    DispatchAction, DumpId, Hook, HOOKS, MAPPING_COLUMNS, MAPPING_TITLE, classify, dispatch_each,
    is_notify_call, is_reply_suppressed, key_name, pressed,
};
pub use dump_kernel::{PageAction, PageCursor};
pub use sef::{
    LifecycleAction, SefCallbacks, SefInitInfo, SefInitType, SefTransport, SIGTERM,
    UnimplementedTransport,
};
pub use state::IsServerState;
pub use tty_fkey::{
    FkeyCtlError, FkeyCtlTransport, FkeyId, FkeyReq, UnimplementedFkeyCtl, map_unmap_keys,
    pull_events,
};

use minix_types::{EDONTREPLY, Errno, OK};

/// The IS server orchestrator.
///
/// C: `main()` — `minix3/minix/servers/is/main.c:31-71`. Owns the inbox
/// state ([`IsServerState`], C globals main.c:14-17) and drives the
/// get-work → classify → reply skeleton. The `do_fkey_pressed` (03)
/// mechanism arrives with its owning doc; until then its seam fails closed
/// with `ENOSYS`. The `map_unmap_fkeys` (02) seam is wired
/// (02-is-fkey-contract.md).
///
/// No `IS_PROC_NR` constant exists here on purpose (A-9): Minix3 defines
/// none — the endpoint is allocated by RS at load time and injected through
/// the transport, never named.
pub struct IsServer<T: SefTransport, F: FkeyCtlTransport> {
    state: IsServerState,
    transport: T,
    fkey: F,
    /// Observer registration outstanding at TTY (02).
    fkey_mapped: bool,
}

impl<T: SefTransport, F: FkeyCtlTransport> IsServer<T, F> {
    /// Creates the server over injected transports (tests / wiring).
    pub fn new(transport: T, fkey: F) -> Self {
        Self { state: IsServerState::new(), transport, fkey, fkey_mapped: false }
    }

    /// Runs SEF startup and the fresh-boot init (the boot anchor).
    ///
    /// C: `sef_startup()` → `sef_cb_init_fresh()` — main.c:88,94-102.
    pub fn startup(&mut self) -> Result<i32, Errno> {
        self.transport.startup();
        let info = SefInitInfo::default();
        self.init_fresh(SefInitType::Fresh, &info)
    }

    /// One main-loop iteration: get work, classify, maybe reply.
    ///
    /// C: one `while (TRUE)` pass — main.c:44-68. Returns the lifecycle
    /// decision so tests can drive single steps without diverging.
    pub fn step(&mut self) -> LifecycleAction {
        // C: get_work() — main.c:46,121-130. Receive failure is fatal
        // (main.c:126-127 `panic`), so transport errors panic here too.
        let (caller, call_nr) = match self.transport.receive(&mut self.state.inbox) {
            Ok(pair) => pair,
            Err(status) => panic!("sef_receive failed!: {status}"),
        };
        self.state.caller = caller;
        self.state.call_nr = call_nr;

        let result = match classify(call_nr, caller) {
            DispatchAction::HandleFkey => self.handle_fkey_pressed(),
            DispatchAction::Suppress => {
                // C: non-notify arm warns (main.c:60-61); the non-TTY
                // notify default is silent (main.c:53-56, FIXME).
                // Only the warn path is observable here: classify() has
                // already merged both arms into Suppress, so warn exactly
                // when the message is not a notification.
                if !is_notify_call(call_nr) {
                    self.transport.warn_illegal(call_nr, caller);
                }
                EDONTREPLY
            }
        };

        // C: if (result != EDONTREPLY) reply(who_e, result) — main.c:65-68.
        if !is_reply_suppressed(result) {
            self.state.reply_buf.m_type = result;
            let dest = self.state.caller;
            if self.transport.send(dest, &self.state.reply_buf).is_err() {
                panic!("unable to send reply!: {result}");
            }
        }
        LifecycleAction::Continue
    }

    /// The main loop. C: `while (TRUE)` — main.c:44-69.
    pub fn run(&mut self) -> ! {
        loop {
            self.step();
        }
    }

    /// Executes one matched dump (03 seam filled by 05~10).
    ///
    /// C dump bodies return void (dmp.c) — there is no error to propagate,
    /// so this is intentionally an empty dispatch point until 05~10 land
    /// their `DumpId` arms here.
    fn run_dump(&mut self, _dump: DumpId) {}

    /// Handles a TTY function-key notification.
    ///
    /// C: `do_fkey_pressed(m)` — `minix3/minix/servers/is/dmp.c:73-101`.
    /// Pulls the pending bitmaps (`fkey_events`, dmp.c:83), warns on
    /// transport-level failure (`s < 0`, dmp.c:84-86 — Minix errnos are
    /// non-negative, so only a failed `_taskcall` trips this), dispatches
    /// every match in table order with no break (dmp.c:89-95), and always
    /// suppresses the reply (dmp.c:99). The notification message itself is
    /// never read (A-2 pull model).
    fn handle_fkey_pressed(&mut self) -> i32 {
        let (status, fkeys, sfkeys) = self.fkey.fkey_ctl(FkeyReq::Events, 0, 0);
        if status < 0 {
            self.transport.warn_fkey_events(status);
        }
        // Fixed-size match buffer (no alloc; ≤16 hooks). `DumpId: Copy`
        // sidesteps the borrow of `self` across dispatch and execution.
        let mut matched = [DumpId::Proctab; 16];
        let mut n = 0usize;
        dispatch_each(fkeys, sfkeys, |hook| {
            matched[n] = hook.dump;
            n += 1;
        });
        for dump in matched.iter().take(n) {
            self.run_dump(*dump);
        }
        EDONTREPLY
    }

    /// Registers or releases the TTY fkey observer set.
    ///
    /// C: `map_unmap_fkeys(map)` — `minix3/minix/servers/is/dmp.c:44-68`.
    /// The key set derives from the hooks table (03 owns it) instead of the
    /// interim 02 list. Failure is non-fatal (C warns, dmp.c:63-65): the
    /// error is swallowed after recording that no registration is
    /// outstanding.
    fn request_fkey_map(&mut self, map: bool) -> Result<i32, Errno> {
        let mut keys = [FkeyId::F1; 16];
        for (slot, hook) in keys.iter_mut().zip(HOOKS.iter()) {
            *slot = hook.key;
        }
        match map_unmap_keys(&mut self.fkey, map, &keys) {
            Ok(()) => {
                self.fkey_mapped = map;
                Ok(OK)
            }
            Err(_) => {
                self.fkey_mapped = false;
                Ok(OK)
            }
        }
    }
}

impl<T: SefTransport, F: FkeyCtlTransport> SefCallbacks for IsServer<T, F> {
    /// C: `sef_cb_init_fresh` — main.c:94-102 (`map_unmap_fkeys(TRUE)`).
    /// `[ARCH: A-10]` STATELESS: Lu/Restart share this body by default.
    /// `map_unmap_fkeys` is void in C: registration failure never fails
    /// the boot (dmp.c:63-65 warns only), so the result is intentionally
    /// not propagated.
    fn init_fresh(
        &mut self,
        _init_type: SefInitType,
        _info: &SefInitInfo,
    ) -> Result<i32, Errno> {
        // 02: register the fkey observer set at TTY (best-effort, C-void).
        let _ = self.request_fkey_map(true);
        Ok(OK)
    }

    /// C: `sef_cb_signal_handler` — main.c:107-116.
    /// Non-TERM returns untouched; TERM unmaps first, then shuts down
    /// (order matters — main.c:113 before :115).
    /// `[ARCH: A-10]` cleanup half.
    fn signal_handler(&mut self, signo: i32) -> LifecycleAction {
        if signo != SIGTERM {
            return LifecycleAction::Continue;
        }
        // 02 seam: release the TTY observer registration. Best-effort:
        // shutdown proceeds even if the release fails (C calls it void).
        let _ = self.request_fkey_map(false);
        self.fkey_mapped = false;
        LifecycleAction::Shutdown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{Endpoint, Message};
    use std::vec::Vec;

    /// Fake transport: scripted inbox, recorded sends/warnings.
    /// Also models the A-11 ping transparency: scripted ping frames are
    /// consumed inside `receive` and never surface to the classifier.
    struct FakeTransport {
        /// Scripted (caller, call_nr) frames; pings are `None` and skipped.
        script: Vec<Option<(Endpoint, i32)>>,
        startups: u32,
        pub sends: Vec<(Endpoint, i32)>,
        pub warnings: Vec<(i32, Endpoint)>,
        pub event_warnings: Vec<i32>,
        pub fail_receive: bool,
        pub fail_send: bool,
    }

    impl FakeTransport {
        fn new(script: Vec<Option<(Endpoint, i32)>>) -> Self {
            Self {
                script,
                startups: 0,
                sends: Vec::new(),
                warnings: Vec::new(),
                event_warnings: Vec::new(),
                fail_receive: false,
                fail_send: false,
            }
        }
    }

    impl SefTransport for FakeTransport {
        fn startup(&mut self) {
            self.startups += 1;
        }

        fn receive(&mut self, _inbox: &mut Message) -> Result<(Endpoint, i32), i32> {
            if self.fail_receive {
                return Err(-1);
            }
            // A-11: ping frames are answered inside sef_receive and skipped.
            while let Some(frame) = self.script.first().cloned() {
                self.script.remove(0);
                if let Some(pair) = frame {
                    return Ok(pair);
                }
            }
            panic!("fake transport: script exhausted");
        }

        fn send(&mut self, dest: Endpoint, reply: &Message) -> Result<(), i32> {
            if self.fail_send {
                return Err(-1);
            }
            self.sends.push((dest, reply.m_type));
            Ok(())
        }

        fn warn_illegal(&mut self, call_nr: i32, sender: Endpoint) {
            self.warnings.push((call_nr, sender));
        }

        fn warn_fkey_events(&mut self, status: i32) {
            self.event_warnings.push(status);
        }
    }

    const FKEY_NOTIFY: i32 = 0x1000;

    /// Accommodating fkey double: records MAP/UNMAP calls, always OK.
    /// EVENTS answers from a scripted triple (03).
    struct FakeFkey {
        pub calls: Vec<(bool, u32, u32)>,
        pub events_answer: (i32, u32, u32),
    }

    impl FakeFkey {
        fn new() -> Self {
            Self { calls: Vec::new(), events_answer: (OK, 0, 0) }
        }
    }

    impl FkeyCtlTransport for FakeFkey {
        fn fkey_ctl(&mut self, req: FkeyReq, fkeys: u32, sfkeys: u32) -> (i32, u32, u32) {
            if req == FkeyReq::Events {
                return self.events_answer;
            }
            self.calls.push((req == FkeyReq::Map, fkeys, sfkeys));
            (OK, 0, 0)
        }
    }

    fn server(script: Vec<Option<(Endpoint, i32)>>) -> IsServer<FakeTransport, FakeFkey> {
        IsServer::new(FakeTransport::new(script), FakeFkey::new())
    }

    #[test]
    fn test_step_tty_notify_dispatches_and_suppresses() {
        // C: do_fkey_pressed pulls EVENTS, runs matches, returns EDONTREPLY
        // (dmp.c:83-99) → reply gate suppresses (main.c:66).
        let mut fk = FakeFkey::new();
        fk.events_answer = (OK, 1 << 1, 0); // F1 pressed
        let mut s = IsServer::new(FakeTransport::new(Vec::from([Some((Endpoint::TTY, FKEY_NOTIFY))])), fk);
        assert_eq!(s.step(), LifecycleAction::Continue);
        assert!(s.transport.sends.is_empty(), "EDONTREPLY suppresses the reply");
        assert!(s.transport.warnings.is_empty());
        assert!(s.transport.event_warnings.is_empty());
    }

    #[test]
    fn test_step_events_failure_warns_but_still_suppresses() {
        // C: s < 0 → warn, then dispatch anyway (dmp.c:84-87).
        let mut fk = FakeFkey::new();
        fk.events_answer = (-5, 0, 0);
        let mut s = IsServer::new(FakeTransport::new(Vec::from([Some((Endpoint::TTY, FKEY_NOTIFY))])), fk);
        assert_eq!(s.step(), LifecycleAction::Continue);
        assert_eq!(s.transport.event_warnings, [-5]);
        assert!(s.transport.sends.is_empty());
    }

    #[test]
    fn test_step_non_tty_notify_suppressed_silently() {
        // C default arm: silent EDONTREPLY (main.c:53-56, FIXME).
        let mut s = server(Vec::from([Some((Endpoint::RS, FKEY_NOTIFY))]));
        assert_eq!(s.step(), LifecycleAction::Continue);
        assert!(s.transport.sends.is_empty(), "suppressed: no reply");
        assert!(s.transport.warnings.is_empty(), "C default arm is silent");
    }

    #[test]
    fn test_step_non_notify_warns_and_suppresses() {
        // C else arm: printf warning + EDONTREPLY (main.c:59-63).
        let mut s = server(Vec::from([Some((Endpoint::RS, 0x42))]));
        assert_eq!(s.step(), LifecycleAction::Continue);
        assert!(s.transport.sends.is_empty());
        assert_eq!(s.transport.warnings, Vec::from([(0x42, Endpoint::RS)]));
    }

    #[test]
    fn test_step_ping_never_reaches_classifier() {
        // A-11: None frames (pings) are consumed inside receive.
        let mut s = server(Vec::from([None, None, Some((Endpoint::RS, 0x42))]));
        assert_eq!(s.step(), LifecycleAction::Continue);
        assert_eq!(s.transport.warnings.len(), 1, "only the real message warns");
    }

    #[test]
    #[should_panic(expected = "sef_receive failed!")]
    fn test_receive_failure_panics() {
        // C: panic("sef_receive failed!") — main.c:126.
        let mut s = IsServer::new(
            FakeTransport { fail_receive: true, ..FakeTransport::new(Vec::new()) },
            FakeFkey::new(),
        );
        let _ = s.step();
    }

    #[test]
    fn test_send_path_unreachable_through_step() {
        // Both step arms yield EDONTREPLY (C: main.c:53-63 + dmp.c:99), so
        // even a failing sender is never invoked — the send-failure panic
        // (main.c:144 parity) is defensive and unreachable via step().
        let mut s = IsServer::new(
            FakeTransport {
                fail_send: true,
                ..FakeTransport::new(Vec::from([Some((Endpoint::TTY, FKEY_NOTIFY))]))
            },
            FakeFkey::new(),
        );
        assert_eq!(s.step(), LifecycleAction::Continue);
        assert!(s.transport.sends.is_empty());
    }

    #[test]
    fn test_startup_registers_fkey_set() {
        // C: sef_startup() (main.c:88) then init_fresh → map_unmap_fkeys(TRUE)
        // (main.c:99); registration failure never fails the boot (void).
        let mut s = server(Vec::new());
        assert_eq!(s.startup(), Ok(OK));
        assert_eq!(s.transport.startups, 1);
        assert!(s.fkey_mapped);
        assert_eq!(s.fkey.calls.len(), 1);
        assert!(s.fkey.calls[0].0, "MAP=true");
    }

    #[test]
    fn test_signal_non_term_keeps_state() {
        let mut s = server(Vec::new());
        assert_eq!(s.signal_handler(2), LifecycleAction::Continue);
    }

    #[test]
    fn test_signal_term_requests_shutdown() {
        let mut s = server(Vec::new());
        assert_eq!(s.signal_handler(SIGTERM), LifecycleAction::Shutdown);
        assert!(!s.fkey_mapped);
    }
}
