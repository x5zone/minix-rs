//! FKEY observer client logic (02-is-fkey-contract.md §4.1).
//!
//! C: `map_unmap_fkeys` (`minix3/minix/servers/is/dmp.c:44-68`) +
//! `fkey_ctl` (`minix3/minix/lib/libsys/fkey_ctl.c`) + the `fkey_map`/
//! `fkey_unmap`/`fkey_events` macros (`minix3/minix/include/minix/sysutil.h:
//! 43-46`). Wire types live in `minix-types::ipc::tty` (`[ARCH: A-1]`).
//!
//! The TTY side (`do_fkey_ctl`/`func_key`, keyboard.c) is NOT implemented
//! here — it belongs to the future TTY crate. Tests mirror its contract
//! with an explicitly named `FakeTty` (scaffold, not a second
//! implementation).

use minix_types::{FKEY_EVENTS, FKEY_MAP, FKEY_UNMAP, OK};

/// An observable function key.
///
/// Unifies the three C numbering layers (02-is-fkey-contract.md §1.3):
/// key code (`0x110`…), bitmap bit (1…12), array index (0…11, TTY-private).
/// Bit 0 has no variant — "bit 0 unused" is enforced by the type system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FkeyId {
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Sf1,
    Sf2,
    Sf3,
    Sf4,
    Sf5,
    Sf6,
    Sf7,
    Sf8,
    Sf9,
    Sf10,
    Sf11,
    Sf12,
}

impl FkeyId {
    /// Wire bit number (1-12 per bank). C: `hooks[h].key - F1 + 1` —
    /// `minix3/minix/servers/is/dmp.c:55-58`. There is no bit 0.
    pub const fn bit(self) -> u32 {
        match self {
            FkeyId::F1 | FkeyId::Sf1 => 1,
            FkeyId::F2 | FkeyId::Sf2 => 2,
            FkeyId::F3 | FkeyId::Sf3 => 3,
            FkeyId::F4 | FkeyId::Sf4 => 4,
            FkeyId::F5 | FkeyId::Sf5 => 5,
            FkeyId::F6 | FkeyId::Sf6 => 6,
            FkeyId::F7 | FkeyId::Sf7 => 7,
            FkeyId::F8 | FkeyId::Sf8 => 8,
            FkeyId::F9 | FkeyId::Sf9 => 9,
            FkeyId::F10 | FkeyId::Sf10 => 10,
            FkeyId::F11 | FkeyId::Sf11 => 11,
            FkeyId::F12 | FkeyId::Sf12 => 12,
        }
    }

    /// Whether this is a shifted key (sfkeys bank).
    pub const fn is_shifted(self) -> bool {
        matches!(
            self,
            FkeyId::Sf1
                | FkeyId::Sf2
                | FkeyId::Sf3
                | FkeyId::Sf4
                | FkeyId::Sf5
                | FkeyId::Sf6
                | FkeyId::Sf7
                | FkeyId::Sf8
                | FkeyId::Sf9
                | FkeyId::Sf10
                | FkeyId::Sf11
                | FkeyId::Sf12
        )
    }

    /// Key code. C: `F1 (0x10 + EXT)` — `minix3/minix/include/minix/keymap.h:
    /// 93-104`, `SF1 (0x10 + SHIFT)` — keymap.h:135-146.
    pub const fn key_code(self) -> i32 {
        match self {
            FkeyId::F1 => minix_types::F1,
            FkeyId::F2 => minix_types::F2,
            FkeyId::F3 => minix_types::F3,
            FkeyId::F4 => minix_types::F4,
            FkeyId::F5 => minix_types::F5,
            FkeyId::F6 => minix_types::F6,
            FkeyId::F7 => minix_types::F7,
            FkeyId::F8 => minix_types::F8,
            FkeyId::F9 => minix_types::F9,
            FkeyId::F10 => minix_types::F10,
            FkeyId::F11 => minix_types::F11,
            FkeyId::F12 => minix_types::F12,
            FkeyId::Sf1 => minix_types::SF1,
            FkeyId::Sf2 => minix_types::SF2,
            FkeyId::Sf3 => minix_types::SF3,
            FkeyId::Sf4 => minix_types::SF4,
            FkeyId::Sf5 => minix_types::SF5,
            FkeyId::Sf6 => minix_types::SF6,
            FkeyId::Sf7 => minix_types::SF7,
            FkeyId::Sf8 => minix_types::SF8,
            FkeyId::Sf9 => minix_types::SF9,
            FkeyId::Sf10 => minix_types::SF10,
            FkeyId::Sf11 => minix_types::SF11,
            FkeyId::Sf12 => minix_types::SF12,
        }
    }
}

