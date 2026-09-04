//! Main-loop message classifier + dump dispatch table.
//!
//! Classifier (01-is-init-main.md §4.1). C: the two-level branch in `main()`
//! — `minix3/minix/servers/is/main.c:48-63`. Pure function, mirroring
//! `os/servers/rs/src/dispatch.rs:70-82`.
//!
//! Dispatch table (03-is-dump-dispatch.md §4.1). C: `hooks[]`/`NHOOKS`
//! (`minix3/minix/servers/is/dmp.c:11-40`), `pressed` (dmp.c:70-72),
//! the `do_fkey_pressed` matching loop (dmp.c:89-95), `key_name`
//! (dmp.c:103-117), `mapping_dmp` layout (dmp.c:118-132).

use minix_types::{EDONTREPLY, Endpoint};

use crate::tty_fkey::FkeyId;

// ── 01: classifier ──────────────────────────────────────────────

/// Kernel notification base. C: `NOTIFY_MESSAGE` —
/// `minix3/minix/include/minix/com.h:90`.
pub const NOTIFY_MESSAGE: i32 = 0x1000;

/// TTY slot number. C: `TTY_PROC_NR ((endpoint_t) 5)` —
/// `minix3/minix/include/minix/com.h:64`.
pub const TTY_PROC_SLOT: i32 = 5;

/// Notification range width. C: the `< 0x100` in `is_notify` — com.h:93.
const NOTIFY_RANGE: u32 = 0x100;

/// Whether a message type is a kernel notification.
///
/// C: `is_notify(a) ((unsigned) ((a) - NOTIFY_MESSAGE) < 0x100)` —
/// `minix3/minix/include/minix/com.h:93`.
///
/// NOTE (com.h:91 FIXME, acknowledged): upstream notes the old
/// `is_notify(a)` form "should be replaced by `is_ipc_notify(status)`"
/// (status-word check, com.h:92). IS keeps the old form deliberately:
/// switching would change `get_work`'s signature to carry the receive
/// status word back — a behavior change, rejected under the conservative
/// rewrite orientation (01-is-init-main.md §2.3). Keeping is not evolution,
/// so no `[ARCH:]` tag.
pub const fn is_notify_call(call_nr: i32) -> bool {
    ((call_nr - NOTIFY_MESSAGE) as u32) < NOTIFY_RANGE
}

/// One main-loop classification outcome.
///
/// C: the `result` arms in `main()` — main.c:48-63. `HandleFkey` routes to
/// `do_fkey_pressed` (03); `Suppress` covers both the non-TTY notify
/// `default` (main.c:53-56) and the non-notify branch (main.c:59-63).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchAction {
    /// TTY notification → `do_fkey_pressed` (03-is-dump-dispatch.md).
    HandleFkey,
    /// Anything else → `EDONTREPLY`, reply path skipped.
    Suppress,
}

/// Classifies one received message.
///
/// C: `if (is_notify(callnr)) switch (_ENDPOINT_P(who_e))` — main.c:48-58.
/// The sender is compared by **slot** (`Endpoint::slot`, i.e. `_ENDPOINT_P`),
/// never by raw endpoint value: the high bits are the generation counter,
/// so raw equality breaks across TTY restarts while the slot is stable.
///
/// The C `default` arm's silence (no warning log, unlike the non-notify
/// arm's `printf` — main.c:54 vs main.c:60-61) is preserved as-is; see
/// 01-is-init-main.md §2.3. Do not "fix" it with a log line.
pub const fn classify(call_nr: i32, sender: Endpoint) -> DispatchAction {
    if is_notify_call(call_nr) && sender.slot() == TTY_PROC_SLOT {
        DispatchAction::HandleFkey
    } else {
        DispatchAction::Suppress
    }
}

/// Whether a handler result suppresses the reply.
///
/// C: `if (result != EDONTREPLY) { reply(who_e, result); }` — main.c:65-68.
/// `EDONTREPLY` (= 203) is a reply-suppression sentinel, not an error code
/// (`os/libs/minix-types/src/types/errno.rs:94`, citing sys/errno.h:199).
pub const fn is_reply_suppressed(result: i32) -> bool {
    result == EDONTREPLY
}

// ── 03: dispatch table ──────────────────────────────────────────

