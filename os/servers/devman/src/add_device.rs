//! Device registration: ADD handler (doc 07-devm-add-device).
//!
//! C: `do_add_device` (device.c:223-277) + `devman_dev_add_child`
//! (:345-397) + `devman_dev_add_info` (:404-418) +
//! `devman_dev_add_static_info` (:314-339).
//!
//! Entry boundary (05 §2.2): the grant copy is **transport** business
//! (`malloc` + `sys_safecopyfrom` + ENOMEM/EINVAL mapping) — this module
//! starts from local, already-copied wire bytes. Handler errors mirror
//! C's reply codes one-to-one so the transport can `apply_reply` them.

use alloc::string::String;
use minix_types::Endpoint;

use crate::device_tree::DeviceTree;
use crate::files::{register_file, FileEntry, FileKind, StaticFile};
use crate::structs::{Attribute, Device, DeviceId, DeviceState, Event, DEVMAN_STRING_LEN};
use crate::vtreefs::InodeTree;
use crate::wire::{EntryType, ParsedDevice};

/// Build one device from a decoded ADD description and link it in.
///
/// Mirrors `do_add_device` + `devman_dev_add_child` step for step:
/// parent lookup → `ENODEV`; id alloc; object build (`UNBOUND`,
/// owner = source, refcount 1 — device.c:366/271-272); framework dir
/// (`add_inode` with the wire name); attribute files (`STATIC` only —
/// `DYNAMIC`/`DEVICE` entries are skipped silently, exactly like C's
/// `-1`-return-ignored loop, device.c:410-418 + A-6); `devman_id` file;
/// link into the parent; ADD event via `on_event`.
///
/// `on_event` injects the sink (production wires the events file queue;
/// tests collect): the handler itself owns no queue.
/// `budget` threads C's `len` parameter into path generation (04):
/// bare callers pass `DEVMAN_STRING_LEN`, event lines pass `- 11` —
/// here always the event line, so `DEVMAN_STRING_LEN - 11`.
/// Undo a failed ADD that already passed `alloc_id`: release cookies
/// registered by the successfully-staged attributes, delete the staged
/// framework subtree (recursive — children first, inode.rs `delete` —
/// and every staged node still has refcount 0, so each slot reaps at
/// once), and hand the id back to the allocator (DM-P1-1). The publish
/// point of an ADD is `insert` below; everything before it must leave
/// zero residue, or one failed registration — a driver retrying a
/// duplicate, the everyday case — would strand a hole in the dense id
/// space and wedge every later ADD (`insert`'s `id == len` check).
/// C has no counterpart to unwind: its `add_inode` results go
/// unchecked (device.c:373-375), so a failure there corrupts instead.
fn unwind_staged(
    fw: &mut InodeTree,
    tree: &mut DeviceTree,
    id: DeviceId,
    dir_ino: Option<crate::vtreefs::Ino>,
    staged: Option<&Device>,
    e: minix_types::Errno,
) -> minix_types::Errno {
    if let Some(d) = staged {
        for cookie in d
            .attrs
            .iter()
            .filter_map(|a| a.binding.as_ref().and_then(|b| b.cookie))
        {
            // Best effort: the table entry is a tombstone-to-be either way.
            let _ = crate::files::with_files(|s| s.unregister(cookie));
        }
    }
    if let Some(dir) = dir_ino {
        let _ = fw.delete(dir);
    }
    tree.rollback_id(id);
    e
}