/// Assembles the two wire bitmaps from a key list (pure).
///
/// C: the `for (h...)` accumulation — dmp.c:50-58. Empty input yields
/// `(0, 0)` (C zero-initialises before the loop — dmp.c:48).
pub const fn fkey_bits(keys: &[FkeyId]) -> (u32, u32) {
    let mut fkeys = 0u32;
    let mut sfkeys = 0u32;
    let mut i = 0;
    while i < keys.len() {
        let bit = keys[i].bit();
        if keys[i].is_shifted() {
            sfkeys |= 1 << bit;
        } else {
            fkeys |= 1 << bit;
        }
        i += 1;
    }
    (fkeys, sfkeys)
}

/// FKEY sub-command.
///
/// C: `FKEY_MAP 10` / `FKEY_UNMAP 11` / `FKEY_EVENTS 12` —
/// `minix3/minix/include/minix/com.h:875-877`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FkeyReq {
    Map,
    Unmap,
    Events,
}

impl FkeyReq {
    /// Wire code.
    pub const fn code(self) -> i32 {
        match self {
            FkeyReq::Map => FKEY_MAP,
            FkeyReq::Unmap => FKEY_UNMAP,
            FkeyReq::Events => FKEY_EVENTS,
        }
    }
}

/// TTY FKEY transport: the `fkey_ctl` client boundary.
///
/// C: `fkey_ctl(req, *fkeys, *sfkeys)` — `minix3/minix/lib/libsys/fkey_ctl.c`
/// (`_taskcall`, synchronous, no grants). Returns `(status, leftover_fkeys,
/// leftover_sfkeys)`: status is the overall result, leftover bitmaps carry
/// the bits TTY did NOT consume (C writeback — fkey_ctl.c:26-27).
/// Production implementation is a forward reference (lands with the
/// `minix-sys` `_taskcall` wiring, 01-is-init-main.md §3 D2 pattern).
pub trait FkeyCtlTransport {
    fn fkey_ctl(&mut self, req: FkeyReq, fkeys: u32, sfkeys: u32) -> (i32, u32, u32);
}

/// Fail-closed transport until the `minix-sys` wiring lands (01 `sef.rs`
/// `UnimplementedTransport` pattern).
#[derive(Debug, Default)]
pub struct UnimplementedFkeyCtl;

impl FkeyCtlTransport for UnimplementedFkeyCtl {
    fn fkey_ctl(&mut self, _req: FkeyReq, _fkeys: u32, _sfkeys: u32) -> (i32, u32, u32) {
        panic!("IS fkey transport: _taskcall wiring pending (02-is-fkey-contract.md §3 D3)");
    }
}

/// MAP/UNMAP failure detail: status plus the bits TTY kept.
///
/// C: `s != OK` → `printf("IS: warning, fkey_ctl failed: %d")` —
/// dmp.c:63-65. The leftovers let the caller retry or diagnose per-bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FkeyCtlError {
    pub status: i32,
    pub leftover_fkeys: u32,
    pub leftover_sfkeys: u32,
}

/// Registers or unregisters a key set at TTY.
///
/// C: `map_unmap_fkeys(map)` — dmp.c:44-68, minus the hooks-table read
/// (the `keys` argument replaces the global `hooks[]`; 03 supplies it).
/// Failure warns at the caller, never panics (registration is non-fatal —
/// §2.6); the error carries leftovers instead.
pub fn map_unmap_keys<C: FkeyCtlTransport>(
    client: &mut C,
    map: bool,
    keys: &[FkeyId],
) -> Result<(), FkeyCtlError> {
    let (fkeys, sfkeys) = fkey_bits(keys);
    let req = if map { FkeyReq::Map } else { FkeyReq::Unmap };
    let (status, leftover_fkeys, leftover_sfkeys) = client.fkey_ctl(req, fkeys, sfkeys);
    if status != OK {
        return Err(FkeyCtlError { status, leftover_fkeys, leftover_sfkeys });
    }
    Ok(())
}

