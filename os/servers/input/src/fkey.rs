//! F-key observer registry — the TTY half of the IS debug-dump channel.
//!
//! C: `do_fkey_ctl` (`minix3/minix/drivers/tty/tty/arch/i386/keyboard.c:427-527`,
//! dispatched at `tty.c:206-208`) and `func_key`
//! (`keyboard.c:532-570`, gated by `debug_fkeys` at `keyboard.c:206`).
//! The IS half lives in `os/servers/is/src/tty_fkey.rs` (map/unmap macros
//! and the events pull); this module is the driver-side registry those
//! macros talk to.
//!
//! Two domain notes before the code:
//! - **Bit numbering**: bit 0 is unused; observer slots are indexed 0..12
//!   while wire bits run 1..=12 (`bit_isset(..., i+1)` in C).
//! - **The two key domains**: the registry itself is domain-free (a bank
//!   plus an index); the event-path translation lives in
//!   [`hit_of`]. The Shift bank has no code-domain cross yet — see
//!   [`hit_of`]'s notes.

use minix_types::{Endpoint, EPERM, EINVAL, FKEY_EVENTS, FKEY_MAP, FKEY_UNMAP, OK};

/// How many function keys per bank (F1..F12 and Shift F1..F12).
pub const FKEYS_PER_BANK: usize = 12;

/// One registered watcher: who to wake, and how many presses it missed.
///
/// C: `obs_t { int proc_nr; int events; }` — `NONE` means the slot is
/// vacant (`keyboard.c:72-73` declares the two 12-slot banks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FkeyObserver {
    /// Who to notify (vacant slot = `None`).
    pub endpoint: Option<Endpoint>,
    /// Presses since the last `FKEY_EVENTS` pull.
    pub events: u32,
}

impl FkeyObserver {
    /// The vacant slot. C: `proc_nr = NONE; events = 0;`.
    pub const VACANT: Self = Self {
        endpoint: None,
        events: 0,
    };
}

/// The two observer banks (plain and Shift).
///
/// C: `fkey_obs[12]` / `sfkey_obs[12]` (`keyboard.c:72-73`).
#[derive(Debug, Clone, Copy)]
pub struct FkeyTable {
    /// F1..F12 observers.
    pub fkeys: [FkeyObserver; FKEYS_PER_BANK],
    /// Shift F1..F12 observers.
    pub sfkeys: [FkeyObserver; FKEYS_PER_BANK],
    /// Whether presses are counted at all. C gates the whole call path on
    /// the boot flag `debug_fkeys` (`keyboard.c:206` —
    /// `if (debug_fkeys && func_key(scode)) continue;`); the Rust event
    /// loop carries the same gate here. The flag comes from the boot
    /// environment (`rc` starts IS only when it is set), which the input
    /// server does not parse yet — a fresh server starts with it off, so
    /// presses are not counted until the env wiring lands.
    pub enabled: bool,
}

impl FkeyTable {
    /// A cleared table — C `input_init`'s clearing loop
    /// (`keyboard.c:408-413` writes `proc_nr = NONE; events = 0` twice).
    pub const fn new(enabled: bool) -> Self {
        Self {
            fkeys: [FkeyObserver::VACANT; FKEYS_PER_BANK],
            sfkeys: [FkeyObserver::VACANT; FKEYS_PER_BANK],
            enabled,
        }
    }

    fn bank(&mut self, sf: bool) -> &mut [FkeyObserver; FKEYS_PER_BANK] {
        if sf {
            &mut self.sfkeys
        } else {
            &mut self.fkeys
        }
    }

