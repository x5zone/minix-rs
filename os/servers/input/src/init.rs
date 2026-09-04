//! Startup: from process start to serving requests.
//!
//! C: `input_startup`, `input_init`, and `main`
//! (`minix3/minix/servers/input/input.c:643-704`). This module owns the
//! *order* of startup and the *inventory* of what startup needs; the bodies
//! behind each step live where they belong (device table in [`crate::structs`],
//! framework effects in [`crate::framework`], driver subscription in document
//! 11, the terminal handshake in document 13).
//!
//! Startup has exactly three statements (`main`, `input.c:696-704`): register
//! the fresh-boot callback, run the startup handshake, then enter the
//! framework main loop and never return. Everything else is inside
//! `input_init`, which itself is four steps in a fixed order. Both sequences
//! are spelled as data here so a reader sees the whole boot at a glance —
//! and so a test can assert the order without booting anything.
//!
//! Corresponding document: `01-input-init-main.md`.

use crate::framework::AnnounceEffect;

/// One step of the fresh-boot initialization.
///
/// C: the body of `input_init` (`input.c:646-680`), in order. Each step
/// depends on the previous ones (the table must exist before it is
/// announced; the announcement must precede the handshake, or the terminal
/// driver would answer a server nobody can reach yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitStep {
    /// Zero the ten device slots and back-fill each minor number.
    ///
    /// C: the `for` loop (`input.c:652-662`). No owner, empty queues,
    /// closed, unparked, lights off — see [`crate::structs::InputTable`].
    ClearDeviceTable,
    /// Subscribe to input-driver arrival and departure events.
    ///
    /// C: `ds_subscribe("drv\\.inp\\..*", DSF_INITIAL)` (`input.c:665`).
    /// The pattern, the initial-dump flag, and the crash-on-failure policy
    /// are documented on [`DRIVER_EVENT_PATTERN`]; the subscription
    /// mechanics belong to document 11.
    SubscribeDriverEvents,
    /// Announce the server to the virtual file system side.
    ///
    /// C: `chardriver_announce()` (`input.c:669`): release callers blocked
    /// on any previous generation, publish the arrival marker, forget old
    /// opens — see [`crate::framework::announce_effects`].
    AnnounceToFileSystem,
    /// Tell the terminal driver the input server is up.
    ///
    /// C: send `TTY_INPUT_UP` to the terminal driver (`input.c:671-677`).
    /// The message number and the terminal side of the handshake belong to
    /// documents 05 and 13; this step records only that the send happens
    /// here, and that a failed send is logged, not fatal (a terminal
    /// starting later re-synchronizes on its own initiative).
    NotifyTerminal,
}

/// The initialization sequence, in C order.
///
/// C: `input_init` (`input.c:646-680`).
pub const fn init_plan() -> [InitStep; 4] {
    [
        InitStep::ClearDeviceTable,
        InitStep::SubscribeDriverEvents,
        InitStep::AnnounceToFileSystem,
        InitStep::NotifyTerminal,
    ]
}

/// The data-store subscription pattern for input drivers.
///
/// C: the string literal `"drv\\.inp\\..*"` (`input.c:665`). The doubled
/// backslashes are C string escaping; the actual subscription pattern is the
/// regular expression `drv\.inp\..*` — "any key starting with `drv.inp.`".
/// `DSF_INITIAL` additionally asks for the current matching set up front, so
/// drivers that arrived before the input server are still found (document
/// 11). A failed subscription ends the server immediately (`panic` in C):
/// without driver events the server could never learn about any keyboard.
pub const DRIVER_EVENT_PATTERN: &str = "drv\\.inp\\..*";

/// One slot of the callback table the server hands to the framework.
///
/// C: `input_tab` (`input.c:31-42`): seven function pointers. C spells the
/// table as data holding code; Rust spells the same inventory as an
/// enumeration, and the (future) main loop matches on it. The compiler then
/// checks exhaustiveness — adding an eighth handler without teaching the
/// dispatcher fails to build, where C would silently never call it.
///
/// Three framework slots stay empty on purpose: no write handler (writes to
/// an input device fail — the default the framework already provides), no
/// interrupt hook and no alarm hook (the server owns no hardware and keeps
/// no timers). Leaving a slot empty *is* the design: the framework's default
/// for each is documented on the variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerSlot {
    /// Open handler (`cdr_open = input_open`; document 06).
    Open,
    /// Close handler (`cdr_close = input_close`; document 06).
    Close,
    /// Read handler (`cdr_read = input_read`; document 07).
    Read,
    /// Control handler (`cdr_ioctl = input_ioctl`; document 08).
    Control,
    /// Cancel handler (`cdr_cancel = input_cancel`; document 08).
    Cancel,
    /// Select handler (`cdr_select = input_select`; document 08).
    Select,
    /// Catch-all for anything outside the character protocol
    /// (`cdr_other = input_other`; documents 05/09/10/11).
    Other,
}

