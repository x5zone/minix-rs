//! Driver lifecycle: allocation, connect, disconnect, and arrival checks.
//!
//! C: `input_alloc_id` (`minix3/minix/servers/input/input.c:430-470`),
//! `input_connect` (`input.c:475-528`), `input_disconnect`
//! (`input.c:533-553`), `input_check` (`input.c:558-603`).
//!
//! Drivers come and go; slots are reused. The lifecycle has four verbs:
//! a driver *arrives* (the data store says so — [`key_is_new_driver`]
//! filters the key), the server *connects* it (label verified elsewhere,
//! then [`connect_driver`] assigns slots and reports what to send back),
//! the driver *leaves* ([`disconnect_device`] wakes its waiters with an
//! error and frees the slots), and the periodic *check* reconciles the two
//! directions (new keys in, dead labels out).
//!
//! The data-store round-trips (subscribe, check, retrieve, publish) are
//! transport: this module starts from "a key arrived" / "a label died" and
//! decides slot fate. Label comparison uses the stored NUL-terminated
//! bytes, mirroring C's `strcmp` on the fixed arrays.
//!
//! Corresponding document: `11-input-driver-connect.md`.

use crate::setleds::remembered_lights;
use crate::structs::{
    DeviceIndex, FIRST_KEYBOARD_INDEX, FIRST_MOUSE_INDEX, InputDevice, InputTable,
    LAST_KEYBOARD_INDEX, LAST_MOUSE_INDEX, Minor,
};
use minix_types::{DRIVER_KEY_PREFIX, Endpoint, INPUT_DEV_KBD, INPUT_DEV_MOUSE, INVALID_INPUT_ID};

// ── Arrival filter ──

/// Whether a data-store key announces an input driver, and its label.
///
/// C: `input_check` (`input.c:577-582`) skips keys without the prefix and
/// takes the label as the remainder. Spelled through the shared
/// [`DRIVER_KEY_PREFIX`] (same dots the client publishes, document 12), so
/// the two sides cannot disagree on the prefix.
pub fn key_is_new_driver(key: &str) -> Option<&str> {
    key.strip_prefix(DRIVER_KEY_PREFIX)
}

// ── Slot allocation ──

/// Claims a slot for a driver of one kind (keyboards or mice).
///
/// C: `input_alloc_id` (`input.c:430-470`). Scans the kind's window
/// low-to-high: a slot owned under the *same* label is reused (its owner
/// refreshed — the same driver announcing again, e.g. after its own
/// restart); otherwise the first slot that is both ownerless *and* unopened
/// is claimed, labelled, and returned. A slot that lost its driver but is
/// still opened is *skipped* — handing a live reader's queue to a new
/// driver would mix two drivers' events (`input.c:450-451`, "Do not
/// allocate the ID of a disconnected but open device"). Exhaustion answers
/// `None` (C returns `INVALID_INPUT_ID` and logs "out of slots").
///
/// `mouse` selects the window (mice when true). `label` is the driver's
/// registry name as bytes (compared against the stored NUL-terminated
/// names, C `strcmp` semantics).
pub fn alloc_id(
    table: &mut InputTable,
    mouse: bool,
    owner: Endpoint,
    label: &[u8],
) -> Option<DeviceIndex> {
    let (start, end) = if mouse {
        (FIRST_MOUSE_INDEX, LAST_MOUSE_INDEX)
    } else {
        (FIRST_KEYBOARD_INDEX, LAST_KEYBOARD_INDEX)
    };
    let mut free: Option<usize> = None;
    let mut slot = start;
    while slot <= end {
        let device = &table.devices[slot];
        if device.owner != Endpoint::NONE {
            if label_eq(device, label) {
                table.devices[slot].owner = owner;
                return Some(DeviceIndex(slot));
            }
        } else if !device.opened && free.is_none() {
            free = Some(slot);
        }
        slot += 1;
    }
    match free {
        Some(slot) => {
            let device = &mut table.devices[slot];
            device.owner = owner;
            device.set_label(label);
            Some(DeviceIndex(slot))
        }
        None => None,
    }
}

/// Whether a slot's stored label equals a registry name.
///
/// C `strcmp` on the fixed `label` array: comparison stops at the first NUL
/// on either side, so overlong names truncated at store time still match
/// their full form only when the stored bytes equal the name's prefix…
/// precisely: equality holds when the name bytes equal the stored bytes up
/// to the first NUL in either. Implemented through `label_bytes` (up to the
/// first NUL), which is exactly `strcmp` for NUL-terminated inputs.
fn label_eq(device: &InputDevice, label: &[u8]) -> bool {
    let stored = device.label_bytes();
    let end = label
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(label.len());
    stored == &label[..end]
}

// ── Connect ──

