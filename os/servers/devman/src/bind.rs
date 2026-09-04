//! Bind/unbind: RS handshake + owner forwarding (doc 09-devm-bind-unbind).
//!
//! C: `do_bind_device` (bind.c:7-50) + `do_unbind_device` (:56-104).
//!
//! Entry boundary: the RS-only gate (`check_rs`, 05) runs *inside* each
//! handler, exactly where C checks. Transport business (the actual
//! `ipc_sendrec` to the owner, the async reply to RS) is modeled as
//! returned [`Action`]s — pure, testable, no IPC needed.

use minix_types::{Endpoint, Errno};

use crate::device_tree::DeviceTree;
use crate::ipc::check_rs;
use crate::structs::{DeviceId, DeviceState};

/// What the transport must do after a bind/unbind handler runs.
/// `Dropped` (EPERM, non-RS sender) explicitly sends **nothing**
/// (bind.c:14-19/63-68 return without sending — 05 §2.4).
/// `Reply(Ok(()))` means "reply OK"; `Reply(Err(e))` means "reply e".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Forward the request to the device owner (`ipc_sendrec` in C);
    /// the driver's response comes back through `on_bind_response` /
    /// `on_unbind_response`.
    Forward {
        owner: Endpoint,
        bind: bool,
        device: DeviceId,
        endpoint: Endpoint,
    },
    /// Reply to RS (`apply_reply` + `ipc_send(RS)`).
    Reply(Result<(), Errno>),
    /// Send nothing at all (EPERM path only).
    Dropped,
}

/// C: `do_bind_device` through the forward (bind.c:7-32) — gate, find
/// (`ENODEV` when missing, :44-46), else forward to the owner.
/// State changes happen in [`on_bind_response`], not here (C changes
/// state after `sendrec` returns).
pub fn do_bind(
    tree: &DeviceTree,
    source: Endpoint,
    device: DeviceId,
    endpoint: Endpoint,
) -> Action {
    // C: non-RS → RESULT = EPERM, no reply sent (:14-19).
    if check_rs(source).is_err() {
        return Action::Dropped;
    }
    match tree.get(device) {
        // C forwards to dev->owner (:32); owner is always set by ADD
        // (device.c:273) — the fallback only fires for hand-built
        // states and keeps the message routable instead of sending
        // it to endpoint 0.
        Some(dev) => Action::Forward {
            owner: dev.owner.unwrap_or(source),
            bind: true,
            device,
            endpoint,
        },
        None => Action::Reply(Err(Errno::ENODEV)),
    }
}

/// C: `do_bind_device` response half (bind.c:32-43) — sendrec failure →
/// keep the transport error; driver non-OK → keep it, no state change;
/// OK → `BOUND` + `get` (ref held while bound).
pub fn on_bind_response(
    tree: &mut DeviceTree,
    device: DeviceId,
    driver_result: Result<(), Errno>,
) -> Result<(), Errno> {
    driver_result?;
    match tree.get_mut_for_state(device) {
        Some(dev) => {
            dev.state = DeviceState::Bound;
            dev.refcount += 1;
            Ok(())
        }
        None => Err(Errno::ENODEV),
    }
}

/// C: `do_unbind_device` through the forward (bind.c:56-84) — same gate
/// and find shape as bind.
pub fn do_unbind(
    tree: &DeviceTree,
    source: Endpoint,
    device: DeviceId,
    endpoint: Endpoint,
) -> Action {
    // C: non-RS → RESULT = EPERM, no reply sent (:62-68).
    if check_rs(source).is_err() {
        return Action::Dropped;
    }
    match tree.get(device) {
        Some(dev) => Action::Forward {
            owner: dev.owner.unwrap_or(source),
            bind: false,
            device,
            endpoint,
        },
        None => Action::Reply(Err(Errno::ENODEV)),
    }
}

