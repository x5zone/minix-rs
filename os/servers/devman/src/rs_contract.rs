//! RS↔devman handshake contract, RS side modeled (doc 12-rs-integration).
//!
//! C: `publish_service` devman block (manager.c:840-851) +
//! `unpublish_service` devman block (:897-909) + `init_slot` inherit
//! (:1742) + `system.conf:422-429` (service permissions, doc-only).
//!
//! Entry boundary: DS lookup and `ipc_sendrec` are [`RsTransport`]
//! injections — this module is the RS *decision* logic (when to bind,
//! what failure means). RS lives in another stage; this contract keeps
//! devman's view of it testable without RS.

use minix_types::{Endpoint, Errno};

/// What RS publishes about a service relevant to devman
/// (C: `rpub->devman_id`, `rpub->endpoint` — rs.h:139/182).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishInfo {
    pub devman_id: i32,
    pub endpoint: Endpoint,
}

/// RS-side transport needs (injected; kernel/DS in production).
pub trait RsTransport {
    /// C: `ds_retrieve_label_endpt("devman", &ep)` (manager.c:841/898).
    fn devman_endpoint(&mut self) -> Result<Endpoint, Errno>;
    /// C: `ipc_sendrec(ep, &m)` with `DEVMAN_BIND` (manager.c:848-849).
    /// Returns the driver's `RESULT` word (transport errors as `Err`).
    fn send_bind(&mut self, devman: Endpoint, device: i32, endpoint: Endpoint) -> Result<i32, Errno>;
    /// C: `ipc_sendrec(ep, &m)` with `DEVMAN_UNBIND` (manager.c:905-906).
    fn send_unbind(
        &mut self,
        devman: Endpoint,
        device: i32,
        endpoint: Endpoint,
    ) -> Result<i32, Errno>;
}

/// Publish outcome (C: `publish_service`, manager.c:840-856).
/// `NotApplicable` (id == 0, no devman traffic at all) is the common
/// case — most services have no devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    NotApplicable,
    Bound,
    /// C: `kill_service(rp, …)` (manager.c:843/851) — publish failure
    /// kills the service being published. The string names the cause
    /// (C's literal messages, kept for grep-ability).
    KillService(&'static str),
}

/// C: `publish_service` devman block (manager.c:840-851) — id-gated DS
/// lookup → BIND → RESULT check, any failure kills the service.
pub fn publish(
    t: &mut impl RsTransport,
    info: PublishInfo,
) -> PublishOutcome {
    if info.devman_id == 0 {
        return PublishOutcome::NotApplicable;
    }
    let devman = match t.devman_endpoint() {
        Ok(ep) => ep,
        Err(_) => return PublishOutcome::KillService("devman not running?"),
    };
    match t.send_bind(devman, info.devman_id, info.endpoint) {
        Ok(0) => PublishOutcome::Bound,
        _ => PublishOutcome::KillService("devman bind device failed"),
    }
}

/// Unpublish outcome (C: `unpublish_service`, manager.c:897-909).
/// Failures only warn (`printf`, :894/:908) — unpublish never kills.
/// `Warned` carries nothing (C keeps `result` but only prints).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnpublishOutcome {
    NotApplicable,
    Unbound,
    Warned,
}

/// C: `unpublish_service` devman block — id-gated DS lookup → UNBIND.
/// DS failure prints and continues (:900-901); send/RESULT failure
/// prints "devman unbind device failed" (:907-908).
pub fn unpublish(
    t: &mut impl RsTransport,
    info: PublishInfo,
) -> UnpublishOutcome {
    if info.devman_id == 0 {
        return UnpublishOutcome::NotApplicable;
    }
    let devman = match t.devman_endpoint() {
        Ok(ep) => ep,
        Err(_) => return UnpublishOutcome::Warned,
    };
    match t.send_unbind(devman, info.devman_id, info.endpoint) {
        Ok(0) => UnpublishOutcome::Unbound,
        _ => UnpublishOutcome::Warned,
    }
}