    /// `TTY_FKEY_CONTROL`'s three verbs, bit semantics verbatim.
    ///
    /// C `do_fkey_ctl` (`keyboard.c:427-527`):
    /// - `FKEY_MAP` — every set bit claims the slot for `source` (the
    ///   slot-busy check is dead code in C), clears the claimed bit from
    ///   the request bitmap, and always answers `OK`;
    /// - `FKEY_UNMAP` — a set bit clears only the caller's own slot; a bit
    ///   owned by someone else answers `EPERM` but the remaining bits are
    ///   still processed, and cleared bits leave the request bitmap;
    /// - `FKEY_EVENTS` — the caller's pending presses are folded back into
    ///   the reply bitmaps (bit set and counter zeroed per slot), and the
    ///   result is `OK`.
    ///
    /// Returns `(result, fkeys, sfkeys)` — the reply bitmaps are the
    /// caller's own bitmaps with the accepted/cleared bits removed
    /// (MAP/UNMAP) or the pending-press bitmaps (EVENTS).
    pub fn control(
        &mut self,
        source: Endpoint,
        request: i32,
        mut fkeys: i32,
        mut sfkeys: i32,
    ) -> (i32, i32, i32) {
        if request == FKEY_EVENTS {
            // C: a pending count on slot i sets wire bit (i+1) in the reply
            // and zeroes the counter (keyboard.c:500-519).
            let mut out_fkeys = 0i32;
            let mut out_sfkeys = 0i32;
            for index in 0..FKEYS_PER_BANK {
                let wire_bit = 1i32 << (index + 1);
                if self.fkeys[index].endpoint == Some(source) && self.fkeys[index].events != 0 {
                    out_fkeys |= wire_bit;
                    self.fkeys[index].events = 0;
                }
                if self.sfkeys[index].endpoint == Some(source) && self.sfkeys[index].events != 0 {
                    out_sfkeys |= wire_bit;
                    self.sfkeys[index].events = 0;
                }
            }
            return (OK, out_fkeys, out_sfkeys);
        }
        if request != FKEY_MAP && request != FKEY_UNMAP {
            return (EINVAL, fkeys, sfkeys);
        }
        let mut result = OK;
        let mut banks = [
            core::mem::replace(&mut self.fkeys, [FkeyObserver::VACANT; FKEYS_PER_BANK]),
            core::mem::replace(&mut self.sfkeys, [FkeyObserver::VACANT; FKEYS_PER_BANK]),
        ];
        for (bank_index, bank) in banks.iter_mut().enumerate() {
            let bitmap = if bank_index == 0 { &mut fkeys } else { &mut sfkeys };
            for (index, slot) in bank.iter_mut().enumerate() {
                let wire_bit = 1i32 << (index + 1);
                if *bitmap & wire_bit == 0 {
                    continue;
                }
                if request == FKEY_MAP {
                    *slot = FkeyObserver {
                        endpoint: Some(source),
                        events: 0,
                    };
                    *bitmap &= !wire_bit;
                } else if slot.endpoint == Some(source) {
                    *slot = FkeyObserver::VACANT;
                    *bitmap &= !wire_bit;
                } else {
                    // Someone else's key: report failure but keep going
                    // (C "report failure, but try rest" — keyboard.c:489).
                    result = EPERM;
                }
            }
        }
        self.fkeys = banks[0];
        self.sfkeys = banks[1];
        (result, fkeys, sfkeys)
    }

    /// One press arrives. Returns the observer to notify, if any.
    ///
    /// C `func_key` (`keyboard.c:532-570`): the counter increments for the
    /// bank slot whether or not anyone is registered (the increment sits
    /// before the `proc_nr != NONE` check), and only a registered observer
    /// gets the notification. Gated on `enabled` (the `debug_fkeys` boot
    /// flag, `keyboard.c:206`).
    pub fn key_press(&mut self, page: i32, code: i32) -> Option<Endpoint> {
        if !self.enabled {
            return None;
        }
        let (sf, index) = hit_of(page, code)?;
        let slot = &mut self.bank(sf)[index];
        slot.events += 1;
        slot.endpoint
    }
}

