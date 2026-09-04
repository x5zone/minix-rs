//! TTY function-key observer protocol constants.
//!
//! Corresponds to Minix3's FKEY observer contract between the TTY driver
//! and its clients (here: the IS server): `TTY_FKEY_CONTROL`
//! (`minix3/minix/include/minix/com.h:874-877`) and the key codes
//! (`minix3/minix/include/minix/keymap.h:93-104` / `:135-146`).
//!
//! The wire payloads live with the other `Mess*` types in
//! [`crate::MessLsysTtyFkeyCtl`] / [`crate::MessTtyLsysFkeyCtl`]
//! (`message.rs`, C `ipc.h:1447-1454` / `:1925-1931`).
//!
//! `[ARCH: A-1]`: C expresses the bitmaps as bare `int` fields; Rust keeps
//! the wire layout (`#[repr(C)]`, 56-byte payloads) but the IS crate wraps
//! bit semantics in `FkeyId` (`os/servers/is/src/tty_fkey.rs`) so bit 0
//! (never used on the wire) is inexpressible.
//!
//! Authority (§2.4g): the FKEY/key-code constants below are the single
//! definition in minix-rs. `03-is-dump-dispatch.md`'s hooks table imports
//! from here; do not redefine per-crate.

/// TTY request base. C: `TTY_RQ_BASE 0x1300` — com.h:872.
pub const TTY_RQ_BASE: i32 = 0x1300;

/// Control an F-key at TTY. C: `TTY_FKEY_CONTROL (TTY_RQ_BASE + 1)` — com.h:874.
pub const TTY_FKEY_CONTROL: i32 = TTY_RQ_BASE + 1;

/// Observe function key. C: `FKEY_MAP 10` — com.h:875.
pub const FKEY_MAP: i32 = 10;

/// Stop observing function key. C: `FKEY_UNMAP 11` — com.h:876.
pub const FKEY_UNMAP: i32 = 11;

/// Request open key presses. C: `FKEY_EVENTS 12` — com.h:877.
pub const FKEY_EVENTS: i32 = 12;

/// Number of observable keys per bank (F1-F12 / Shift F1-F12).
/// C: the `for (i=0; i < 12; i++)` loops — keyboard.c.
pub const FKEY_COUNT: usize = 12;

// Function-key codes. C: `minix3/minix/include/minix/keymap.h:93-104`
// (`Fn (0x10+n + EXT)`, `EXT 0x0100` — keymap.h:14).
/// C: `F1 (0x10 + EXT)` — keymap.h:93.
pub const F1: i32 = 0x110;
/// C: `F2` — keymap.h:94.
pub const F2: i32 = 0x111;
/// C: `F3` — keymap.h:95.
pub const F3: i32 = 0x112;
/// C: `F4` — keymap.h:96.
pub const F4: i32 = 0x113;
/// C: `F5` — keymap.h:97.
pub const F5: i32 = 0x114;
/// C: `F6` — keymap.h:98.
pub const F6: i32 = 0x115;
/// C: `F7` — keymap.h:99.
pub const F7: i32 = 0x116;
/// C: `F8` — keymap.h:100.
pub const F8: i32 = 0x117;
/// C: `F9` — keymap.h:101.
pub const F9: i32 = 0x118;
/// C: `F10` — keymap.h:102.
pub const F10: i32 = 0x119;
/// C: `F11` — keymap.h:103.
pub const F11: i32 = 0x11A;
/// C: `F12 (0x1B + EXT)` — keymap.h:104.
pub const F12: i32 = 0x11B;

// Shifted function-key codes. C: `minix3/minix/include/minix/keymap.h:135-146`
// (`SFn (0x10+n + SHIFT)`, `SHIFT 0x0400` — keymap.h:16).
/// C: `SF1 (0x10 + SHIFT)` — keymap.h:135.
pub const SF1: i32 = 0x410;
/// C: `SF2` — keymap.h:136.
pub const SF2: i32 = 0x411;
/// C: `SF3` — keymap.h:137.
pub const SF3: i32 = 0x412;
/// C: `SF4` — keymap.h:138.
pub const SF4: i32 = 0x413;
/// C: `SF5` — keymap.h:139.
pub const SF5: i32 = 0x414;
/// C: `SF6` — keymap.h:140.
pub const SF6: i32 = 0x415;
/// C: `SF7` — keymap.h:141.
pub const SF7: i32 = 0x416;
/// C: `SF8` — keymap.h:142.
pub const SF8: i32 = 0x417;
/// C: `SF9` — keymap.h:143.
pub const SF9: i32 = 0x418;
/// C: `SF10` — keymap.h:144.
pub const SF10: i32 = 0x419;
/// C: `SF11` — keymap.h:145.
pub const SF11: i32 = 0x41A;
/// C: `SF12 (0x1B + SHIFT)` — keymap.h:146.
pub const SF12: i32 = 0x41B;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MessLsysTtyFkeyCtl, MessTtyLsysFkeyCtl};
    use core::mem::size_of;

    #[test]
    fn test_request_layout() {
        // C: ipc.h:1454 `_ASSERT_MSG_SIZE(mess_lsys_tty_fkey_ctl)`.
        assert_eq!(size_of::<MessLsysTtyFkeyCtl>(), 56);
        let m = MessLsysTtyFkeyCtl {
            request: FKEY_MAP,
            fkeys: 0x2,
            sfkeys: 0,
            _padding: [0u8; 44],
        };
        assert_eq!((m.request, m.fkeys, m.sfkeys), (10, 0x2, 0));
    }

    #[test]
    fn test_reply_layout() {
        // C: ipc.h:1931 `_ASSERT_MSG_SIZE(mess_tty_lsys_fkey_ctl)`.
        assert_eq!(size_of::<MessTtyLsysFkeyCtl>(), 56);
        let m = MessTtyLsysFkeyCtl {
            fkeys: 0x4,
            sfkeys: 0,
            _padding: [0u8; 48],
        };
        assert_eq!((m.fkeys, m.sfkeys), (0x4, 0));
    }

    #[test]
    fn test_command_codes() {
        // C: com.h:874-877.
        assert_eq!(TTY_FKEY_CONTROL, 0x1301);
        assert_eq!((FKEY_MAP, FKEY_UNMAP, FKEY_EVENTS), (10, 11, 12));
    }

    #[test]
    fn test_key_codes_spot_and_contiguity() {
        // C: keymap.h:93-104 (EXT 0x100) / :135-146 (SHIFT 0x400).
        assert_eq!((F1, F12), (0x110, 0x11B));
        assert_eq!((SF1, SF12), (0x410, 0x41B));
        assert_eq!(F12 - F1, 11);
        assert_eq!(SF12 - SF1, 11);
        assert_eq!((F3, SF5), (0x112, 0x414));
    }
}
