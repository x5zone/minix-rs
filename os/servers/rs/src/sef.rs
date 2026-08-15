//! SEF callback registration (ARCH A-7).
//!
//! Mirrors `sef_local_startup()` (`minix3/minix/servers/rs/main.c:136-152`):
//! RS registers 7 SEF callbacks before entering the main loop. RS is the
//! **only** user-space service that registers the full callback set — it is
//! the SEF *provider* (every other service's init protocol is implemented by
//! RS).
//!
//! C uses global function pointers (`sef_setcb_*`, libsys/sef.c); Rust models
//! the registration table as a plain struct, so the callback set is a value
//! that can be constructed, inspected and passed around (01-rs-boot-init.md
//! §3.3).
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

/// Init callback type. C: `sef_cb_init_t` — sef.h:56.
pub type SefInitCb = fn(SefInitType, &SefInitInfo) -> Result<i32, i32>;
/// Init-response callback type. C: `sef_cb_init_response_t`.
pub type SefMsgCb = fn(&minix_types::Message) -> Result<i32, i32>;
/// Signal handler callback type. C: `sef_cb_signal_handler_t`.
pub type SefSignalCb = fn(i32);
/// Signal manager callback type. C: `sef_cb_signal_manager_t`.
pub type SefSignalMgrCb = fn(i32, i32) -> i32;

/// The 7-slot SEF callback table.
///
/// C: `sef_local_startup()` — main.c:136-152. `local_startup()` builds the
/// full table; `startup()` dispatches the SEF init message to the right
/// callback by [`SefInitType`].
#[derive(Debug, Clone, Copy)]
pub struct SefCallbacks {
    /// C: `sef_setcb_init_fresh` — main.c:139.
    pub init_fresh: SefInitCb,
    /// C: `sef_setcb_init_restart` — main.c:140 (mechanism: 18).
    pub init_restart: SefInitCb,
    /// C: `sef_setcb_init_lu` — main.c:141 (mechanism: 18).
    pub init_lu: SefInitCb,
    /// C: `sef_setcb_init_response` — main.c:144 (mechanism: 12).
    pub init_response: SefMsgCb,
    /// C: `sef_setcb_lu_response` — main.c:145 (mechanism: 12).
    pub lu_response: SefMsgCb,
    /// C: `sef_setcb_signal_handler` — main.c:148 (mechanism: 06).
    pub signal_handler: SefSignalCb,
    /// C: `sef_setcb_signal_manager` — main.c:149 (mechanism: 06).
    pub signal_manager: SefSignalMgrCb,
}

impl SefCallbacks {
    /// Builds the full RS callback table.
    ///
    /// C: `sef_local_startup()` — main.c:136-152. The actual callback bodies
    /// are wired by the owning docs (12/18/06); placeholder functions fail
    /// closed until then.
    pub fn local_startup() -> Self {
        Self {
            init_fresh: init_fresh_placeholder,
            init_restart: init_restart_placeholder,
            init_lu: init_lu_placeholder,
            init_response: init_response_placeholder,
            lu_response: lu_response_placeholder,
            signal_handler: signal_handler_placeholder,
            signal_manager: signal_manager_placeholder,
        }
    }

    /// Dispatches the SEF init message to the registered callback.
    ///
    /// C: `sef_startup()` — libsys/sef.c: reads the `SEF_INIT` message and
    /// calls `init_fresh`/`init_lu`/`init_restart` by `info.init_type`.
    /// The message receive mechanism itself is 12-rs-init-run.md (RS_INIT
    /// branch of the main loop).
    pub fn startup(&self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, i32> {
        match init_type {
            SefInitType::Fresh => (self.init_fresh)(init_type, info),
            SefInitType::Lu => (self.init_lu)(init_type, info),
            SefInitType::Restart => (self.init_restart)(init_type, info),
        }
    }
}

// ── Placeholders (fail-closed until owning docs land) ───────────────────────

fn init_fresh_placeholder(_t: SefInitType, _info: &SefInitInfo) -> Result<i32, i32> {
    unimplemented!("sef_cb_init_fresh body: 01-rs-boot-init.md (BootInit::init_fresh)")
}

fn init_restart_placeholder(_t: SefInitType, _info: &SefInitInfo) -> Result<i32, i32> {
    unimplemented!("sef_cb_init_restart body: 18-rs-self-lifecycle.md")
}

fn init_lu_placeholder(_t: SefInitType, _info: &SefInitInfo) -> Result<i32, i32> {
    unimplemented!("sef_cb_init_lu body: 18-rs-self-lifecycle.md")
}

fn init_response_placeholder(_m: &minix_types::Message) -> Result<i32, i32> {
    unimplemented!("sef_cb_init_response body: 12-rs-init-run.md")
}

fn lu_response_placeholder(_m: &minix_types::Message) -> Result<i32, i32> {
    unimplemented!("sef_cb_lu_response body: 12-rs-init-run.md")
}

fn signal_handler_placeholder(_signo: i32) {
    unimplemented!("sef_cb_signal_handler body: 06-rs-main-loop.md")
}

fn signal_manager_placeholder(_target: i32, _signo: i32) -> i32 {
    unimplemented!("sef_cb_signal_manager body: 06-rs-main-loop.md")
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Message;

    fn ok_fresh(_t: SefInitType, _i: &SefInitInfo) -> Result<i32, i32> {
        Ok(0)
    }

    #[test]
    fn test_local_startup_registers_all() {
        let cb = SefCallbacks::local_startup();
        // The full 7-slot table is populated (main.c:139-149). Callback
        // identity is intentionally not compared (fn addresses may vary across
        // codegen units); the meaningful contract is that the wired callback
        // fails closed instead of silently succeeding.
        let info = SefInitInfo::default();
        let result = std::panic::catch_unwind(|| cb.startup(SefInitType::Fresh, &info));
        assert!(
            result.is_err(),
            "init_fresh placeholder must fail closed (unimplemented)"
        );
    }

    #[test]
    fn test_startup_dispatches_fresh() {
        let mut cb = SefCallbacks::local_startup();
        cb.init_fresh = ok_fresh;
        let info = SefInitInfo::default();
        assert_eq!(cb.startup(SefInitType::Fresh, &info), Ok(0));
    }

    #[test]
    fn test_startup_dispatches_lu() {
        let mut cb = SefCallbacks::local_startup();
        cb.init_lu = ok_fresh;
        let info = SefInitInfo::default();
        assert_eq!(cb.startup(SefInitType::Lu, &info), Ok(0));
    }

    #[test]
    fn test_startup_dispatches_restart() {
        let mut cb = SefCallbacks::local_startup();
        cb.init_restart = ok_fresh;
        let info = SefInitInfo::default();
        assert_eq!(cb.startup(SefInitType::Restart, &info), Ok(0));
    }

    #[test]
    fn test_msg_cb_type_is_callable() {
        // Type-level check: SefMsgCb accepts a Message reference.
        let _f: SefMsgCb = |_m: &Message| Ok(0);
    }
}