/// Pulls pending key-press bitmaps (destructive read).
///
/// C: `fkey_events(&fkeys, &sfkeys)` before dispatch — dmp.c:83-87.
/// TTY zeroes-then-fills owned pending bits and always returns OK for a
/// well-formed call (keyboard.c:509-526); a negative status means the
/// transport itself failed (`_taskcall` error, checked as `s < 0` —
/// dmp.c:84), so it is surfaced, not swallowed (03 refines the 02 contract).
pub fn pull_events<C: FkeyCtlTransport>(client: &mut C) -> (i32, u32, u32) {
    client.fkey_ctl(FkeyReq::Events, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirror of the TTY observer state machine (keyboard.c:439-526,532-585).
    /// Test scaffold — NOT a TTY implementation.
    struct FakeTty {
        owner_f: [i32; 12],
        owner_s: [i32; 12],
        events_f: [u32; 12],
        events_s: [u32; 12],
        notified: Vec<i32>,
    }

    const ME: i32 = 100;
    const OTHER: i32 = 200;

    impl FakeTty {
        fn new() -> Self {
            Self {
                owner_f: [-1; 12],
                owner_s: [-1; 12],
                events_f: [0; 12],
                events_s: [0; 12],
                notified: Vec::new(),
            }
        }

        /// Mirror of `func_key` press path (keyboard.c:532-585): counter
        /// bumps even with no observer; notify only fires for an owner.
        fn press(&mut self, key: FkeyId) {
            let idx = (key.bit() - 1) as usize;
            let (owners, events) = if key.is_shifted() {
                (&self.owner_s, &mut self.events_s)
            } else {
                (&self.owner_f, &mut self.events_f)
            };
            events[idx] += 1;
            if owners[idx] != -1 {
                self.notified.push(owners[idx]);
            }
        }
    }

    impl FkeyCtlTransport for FakeTty {
        fn fkey_ctl(&mut self, req: FkeyReq, fkeys: u32, sfkeys: u32) -> (i32, u32, u32) {
            let mut status = OK;
            let mut lf = fkeys;
            let mut ls = sfkeys;
            // MAP: unconditional overwrite (DEAD_CODE EBUSY rationale —
            // crash recovery by overtaking, keyboard.c:443-475).
            // UNMAP: owner check → EPERM, try rest (keyboard.c:477-507).
            // EVENTS: zero-fill owned pending, always OK (keyboard.c:509-526).
            match req {
                FkeyReq::Map => {
                    for i in 0..12usize {
                        let bit = 1u32 << (i + 1);
                        if fkeys & bit != 0 {
                            self.owner_f[i] = ME;
                            self.events_f[i] = 0;
                            lf &= !bit;
                        }
                        if sfkeys & bit != 0 {
                            self.owner_s[i] = ME;
                            self.events_s[i] = 0;
                            ls &= !bit;
                        }
                    }
                }
                FkeyReq::Unmap => {
                    for i in 0..12usize {
                        let bit = 1u32 << (i + 1);
                        if fkeys & bit != 0 {
                            if self.owner_f[i] == ME {
                                self.owner_f[i] = -1;
                                self.events_f[i] = 0;
                                lf &= !bit;
                            } else {
                                status = minix_types::EPERM;
                            }
                        }
                        if sfkeys & bit != 0 {
                            if self.owner_s[i] == ME {
                                self.owner_s[i] = -1;
                                self.events_s[i] = 0;
                                ls &= !bit;
                            } else {
                                status = minix_types::EPERM;
                            }
                        }
                    }
                }
                FkeyReq::Events => {
                    lf = 0;
                    ls = 0;
                    for i in 0..12usize {
                        let bit = 1u32 << (i + 1);
                        if self.owner_f[i] == ME && self.events_f[i] != 0 {
                            lf |= bit;
                            self.events_f[i] = 0;
                        }
                        if self.owner_s[i] == ME && self.events_s[i] != 0 {
                            ls |= bit;
                            self.events_s[i] = 0;
                        }
                    }
                }
            }
            (status, lf, ls)
        }
    }

    #[test]
    fn test_req_codes_match_com() {
        // C: com.h:875-877.
        assert_eq!((FkeyReq::Map.code(), FkeyReq::Unmap.code(), FkeyReq::Events.code()), (10, 11, 12));
    }

    #[test]
    fn test_bit_numbers() {        // C: key-F1+1 — dmp.c:55-58. Bit 0 unreachable (no variant maps to 0).
        assert_eq!((FkeyId::F1.bit(), FkeyId::F12.bit()), (1, 12));
        assert_eq!((FkeyId::Sf1.bit(), FkeyId::Sf12.bit()), (1, 12));
        assert!(FkeyId::F5.bit() >= 1);
    }

    #[test]
    fn test_key_codes_match_keymap() {
        assert_eq!(FkeyId::F1.key_code(), 0x110);
        assert_eq!(FkeyId::F12.key_code(), 0x11B);
        assert_eq!(FkeyId::Sf1.key_code(), 0x410);
        assert_eq!(FkeyId::Sf12.key_code(), 0x41B);
    }

    #[test]
    fn test_fkey_bits_assembly() {
        assert_eq!(fkey_bits(&[]), (0, 0));
        assert_eq!(fkey_bits(&[FkeyId::F1]), (0b10, 0));
        assert_eq!(fkey_bits(&[FkeyId::Sf12]), (0, 1 << 12));
        // Hooks-derived key set (03): 8 + 8 bits, bit 0 never set.
        let keys: [FkeyId; 16] = crate::dispatch::HOOKS.map(|h| h.key);
        let (f, s) = fkey_bits(&keys);
        assert_eq!(f.count_ones(), 8);
        assert_eq!(s.count_ones(), 8);
        assert_eq!(f & 1, 0, "bit 0 never set");
    }

    #[test]
    fn test_hooks_key_set_matches_c_hooks_column() {
        // C: dmp.c:18-35 hooks key column (16 keys). The interim INIT_FKEYS
        // is gone (03); the hooks table is the single source.
        use crate::dispatch::HOOKS;
        assert_eq!(HOOKS.len(), 16);
        let keys: [FkeyId; 16] = HOOKS.map(|h| h.key);
        assert!(keys.contains(&FkeyId::F1));
        assert!(keys.contains(&FkeyId::Sf9));
        assert!(!keys.contains(&FkeyId::F2));
        assert!(!keys.contains(&FkeyId::F9));
        assert!(!keys.contains(&FkeyId::Sf7));
    }

    #[test]
    fn test_map_overwrites_for_crash_recovery() {
        // DEAD_CODE rationale: overtaking existing mappings (keyboard.c).
        let mut tty = FakeTty::new();
        tty.owner_f[0] = OTHER; // stale registration from crashed incarnation
        let (status, lf, ls) = tty.fkey_ctl(FkeyReq::Map, 0b10, 0);
        assert_eq!((status, lf, ls), (OK, 0, 0));
        assert_eq!(tty.owner_f[0], ME);
    }

    #[test]
    fn test_unmap_non_owner_eperm_keeps_bit() {
        let mut tty = FakeTty::new();
        tty.owner_f[0] = OTHER;
        let (status, lf, _) = tty.fkey_ctl(FkeyReq::Unmap, 0b10, 0);
        assert_eq!(status, minix_types::EPERM);
        assert_eq!(lf, 0b10, "failed bit retained in writeback");
        assert_eq!(tty.owner_f[0], OTHER, "owner untouched");
    }

    #[test]
    fn test_events_consume_only_mine() {
        let mut tty = FakeTty::new();
        tty.owner_f[0] = ME;
        tty.owner_f[1] = OTHER;
        tty.events_f[0] = 3;
        tty.events_f[1] = 5;
        let (status, f, s) = tty.fkey_ctl(FkeyReq::Events, 0, 0);
        assert_eq!((status, f, s), (OK, 0b10, 0));
        assert_eq!(tty.events_f[0], 0, "consumed");
        assert_eq!(tty.events_f[1], 5, "other owner's counter intact");
    }

    #[test]
    fn test_press_counts_without_owner_notifies_with_owner() {
        // C: events++ unconditional; ipc_notify only if proc_nr != NONE.
        let mut tty = FakeTty::new();
        tty.press(FkeyId::F1);
        assert_eq!(tty.events_f[0], 1);
        assert!(tty.notified.is_empty());
        tty.owner_f[0] = ME;
        tty.press(FkeyId::F1);
        assert_eq!(tty.notified, [ME]);
    }

    #[test]
    fn test_map_unmap_keys_err_carries_leftovers() {
        let mut tty = FakeTty::new();
        assert!(map_unmap_keys(&mut tty, true, &[FkeyId::F1]).is_ok());
        tty.owner_f[0] = OTHER; // simulate a stolen slot
        let err = map_unmap_keys(&mut tty, false, &[FkeyId::F1]).unwrap_err();
        assert_eq!(err.status, minix_types::EPERM);
        assert_eq!(err.leftover_fkeys, 0b10);
    }

    #[test]
    fn test_pull_events_passthrough() {
        let mut tty = FakeTty::new();
        tty.owner_f[2] = ME;
        tty.events_f[2] = 1;
        tty.owner_s[8] = ME;
        tty.events_s[8] = 2;
        assert_eq!(pull_events(&mut tty), (OK, 1 << 3, 1 << 9));
    }
}