pub fn do_add(
    tree: &mut DeviceTree,
    fw: &mut InodeTree,
    parent: DeviceId,
    parsed: &ParsedDevice,
    source: Endpoint,
    on_event: &mut dyn FnMut(Event),
) -> Result<DeviceId, minix_types::Errno> {
    use minix_types::Errno;
    // C: _find_dev(parent_dev_id) NULL → ENODEV (device.c:246-251).
    let parent_dev = tree.get(parent).ok_or(Errno::ENODEV)?;
    let parent_ino = parent_dev.binding.ok_or(Errno::ENODEV)?.ino;
    // OQ-3 (closed): devmand parses event paths with %s (13 §2.1) — a
    // name containing ASCII whitespace would silently mis-split the
    // event line. C has no check (new behavior); reject with EINVAL
    // (silent mismatch is worse than failure).
    if parsed.name.bytes().any(|b| b.is_ascii_whitespace()) {
        return Err(Errno::EINVAL);
    }
    // Everything between this alloc and `insert` is fallible prep; any
    // failure takes the full unwind below (publish point = insert).
    let id = tree.alloc_id()?;
    let mut dev = Device {
        id,
        name: Some(parsed.name.clone()),
        // C: ref_count = 1 (device.c:366) — the object's own birth ref.
        // (Parent membership refs are separate: get(parent) after insert
        // below. Tree ownership expresses membership; refcount expresses
        // deletion eligibility — 07 §3.2.)
        refcount: 1,
        // C: state = UNBOUND (device.c:271).
        state: DeviceState::Unbound,
        // C: owner = m_source (device.c:273).
        owner: Some(source),
        binding: None,
        parent: Some(parent),
        info: None,
        children: alloc::vec::Vec::new(),
        attrs: alloc::vec::Vec::new(),
    };
    // C: add_inode(parent dir, wire name, dir stat) (device.c:373-375).
    let dir_ino = match fw.add(
        parent_ino,
        &parsed.name,
        crate::device_tree::default_dir_stat(),
        0,
    ) {
        Ok(ino) => ino,
        // Directory never materialized: only the id needs returning.
        Err(_) => return Err(unwind_staged(fw, tree, id, None, None, Errno::ENOMEM)),
    };
    dev.binding = Some(crate::structs::FileBinding {
        ino: dir_ino,
        cookie: None,
    });
    // C: per-entry add_info loop (device.c:385-389). STATIC materializes;
    // DYNAMIC/DEVICE fall to -1, ignored (device.c:410-418).
    for e in &parsed.entries {
        if e.ty != EntryType::Static {
            continue;
        }
        if let Err(err) = add_static(fw, &mut dev, &e.name, &e.data) {
            return Err(unwind_staged(fw, tree, id, Some(dir_ino), Some(&dev), err));
        }
    }
    // C: snprintf(id) + add_static_info(dev, "devman_id") (device.c:392-393).
    let mut id_text = String::new();
    {
        use core::fmt::Write as _;
        let _ = core::write!(id_text, "{}", id.0);
    }
    if let Err(err) = add_static(fw, &mut dev, "devman_id", &id_text) {
        return Err(unwind_staged(fw, tree, id, Some(dir_ino), Some(&dev), err));
    }
    if let Err(err) = tree.insert(parent, dev) {
        // Unreachable by construction: the parent was verified above and
        // the id is dense (rollback_id's no-interleave argument). `dev` —
        // and with it the staged cookies — has already moved into
        // `insert`, so the cookie half of the unwind cannot run here;
        // the framework half still must.
        return Err(unwind_staged(fw, tree, id, Some(dir_ino), None, err));
    }
    // C: INSERT_HEAD then get(parent) (device.c:395-397) — member ref,
    // balancing del_device's put(parent). Kept: DEL survival of parents
    // with live children depends on the count (08 §2.4).
    crate::del_device::get_device(tree, parent);
    // C: devman_device_add_event(dev) (device.c:276) — "ADD " + path
    // (117 budget) + " 0x%08x" (device.c:75-102).
    let path = tree.generate_path(fw, id, DEVMAN_STRING_LEN - 11)?;
    let mut line = String::from("ADD ");
    line.push_str(&path);
    {
        use core::fmt::Write as _;
        let _ = core::write!(line, " 0x{:08x}", id.0);
    }
    // Over-long lines are ENAMETOOLONG, never truncated (03 §3.5; C
    // would overflow event->data — hardening, 07 §3.4).
    on_event(Event::new(&line)?);
    Ok(id)
}

