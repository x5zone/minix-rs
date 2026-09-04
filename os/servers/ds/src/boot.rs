//! DS boot mapping: the fresh anchor that shadows the boot table.
//!
//! Mirrors `map_service`/`sef_cb_init_fresh` (`minix3/minix/servers/ds/
//! store.c:229-282`). 06-ds-boot-mapping.md.
//!
//! The module owns the shadowing motion and nothing else: resetting both
//! tables, shadowing one boot entry per seat, and batching the boot list.
//! Transport (safecopy of the RS table, 02/12), the notify ring (10, D5),
//! the abort verdict (the sef owner in 01, D3), and naming judgement (05)
//! stay out.
//!
//! Single-threaded event loop: pure functions over caller-held tables, no
//! shared state.

use minix_types::{DS_MAX_KEYLEN, DsFlags, ENOMEM, Endpoint};

use crate::slots::{EntrySlot, alloc_entry_slot};
use crate::store::{DataBody, DsStore};
use crate::subscription::DsSubs;

/// RS label bound (`rs.h:58`: `RS_MAX_LABEL_LEN 16`).
///
/// The bound rides the type: a wider RS label in future is a compile-time
/// event here, not a silent lane overrun (D1).
pub const RS_LABEL_LEN: usize = 16;

/// The boot owner's lane (`store.c:242`: `strcpy(dsp->owner, "rs")`).
///
/// The master follows the table: the grant source (RS) is the registrar,
/// so its name is the constant — one place to change if the registrar
/// is ever renamed (D6).
pub const RS_OWNER_LANE: &[u8; DS_MAX_KEYLEN] = &{
    let mut lane = [0u8; DS_MAX_KEYLEN];
    lane[0] = b'r';
    lane[1] = b's';
    lane
};

/// One boot-table shadow (`struct rprocpub`, `rs.h:165-183`, subset).
///
/// Only the three lanes the shadow needs: occupancy, the endpoint number,
/// and the service name. The rest of `rprocpub` (masks, PCI, domains) is
/// RS scheduling material — another owner's domain (D1).
#[derive(Debug, Clone, Copy)]
pub struct BootService {
    /// Boot-table occupancy. C: `rprocpub.in_use` — rs.h:166.
    pub in_use: bool,
    /// Service endpoint. C: `rprocpub.endpoint` — rs.h:168.
    pub endpoint: Endpoint,
    /// Service label, NUL-stopped. C: `rprocpub.label[16]` — rs.h:177.
    pub label: [u8; RS_LABEL_LEN],
}

/// Empty both tables (`sef_cb_init_fresh` reset, `store.c:261-266`).
///
/// The anchor's first step: every seat vacant before any shadow lands.
/// C clears flags only; full clearing is the superset — outward-equal on
/// a fresh boot (memory starts zeroed) and simpler ever after (D2).
pub fn reset_tables(store: &mut DsStore, subs: &mut DsSubs) {
    store.fill(None);
    subs.fill(None);
}

/// Copy a label lane with three stops (`strcpy` made explicit, D4).
///
/// Copies until the first NUL, the label bound (16), or the lane bound
/// (79, keeping the terminator) — whichever stops first. C trusts the
/// RS-side bound implicitly; the explicit triple bound stays safe even
/// if that trust ever breaks.
fn copy_label(lane: &mut [u8; DS_MAX_KEYLEN], label: &[u8; RS_LABEL_LEN]) {
    // RS_LABEL_LEN (16) < DS_MAX_KEYLEN - 1 (79): the label bound always
    // stops first, so the lane bound lives in the type, not the loop.
    // (clippy::redundant_comparisons: the second check was dead.)
    let mut n = 0;
    while n < RS_LABEL_LEN && label[n] != 0 {
        lane[n] = label[n];
        n += 1;
    }
    lane[n] = 0;
}

/// Shadow one boot service (`map_service`, `store.c:229-249`).
///
/// Take a seat (full house reads `Err(ENOMEM)`, `store.c:235-237`),
/// then fill the three lanes: the label key (`store.c:240`), the
/// endpoint number (`store.c:241` — labels ride the number lane, 03 D2),
/// and the RS owner (`store.c:242`), flagged in-use label (`store.c:243`).
///
/// The notify ring (`update_subscribers(dsp, 1)`, `store.c:246`) is NOT
/// rung here: the ring lives in 10 and is not yet built — the hook stands
/// documented (§4.3) instead of wired hollow (D5).
pub fn map_service(
    store: &mut DsStore,
    label: &[u8; RS_LABEL_LEN],
    endpoint: Endpoint,
) -> Result<EntrySlot, i32> {
    let slot = alloc_entry_slot(store).ok_or(ENOMEM)?;
    let seat = &mut store[slot.index()];
    let mut entry = crate::store::DataEntry {
        flags: DsFlags::IN_USE | DsFlags::TYPE_LABEL,
        key: [0u8; DS_MAX_KEYLEN],
        owner: *RS_OWNER_LANE,
        // C writes `(u32_t) endpoint`: the `as u32` wrap is the same
        // conversion, not an extension.
        body: DataBody {
            u32: endpoint.0 as u32,
        },
    };
    copy_label(&mut entry.key, label);
    *seat = Some(entry);
    Ok(slot)
}

