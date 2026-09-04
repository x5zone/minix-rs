//! Light sending: broadcast or single-keyboard dispatch with memory.
//!
//! C: `input_set_leds` (`minix3/minix/servers/input/input.c:204-240`). When
//! the terminal asks for new indicator lights (or a control request carries
//! new light bits — document 08), the server walks the four keyboard slots:
//! every addressed slot gets the new mask saved, and every addressed slot
//! that currently has a driver gets the mask sent. A keyboard-multiplexer
//! address means "all four"; a single keyboard minor means "that one"; a
//! mouse minor means "none" (mice have no lights — the request evaporates
//! silently, by loop construction, not by an explicit check).
//!
//! Two halves again: [`plan_light_targets`] selects and [`apply_light_save`]
//! remembers; the transport sends to owned targets and logs send failures
//! (C logs, never fails the request — `input.c:230-234`).
//!
//! The "memory" half is the point of the module: the saved mask survives
//! driver restarts, and a freshly connected keyboard is lit from it
//! (document 11 reads it back through [`remembered_lights`]).
//!
//! Corresponding document: `10-input-setleds.md`.

use crate::structs::{DeviceIndex, InputDevice, InputTable, Minor};
use minix_types::Endpoint;

// ── Target selection ──

/// One addressed keyboard slot.
///
/// `owned` tells the transport whether to send: an unowned slot still gets
/// its mask saved (a driver may arrive later and must find the current
/// lights waiting), but there is nobody to send to today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightTarget {
    /// The addressed slot.
    pub index: DeviceIndex,
    /// Whether the slot currently has a driver (send only then).
    pub owned: bool,
}

/// Selects the keyboard slots a light request addresses, in slot order.
///
/// `minor` is the addressed minor number: the keyboard-multiplexer minor
/// addresses all four keyboard slots, a single keyboard minor addresses
/// its own slot, and anything else (notably mouse minors) addresses
/// nothing — the C loop (`input.c:221-225`) ranges over keyboard slots
/// only, so a mouse minor matches no iteration.
///
/// Returns the addressed slots with their ownership; the caller saves the
/// mask on all of them and sends to the owned ones.
pub fn plan_light_targets(table: &InputTable, minor: Minor) -> [Option<LightTarget>; 4] {
    let mut out: [Option<LightTarget>; 4] = [None, None, None, None];
    let mut slot = crate::structs::FIRST_KEYBOARD_INDEX;
    let mut pos = 0;
    while slot <= crate::structs::LAST_KEYBOARD_INDEX {
        let device = &table.devices[slot];
        if minor.is_keyboard_multiplexer() || device.minor == minor {
            out[pos] = Some(LightTarget {
                index: DeviceIndex(slot),
                owned: device.owner != Endpoint::NONE,
            });
        }
        pos += 1;
        slot += 1;
    }
    out
}

// ── Memory ──

/// Saves a light mask on a slot.
///
/// C: `dev->leds = mask` (`input.c:228`) — "Save the new state; the driver
/// might (re)start later." Unconditional: addressed means remembered,
/// whether or not a driver is there to hear it today.
pub fn apply_light_save(device: &mut InputDevice, mask: u32) {
    device.leds = mask;
}

/// Reads back the remembered lights of a slot.
///
/// Used at connect time (document 11): the fresh driver is lit from the
/// saved mask, so lights survive the crash in between.
pub const fn remembered_lights(device: &InputDevice) -> u32 {
    device.leds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structs::{KEYBOARD_MULTIPLEXER_MINOR, MOUSE_MULTIPLEXER_MINOR};

    fn owned_keyboards() -> InputTable {
        // Slots 1 and 3 owned, 2 and 4 driverless.
        let mut table = InputTable::fresh();
        table.devices[1].owner = Endpoint(3);
        table.devices[3].owner = Endpoint(5);
        table
    }

    fn hit_list(planned: &[Option<LightTarget>; 4]) -> [(usize, bool); 4] {
        let mut out = [(0usize, false); 4];
        for (i, entry) in planned.iter().enumerate() {
            if let Some(target) = entry {
                out[i] = (target.index.0, target.owned);
            }
        }
        out
    }

    #[test]
    fn test_multiplexer_addresses_all_four() {
        // C: input.c:224 — the multiplexer minor matches every iteration.
        let table = owned_keyboards();
        let planned = plan_light_targets(&table, Minor(KEYBOARD_MULTIPLEXER_MINOR));
        assert_eq!(
            hit_list(&planned),
            [(1, true), (2, false), (3, true), (4, false)]
        );
    }

    #[test]
    fn test_single_minor_addresses_itself() {
        // C: input.c:224 — one matching iteration.
        let table = owned_keyboards();
        let planned = plan_light_targets(&table, Minor(2));
        assert_eq!(planned[0], None);
        assert_eq!(
            planned[1],
            Some(LightTarget {
                index: DeviceIndex(2),
                owned: false
            })
        );
        assert_eq!(planned[2], None);
        assert_eq!(planned[3], None);
    }

    #[test]
    fn test_mouse_minor_addresses_nothing() {
        // C: the loop ranges over keyboard slots only — a mouse minor
        // matches no iteration and the request evaporates silently.
        let table = owned_keyboards();
        for minor in [MOUSE_MULTIPLEXER_MINOR, 65, 66, 67, 68, 99, -1] {
            assert_eq!(
                plan_light_targets(&table, Minor(minor)),
                [None, None, None, None]
            );
        }
    }

    #[test]
    fn test_save_and_recall_survives_restart() {
        // C: input.c:228 save; document 11 recalls at connect.
        let mut table = InputTable::fresh();
        apply_light_save(&mut table.devices[2], 0x6);
        assert_eq!(remembered_lights(&table.devices[2]), 0x6);
        // Other slots are unaffected.
        assert_eq!(remembered_lights(&table.devices[1]), 0);
    }
}