/// C: `do_unbind_device` response half (bind.c:80-95) — sendrec failure →
/// keep it; driver error other than 19 (`ENODEV`, "driver deleted the
/// device already?", :85-86) → keep it; else (`OK` **or** 19): state →
/// `UNBOUND` unless `ZOMBIE`, `put`, and the reply is forced `Ok`
/// (C overwrites `RESULT = OK` even for 19, :94).
pub fn on_unbind_response(
    tree: &mut DeviceTree,
    fw: &mut crate::vtreefs::InodeTree,
    device: DeviceId,
    driver_result: Result<(), Errno>,
) -> Result<(), Errno> {
    if let Err(e) = driver_result
        && e != Errno::ENODEV
    {
        return Err(e);
    }
    if let Some(dev) = tree.get_mut_for_state(device)
        && dev.state != DeviceState::Zombie
    {
        dev.state = DeviceState::Unbound;
    }
    // C: put unconditionally on this path (:93) — may reap (08).
    let _ = crate::del_device::put_device(tree, fw, device);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::add_device::do_add;
    use crate::device_tree::default_file_stat;
    use crate::wire::parse_device;
    use alloc::vec::Vec;
    use minix_types::RS_PROC_NR;

    fn harness() -> (DeviceTree, crate::vtreefs::InodeTree) {
        let mut fw =
            crate::vtreefs::InodeTree::new(64, crate::device_tree::default_dir_stat()).unwrap();
        let tree = DeviceTree::new(&mut fw, default_file_stat()).unwrap();
        (tree, fw)
    }

    fn wire_one(name: &str) -> Vec<u8> {
        let mut buf = alloc::vec![0u8; 16];
        buf[0..4].copy_from_slice(&0i32.to_le_bytes());
        buf[4..8].copy_from_slice(&0i32.to_le_bytes());
        let mut s = Vec::new();
        let mut push = |t: &str| -> u32 {
            let o = (buf.len() + s.len()) as u32;
            s.extend_from_slice(t.as_bytes());
            s.push(0);
            o
        };
        let no = push(name);
        buf[8..12].copy_from_slice(&no.to_le_bytes());
        buf.extend_from_slice(&s);
        buf
    }

    fn add(tree: &mut DeviceTree, fw: &mut crate::vtreefs::InodeTree) -> DeviceId {
        let (_, parsed) = parse_device(&wire_one("usb")).unwrap();
        let mut events = Vec::new();
        do_add(
            tree,
            fw,
            DeviceId::ROOT,
            &parsed,
            Endpoint(9),
            &mut |ev| events.push(ev),
        )
        .unwrap()
    }

    #[test]
    fn non_rs_is_dropped_silently() {
        // C: EPERM written, nothing sent (bind.c:14-19/63-68). The
        // Dropped action IS the no-send lock (05 §3.3).
        let (tree, _) = harness();
        assert_eq!(
            do_bind(&tree, Endpoint(9), DeviceId(1), Endpoint(9)),
            Action::Dropped
        );
        assert_eq!(
            do_unbind(&tree, Endpoint(9), DeviceId(1), Endpoint(9)),
            Action::Dropped
        );
    }

    #[test]
    fn missing_is_enodev_reply() {
        // C: ENODEV reply (bind.c:44-46/96-99).
        let (tree, _) = harness();
        assert_eq!(
            do_bind(&tree, RS_PROC_NR, DeviceId(99), Endpoint(9)),
            Action::Reply(Err(Errno::ENODEV))
        );
        assert_eq!(
            do_unbind(&tree, RS_PROC_NR, DeviceId(99), Endpoint(9)),
            Action::Reply(Err(Errno::ENODEV))
        );
    }

    #[test]
    fn bind_ok_holds_ref_until_unbind() {
        let (mut tree, mut fw) = harness();
        let id = add(&mut tree, &mut fw);
        // Forward to the owner (driver endpoint 9 = owner from ADD).
        match do_bind(&tree, RS_PROC_NR, id, Endpoint(4)) {
            Action::Forward { owner, bind: true, device, endpoint } => {
                assert_eq!(owner, Endpoint(9));
                assert_eq!(device, id);
                assert_eq!(endpoint, Endpoint(4));
            }
            other => panic!("expected forward, got {other:?}"),
        }
        // Driver OK → BOUND + ref.
        assert_eq!(on_bind_response(&mut tree, id, Ok(())), Ok(()));
        assert_eq!(tree.get(id).unwrap().state, DeviceState::Bound);
        // Driver error → no state change.
        assert_eq!(
            on_bind_response(&mut tree, id, Err(Errno::EIO)),
            Err(Errno::EIO)
        );
        assert_eq!(tree.get(id).unwrap().state, DeviceState::Bound);
        // Unbind OK → UNBOUND + ref released (reap needs no other refs:
        // creation ref 1 + bind ref 1 - del? no del here — stays live).
        match do_unbind(&tree, RS_PROC_NR, id, Endpoint(4)) {
            Action::Forward { bind: false, .. } => {}
            other => panic!("expected forward, got {other:?}"),
        }
        assert_eq!(
            on_unbind_response(&mut tree, &mut fw, id, Ok(())),
            Ok(())
        );
        assert_eq!(tree.get(id).unwrap().state, DeviceState::Unbound);
    }

    #[test]
    fn unbind_enodev_tolerated_and_forced_ok() {
        // C: driver 19 → still UNBOUND + put + reply OK (:85-95).
        let (mut tree, mut fw) = harness();
        let id = add(&mut tree, &mut fw);
        on_bind_response(&mut tree, id, Ok(())).unwrap();
        assert_eq!(
            on_unbind_response(&mut tree, &mut fw, id, Err(Errno::ENODEV)),
            Ok(())
        );
        assert_eq!(tree.get(id).unwrap().state, DeviceState::Unbound);
        // Other driver errors propagate.
        assert_eq!(
            on_unbind_response(&mut tree, &mut fw, id, Err(Errno::EIO)),
            Err(Errno::EIO)
        );
    }
}