/// Future dump owner, in hooks-table order.
///
/// C: the `function` column — dmp.c:18-35. Each variant is implemented by
/// its owning doc (05~10); `Mapping` is implemented alongside the output
/// channel ([ARCH: A-6]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpId {
    Proctab,
    Image,
    Privileges,
    Monparams,
    Irqtab,
    Kmessages,
    Vm,
    Kenv,
    Mproc,
    Sigaction,
    Fproc,
    Dtab,
    Mapping,
    Rproc,
    DataStore,
    Procstack,
}

/// One function-key → dump binding.
///
/// C: `struct hook_entry {int key; void(*function)(void); char *name}` —
/// dmp.c:11-15. The function pointer becomes [`DumpId`] (03 has no bodies
/// to point at, and a table of pointers would be untestable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hook {
    pub key: FkeyId,
    pub name: &'static str,
    pub dump: DumpId,
}

/// The 16 observable keys, in C table order.
///
/// C: `hooks[]` — dmp.c:18-35. `NHOOKS` is `HOOKS.len()`.
pub static HOOKS: &[Hook; 16] = &[
    Hook { key: FkeyId::F1, name: "Kernel process table", dump: DumpId::Proctab },
    Hook { key: FkeyId::F3, name: "System image", dump: DumpId::Image },
    Hook { key: FkeyId::F4, name: "Process privileges", dump: DumpId::Privileges },
    Hook { key: FkeyId::F5, name: "Boot monitor parameters", dump: DumpId::Monparams },
    Hook { key: FkeyId::F6, name: "IRQ hooks and policies", dump: DumpId::Irqtab },
    Hook { key: FkeyId::F7, name: "Kernel messages", dump: DumpId::Kmessages },
    Hook { key: FkeyId::F8, name: "VM status and process maps", dump: DumpId::Vm },
    Hook { key: FkeyId::F10, name: "Kernel parameters", dump: DumpId::Kenv },
    Hook { key: FkeyId::Sf1, name: "Process manager process table", dump: DumpId::Mproc },
    Hook { key: FkeyId::Sf2, name: "Signals", dump: DumpId::Sigaction },
    Hook { key: FkeyId::Sf3, name: "Filesystem process table", dump: DumpId::Fproc },
    Hook { key: FkeyId::Sf4, name: "Device/Driver mapping", dump: DumpId::Dtab },
    Hook { key: FkeyId::Sf5, name: "Print key mappings", dump: DumpId::Mapping },
    Hook { key: FkeyId::Sf6, name: "Reincarnation server process table", dump: DumpId::Rproc },
    Hook { key: FkeyId::Sf8, name: "Data store contents", dump: DumpId::DataStore },
    Hook { key: FkeyId::Sf9, name: "Processes with stack traces", dump: DumpId::Procstack },
];

/// Whether a key code falls in range with its bitmap bit set.
///
/// C: `pressed(start,end,bitfield,key)` —
/// `(((start) <= (key)) && ((end) >= (key)) && bit_isset((bitfield), ((key) - (start) + 1)))`
/// — dmp.c:70-72. Mirrored verbatim (including the `+1`: bit 0 unused).
pub const fn pressed(start: i32, end: i32, bitfield: u32, key: i32) -> bool {
    start <= key && key <= end && (bitfield & (1 << ((key - start + 1) as u32))) != 0
}

/// Visits every hook whose key is pressed, in table order.
///
/// C: the `for(h...) if/else-if → hooks[h].function()` loop — dmp.c:89-95.
/// No early break: several keys may be pending in one round and each runs
/// in table order. Callback style keeps `no_std` allocation-free.
pub fn dispatch_each(fkeys: u32, sfkeys: u32, mut visit: impl FnMut(&Hook)) {
    for hook in HOOKS {
        let code = hook.key.key_code();
        // C is if/else-if (dmp.c:90-94); `||` is equivalent — a hook key
        // lives in exactly one bank, so at most one arm can match.
        if pressed(minix_types::F1, minix_types::F12, fkeys, code)
            || pressed(minix_types::SF1, minix_types::SF12, sfkeys, code)
        {
            visit(hook);
        }
    }
}

