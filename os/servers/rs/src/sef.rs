//! SEF callback registration (ARCH A-7).
//!
//! Mirrors `sef_local_startup()` (`minix3/minix/servers/rs/main.c:136-152`):
//! RS registers 7 SEF callbacks before entering the main loop. RS is the
//! **only** user-space service that registers the full callback set — it is
//! the SEF *provider* (every other service's init protocol is implemented by
//! RS).
//!
//! C uses global function pointers (`sef_setcb_*`, libsys/sef.c) whose bodies
//! reach the C globals (`rproc[]`, `rupdate`, ...). A Rust `fn` pointer has no
//! capture — it cannot reach the server state — so the callback set is modeled
//! as a **trait** implemented by [`crate::RsServer`] (N5 — todo §11): the
//! callbacks become state-machine methods on the server, and `&mut self` is
//! exactly the single-threaded user-space execution model (AGENTS.md).
//! 01-rs-boot-init.md §3.3.
//!
//! Callback *mechanisms* live in their ownership docs; this module only
//! defines the table and the init-type dispatch:
//!
//! | Callback | C registration | Mechanism doc |
//! |----------|----------------|---------------|
//! | [`SefCallbacks::init_fresh`] | main.c:139 | 01 (this boot) |
//! | [`SefCallbacks::init_restart`] | main.c:140 | 18-rs-self-lifecycle.md |
//! | [`SefCallbacks::init_lu`] | main.c:141 | 18-rs-self-lifecycle.md |
//! | [`SefCallbacks::init_response`] | main.c:144 | 12-rs-init-run.md |
//! | [`SefCallbacks::lu_response`] | main.c:145 | 12-rs-init-run.md |
//! | [`SefCallbacks::signal_handler`] | main.c:148 | 06-rs-main-loop.md |
//! | [`SefCallbacks::signal_manager`] | main.c:149 | 06-rs-main-loop.md |

use minix_types::{Endpoint, Errno};

/// SEF init type carried in the SEF_INIT message.
///
/// C: `SEF_INIT_FRESH=0` / `SEF_INIT_LU=1` / `SEF_INIT_RESTART=2` —
/// `minix3/minix/include/minix/sef.h:93-95`.
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
/// C: `sef_init_info_t` — `minix3/minix/include/minix/sef.h:53`. Field
/// semantics (endpoint/old_endpoint/state transfer) are consumed by
/// 12-rs-init-run.md and 18-rs-self-lifecycle.md; this module only carries
/// the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SefInitInfo {
    /// C: `info->endpoint` (the service's own endpoint).
    pub endpoint: i32,
    /// C: `info->old_endpoint` (previous incarnation during LU/restart).
    pub old_endpoint: i32,
}

/// Which handler the restart callback dispatches to (A3).
///
/// C's restart *table entry* is runtime state: `sef_local_startup()`
/// registers RS's own handler (main.c:140 `sef_setcb_init_restart(
/// sef_cb_init_restart)` — the full takeover chain), and
/// `sef_cb_init_lu` rebinds the entry to the stateful transfer generic
/// (main.c:558 `sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL)`,
/// sef.h:85) *before* the LU default runs, so a later restart takes the
/// stateful path (libsys/sef_init.c:317-330) instead of RS's full chain.
/// A static trait cannot express this — the rebind target is a field of
/// the server (A3 方案 a: the rebind is a single point, so one enum field
/// beats reifying the whole callback set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RestartCb {
    /// RS's own restart handler — the startup registration (main.c:140;
    /// body main.c:499-544).
    #[default]
    Rs,
    /// The stateful transfer generic (libsys/sef_init.c:317-330) — set by
    /// the LU rebind (main.c:558). The transfer itself (checkpoint/identity
    /// state data) is 17/18 号; until then this arm fails closed.
    Stateful,
}

