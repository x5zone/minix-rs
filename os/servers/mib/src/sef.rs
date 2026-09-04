//! MIB startup: settle in, register twice, lose the leaves on restart.
//!
//! Mirrors `mib_startup()` (`minix3/minix/servers/mib/main.c:415-428`).
//! 01-mib-init-main.md.
//!
//! The lifecycle owns the role split and nothing else: which init names
//! exist and what each promises about state. The init body itself
//! (`mib_init`, main.c:384-410 — subtree wiring, tree walk, remote reset)
//! lands in 04; the SEF transport (`sef_setcb_*`, `sef_startup`) stays in
//! `minix-sef`, mirroring how DS leaves its restart body to libsef.

/// The init names MIB registers (`mib_startup`, `main.c:419-425`).
///
/// Both register the *same* body (`mib_init`), but the promise differs:
/// a fresh boot builds the static tree from nothing, while a restart
/// rebuilds only the static tree — every node created at run time is
/// gone (`main.c:420-424`). The comment is blunt: running with only the
/// static tree beats not running at all. This is the mirror image of DS,
/// whose restart keeps everything (`SEF_CB_INIT_RESTART_STATEFUL`):
/// a registry that forgot would be useless, but a sysctl tree that
/// forgot its run-time leaves is merely poorer — the static skeleton
/// still answers every compiled-in name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MibInitKind {
    /// Fresh boot. C: `sef_setcb_init_fresh(mib_init)` — main.c:419.
    Fresh,
    /// Restart, dynamic state dropped. C:
    /// `sef_setcb_init_restart(mib_init)` — main.c:425. Static tree
    /// rebuilt by the same body; run-time nodes (08) are not restored.
    /// Callers must re-create (or re-register, 12/22) what they added.
    RestartLossy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_kinds_name_the_promise() {
        // Two registrations, one body, two promises (`419-425`).
        assert_eq!(MibInitKind::Fresh, MibInitKind::Fresh);
        assert_ne!(MibInitKind::Fresh, MibInitKind::RestartLossy);
    }
}