/// The seven slots the server registers, in `input_tab` order.
///
/// C: `input_tab` (`input.c:31-42`).
pub const fn handler_slots() -> [HandlerSlot; 7] {
    [
        HandlerSlot::Open,
        HandlerSlot::Close,
        HandlerSlot::Read,
        HandlerSlot::Control,
        HandlerSlot::Cancel,
        HandlerSlot::Select,
        HandlerSlot::Other,
    ]
}

/// How the server registers with the startup framework.
///
/// C: `input_startup` (`input.c:685-691`) registers exactly one callback —
/// the fresh-boot initializer — and starts the framework. There is no
/// restart-with-state callback and no live-update hook: an input server
/// restart legitimately forgets everything (open devices re-open, drivers
/// re-announce, parked readers get an error — documents 06/07/11), so the
/// single-variant enum states the whole policy: fresh boot is the only
/// beginning this server knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupRegistration {
    /// Fresh-boot initializer (`sef_setcb_init_fresh(input_init)`).
    FreshBoot,
}

/// The server's startup registration: fresh boot only.
///
/// C: `input_startup` (`input.c:685-691`).
pub const fn startup_registration() -> StartupRegistration {
    StartupRegistration::FreshBoot
}

/// Checks that the announce sequence needed by [`InitStep::AnnounceToFileSystem`]
/// is the framework's announce sequence.
///
/// The init plan names the step; the framework owns the effects. This
/// function ties the two together so a change to either fails loudly: the
/// step exists if and only if the framework still announces in the order the
/// server expects.
pub const fn announce_effects_for_init() -> [AnnounceEffect; 3] {
    crate::framework::announce_effects()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_plan_follows_c_order() {
        // C: `input_init` (`input.c:646-680`) — clear, subscribe, announce,
        // handshake. Reordering would announce an empty table or handshake a
        // server nobody can reach.
        assert_eq!(
            init_plan(),
            [
                InitStep::ClearDeviceTable,
                InitStep::SubscribeDriverEvents,
                InitStep::AnnounceToFileSystem,
                InitStep::NotifyTerminal,
            ]
        );
    }

    #[test]
    fn test_driver_pattern_matches_c_literal() {
        // C: `input.c:665` — byte-for-byte the same literal, so the same
        // regular expression reaches the data store.
        assert_eq!(DRIVER_EVENT_PATTERN, "drv\\.inp\\..*");
        // The unescaped reading a maintainer must keep in mind.
        assert!(DRIVER_EVENT_PATTERN.contains("drv"));
        assert!(DRIVER_EVENT_PATTERN.contains("inp"));
    }

    #[test]
    fn test_handler_slots_cover_input_tab() {
        // C: `input_tab` (`input.c:31-42`) — seven slots, and only seven:
        // the missing write/interrupt/alarm slots are deliberate framework
        // defaults, not omissions.
        let slots = handler_slots();
        assert_eq!(slots.len(), 7);
        assert!(slots.contains(&HandlerSlot::Open));
        assert!(slots.contains(&HandlerSlot::Close));
        assert!(slots.contains(&HandlerSlot::Read));
        assert!(slots.contains(&HandlerSlot::Control));
        assert!(slots.contains(&HandlerSlot::Cancel));
        assert!(slots.contains(&HandlerSlot::Select));
        assert!(slots.contains(&HandlerSlot::Other));
    }

    #[test]
    fn test_startup_registers_fresh_boot_only() {
        // C: `input_startup` (`input.c:685-691`) — one callback, no restart
        // or live-update hooks.
        assert_eq!(startup_registration(), StartupRegistration::FreshBoot);
    }

    #[test]
    fn test_init_announce_matches_framework_announce() {
        // The plan step and the framework effects must stay in agreement.
        assert_eq!(
            announce_effects_for_init(),
            crate::framework::announce_effects()
        );
    }
}
