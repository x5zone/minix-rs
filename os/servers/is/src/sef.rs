//! SEF lifecycle surface (01-is-init-main.md §4.1).
//!
//! C: `sef_local_startup()` — `minix3/minix/servers/is/main.c:76-89`.
//! Modeled as traits, following `os/servers/rs/src/sef.rs:60-91`: a C `fn`
//! pointer cannot capture anything, so callback bodies that need server
//! state become trait methods on the server instead of globals.
//!
//! The SEF *framework* (`sef_startup`, `sef_receive` internals) lives in
//! `minix-sef` (currently a stub); this module only declares the surface IS
//! consumes. Production wiring is a forward reference (see
//! [`UnimplementedTransport`]).

use minix_types::{Endpoint, Errno, Message};

/// Termination signal. C: `SIGTERM 15` — `minix3/sys/sys/signal.h:67`.
pub const SIGTERM: i32 = 15;

/// SEF init type carried in the SEF_INIT message.
///
/// C: `SEF_INIT_FRESH=0` / `SEF_INIT_LU=1` / `SEF_INIT_RESTART=2` —
/// `minix3/minix/include/minix/sef.h:93-95`. Mirrors
/// `os/servers/rs/src/sef.rs:33-44`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SefInitType {
    /// Fresh boot. C: `SEF_INIT_FRESH` — sef.h:93.
    Fresh,
    /// Init after live update. C: `SEF_INIT_LU` — sef.h:94.
    Lu,
    /// Init after restart. C: `SEF_INIT_RESTART` — sef.h:95.
    Restart,
}

/// SEF init info passed to init callbacks.
///
/// C: `sef_init_info_t` — `minix3/minix/include/minix/sef.h:53`. IS ignores
/// both fields (`UNUSED` — main.c:94); the type is carried for signature
/// parity with the SEF framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SefInitInfo {
    /// C: `info->endpoint` (the service's own endpoint).
    pub endpoint: i32,
    /// C: `info->old_endpoint` (previous incarnation during LU/restart).
    pub old_endpoint: i32,
}

/// The IS SEF callback set.
///
/// C: the four `sef_setcb_*` registrations — main.c:80-85.
/// `[ARCH: A-10]`: the three init callbacks share one body (STATELESS —
/// restart rebuilds the fkey observer registration, nothing is restored),
/// expressed here as default methods forwarding to [`SefCallbacks::init_fresh`].
/// A branchless forward is machine-checkable evidence of statelessness.
pub trait SefCallbacks {
    /// C: `sef_cb_init_fresh` — main.c:94-102 (registers fkey mapping, 02).
    fn init_fresh(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno>;

    /// C: `sef_setcb_init_restart(sef_cb_init_fresh)` — main.c:82.
    /// Default: same body as fresh (STATELESS, A-10).
    fn init_restart(
        &mut self,
        init_type: SefInitType,
        info: &SefInitInfo,
    ) -> Result<i32, Errno> {
        self.init_fresh(init_type, info)
    }

    /// C: `sef_setcb_init_lu(sef_cb_init_fresh)` — main.c:81.
    /// Default: same body as fresh (STATELESS, A-10).
    fn init_lu(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno> {
        self.init_fresh(init_type, info)
    }

    /// C: `sef_cb_signal_handler` — main.c:107-116. Only `SIGTERM` acts;
    /// anything else returns without touching state. Returns the lifecycle
    /// decision instead of calling `exit(0)` (main.c:115): a library that
    /// exits the process cannot be unit-tested; the binary owns divergence.
    fn signal_handler(&mut self, signo: i32) -> LifecycleAction;
}

/// What the server should do after a signal callback runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleAction {
    /// Keep running the main loop.
    Continue,
    /// Shut down (C: post-unmap `exit(0)` — main.c:113-115).
    Shutdown,
}

/// Kernel/transport boundary consumed by [`crate::IsServer`].
///
/// Groups the four framework calls `main.c` needs: `sef_startup` (:88),
/// `sef_receive` (:125), `ipc_send` (:143), and the illegal-request warning
/// (`printf("IS: warning, ...")` (:60-61), routed here as `warn_illegal` so
/// the diagnostic channel stays behind the boundary — `[ARCH: A-6]`).
/// Production implementation lands with the `minix-sef`/`minix-sys` wiring
/// (forward reference); tests inject fakes.
///
/// SEF ping transparency invariant (A-11): `receive` must behave like
/// `sef_receive` — `SEF_PING_REQUEST_TYPE` messages are answered inside
/// (`do_sef_ping_request`, `sef_ping.c:21`) and never returned
/// (`sef.c:208-214` `continue`). The classifier therefore never sees a ping.
pub trait SefTransport {
    /// Run SEF startup. C: `sef_startup()` — main.c:88.
    fn startup(&mut self);
    /// Block until a message arrives; returns `(sender, call_nr)`.
    /// C: `sef_receive(ANY, &m_in)` + writeback — main.c:125-129.
    fn receive(&mut self, inbox: &mut Message) -> Result<(Endpoint, i32), i32>;
    /// Send a reply. C: `ipc_send(who, &m_out)` — main.c:143.
    fn send(&mut self, dest: Endpoint, reply: &Message) -> Result<(), i32>;
    /// Emit the illegal-request warning. C: `printf("IS: warning, got
    /// illegal request %d from %d\n", ...)` — main.c:60-61.
    fn warn_illegal(&mut self, call_nr: i32, sender: Endpoint);
    /// Emit the EVENTS-pull failure warning. C: `printf("IS: warning,
    /// fkey_events failed: %d\n", s)` — dmp.c:84-86. Same diagnostic
    /// channel as [`SefTransport::warn_illegal`] ([ARCH: A-6]); split into
    /// its own method so each C call site stays greppable (03).
    fn warn_fkey_events(&mut self, status: i32);
}

/// Fail-closed transport until the `minix-sef`/`minix-sys` wiring lands.
///
/// Mirrors `os/servers/rs/src/boot.rs` (`UnimplementedKernelApi`): every
/// method panics with a pointer to the owning doc instead of silently
/// succeeding (fail-closed, T2 pattern).
#[derive(Debug, Default)]
pub struct UnimplementedTransport;

impl SefTransport for UnimplementedTransport {
    fn startup(&mut self) {
        panic!("IS transport: sef_startup wiring pending (01-is-init-main.md §3 D2)");
    }

