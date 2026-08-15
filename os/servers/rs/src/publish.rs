//! Service publishing: DS label registration and device binding triggers.
//!
//! Mirrors `minix3/minix/servers/rs/manager.c:787-921` (`publish_service` —
//! 787, `unpublish_service` — 864). 11-rs-publish.md.
//!
//! The kernel/IPC-coupled steps (`ds_publish_label`, `mapdriver`,
//! `pci_set_acl`/`pci_del_acl`, the `DEVMAN_BIND`/`DEVMAN_UNBIND` messages,
//! the second `setuid(0)` hack) are wired through 19-rs-external-interfaces.md
//! (DEFERRED). This module owns the pure decision predicates and the
//! best-effort result aggregation of `unpublish_service`.

use crate::service_slot::PublicSlot;

/// Whether the service is a driver that must be mapped into VFS.
///
/// C: `publish_service` — manager.c:806: `dev_nr > 0 || nr_domain > 0`.
/// `NO_DEV` is 0 (include/minix/const.h:132, 02-rs-process-table.md §2.4).
pub fn should_map_driver(pub_: &PublicSlot) -> bool {
    pub_.dev_nr > 0 || pub_.nr_domain > 0
}

/// Whether the service carries PCI ACL entries.
///
/// C: `publish_service` — manager.c:826-838 (under `USE_PCI`):
/// `rsp_nr_device || rsp_nr_class`. ARCH A-10: minix-rs has no PCI driver
/// face, so the `rs_pci` structure (rs.h:154-161) is not modelled and the
/// predicate is fail-closed (`false`); the semantic contract ("non-zero
/// device/class counts trigger `pci_set_acl`") is preserved for the 19
/// wiring (11-rs-publish.md §3.3).
pub fn should_set_pci_acl(_pub_: &PublicSlot) -> bool {
    false
}

/// Whether the service must be bound to a devman device.
///
/// C: `publish_service` — manager.c:840, 897: `devman_id != 0`. The C `int`
/// (0 = unbound) maps to `Option<i32>` (02-rs-process-table.md): `None` or
/// `Some(0)` means unbound (11-rs-publish.md §3.2).
pub fn should_bind_devman(pub_: &PublicSlot) -> bool {
    pub_.devman_id.is_some_and(|id| id != 0)
}

/// Aggregates the best-effort result of `unpublish_service`.
///
/// C: `unpublish_service` — manager.c:864-920. `ds_delete_label` and
/// `pci_del_acl` failures are recorded only when the system is not shutting
/// down (manager.c:877-882, 886-893); the devman unbind failures only log
/// and never change the result (manager.c:897-914). `shutting_down` is
/// injected (C reads the `shutting_down` global, glo.h:46, set by
/// `do_shutdown` — 13-rs-control-requests.md).
///
/// C overwrites `result` on each recorded failure, so the *last* failure's
/// error wins; both call sites (manager.c:1138, request.c:143) ignore the
/// return value, so the specific code is unobservable. Rust keeps the same
/// shape with `EIO` placeholders. The `devman_ok` parameter is the outcome
/// of the `DEVMAN_UNBIND` exchange; per C it never affects the result.
pub fn unpublish_result(
    ds_delete_ok: bool,
    pci_del_ok: bool,
    devman_ok: bool,
    shutting_down: bool,
) -> i32 {
    let mut result = 0; // C: result = OK
    if !ds_delete_ok && !shutting_down {
        result = minix_types::EIO; // C records `r` (the ds error) — shape kept
    }
    if !pci_del_ok && !shutting_down {
        result = minix_types::EIO;
    }
    let _ = devman_ok; // C: devman failures only log (manager.c:897-914)
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_slot::PublicSlot;

    #[test]
    fn test_should_map_driver_dev_nr() {
        // C: manager.c:806 — dev_nr > 0 triggers mapdriver.
        let mut p = PublicSlot::vacant();
        p.dev_nr = 3;
        assert!(should_map_driver(&p));
    }

    #[test]
    fn test_should_map_driver_domain() {
        // C: manager.c:806 — nr_domain > 0 triggers mapdriver.
        let mut p = PublicSlot::vacant();
        p.nr_domain = 1;
        assert!(should_map_driver(&p));
    }

    #[test]
    fn test_should_map_driver_none() {
        // NO_DEV = 0 (const.h:132) and no domains → no mapdriver.
        let p = PublicSlot::vacant();
        assert!(!should_map_driver(&p));
    }

    #[test]
    fn test_should_bind_devman() {
        // C: manager.c:840, 897 — devman_id != 0.
        let mut p = PublicSlot::vacant();
        assert!(!should_bind_devman(&p)); // None → unbound
        p.devman_id = Some(0);
        assert!(!should_bind_devman(&p)); // Some(0) ≡ C's 0
        p.devman_id = Some(5);
        assert!(should_bind_devman(&p));
    }

    #[test]
    fn test_unpublish_result_shutting_down() {
        // C: manager.c:877-893 — failures recorded only when !shutting_down.
        assert_eq!(unpublish_result(false, true, true, false), minix_types::EIO);
        assert_eq!(
            unpublish_result(false, false, true, false),
            minix_types::EIO
        );
        // Shutting down suppresses error recording.
        assert_eq!(unpublish_result(false, false, true, true), 0);
        // devman failures never change the result (manager.c:897-914).
        assert_eq!(unpublish_result(true, true, false, false), 0);
    }
}