/// The 7-method SEF callback set (ARCH A-7, N5).
///
/// C: `sef_local_startup()` — main.c:136-152. Modeled as a trait instead of
/// a fn-pointer struct so every callback body can reach the server state: a
/// `fn` pointer cannot capture anything, which would force the 12/18/06
/// callback bodies back to globals (the C-ism a rewrite should drop).
/// [`crate::RsServer`] implements the trait; `RsServer::init` dispatches
/// [`SefInitType`] through it (01-rs-boot-init.md §3.3).
pub trait SefCallbacks {
    /// C: `sef_cb_init_fresh` — main.c:139, dispatched by
    /// `sef_startup()`. The fresh boot IS the 4-step `BootInit::init_fresh`
    /// (01-rs-boot-init.md); `RsServer::init(Fresh)` routes here.
    fn init_fresh(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno>;

    /// C: `sef_cb_init_restart` — main.c:140. Mechanism: 18-rs-self-lifecycle.md.
    fn init_restart(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno>;

    /// C: `sef_cb_init_lu` — main.c:141. Mechanism: 18-rs-self-lifecycle.md.
    fn init_lu(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno>;

    /// C: `sef_cb_init_response` — main.c:144. Mechanism: 12-rs-init-run.md.
    fn init_response(&mut self, m: &minix_types::Message) -> Result<i32, Errno>;

    /// C: `sef_cb_lu_response` — main.c:145. Mechanism: 12-rs-init-run.md.
    fn lu_response(&mut self, m: &minix_types::Message) -> Result<i32, Errno>;

    /// C: `sef_cb_signal_handler` — main.c:148. Mechanism: 06-rs-main-loop.md.
    fn signal_handler(&mut self, signo: i32);

    /// C: `sef_cb_signal_manager` — main.c:149. Mechanism: 06-rs-main-loop.md.
    ///
    /// Signature mirrors the C callback type verbatim —
    /// `int(*)(endpoint_t target, int signo)` (sef.h:270): `target` is the
    /// signal-manager endpoint the request is forwarded to, `signo` the
    /// signal number. Typed as [`Endpoint`] (not a bare `i32`) so the two
    /// arguments cannot be transposed at a call site (R26, todo §18).
    fn signal_manager(&mut self, target: Endpoint, signo: i32) -> Result<i32, Errno>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Message;

    fn server() -> crate::RsServer {
        crate::RsServer::with_kernel(
            crate::BootTables::placeholder(),
            alloc::boxed::Box::new(crate::boot::UnimplementedKernelApi),
        )
    }

    #[test]
    fn test_deferred_callbacks_fail_closed() {
        // N5: the 12/18/06 callback bodies are not wired yet — every one of
        // them must fail closed with `Err(ENOSYS)` instead of panicking or
        // silently succeeding (T2 pattern).
        let mut s = server();
        let info = SefInitInfo::default();
        assert_eq!(
            s.init_restart(SefInitType::Restart, &info),
            Err(Errno::ENOSYS)
        );
        assert_eq!(s.init_lu(SefInitType::Lu, &info), Err(Errno::ENOSYS));
        let m = Message::default();
        // init_response is the LIVE sef wrapper (12 decode landed): the
        // underlying do_init_ready fails closed without state (ENOSYS) and
        // the wrapper normalizes it to the C `int` shape — R3 keeps
        // lu_response (16-gated) on the raw Err form.
        assert_eq!(s.init_response(&m), Ok(Errno::ENOSYS.to_i32()));
        assert_eq!(s.lu_response(&m), Err(Errno::ENOSYS));
        assert_eq!(s.signal_manager(Endpoint::RS, 1), Err(Errno::ENOSYS));
        // E-6: the fresh boot through the fail-closed kernel face fails with
        // Kernel(ENOSYS) — the wire face keeps ENOSYS and the typed cause
        // lands on the server diagnostic for the fatal-boot report.
        assert_eq!(s.init(SefInitType::Fresh), Err(Errno::ENOSYS));
        assert_eq!(
            s.boot_diagnostic(),
            Some(crate::boot::BootError::Kernel(Errno::ENOSYS))
        );
    }

    #[test]
    fn test_init_lu_rebinds_restart_cb() {
        // A3 (main.c:553-556): sef_cb_init_lu rebinds the restart table
        // entry to the stateful generic BEFORE the LU flow runs — the
        // rebind survives even when the LU fails (C never un-rebinds), and
        // the next restart dispatches the stateful arm, not RS's chain.
        let mut s = server();
        assert_eq!(s.restart_cb, crate::sef::RestartCb::Rs); // main.c:140
        let info = SefInitInfo::default();
        // The LU fails closed (no runtime state yet) — the rebind already
        // happened.
        assert_eq!(s.init_lu(SefInitType::Lu, &info), Err(Errno::ENOSYS));
        assert_eq!(s.restart_cb, crate::sef::RestartCb::Stateful);
        // The rebound entry dispatches the stateful transfer (fail-closed
        // until the 17/18 state machinery lands); the wire face is a bare
        // errno either way, the typed field carries the distinction.
        assert_eq!(
            s.init_restart(SefInitType::Restart, &info),
            Err(Errno::ENOSYS)
        );
    }
}
