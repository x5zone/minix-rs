//! SEF lifecycle surface (01-is-init-main.md §4.1).
//!
//! C: `sef_local_startup()` — `minix3/minix/servers/is/main.c:76-89`.
//! Modeled as traits, following `os/servers/rs/src/sef.rs:60-91`: a C `fn`
//! pointer cannot capture anything, so callback bodies that need server
//! state become trait methods on the server instead of globals.
//!
//! The SEF *framework* (`sef_startup`, `sef_receive` internals) lives in
//! `minix-sef`; this module adapts it to the seam IS consumes. Production
//! binding is [`SysSefTransport`] (below), which routes through `minix-sys`
//!'s trap transport (a hosted build returns EIO honestly until E1 real
//! hardware is powered on).

use minix_sys::ipc::IpcTransport as _;
use core::fmt;
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
    ///
    /// `on_signal` is the seam for C's registered signal handler
    /// (`sef_setcb_signal_handler`, main.c:85): SEF calls it *inside* the
    /// receive step — once per pending signal number
    /// (`sef_signal.c:96-128`) — so a decision made there cannot travel
    /// through the return value of this method. The implementation must
    /// therefore hand every number to the callback before it reports the
    /// frame it actually got.
    fn receive(
        &mut self,
        inbox: &mut Message,
        on_signal: &mut dyn FnMut(i32),
        cancel: &minix_sef::SefCancel,
    ) -> Result<(Endpoint, i32), i32>;
    /// Send a reply. C: `ipc_send(who, &m_out)` — main.c:143.
    fn send(&mut self, dest: Endpoint, reply: &Message) -> Result<(), i32>;
    /// State-control verb (C `sys_statectl`, libsys sys_statectl.c:3-11) —
    /// the birth protocol's paragraph-1 kernel call, `process_init`.
    /// Production wires the trap transport; test doubles model an
    /// accepting kernel.
    fn statectl(&mut self, request: i32, address: u64, length: i32) -> Result<i32, i32>;
    /// Send-and-receive. C: `ipc_sendrec` — sef_cb_init_response_rs_reply
    /// (sef_init.c:458-466). The birth-report leg parks in receive(RS) for
    /// RS's catch reply instead of doing a fire-and-forget send.
    fn send_rec(&mut self, dest: Endpoint, reply: &mut Message) -> Result<(), i32>;
    /// Emit the illegal-request warning. C: `printf("IS: warning, got
    /// illegal request %d from %d\n", ...)` — main.c:60-61.
    fn warn_illegal(&mut self, call_nr: i32, sender: Endpoint);
    /// Emit the EVENTS-pull failure warning. C: `printf("IS: warning,
    /// fkey_events failed: %d\n", s)` — dmp.c:84-86. Same diagnostic
    /// channel as [`SefTransport::warn_illegal`] ([ARCH: A-6]); split into
    /// its own method so each C call site stays greppable (03).
    fn warn_fkey_events(&mut self, status: i32);
    /// Emit the fkey_ctl (MAP/UNMAP) failure warning. C: `printf("IS:
    /// warning, fkey_ctl failed: %d\n", s)` — dmp.c:63-65. Same diagnostic
    /// channel as [`SefTransport::warn_illegal`] ([ARCH: A-6]); its own
    /// method keeps the third C printf call site greppable (the 02
    /// caller-warns invariant, 02-is-fkey-contract.md §4.3).
    fn warn_fkey_ctl(&mut self, status: i32);
    /// The diagnostic output channel ([ARCH: A-6]). C's `printf` writes to
    /// the service's stdout (libc stdio → log driver, etc/usr/rc:290); a
    /// no_std process has no stdio, so the channel is part of the transport
    /// wiring: a [`core::fmt::Write`] sink the dump bodies render through.
    /// The production implementation lands with the minix-sef/minix-sys
    /// wiring (kernel diag channel or log-driver IPC — plan §4 A-6).
    fn diag_out(&mut self) -> &mut dyn fmt::Write;
}

/// Production SEF transport: `receive` goes through `minix-sef`'s
/// `sef_receive_status` (the ping transparency invariant A-11 is guaranteed by
/// the library layer), `send` uses the trap transport's blocking `send` (C
/// `ipc_send`), and the three `warn_*` methods plus
/// [`SefTransport::diag_out`] use the A-6 diagnostic seam — the `SYS_DIAGCTL`
/// code 1 console channel (minix-sys `sys_diagctl_write`; rs reaches the same
/// exit through its diagnostic seam).
pub struct SysSefTransport {
    ipc: SefIpcAdapter,
    diag: DiagWriter,
}

impl SysSefTransport {
    pub const fn new() -> Self {
        Self {
            ipc: SefIpcAdapter { inner: minix_sys::ipc::DirectTrapTransport },
            diag: DiagWriter,
        }
    }
}

impl Default for SysSefTransport {
    fn default() -> Self {
        Self::new()
    }
}

/// Adapts `minix-sef`'s `SefIpc` verbs onto the trap transport, following the
/// same form already used by RS and VM.
struct SefIpcAdapter {
    inner: minix_sys::ipc::DirectTrapTransport,
}