/// What a connect must send back and restore.
///
/// C: `input_connect` (`input.c:475-528`) after the label check (the check
/// itself — retrieve the owner's label, compare — is transport and lives
/// with the dispatcher): allocate a keyboard slot and/or a mouse slot per
/// the type mask (each possibly unassigned — allocation failures still get
/// a configuration reply carrying the invalid id, `input.c:500-508`), send
/// the configuration, and when a keyboard slot was assigned, relight it
/// from memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectReport {
    /// Assigned keyboard slot, if any.
    pub keyboard_slot: Option<DeviceIndex>,
    /// Assigned mouse slot, if any.
    pub mouse_slot: Option<DeviceIndex>,
    /// Light mask to restore on the keyboard slot, if one was assigned.
    pub restore_lights: Option<(Minor, u32)>,
}

/// Connects a label-verified driver: assigns slots, reports the reply.
///
/// `wants_keyboard` / `wants_mouse` decode the driver's type mask
/// (`INPUT_DEV_KBD` / `INPUT_DEV_MOUSE` bits of the data-store value).
/// The configuration reply goes out even when both allocations fail —
/// carrying invalid ids, which tells the driver it is (partly) disabled
/// (C sends unconditionally, `input.c:514-523`). Light restore reads the
/// slot's memory (`input.c:525-527`).
pub fn connect_driver(
    table: &mut InputTable,
    wants_keyboard: bool,
    wants_mouse: bool,
    owner: Endpoint,
    label: &[u8],
) -> ConnectReport {
    let keyboard_slot = if wants_keyboard {
        alloc_id(table, false, owner, label)
    } else {
        None
    };
    let mouse_slot = if wants_mouse {
        alloc_id(table, true, owner, label)
    } else {
        None
    };
    let restore_lights = keyboard_slot.map(|index| {
        let device = &table.devices[index.0];
        (device.minor, remembered_lights(device))
    });
    ConnectReport {
        keyboard_slot,
        mouse_slot,
        restore_lights,
    }
}

/// Decodes a data-store type mask into (wants keyboard, wants mouse).
///
/// C: `typemask & INPUT_DEV_KBD / INPUT_DEV_MOUSE` (`input.c:509-512`).
pub const fn wants_from_typemask(typemask: u16) -> (bool, bool) {
    (
        typemask & INPUT_DEV_KBD != 0,
        typemask & INPUT_DEV_MOUSE != 0,
    )
}

// ── Disconnect ──

/// What a disconnect must transmit.
///
/// C: `input_disconnect` (`input.c:533-553`): a parked reader is answered
/// with an input/output error (its driver is gone; waiting longer is
/// pointless), a recorded selector is notified readable (so it re-queries
/// and learns the device is driverless — the read query on an inactive
/// device reports ready-as-error, document 08), then the slot is freed.
/// Deliberately *not* cleared: the queue (a reconnecting driver continues
/// where it left off only if nobody reads stale events — document 09
/// owns that discussion), the opened flag, the label, the light memory.
/// Contrast [`crate::handlers::apply_close`], which *does* clear the queue:
/// close ends a reader relationship, disconnect interrupts a driver
/// relationship — different ends, different cleanups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisconnectEffects {
    /// Answer the parked reader with an error: (caller, request id).
    pub answer_reader: Option<(Endpoint, u32)>,
    /// Notify the recorded selector as readable: (selector, minor).
    pub notify_selector: Option<(Endpoint, Minor)>,
}

/// Disconnects a slot: wakes its waiters with the going-away news, frees it.
pub fn disconnect_device(device: &mut InputDevice) -> DisconnectEffects {
    let answer_reader = if device.suspended {
        device.suspended = false;
        Some((device.caller, device.request_id))
    } else {
        None
    };
    let notify_selector = if device.has_selector() {
        let selector = device.selector;
        let minor = device.minor;
        device.selector = Endpoint::NONE;
        Some((selector, minor))
    } else {
        None
    };
    device.owner = Endpoint::NONE;
    DisconnectEffects {
        answer_reader,
        notify_selector,
    }
}

/// The invalid slot marker for configuration replies.
///
/// Re-exported at the use site for readability: allocation failure travels
/// as this value (C `INVALID_INPUT_ID`, `input.c:443`), never as a bare -1.
pub const NO_SLOT: i32 = INVALID_INPUT_ID;

#[cfg(test)]
mod tests {
    use super::*;

    fn labelled(table: &mut InputTable, slot: usize, owner: Endpoint, label: &[u8]) {
        table.devices[slot].owner = owner;
        table.devices[slot].set_label(label);
    }

    #[test]
    fn test_arrival_filter_matches_c() {
        // C: input.c:577-582 — prefix gate, label is the remainder.
        assert_eq!(key_is_new_driver("drv.inp.kbd0"), Some("kbd0"));
        assert_eq!(key_is_new_driver("drv.chr.kbd0"), None);
        assert_eq!(key_is_new_driver("other"), None);
    }

