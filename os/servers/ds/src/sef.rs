//! DS startup: settle in, register four promises, wait to be asked.
//!
//! Mirrors `sef_local_startup()` (`minix3/minix/servers/ds/main.c:93-104`).
//! 01-ds-init-main.md.
//!
//! The lifecycle owns the role split and nothing else: which init names
//! exist, and which transfer hook the startup registers. The fresh-boot
//! body (`sef_cb_init_fresh`, 06), restart state keeping (libsef
//! generic), and the transfer body (06, A-6) stay out.

/// The init names DS registers (`sef_local_startup`, `main.c:96-97`).
///
/// Only two: a fresh boot runs DS code; a restart reuses the libsef
/// generic (`SEF_CB_INIT_RESTART_STATEFUL`, `sef.h:85`) and needs no
/// DS body — so this is an enum, not a trait (a one-method trait
/// would be decoration: no second implementor, never a bound).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsInitKind {
    /// Fresh boot. C: `sef_setcb_init_fresh(sef_cb_init_fresh)` — main.c:96.
    Fresh,
    /// Restart with state kept. C: `sef_setcb_init_restart
    /// (SEF_CB_INIT_RESTART_STATEFUL)` — main.c:97; the generic lives in
    /// libsef, DS contributes no code.
    RestartStateful,
}

/// The Live Update transfer hook DS registers (`main.c:100`).
///
/// C: `sef_llvm_ds_st_init()` (`sef.h:372`) — the weak magic hook that
/// walks DS static memory on update (A-6). minix-rs has no LLVM magic:
/// the transfer body becomes explicit serialization in 06, so this enum
/// only names the promise here — the startup registers it, 06 fulfils it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveUpdateHook {
    /// DS state transfer registered. C: `sef_llvm_ds_st_init()` — main.c:100.
    DsStateTransfer,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_kinds() {
        // Two registrations (`main.c:96-97`); restart is generic (sef.h:85).
        assert_eq!(DsInitKind::Fresh, DsInitKind::Fresh);
        assert_ne!(DsInitKind::Fresh, DsInitKind::RestartStateful);
    }

    #[test]
    fn test_hook_named() {
        // The hook is named at startup (`main.c:100`); its body lives in 06.
        assert_eq!(
            LiveUpdateHook::DsStateTransfer,
            LiveUpdateHook::DsStateTransfer
        );
    }
}
