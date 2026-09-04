//! Device removal: DEL handler + refcount + reaper (doc 08-devm-del-device).
//!
//! C: `do_del_device` (device.c:424-455) + `devman_get_device` (:460-467)
//! + `devman_put_device` (:471-480) + `devman_del_device` (:485-515).
//!
//! Entry boundary: like 07, this module starts past the transport
//! (no grant copy here — DEL carries only an id). Errors mirror C's
//! reply codes so the transport can `apply_reply` them unchanged.

use alloc::string::String;
use minix_types::Errno;

use crate::device_tree::DeviceTree;
use crate::files::with_files;
use crate::structs::{DeviceId, DeviceState, Event};
use crate::vtreefs::InodeTree;

/// C: `devman_get_device` (device.c:460-467) — NULL/root are no-ops,
/// else +1. (Root immunity: the root is never freed; NULL guards the
/// `dev == NULL` paths C threads through.)
pub fn get_device(tree: &mut DeviceTree, id: DeviceId) {
    if id == DeviceId::ROOT {
        return;
    }
    if let Some(slot) = tree
        .devices_mut_for_refcount(id)
    {
        *slot += 1;
    }
}

/// C: `devman_put_device` (device.c:471-480) — NULL/root are no-ops,
/// else -1, firing `del_device` at zero.
/// `fw`/`on_event` thread through for the zero case (C recurses into
/// `del_device`, which emits nothing itself — the REMOVE event was
/// already queued by `do_del` *before* the put; order matters, 08 §2.2).
pub fn put_device(
    tree: &mut DeviceTree,
    fw: &mut InodeTree,
    id: DeviceId,
) -> Result<(), Errno> {
    if id == DeviceId::ROOT {
        return Ok(());
    }
    let zero = match tree.devices_mut_for_refcount(id) {
        Some(slot) => {
            if *slot == 0 {
                // C asserts count > 0 implicitly (underflow would wrap);
                // Rust refuses (same hardening as 02's release()).
                return Err(Errno::EINVAL);
            }
            *slot -= 1;
            *slot == 0
        }
        None => return Err(Errno::ENODEV),
    };
    if zero {
        del_device(tree, fw, id)?;
    }
    Ok(())
}

/// C: `devman_del_device` (device.c:485-515) — free attribute files
/// (framework delete + store unregister + drop), free the device dir,
/// unlink from the parent (tree children list + tombstone), put the
/// parent once (cascade like C), drop the wire info (owned value —
/// no `free` needed), drop the object.
/// The missing-children check C comments about ("does device have
/// children -> error") has no code behind it; the refcount math covers
/// it (08 §2.3) — and this port asserts nothing extra either.
/// Root deletion is `EINVAL` (C asserts `node != &inode[0]` at the
/// framework layer too).
pub fn del_device(
    tree: &mut DeviceTree,
    fw: &mut InodeTree,
    id: DeviceId,
) -> Result<(), Errno> {
    if id == DeviceId::ROOT {
        return Err(Errno::EINVAL);
    }
    let dev = tree.get(id).ok_or(Errno::ENODEV)?;
    let parent = dev.parent.ok_or(Errno::EINVAL)?;
    // Attribute files: framework delete + store unregister each.
    // (Collect first: unregister mutates the store, not the tree —
    // no aliasing, but keep the phases visibly separate like C.)
    let attr_bindings: Vec<(crate::vtreefs::Ino, Option<usize>)> = dev
        .attrs
        .iter()
        .filter_map(|a| a.binding.map(|b| (b.ino, b.cookie)))
        .collect();
    let dir_ino = dev.binding.ok_or(Errno::EINVAL)?.ino;
    for (ino, cookie) in attr_bindings {
        // Framework delete failure on a half-torn tree would abort C
        // (asserts); here continue best-effort is wrong — propagate.
        // (In practice infallible: inos came from live adds.)
        fw.delete(ino)?;
        if let Some(c) = cookie {
            with_files(|s| s.unregister(c))?;
        }
    }
    // Device dir itself.
    fw.delete(dir_ino)?;
    // Unlink from parent + tombstone + put parent once (C :505-509).
    tree.unlink_child(parent, id)?;
    tree.remove(id)?;
    // Cascade exactly like C (`devman_put_device(dev->parent)`).
    put_device(tree, fw, parent)?;
    Ok(())
}

/// C: `do_del_device` (device.c:424-455) — find (`ENODEV` + `printf`
/// when missing, no event); REMOVE event; `BOUND → ZOMBIE` (only from
/// BOUND — an UNBOUND device stays UNBOUND through delete); `put`
/// (which may reap); reply `res` (`0` unless missing).
/// `on_event` injects the sink (same split as 07).
pub fn do_del(
    tree: &mut DeviceTree,
    fw: &mut InodeTree,
    id: DeviceId,
    on_event: &mut dyn FnMut(Event),
) -> Result<(), Errno> {
    // C: dev == NULL → printf + ENODEV, no event (device.c:434-438).
    let state = tree.get(id).ok_or(Errno::ENODEV)?.state;
    // C: remove_event BEFORE the state change (device.c:441).
    let path = tree.generate_path(fw, id, crate::structs::DEVMAN_STRING_LEN - 11)?;
    let mut line = String::from("REMOVE ");
    line.push_str(&path);
    {
        use core::fmt::Write as _;
        let _ = core::write!(line, " 0x{:08x}", id.0);
    }
    on_event(Event::new(&line)?);
    // C: BOUND → ZOMBIE only (device.c:442-444).
    if state == DeviceState::Bound
        && let Some(dev) = tree.get_mut_for_state(id)
    {
        dev.state = DeviceState::Zombie;
    }
    put_device(tree, fw, id)?;
    Ok(())
}