    #[test]
    fn test_alloc_claims_lowest_free_unopened() {
        // C: input.c:444-454 — low to high, first ownerless unopened slot.
        let mut table = InputTable::fresh();
        assert_eq!(
            alloc_id(&mut table, false, Endpoint(3), b"kbd0"),
            Some(DeviceIndex(1))
        );
        assert_eq!(table.devices[1].owner, Endpoint(3));
        assert_eq!(table.devices[1].label_bytes(), b"kbd0");
        assert_eq!(
            alloc_id(&mut table, false, Endpoint(4), b"kbd1"),
            Some(DeviceIndex(2))
        );
        assert_eq!(
            alloc_id(&mut table, true, Endpoint(4), b"mouse0"),
            Some(DeviceIndex(6))
        );
    }

    #[test]
    fn test_alloc_reuses_same_label_with_new_owner() {
        // C: input.c:445-449 — same label reuses, owner refreshed.
        let mut table = InputTable::fresh();
        labelled(&mut table, 1, Endpoint(3), b"kbd0");
        assert_eq!(
            alloc_id(&mut table, false, Endpoint(7), b"kbd0"),
            Some(DeviceIndex(1))
        );
        assert_eq!(table.devices[1].owner, Endpoint(7));
    }

    #[test]
    fn test_alloc_skips_disconnected_open_slot() {
        // C: input.c:450-452 — ownerless but opened slots are skipped.
        let mut table = InputTable::fresh();
        table.devices[1].opened = true; // disconnected (owner NONE), still open
        table.devices[2].opened = true;
        table.devices[3].opened = true;
        table.devices[4].opened = true;
        assert_eq!(alloc_id(&mut table, false, Endpoint(3), b"kbd0"), None);
        // ... but the same label owned elsewhere still reuses.
        labelled(&mut table, 2, Endpoint(5), b"kbd0");
        table.devices[2].opened = false;
        assert_eq!(
            alloc_id(&mut table, false, Endpoint(3), b"kbd0"),
            Some(DeviceIndex(2))
        );
    }

    #[test]
    fn test_connect_reports_slots_and_light_restore() {
        // C: input.c:509-527 — mask-driven allocation, unconditional reply
        // content, light restore on keyboard assignment.
        let mut table = InputTable::fresh();
        table.devices[1].leds = 0x6;
        let report = connect_driver(&mut table, true, true, Endpoint(3), b"kbd0");
        assert_eq!(report.keyboard_slot, Some(DeviceIndex(1)));
        assert_eq!(report.mouse_slot, Some(DeviceIndex(6)));
        assert_eq!(report.restore_lights, Some((Minor(1), 0x6)));
        // Exhausted keyboards still report (with no slot): the reply goes
        // out carrying the invalid id (C: input.c:514-523 unconditional).
        for slot in 1..=4 {
            labelled(&mut table, slot, Endpoint(9), b"other");
        }
        let report = connect_driver(&mut table, true, false, Endpoint(3), b"kbd0");
        assert_eq!(report.keyboard_slot, None);
        assert_eq!(report.restore_lights, None);
        assert_eq!(NO_SLOT, INVALID_INPUT_ID);
    }

    #[test]
    fn test_typemask_decoding_matches_c() {
        // C: input.c:509-512.
        assert_eq!(wants_from_typemask(0x01), (true, false));
        assert_eq!(wants_from_typemask(0x02), (false, true));
        assert_eq!(wants_from_typemask(0x03), (true, true));
        assert_eq!(wants_from_typemask(0x00), (false, false));
    }

    #[test]
    fn test_disconnect_wakes_and_frees() {
        // C: input.c:540-552 — reader answered EIO, selector notified,
        // owner cleared; queue/opened/label/lights untouched.
        let mut table = InputTable::fresh();
        labelled(&mut table, 2, Endpoint(3), b"kbd0");
        let device = &mut table.devices[2];
        device.suspended = true;
        device.caller = Endpoint(7);
        device.request_id = 13;
        device.selector = Endpoint(9);
        device.opened = true;
        device.count = 2;
        device.leds = 0x6;
        let effects = disconnect_device(device);
        assert_eq!(effects.answer_reader, Some((Endpoint(7), 13)));
        assert_eq!(effects.notify_selector, Some((Endpoint(9), Minor(2))));
        let device = &table.devices[2];
        assert!(!device.suspended);
        assert!(!device.has_selector());
        assert_eq!(device.owner, Endpoint::NONE);
        // Untouched by design (see DisconnectEffects docs).
        assert!(device.opened);
        assert_eq!(device.count, 2);
        assert_eq!(device.label_bytes(), b"kbd0");
        assert_eq!(device.leds, 0x6);
    }
}