/// Printable key name.
///
/// C: `key_name(key)` — dmp.c:103-117 (`" F%d"` / `"Shift+F%d"`, else `"?"`).
/// The `else "?"` branch is unreachable by construction: every `FkeyId` is
/// in range, so the C fallback is eliminated at the type level instead of
/// transliterated.
pub const fn key_name(key: FkeyId) -> &'static str {
    match key {
        FkeyId::F1 => " F1",
        FkeyId::F2 => " F2",
        FkeyId::F3 => " F3",
        FkeyId::F4 => " F4",
        FkeyId::F5 => " F5",
        FkeyId::F6 => " F6",
        FkeyId::F7 => " F7",
        FkeyId::F8 => " F8",
        FkeyId::F9 => " F9",
        FkeyId::F10 => " F10",
        FkeyId::F11 => " F11",
        FkeyId::F12 => " F12",
        FkeyId::Sf1 => "Shift+F1",
        FkeyId::Sf2 => "Shift+F2",
        FkeyId::Sf3 => "Shift+F3",
        FkeyId::Sf4 => "Shift+F4",
        FkeyId::Sf5 => "Shift+F5",
        FkeyId::Sf6 => "Shift+F6",
        FkeyId::Sf7 => "Shift+F7",
        FkeyId::Sf8 => "Shift+F8",
        FkeyId::Sf9 => "Shift+F9",
        FkeyId::Sf10 => "Shift+F10",
        FkeyId::Sf11 => "Shift+F11",
        FkeyId::Sf12 => "Shift+F12",
    }
}

/// `mapping_dmp` title line. C: `printf("Function key mappings ...")` — dmp.c:123.
pub const MAPPING_TITLE: &str = "Function key mappings for debug dumps in IS server.";
/// `mapping_dmp` column header. C: `printf("        Key   Description\n")` — dmp.c:124.
pub const MAPPING_COLUMNS: &str = "        Key   Description";

#[cfg(test)]
mod tests {
    use super::*;

    fn tty_gen0() -> Endpoint {
        Endpoint::TTY
    }

    fn tty_reborn() -> Endpoint {
        // Same slot, different generation — raw value differs from TTY.
        Endpoint::from_generation_slot(3, TTY_PROC_SLOT)
    }

    #[test]
    fn test_notify_tty_dispatched() {
        // C: case TTY_PROC_NR → do_fkey_pressed — main.c:50-52.
        assert_eq!(classify(NOTIFY_MESSAGE, tty_gen0()), DispatchAction::HandleFkey);
    }

    #[test]
    fn test_notify_tty_slot_not_raw_value() {
        // Generation bits must not matter: _ENDPOINT_P semantics.
        let reborn = tty_reborn();
        assert_ne!(reborn, Endpoint::TTY);
        assert_eq!(reborn.slot(), TTY_PROC_SLOT);
        assert_eq!(classify(NOTIFY_MESSAGE, reborn), DispatchAction::HandleFkey);
    }

    #[test]
    fn test_notify_non_tty_suppressed() {
        // C: default → EDONTREPLY — main.c:53-56.
        assert_eq!(classify(NOTIFY_MESSAGE, Endpoint::RS), DispatchAction::Suppress);
    }

    #[test]
    fn test_non_notify_suppressed() {
        // C: else → warning + EDONTREPLY — main.c:59-63.
        assert_eq!(classify(0x1, tty_gen0()), DispatchAction::Suppress);
        assert_eq!(classify(0, Endpoint::RS), DispatchAction::Suppress);
    }

    #[test]
    fn test_is_notify_boundaries() {
        // C: ((a) - 0x1000) < 0x100 — com.h:93.
        assert!(!is_notify_call(0x0FFF));
        assert!(is_notify_call(0x1000));
        assert!(is_notify_call(0x10FF));
        assert!(!is_notify_call(0x1100));
    }

    #[test]
    fn test_reply_suppression_sentinel() {
        // C: if (result != EDONTREPLY) — main.c:66.
        assert!(is_reply_suppressed(EDONTREPLY));
        assert!(!is_reply_suppressed(minix_types::ENOSYS));
        assert!(!is_reply_suppressed(minix_types::OK));
    }

    #[test]
    fn test_c_constants_match() {
        // C: com.h:64 (TTY=5), com.h:90 (NOTIFY_MESSAGE=0x1000).
        assert_eq!(TTY_PROC_SLOT, 5);
        assert_eq!(NOTIFY_MESSAGE, 0x1000);
        assert_eq!(Endpoint::TTY.slot(), TTY_PROC_SLOT);
    }

    #[test]
    fn test_table_has_16_hooks_in_c_order() {
        // C: dmp.c:18-35 + NHOOKS (dmp.c:40).
        assert_eq!(HOOKS.len(), 16);
        assert_eq!(HOOKS[0].key, FkeyId::F1);
        assert_eq!(HOOKS[0].dump, DumpId::Proctab);
        assert_eq!(HOOKS[15].key, FkeyId::Sf9);
        assert_eq!(HOOKS[15].dump, DumpId::Procstack);
        assert_eq!(HOOKS[12].key, FkeyId::Sf5);
        assert_eq!(HOOKS[12].dump, DumpId::Mapping);
    }