/// Run the fresh anchor over a boot list (`sef_cb_init_fresh` loop,
///
/// `store.c:273-279`, table motion only).
///
/// Reset, then shadow every occupied entry in order; idle entries are
/// skipped. The first failure aborts the batch (`Err`) — the C
/// equivalent panics (`store.c:275-277`); the abort verdict belongs to
/// the sef owner (01), so this motion returns instead of presuming it
/// (D3). Returns the shadowed count.
pub fn apply_boot_map(
    store: &mut DsStore,
    subs: &mut DsSubs,
    services: &[BootService],
) -> Result<usize, i32> {
    reset_tables(store, subs);
    let mut mapped = 0;
    for svc in services {
        if !svc.in_use {
            continue;
        }
        map_service(store, &svc.label, svc.endpoint)?;
        mapped += 1;
    }
    Ok(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slots::lookup_label_entry;
    use crate::store::NR_DS_KEYS;

    fn label16(name: &[u8]) -> [u8; RS_LABEL_LEN] {
        let mut label = [0u8; RS_LABEL_LEN];
        label[..name.len()].copy_from_slice(name);
        label
    }

    fn svc(name: &[u8], ep: i32) -> BootService {
        BootService {
            in_use: true,
            endpoint: Endpoint(ep),
            label: label16(name),
        }
    }

    #[test]
    fn test_reset_empties() {
        // The anchor's first step: both tables vacant (`store.c:261-266`).
        let mut store: DsStore = [None; NR_DS_KEYS];
        let mut subs: DsSubs = [None; crate::subscription::NR_DS_SUBS];
        store[0] = Some(crate::store::DataEntry {
            flags: DsFlags::IN_USE,
            key: [0u8; DS_MAX_KEYLEN],
            owner: [0u8; DS_MAX_KEYLEN],
            body: DataBody { u32: 0 },
        });
        reset_tables(&mut store, &mut subs);
        assert!(store.iter().all(|seat| seat.is_none()));
        assert!(subs.iter().all(|seat| seat.is_none()));
    }

    #[test]
    fn test_map_three_lanes() {
        // Key, number, master: the three lanes (`store.c:240-243`).
        let mut store: DsStore = [None; NR_DS_KEYS];
        let slot = map_service(&mut store, &label16(b"rs"), Endpoint(2)).unwrap();
        let entry = slot.get(&store).unwrap();
        assert_eq!(&entry.key[..2], b"rs");
        assert_eq!(entry.owner, *RS_OWNER_LANE);
        assert!(entry.flags.contains(DsFlags::IN_USE | DsFlags::TYPE_LABEL));
        assert_eq!(unsafe { entry.body.u32 }, 2);
    }

    #[test]
    fn test_map_full_reads_enomem() {
        // A full house at anchor time is an error, not a panic here
        // (C: ENOMEM at `store.c:235-237`; the abort verdict is the
        // owner's, D3).
        let mut store: DsStore = [None; NR_DS_KEYS];
        for seat in store.iter_mut() {
            *seat = Some(crate::store::DataEntry {
                flags: DsFlags::IN_USE,
                key: [0u8; DS_MAX_KEYLEN],
                owner: [0u8; DS_MAX_KEYLEN],
                body: DataBody { u32: 0 },
            });
        }
        assert_eq!(map_service(&mut store, &label16(b"x"), Endpoint(9)), Err(ENOMEM));
    }

    #[test]
    fn test_apply_skips_idle() {
        // Idle boot entries cast no shadow (`store.c:274`).
        let mut store: DsStore = [None; NR_DS_KEYS];
        let mut subs: DsSubs = [None; crate::subscription::NR_DS_SUBS];
        let idle = BootService {
            in_use: false,
            endpoint: Endpoint(4),
            label: label16(b"sched"),
        };
        let mapped = apply_boot_map(&mut store, &mut subs, &[idle, svc(b"rs", 2)]).unwrap();
        assert_eq!(mapped, 1);
        assert_eq!(lookup_label_entry(&store, 4), None);
        assert!(lookup_label_entry(&store, 2).is_some());
    }

    #[test]
    fn test_apply_counts_in_order() {
        // Shadows land in boot-list order with a count (`store.c:273-279`).
        let mut store: DsStore = [None; NR_DS_KEYS];
        let mut subs: DsSubs = [None; crate::subscription::NR_DS_SUBS];
        let list = [svc(b"rs", 2), svc(b"pm", 0), svc(b"vfs", 1)];
        assert_eq!(apply_boot_map(&mut store, &mut subs, &list), Ok(3));
        assert_eq!(lookup_label_entry(&store, 0).unwrap().index(), 1);
        assert_eq!(lookup_label_entry(&store, 2).unwrap().index(), 0);
    }

    #[test]
    fn test_label_stop_and_truncation() {
        // Copy stops at NUL; a boundless label stops at the lane bound
        // (D4: three stops).
        let mut store: DsStore = [None; NR_DS_KEYS];
        let full = [b'z'; RS_LABEL_LEN];
        let slot = map_service(&mut store, &full, Endpoint(3)).unwrap();
        let entry = slot.get(&store).unwrap();
        assert_eq!(&entry.key[..RS_LABEL_LEN], &full);
        assert_eq!(entry.key[DS_MAX_KEYLEN - 1], 0);
    }

    #[test]
    fn test_shadow_answers_identity() {
        // A shadowed label answers the identity question (05's reader):
        // endpoint 2 reads back the name "rs".
        let mut store: DsStore = [None; NR_DS_KEYS];
        let mut subs: DsSubs = [None; crate::subscription::NR_DS_SUBS];
        apply_boot_map(&mut store, &mut subs, &[svc(b"rs", 2)]).unwrap();
        assert_eq!(
            &crate::identity::resolve_name(&store, Endpoint(2)).unwrap()[..2],
            b"rs"
        );
    }
}