use alloc::vec::Vec;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::add_device::do_add;
    use crate::device_tree::default_file_stat;
    use crate::structs::DeviceState;
    use crate::wire::parse_device;
    use alloc::vec::Vec;
    use minix_types::Endpoint;

    fn harness() -> (DeviceTree, InodeTree) {
        let mut fw = InodeTree::new(64, crate::device_tree::default_dir_stat()).unwrap();
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

    fn add(tree: &mut DeviceTree, fw: &mut InodeTree, name: &str) -> DeviceId {
        let (_, parsed) = parse_device(&wire_one(name)).unwrap();
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
    fn del_missing_is_enodev_without_event() {
        // C: printf + ENODEV, no event (device.c:434-438).
        let (mut tree, mut fw) = harness();
        let mut events = Vec::new();
        assert_eq!(
            do_del(&mut tree, &mut fw, DeviceId(99), &mut |ev| events.push(ev)),
            Err(Errno::ENODEV)
        );
        assert!(events.is_empty());
    }

    #[test]
    fn del_unbound_removes_and_emits() {
        let (mut tree, mut fw) = harness();
        let id = add(&mut tree, &mut fw, "usb");
        let mut events = Vec::new();
        do_del(&mut tree, &mut fw, id, &mut |ev| events.push(ev)).unwrap();
        // Gone from the tree (tombstoned), framework dir freed.
        assert!(tree.get(id).is_none());
        assert_eq!(tree.find(id), None);
        // REMOVE line, exact format (C: REMOVE + path + id).
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].text(), "REMOVE ./devices/usb/ 0x00000001");
        // State never touched ZOMBIE (was UNBOUND).
    }

    #[test]
    fn del_bound_goes_zombie_first() {
        // C: BOUND → ZOMBIE (device.c:442-444). Reap needs refcount 0:
        // simulate a bound device with an extra ref held (09 holds one).
        let (mut tree, mut fw) = harness();
        let id = add(&mut tree, &mut fw, "usb");
        tree.get_mut_for_state(id).unwrap().state = DeviceState::Bound;
        get_device(&mut tree, id); // extra ref (like 09's bind get)
        let mut events = Vec::new();
        do_del(&mut tree, &mut fw, id, &mut |ev| events.push(ev)).unwrap();
        // Still present (ref held) but ZOMBIE.
        assert_eq!(tree.get(id).unwrap().state, DeviceState::Zombie);
        assert_eq!(events.len(), 1);
        // Release the bind ref → reaped now.
        put_device(&mut tree, &mut fw, id).unwrap();
        assert!(tree.get(id).is_none());
    }

    #[test]
    fn del_parent_with_live_child_survives() {
        // C-faithful member accounting (07 §3.2, 08 §2.4): each live
        // child holds +1 on the parent, so DEL on a non-empty parent
        // broadcasts but does NOT reap; the child DEL cascades back up.
        let (mut tree, mut fw) = harness();
        let usb = add(&mut tree, &mut fw, "usb");
        // Add a grandchild directly (second level).
        let (_, parsed) = parse_device(&wire_one("zero")).unwrap();
        let mut events = Vec::new();
        let zero = {
            use crate::add_device::do_add;
            do_add(
                &mut tree,
                &mut fw,
                usb,
                &parsed,
                Endpoint(9),
                &mut |ev| events.push(ev),
            )
            .unwrap()
        };
        // DEL the parent: REMOVE broadcast, but usb survives (child ref).
        let mut events2 = Vec::new();
        do_del(&mut tree, &mut fw, usb, &mut |ev| events2.push(ev)).unwrap();
        assert_eq!(events2.len(), 1);
        assert!(tree.get(usb).is_some());
        // DEL the child: child reaped, cascade puts usb to zero → reaped.
        let mut events3 = Vec::new();
        do_del(&mut tree, &mut fw, zero, &mut |ev| events3.push(ev)).unwrap();
        assert!(tree.get(zero).is_none());
        assert!(tree.get(usb).is_none());
        assert_eq!(events3.len(), 1);
    }

    #[test]
    fn get_put_root_and_null_are_noops() {
        // C: NULL/root guards (device.c:463/476).
        let (mut tree, mut fw) = harness();
        get_device(&mut tree, DeviceId::ROOT);
        assert!(put_device(&mut tree, &mut fw, DeviceId::ROOT).is_ok());
        assert!(put_device(&mut tree, &mut fw, DeviceId(99)).is_err());
    }
}