/// Translates one event into an observer bank and slot index.
///
/// C `func_key` reads the keymap output — `F1 <= key && key <= F12` or
/// `SF1 <= key && key <= SF12` (`keyboard.c:555-558`), where `F1 = 0x110`
/// and `SF1 = 0x410` (SHIFT-qualified, `keymap.h:104/:135`). The input
/// framework reports keyboard events in its own code domain
/// (`INPUT_KEY_F1 = 0x003A .. INPUT_KEY_F12 = 0x0045`, `input.h:116-127`),
/// so the plain bank translates directly and **the Shift bank has no code
/// domain crossing yet** — the framework's events carry no modifier lane
/// ([`minix_types::InputEvent`] has page/code/value/flags only), so a
/// Shift+Fx press is indistinguishable from a plain press at this layer.
/// The registry and the three verbs above serve both banks regardless
/// (the IS client drives them over the wire); only the press path waits
/// for a modifier lane in the event domain.
pub fn hit_of(page: i32, code: i32) -> Option<(bool, usize)> {
    use minix_types::KeyCode;
    if page != minix_types::EventPage::Keyboard as i32 {
        return None;
    }
    // INPUT_KEY_F1 = 0x003A .. INPUT_KEY_F12 = 0x0045 are twelve
    // consecutive codes (input.h:116-127), so the range check is exact.
    let f1 = KeyCode::F1.0 as i32;
    let f12 = KeyCode::F12.0 as i32;
    if (f1..=f12).contains(&code) {
        Some((false, (code - f1) as usize))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IS: Endpoint = Endpoint(9);
    const OTHER: Endpoint = Endpoint(8);

    #[test]
    fn test_map_claims_all_set_bits_and_reports_leftover() {
        let mut table = FkeyTable::new(false);
        // Bits 1 and 3 (F1, F3) requested; both claimable.
        let (result, fkeys, sfkeys) =
            table.control(IS, FKEY_MAP, 0b1010, 0);
        assert_eq!(result, OK);
        assert_eq!((fkeys, sfkeys), (0, 0), "claimed bits leave the bitmap");
        assert_eq!(table.fkeys[0].endpoint, Some(IS));
        assert_eq!(table.fkeys[2].endpoint, Some(IS));
        assert_eq!(table.fkeys[0].events, 0);
    }

    #[test]
    fn test_unmap_only_own_slots() {
        let mut table = FkeyTable::new(false);
        let _ = table.control(IS, FKEY_MAP, 0b1010, 0);
        // OTHER unmaps bit 1: not the owner → EPERM, bit stays.
        let (result, fkeys, _) = table.control(OTHER, FKEY_UNMAP, 0b1010, 0);
        assert_eq!(result, EPERM);
        assert_eq!(fkeys & 0b10, 0b10, "foreign bit stays requested");
        assert!(table.fkeys[0].endpoint.is_some());
        // IS unmaps both: fine, slots vacate.
        let (result, fkeys, _) = table.control(IS, FKEY_UNMAP, 0b1010, 0);
        assert_eq!(result, OK);
        assert_eq!(fkeys, 0);
        assert!(table.fkeys[0].endpoint.is_none());
        assert!(table.fkeys[2].endpoint.is_none());
    }

    #[test]
    fn test_events_pull_reads_and_zeroes() {
        let mut table = FkeyTable::new(true);
        let _ = table.control(IS, FKEY_MAP, 0b10, 0b10);
        // Two presses land on the F bank slot (bit 1), none on Shift.
        // Event-domain F1 code = 0x003A (INPUT_KEY_F1).
        let _ = table.key_press(0x0007, 0x003A);
        let _ = table.key_press(0x0007, 0x003A);
        let (result, fkeys, sfkeys) =
            table.control(IS, FKEY_EVENTS, 0, 0);
        assert_eq!(result, OK);
        assert_eq!((fkeys, sfkeys), (0b10, 0), "pending presses fold into the reply");
        // Pulled counters are zeroed.
        let (_, fkeys, sfkeys) = table.control(IS, FKEY_EVENTS, 0, 0);
        assert_eq!((fkeys, sfkeys), (0, 0));
    }

    #[test]
    fn test_key_press_notifies_only_registered_and_gates_on_enabled() {
        let mut table = FkeyTable::new(false);
        let _ = table.control(IS, FKEY_MAP, 0b10, 0);
        // Gated off: no counting, no notify. Event-domain F1 = 0x003A.
        assert_eq!(table.key_press(0x0007, 0x003A), None);
        assert_eq!(table.fkeys[0].events, 0);
        table.enabled = true;
        assert_eq!(table.key_press(0x0007, 0x003A), Some(IS));
        assert_eq!(
            table.key_press(0x0007, 0x003B),
            None,
            "unregistered key still counts, notifies nobody"
        );
        assert_eq!(table.fkeys[1].events, 1);
        // A foreign page or key code never hits.
        assert_eq!(table.key_press(0x0001, 0x003A), None);
        assert_eq!(table.key_press(0x0007, 0x003A + 12), None);
    }

    #[test]
    fn test_unknown_request_is_einval() {
        let mut table = FkeyTable::new(false);
        let (result, fkeys, sfkeys) = table.control(IS, 999, 0b1111, 0b1111);
        assert_eq!(result, EINVAL);
        assert_eq!((fkeys, sfkeys), (0b1111, 0b1111), "refused request passes bitmaps through");
    }
}