/// C: `devman_dev_add_static_info` (device.c:314-339) — framework file
/// (`default_file_stat`) + `FileStore` text entry, linked into the
/// device's `attrs`. C truncates over-long text at 127+NUL
/// (`strncpy` + forced NUL, :322-324); Rust pre-truncates explicitly
/// (same bytes, honest call — 07 §3.4) because `Event`-style rejection
/// would break C-identical output for long attributes.
fn add_static(
    fw: &mut InodeTree,
    dev: &mut Device,
    name: &str,
    data: &str,
) -> Result<(), minix_types::Errno> {
    use minix_types::Errno;
    // C truncation, explicit: keep 127 + NUL (device.c:322-324).
    let mut text = String::from(data);
    if text.len() >= DEVMAN_STRING_LEN {
        text.truncate(DEVMAN_STRING_LEN - 1);
    }
    let cookie = register_file(FileEntry {
        kind: FileKind::Static(StaticFile { text }),
    })?;
    let dir_ino = dev.binding.ok_or(Errno::ENODEV)?.ino;
    let ino = match fw.add(
        dir_ino,
        name,
        crate::device_tree::default_file_stat(),
        cookie,
    ) {
        Ok(ino) => ino,
        // Release the just-registered cookie before unwinding: the
        // device's attr list does not know it yet, so the DM-P1-1
        // unwind at the caller cannot see it.
        Err(_) => {
            let _ = crate::files::with_files(|s| s.unregister(cookie));
            return Err(Errno::ENOMEM);
        }
    };
    let mut attr = Attribute {
        name: String::from(name),
        data: String::from(data),
        binding: None,
    };
    // Attribute text keeps the full data (C's st_inode.data is the
    // truncated copy; the file shows truncated — both retained, each
    // faithful to its reader: file readers see C bytes, attr readers
    // see full data. 07 §3.4 records the split.)
    attr.binding = Some(crate::structs::FileBinding {
        ino,
        cookie: Some(cookie),
    });
    dev.attrs.push(attr);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_tree::default_file_stat;
    use crate::structs::DeviceState;
    use crate::wire::parse_device;
    use alloc::vec::Vec;

    fn harness() -> (DeviceTree, InodeTree) {
        let mut fw = InodeTree::new(64, crate::device_tree::default_dir_stat()).unwrap();
        let tree = DeviceTree::new(&mut fw, default_file_stat()).unwrap();
        (tree, fw)
    }

    /// Hand-built wire buffer (mirrors 03's encoder layout).
    fn wire_usb() -> Vec<u8> {
        let mut buf = alloc::vec![0u8; 16 + 16];
        buf[0..4].copy_from_slice(&1i32.to_le_bytes()); // count
        buf[4..8].copy_from_slice(&0i32.to_le_bytes()); // parent = root
        let mut s = Vec::new();
        let mut push = |t: &str| -> u32 {
            let o = (buf.len() + s.len()) as u32;
            s.extend_from_slice(t.as_bytes());
            s.push(0);
            o
        };
        let no = push("usb");
        let an = push("dev_type");
        let ad = push("USB_DEV");
        buf[8..12].copy_from_slice(&no.to_le_bytes());
        buf[16..20].copy_from_slice(&0u32.to_le_bytes()); // STATIC
        buf[20..24].copy_from_slice(&an.to_le_bytes());
        buf[24..28].copy_from_slice(&ad.to_le_bytes());
        buf.extend_from_slice(&s);
        buf
    }

    #[test]
    fn add_full_flow() {
        // do_add_device + add_child observable contract (device.c:223-397).
        let (mut tree, mut fw) = harness();
        let (_, parsed) = parse_device(&wire_usb()).unwrap();
        let mut events = Vec::new();
        let id = do_add(
            &mut tree,
            &mut fw,
            DeviceId::ROOT,
            &parsed,
            Endpoint(9),
            &mut |ev| events.push(ev),
        )
        .unwrap();
        assert_eq!(id, DeviceId(1));
        let dev = tree.get(id).unwrap();
        assert_eq!(dev.name.as_deref(), Some("usb"));
        assert_eq!(dev.state, DeviceState::Unbound);
        assert_eq!(dev.owner, Some(Endpoint(9)));
        assert_eq!(dev.refcount, 1);
        // Attribute + devman_id files materialized (STATIC only).
        assert_eq!(dev.attrs.len(), 2);
        assert!(dev.attrs.iter().any(|a| a.name == "dev_type"));
        assert!(dev.attrs.iter().any(|a| a.name == "devman_id"));
        // Framework mirrors: dir + 2 files under it.
        let dir = tree.get(id).unwrap().binding.unwrap().ino;
        assert!(fw.lookup(dir, "dev_type").is_ok());
        assert!(fw.lookup(dir, "devman_id").is_ok());
        // ADD event line, exact format.
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].text(), "ADD ./devices/usb/ 0x00000001");
    }

    #[test]
    fn add_bad_parent_is_enodev() {        // C: _find_dev NULL → ENODEV (device.c:246-251).
        let (mut tree, mut fw) = harness();
        let (_, parsed) = parse_device(&wire_usb()).unwrap();
        let mut events = Vec::new();
        assert_eq!(
            do_add(
                &mut tree,
                &mut fw,
                DeviceId(99),
                &parsed,
                Endpoint(9),
                &mut |ev| events.push(ev),
            ),
            Err(minix_types::Errno::ENODEV)
        );
        assert!(events.is_empty());
    }

    #[test]
    fn add_skips_dynamic_silently() {
        // C: add_info DYNAMIC → -1, ignored (device.c:410-418).
        let (mut tree, mut fw) = harness();
        let mut buf = wire_usb();
        buf[16..20].copy_from_slice(&1u32.to_le_bytes()); // DYNAMIC
        let (_, parsed) = parse_device(&buf).unwrap();
        let mut events = Vec::new();
        let id = do_add(
            &mut tree,
            &mut fw,
            DeviceId::ROOT,
            &parsed,
            Endpoint(9),
            &mut |ev| events.push(ev),
        )
        .unwrap();
        // Only devman_id materialized; no error.
        let dev = tree.get(id).unwrap();
        assert_eq!(dev.attrs.len(), 1);
        assert_eq!(dev.attrs[0].name, "devman_id");
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn add_whitespace_name_is_einval() {
        // OQ-3 (closed): devmand splits event paths with %s (13 §2.1) —
        // a spaced name would silently mis-split. Rejected, no residue.
        let (mut tree, mut fw) = harness();
        let mut buf = wire_usb();
        // Rewrite the name string in place ("usb" → "u b").
        let name_off = u32::from_le_bytes(buf[8..12].try_into().unwrap()) as usize;
        buf[name_off + 1] = b' ';
        let (_, parsed) = parse_device(&buf).unwrap();
        assert_eq!(parsed.name, "u b");
        let mut events = Vec::new();
        assert_eq!(
            do_add(
                &mut tree,
                &mut fw,
                DeviceId::ROOT,
                &parsed,
                Endpoint(9),
                &mut |ev| events.push(ev),
            ),
            Err(minix_types::Errno::EINVAL)
        );
        assert!(events.is_empty());
        assert_eq!(tree.device_count(), 1); // root only: nothing linked
    }

    /// Wire for two entries sharing one name: the second `fw.add` hits
    /// EEXIST inside `add_static` — the everyday failing ADD (driver
    /// retry / misbehaving client), used here to reach the unwind path.
    fn wire_dup_attr() -> Vec<u8> {
        let mut buf = alloc::vec![0u8; 16 + 2 * 16];
        buf[0..4].copy_from_slice(&2i32.to_le_bytes());
        buf[4..8].copy_from_slice(&0i32.to_le_bytes()); // parent = root
        let mut s = Vec::new();
        let mut push = |t: &str| -> u32 {
            let o = (buf.len() + s.len()) as u32;
            s.extend_from_slice(t.as_bytes());
            s.push(0);
            o
        };
        let no = push("usb");
        let a1n = push("dev_type");
        let a1d = push("USB_DEV");
        let a2n = push("dev_type");
        let a2d = push("HUB");
        buf[8..12].copy_from_slice(&no.to_le_bytes());
        for (i, (n, d)) in [(a1n, a1d), (a2n, a2d)].iter().enumerate() {
            let base = 16 + i * 16;
            buf[base..base + 4].copy_from_slice(&0u32.to_le_bytes()); // STATIC
            buf[base + 4..base + 8].copy_from_slice(&n.to_le_bytes());
            buf[base + 8..base + 12].copy_from_slice(&d.to_le_bytes());
        }
        buf.extend_from_slice(&s);
        buf
    }

    #[test]
    fn add_failure_unwinds_and_retry_succeeds() {
        // DM-P1-1: a failing ADD leaves zero residue — staged dir deleted,
        // cookies released, id returned, parent refcount untouched — so the
        // driver's retry succeeds with the same id the failed attempt had.
        // C has no counterpart: add_inode results go unchecked
        // (device.c:373-375) and no failure path exists to unwind.
        let (mut tree, mut fw) = harness();
        let (_, bad) = parse_device(&wire_dup_attr()).unwrap();
        let mut events = Vec::new();
        assert_eq!(
            do_add(
                &mut tree,
                &mut fw,
                DeviceId::ROOT,
                &bad,
                Endpoint(9),
                &mut |ev| events.push(ev),
            ),
            Err(minix_types::Errno::ENOMEM)
        );
        assert!(events.is_empty());
        // No orphan subtree under devices/, no member ref taken.
        let devices_ino = tree.get(DeviceId::ROOT).unwrap().binding.unwrap().ino;
        assert_eq!(fw.lookup(devices_ino, "usb"), Err(minix_types::Errno::ENOENT));
        assert_eq!(tree.get(DeviceId::ROOT).unwrap().refcount, 0);
        // The retry — the whole point of the unwind — succeeds with id 1.
        let (_, retry) = parse_device(&wire_usb()).unwrap();
        let id = do_add(
            &mut tree,
            &mut fw,
            DeviceId::ROOT,
            &retry,
            Endpoint(9),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(id, DeviceId(1));
        // Root membership is deliberately not counted (get_device's root
        // immunity, del_device.rs — C device.c:462-463 `dev == &root_dev`).
        assert_eq!(tree.get(DeviceId::ROOT).unwrap().refcount, 0);
    }
}