impl minix_sef::SefIpc for SefIpcAdapter {
    fn receive(&mut self, src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        let sts = self.inner.receive(src, msg).map_err(|t| t.0)?;
        Ok(sts.0 as i32)
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        self.inner.notify(dest).map_err(|t| t.0)
    }
}

/// The `fmt::Write` sink for diagnostic output (A-6): every `write_str` goes
/// straight to the kernel diagnostic console — the shape of `printf`, where
/// fragmentation is harmless because the kernel prints segment by segment.
pub struct DiagWriter;

impl fmt::Write for DiagWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        //[SD19-gate] IPC fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = minix_sys::syscall::sys_diagctl_write(
            &minix_sys::syscall::DirectKernelCallTransport,
            s,
        );
        Ok(())
    }
}

impl SefTransport for SysSefTransport {
    fn startup(&mut self) {
        // C sef_startup() (main.c:88) triggers the init callbacks; in Rust the
        // init face is explicit — IsServer::startup calls init_fresh
        // (lib.rs:151-155) — so nothing extra belongs here. The C call-site
        // shape is kept.
    }

    fn receive(
        &mut self,
        inbox: &mut Message,
        on_signal: &mut dyn FnMut(i32),
        cancel: &minix_sef::SefCancel,
    ) -> Result<(Endpoint, i32), i32> {
        // C main.c:125-129 — sef_receive(ANY, &m_in); a ping is intercepted
        // and answered inside the SEF layer (A-11: the classifier never sees a
        // ping). Signal numbers reach the callback within that same receive
        // step (C calls the registered handler directly inside
        // `sef_receive_status`, main.c:85); here the same number is forwarded
        // to the body the main loop supplied. The thin closure is necessary
        // because the library entry point monomorphizes over `impl FnMut`,
        // while this trait, for object safety, can only take `dyn FnMut`.
        let mut handoff = |signo: i32| on_signal(signo);
        let recv = minix_sef::sef_receive_status(
            &mut self.ipc,
            Endpoint::ANY,
            inbox,
            &mut handoff,
            cancel,
        )?;
        Ok((recv.source, recv.message.m_type))
    }

    fn statectl(&mut self, request: i32, address: u64, length: i32) -> Result<i32, i32> {
        // Direct trap call for C `sys_statectl`, the same direct form diag_write uses.
        let r = minix_sys::syscall::sys_statectl(
            &minix_sys::syscall::DirectKernelCallTransport,
            request,
            address,
            length,
        );
        if r == minix_types::OK { Ok(r) } else { Err(r) }
    }

    fn send(&mut self, dest: Endpoint, reply: &Message) -> Result<(), i32> {
        use minix_sys::ipc::IpcTransport as _;
        self.ipc.inner.send(dest, reply).map_err(|t| t.0)
    }

    fn send_rec(&mut self, dest: Endpoint, reply: &mut Message) -> Result<(), i32> {
        use minix_sys::ipc::IpcTransport as _;
        self.ipc.inner.sendrec(dest, reply).map_err(|t| t.0)
    }

    fn warn_illegal(&mut self, call_nr: i32, sender: Endpoint) {
        // C's wording at main.c:60-61.
        use core::fmt::Write as _;
        //[SD19-gate] IPC fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = writeln!(
            self.diag,
            "IS: warning, got illegal request {} from {}",
            call_nr, sender.0
        );
    }

    fn warn_fkey_events(&mut self, status: i32) {
        // C's wording at dmp.c:84-86.
        use core::fmt::Write as _;
        //[SD19-gate] IPC fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = writeln!(self.diag, "IS: warning, fkey_events failed: {}", status);
    }

    fn warn_fkey_ctl(&mut self, status: i32) {
        // C's wording at dmp.c:63-65.
        use core::fmt::Write as _;
        //[SD19-gate] IPC fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = writeln!(self.diag, "IS: warning, fkey_ctl failed: {}", status);
    }

    fn diag_out(&mut self) -> &mut dyn fmt::Write {
        &mut self.diag
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_sys_sef_transport_hosted_is_eio() {
        // The hosted trap transport honestly returns EIO (no faked success
        // before E1 hardware power-on).
        let mut t = super::SysSefTransport::new();
        let mut inbox = Message::default();
        let cancel = minix_sef::SefCancel::new();
        assert_eq!(
            t.receive(&mut inbox, &mut |_sig| {}, &cancel),
            Err(minix_types::EIO)
        );
    }

    #[test]
    fn test_sys_sef_diag_out_writable_and_warns_do_not_panic() {
        // The A-6 seam: write_str goes straight to sys_diagctl_write, and in a
        // hosted build the kernel's EIO is swallowed (warn is best-effort), so
        // writeln! still returns Ok; all three warnings take the same path.
        use core::fmt::Write as _;
        let mut t = super::SysSefTransport::new();
        assert!(writeln!(t.diag_out(), "IS: warning, got illegal request {} from {}", 65, 9).is_ok());
        t.warn_illegal(65, Endpoint(9));
        t.warn_fkey_events(-5);
        t.warn_fkey_ctl(-5);
    }

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