    #[test]
    fn test_table_names_match_c() {
        // C: the `name` column — dmp.c:18-35.
        let names: [&str; 16] = [
            "Kernel process table",
            "System image",
            "Process privileges",
            "Boot monitor parameters",
            "IRQ hooks and policies",
            "Kernel messages",
            "VM status and process maps",
            "Kernel parameters",
            "Process manager process table",
            "Signals",
            "Filesystem process table",
            "Device/Driver mapping",
            "Print key mappings",
            "Reincarnation server process table",
            "Data store contents",
            "Processes with stack traces",
        ];
        for (hook, name) in HOOKS.iter().zip(names) {
            assert_eq!(hook.name, name);
        }
    }

    #[test]
    fn test_pressed_truth_table() {
        // C: pressed(F1,F12,bitfield,key) — dmp.c:70-72.
        assert!(pressed(0x110, 0x11B, 0b10, 0x110)); // F1, bit 1
        assert!(!pressed(0x110, 0x11B, 0b10, 0x111)); // F2, bit unset
        assert!(!pressed(0x110, 0x11B, 0b10, 0x410)); // SF1 out of F range
        assert!(!pressed(0x110, 0x11B, 0b10, 0x10F)); // below range
        assert!(!pressed(0x110, 0x11B, 0, 0x110)); // bit cleared
        assert!(pressed(0x410, 0x41B, 1 << 12, 0x41B)); // SF12, bit 12
    }

    #[test]
    fn test_dispatch_visits_matches_in_table_order_without_break() {
        // C: sequential calls, no break — dmp.c:89-95.
        let mut seen = [DumpId::Proctab; 16];
        let mut n = 0usize;
        // F1 (bit1) + F3 (bit3) + SF9 (bit9).
        dispatch_each((1 << 1) | (1 << 3), 1 << 9, |h| {
            seen[n] = h.dump;
            n += 1;
        });
        assert_eq!(n, 3);
        assert_eq!(seen[0], DumpId::Proctab);
        assert_eq!(seen[1], DumpId::Image);
        assert_eq!(seen[2], DumpId::Procstack);
    }

    #[test]
    fn test_dispatch_empty_bitmap_visits_nothing() {
        let mut n = 0;
        dispatch_each(0, 0, |_| n += 1);
        assert_eq!(n, 0);
    }

    #[test]
    fn test_dispatch_banks_isolated() {
        // F-bank bits must not trigger SF hooks and vice versa.
        let mut n = 0;
        dispatch_each(0xFFFF_FFFE, 0, |_| n += 1);
        assert_eq!(n, 8, "only the 8 F hooks");
        n = 0;
        dispatch_each(0, 0xFFFF_FFFE, |_| n += 1);
        assert_eq!(n, 8, "only the 8 SF hooks");
    }

    #[test]
    fn test_key_name_three_forms() {
        // C: " F%d" / "Shift+F%d" / "?" — dmp.c:103-117.
        assert_eq!(key_name(FkeyId::F1), " F1");
        assert_eq!(key_name(FkeyId::F12), " F12");
        assert_eq!(key_name(FkeyId::Sf1), "Shift+F1");
        assert_eq!(key_name(FkeyId::Sf12), "Shift+F12");
    }

    #[test]
    fn test_key_names_fit_mapping_column() {
        // C format `" %10s.  %s"` — dmp.c:125. Longest name is 9 chars.
        let all = [
            FkeyId::F1, FkeyId::F2, FkeyId::F3, FkeyId::F4, FkeyId::F5, FkeyId::F6,
            FkeyId::F7, FkeyId::F8, FkeyId::F9, FkeyId::F10, FkeyId::F11, FkeyId::F12,
            FkeyId::Sf1, FkeyId::Sf2, FkeyId::Sf3, FkeyId::Sf4, FkeyId::Sf5, FkeyId::Sf6,
            FkeyId::Sf7, FkeyId::Sf8, FkeyId::Sf9, FkeyId::Sf10, FkeyId::Sf11, FkeyId::Sf12,
        ];
        for k in all {
            assert!(key_name(k).len() <= 10, "{k:?} exceeds %10s");
        }
        assert_eq!(MAPPING_TITLE, "Function key mappings for debug dumps in IS server.");
        assert_eq!(MAPPING_COLUMNS, "        Key   Description");
    }
}