    fn receive(&mut self, _inbox: &mut Message) -> Result<(Endpoint, i32), i32> {
        panic!("IS transport: sef_receive wiring pending (01-is-init-main.md §3 D2)");
    }

    fn send(&mut self, _dest: Endpoint, _reply: &Message) -> Result<(), i32> {
        panic!("IS transport: ipc_send wiring pending (01-is-init-main.md §3 D2)");
    }

    fn warn_illegal(&mut self, _call_nr: i32, _sender: Endpoint) {
        panic!("IS transport: diagnostic channel wiring pending ([ARCH: A-6])");
    }

    fn warn_fkey_events(&mut self, _status: i32) {
        panic!("IS transport: diagnostic channel wiring pending ([ARCH: A-6])");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::OK;

    struct Probe {
        fresh_calls: u32,
        last_signo: Option<i32>,
        mapped: bool,
    }

    impl SefCallbacks for Probe {
        fn init_fresh(
            &mut self,
            _t: SefInitType,
            _i: &SefInitInfo,
        ) -> Result<i32, Errno> {
            self.fresh_calls += 1;
            self.mapped = true; // stands in for map_unmap_fkeys(TRUE) (02)
            Ok(OK)
        }

        fn signal_handler(&mut self, signo: i32) -> LifecycleAction {
            self.last_signo = Some(signo);
            if signo != SIGTERM {
                return LifecycleAction::Continue;
            }
            self.mapped = false; // stands in for map_unmap_fkeys(FALSE) (02)
            LifecycleAction::Shutdown
        }
    }

    #[test]
    fn test_three_inits_share_one_body() {
        // [ARCH: A-10] STATELESS: Lu/Restart default to the fresh body.
        let info = SefInitInfo::default();
        let mut p = Probe { fresh_calls: 0, last_signo: None, mapped: false };
        assert_eq!(p.init_restart(SefInitType::Restart, &info), Ok(OK));
        assert_eq!(p.init_lu(SefInitType::Lu, &info), Ok(OK));
        assert_eq!(p.init_fresh(SefInitType::Fresh, &info), Ok(OK));
        assert_eq!(p.fresh_calls, 3);
        assert!(p.mapped);
    }

    #[test]
    fn test_non_term_signal_ignored() {
        // C: if (signo != SIGTERM) return — main.c:110.
        let mut p = Probe { fresh_calls: 0, last_signo: None, mapped: true };
        assert_eq!(p.signal_handler(2), LifecycleAction::Continue);
        assert!(p.mapped, "non-TERM must not touch the mapping");
    }

    #[test]
    fn test_sigterm_unmaps_then_shuts_down() {
        // C: unmap first (main.c:113), exit second (:115) — order matters.
        let mut p = Probe { fresh_calls: 0, last_signo: None, mapped: true };
        assert_eq!(p.signal_handler(SIGTERM), LifecycleAction::Shutdown);
        assert!(!p.mapped, "TERM must release the TTY observer registration");
    }

    #[test]
    fn test_sigterm_value_matches_c() {
        // C: SIGTERM 15 — sys/sys/signal.h:67.
        assert_eq!(SIGTERM, 15);
    }
}