/// C: `init_slot` inherit (manager.c:1742) — the new slot copies the
/// boot image's `devman_id`. Pure copy.
pub fn inherit_devman_id(boot_image_id: i32) -> i32 {
    boot_image_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    struct FakeRs {
        pub ds_ok: bool,
        pub bind_result: Result<i32, Errno>,
        pub unbind_result: Result<i32, Errno>,
        pub log: Vec<&'static str>,
    }

    impl RsTransport for FakeRs {
        fn devman_endpoint(&mut self) -> Result<Endpoint, Errno> {
            if self.ds_ok {
                Ok(Endpoint(3))
            } else {
                Err(Errno::ENODEV)
            }
        }

        fn send_bind(
            &mut self,
            _devman: Endpoint,
            _device: i32,
            _endpoint: Endpoint,
        ) -> Result<i32, Errno> {
            self.log.push("bind");
            self.bind_result
        }

        fn send_unbind(
            &mut self,
            _devman: Endpoint,
            _device: i32,
            _endpoint: Endpoint,
        ) -> Result<i32, Errno> {
            self.log.push("unbind");
            self.unbind_result
        }
    }

    fn info() -> PublishInfo {
        PublishInfo {
            devman_id: 5,
            endpoint: Endpoint(9),
        }
    }

    #[test]
    fn publish_zero_id_is_silent() {
        // Most services: no devman traffic at all (manager.c:840).
        let mut t = FakeRs {
            ds_ok: false,
            bind_result: Ok(0),
            unbind_result: Ok(0),
            log: Vec::new(),
        };
        assert_eq!(
            publish(&mut t, PublishInfo { devman_id: 0, endpoint: Endpoint(9) }),
            PublishOutcome::NotApplicable
        );
        assert!(t.log.is_empty());
    }

    #[test]
    fn publish_failures_kill() {
        // DS down → kill ("devman not running?", manager.c:843).
        let mut t = FakeRs {
            ds_ok: false,
            bind_result: Ok(0),
            unbind_result: Ok(0),
            log: Vec::new(),
        };
        assert_eq!(
            publish(&mut t, info()),
            PublishOutcome::KillService("devman not running?")
        );
        // Transport/result failure → kill ("devman bind device failed").
        let mut t2 = FakeRs {
            ds_ok: true,
            bind_result: Ok(5),
            unbind_result: Ok(0),
            log: Vec::new(),
        };
        assert_eq!(
            publish(&mut t2, info()),
            PublishOutcome::KillService("devman bind device failed")
        );
        // Success → Bound.
        let mut t3 = FakeRs {
            ds_ok: true,
            bind_result: Ok(0),
            unbind_result: Ok(0),
            log: Vec::new(),
        };
        assert_eq!(publish(&mut t3, info()), PublishOutcome::Bound);
    }

    #[test]
    fn unpublish_never_kills() {
        // All failure modes warn-and-continue (manager.c:894/908).
        let mut t = FakeRs {
            ds_ok: false,
            bind_result: Ok(0),
            unbind_result: Ok(0),
            log: Vec::new(),
        };
        assert_eq!(unpublish(&mut t, info()), UnpublishOutcome::Warned);
        let mut t2 = FakeRs {
            ds_ok: true,
            bind_result: Ok(0),
            unbind_result: Err(Errno::EIO),
            log: Vec::new(),
        };
        assert_eq!(unpublish(&mut t2, info()), UnpublishOutcome::Warned);
        let mut t3 = FakeRs {
            ds_ok: true,
            bind_result: Ok(0),
            unbind_result: Ok(0),
            log: Vec::new(),
        };
        assert_eq!(unpublish(&mut t3, info()), UnpublishOutcome::Unbound);
    }

    #[test]
    fn inherit_is_copy() {
        // manager.c:1742.
        assert_eq!(inherit_devman_id(7), 7);
        assert_eq!(inherit_devman_id(0), 0);
    }
}
