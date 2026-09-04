//! Input-driver client library: the driver side of the input protocol.
//!
//! C: `minix3/minix/lib/libinputdriver/inputdriver.c` (206 lines) plus
//! `minix3/minix/include/minix/inputdriver.h` (callback table + prototypes).
//! A keyboard or mouse driver links this library instead of speaking the
//! protocol by hand: announce on boot, file events as they happen, accept
//! configurations and light requests from the server, and run the main loop.
//!
//! Entry boundary (same split as `devman_client`): kernel crossings (label
//! lookup, data-store publish, blocking send, grant handling) are transport
//! and stay out until the IPC transport lands. This module starts from
//! "transport ready": key building, send/drop decisions, sender checks,
//! slot bookkeeping, and message classification — every branch testable
//! without a server on the other end. All C `panic`s become explicit
//! outcomes ([ARCH:A-7], same rule as `devman_client`); drivers decide
//! fail-fast, the library doesn't.
//!
//! Corresponding document: `12-libinputdriver.md`.

use alloc::string::String;
use minix_types::{
    Clock, DRIVER_KEY_PREFIX, Endpoint, INPUT_CONF, INPUT_DEV_KBD, INPUT_DEV_MOUSE, INPUT_SETLEDS,
    INVALID_INPUT_ID, TTY_RQ_BASE,
};

// ── Registration state ──

/// What a driver remembers about the server across calls.
///
/// C: the three file-static variables (`inputdriver.c:11-13`):
/// `input_endpt` (who to send to, `NONE` until configured), `kbd_id` and
/// `mouse_id` (assigned slots, `INVALID_INPUT_ID` until configured).
/// Grouped so tests can hold two drivers' states side by side — C's
/// statics allow exactly one driver per process, and the struct makes that
/// limit visible instead of ambient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverRegistration {
    /// The input server endpoint, or `None` until configured.
    pub server: Option<Endpoint>,
    /// Assigned keyboard slot, or `None` until configured.
    pub keyboard_slot: Option<i32>,
    /// Assigned mouse slot, or `None` until configured.
    pub mouse_slot: Option<i32>,
}

impl DriverRegistration {
    /// Fresh driver: no server, no slots (C statics' initial values).
    pub const fn new() -> Self {
        Self {
            server: None,
            keyboard_slot: None,
            mouse_slot: None,
        }
    }

    /// Whether the driver may send events (configured at least minimally).
    ///
    /// C sends when `input_endpt != NONE` *and* the kind's id is assigned
    /// (`inputdriver.c:49-54`); the per-kind half lives in
    /// [`decide_report`], this is the cheap pre-check.
    pub const fn is_connected(self) -> bool {
        self.server.is_some()
    }

    /// Records a configuration: server endpoint plus both slot assignments.
    ///
    /// C: `do_conf` (`inputdriver.c:103-106`) stores all three unconditionally
    /// — including invalid ids, which disable that kind ("no IDs given,
    /// driver disabled", `inputdriver.c:108-110`). `None` encodes the
    /// invalid id; storing it (rather than refusing) is faithful: the
    /// driver stays alive but silent for the unassigned kind.
    pub fn apply_conf(&mut self, server: Endpoint, keyboard_slot: i32, mouse_slot: i32) {
        self.server = Some(server);
        self.keyboard_slot = slot_or_none(keyboard_slot);
        self.mouse_slot = slot_or_none(mouse_slot);
    }

    /// Whether both kinds came back unassigned (a disabled driver).
    ///
    /// C logs "no IDs given, driver disabled" in exactly this case.
    pub const fn is_disabled(self) -> bool {
        self.keyboard_slot.is_none() && self.mouse_slot.is_none()
    }

    /// Forgets the server after a failed send; keeps the slots.
    ///
    /// C: a failed blocking send resets `input_endpt` to `NONE`
    /// (`inputdriver.c:72-73`) — but leaves the ids alone, so the next
    /// configuration restores full state. Sending stops (guarded by
    /// `is_connected`) until the server reconfigures us.
    pub fn note_server_lost(&mut self) {
        self.server = None;
    }
}

impl Default for DriverRegistration {
    /// Fresh driver (same as [`DriverRegistration::new`]).
    fn default() -> Self {
        Self::new()
    }
}

/// Converts a wire slot id to an `Option`: the invalid id means "none".
const fn slot_or_none(slot: i32) -> Option<i32> {
    if slot == INVALID_INPUT_ID {
        None
    } else {
        Some(slot)
    }
}

// ── Announce ──

/// Builds the data-store key a driver publishes on boot.
///
/// C: `inputdriver_announce` formats `"drv.inp.<label>"`
/// (`inputdriver.c:23,32`). The label lookup (transport) and the publish
/// (transport, panics on failure — `inputdriver.c:29-34`) stay out; the key
/// spelling — the part both sides must agree on — is here, built from the
/// shared [`DRIVER_KEY_PREFIX`].
pub fn announce_key(label: &str) -> String {
    let mut key = String::from(DRIVER_KEY_PREFIX);
    key.push_str(label);
    key
}

/// Assembles the announced device-type mask from two booleans.
///
/// C passes one `type` argument combining `INPUT_DEV_KBD`/`INPUT_DEV_MOUSE`
/// (the pckbd driver sets both when its auxiliary port exists — document
/// 14). Two booleans instead of a raw mask: callers cannot set reserved
/// bits by accident.
pub const fn announce_type(is_keyboard: bool, is_mouse: bool) -> u16 {
    (if is_keyboard { INPUT_DEV_KBD } else { 0 }) | (if is_mouse { INPUT_DEV_MOUSE } else { 0 })
}

// ── Event reporting ──

/// Whether one event may be sent, and with which slot.
///
/// C: `inputdriver_send_event` (`inputdriver.c:49-54`) drops the event when
/// unconfigured (`input_endpt == NONE`) or when the kind is unassigned
/// (`id == INVALID_INPUT_ID`) — *before* touching the transport. The two
/// drops have different meanings: unconnected means "nobody listens yet"
/// (normal before the first configuration), unassigned means "the server
/// gave us no slot for this kind" (we are disabled for it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportVerdict {
    /// Send with this slot number.
    Send {
        /// The assigned table slot for the event's kind.
        slot: i32,
    },
    /// No server configured yet; drop quietly.
    DropUnconnected,
    /// Server configured, but this kind has no slot; drop quietly.
    DropUnassigned,
}

/// Decides one event report for the mouse (`true`) or keyboard kind.
pub const fn decide_report(registration: DriverRegistration, mouse: bool) -> ReportVerdict {
    if registration.server.is_none() {
        return ReportVerdict::DropUnconnected;
    }
    let slot = if mouse {
        registration.mouse_slot
    } else {
        registration.keyboard_slot
    };
    match slot {
        Some(id) => ReportVerdict::Send { slot: id },
        None => ReportVerdict::DropUnassigned,
    }
}

// ── Incoming configuration ──

/// Whether a configuration sender is accepted, and the outcome.
///
/// C: `do_conf` (`inputdriver.c:82-111`) looks the `"input"` label up
/// (transport — a failed lookup ignores the message) and compares it with
/// the sender: strangers are ignored with a log line, the server's message
/// is stored. The lookup itself stays out; the comparison and the
/// consequences are here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfOutcome {
    /// Sender matches the known server: store endpoint and both slots.
    Accept,
    /// Sender is not the server: ignore (log in transport).
    IgnoreForeign,
    /// The server label did not resolve: ignore (log in transport).
    IgnoreLookupFailed,
}

/// Checks a configuration sender against the looked-up server endpoint.
///
/// `looked_up` is the endpoint the `"input"` label resolved to (`None`
/// when the lookup failed); `sender` is the message source.
pub const fn verify_conf_sender(looked_up: Option<Endpoint>, sender: Endpoint) -> ConfOutcome {
    match looked_up {
        None => ConfOutcome::IgnoreLookupFailed,
        Some(expected) => {
            if expected.0 == sender.0 {
                ConfOutcome::Accept
            } else {
                ConfOutcome::IgnoreForeign
            }
        }
    }
}

// ── Incoming light requests ──

/// Whether a light request is accepted, and the mask to invoke.
///
/// C: `do_setleds` (`inputdriver.c:119-135`) ignores requests from anyone
/// but the configured server, then invokes the driver's light callback —
/// *if one is registered* (the `if (idp->idr_leds)` NULL check: a driver
/// without lights silently absorbs the request).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetledsOutcome {
    /// Invoke the light callback with this mask.
    Invoke {
        /// Light mask (`1 << code` per light).
        mask: u32,
    },
    /// Stranger or unconfigured: ignore (log in transport).
    Ignore,
}

/// Checks a light request: sender must be the configured server.
pub const fn accept_setleds(
    registration: DriverRegistration,
    sender: Endpoint,
    mask: u32,
) -> SetledsOutcome {
    match registration.server {
        Some(expected) => {
            if expected.0 == sender.0 {
                SetledsOutcome::Invoke { mask }
            } else {
                SetledsOutcome::Ignore
            }
        }
        None => SetledsOutcome::Ignore,
    }
}

// ── Driver hooks ──

/// Light callback: drive the indicators to a mask.
/// C: `idr_leds(unsigned int leds)` (`inputdriver.h:12`).
pub type LightCallback = fn(mask: u32);
/// Interrupt callback: hardware rang. C: `idr_intr(unsigned int mask)`.
pub type InterruptCallback = fn(mask: u32);
/// Alarm callback: clock tick. C: `idr_alarm(clock_t stamp)`
/// (`inputdriver.h:14`; `clock_t` is 64-bit — `minix_types::Clock`).
pub type AlarmCallback = fn(stamp: Clock);
/// Catch-all for anything else. C: `idr_other(message *, int ipc_status)`.
pub type OtherCallback = fn(message_type: i32, source: Endpoint);

/// The driver's four hooks.
///
/// C: `struct inputdriver` (`inputdriver.h:11-16`) holds four function
/// pointers and — notably — no init hook: birth (announce) is the
/// library's job, life (these four) is the driver's. Each hook is optional
/// (`None` = the C NULL: that notification kind is absorbed silently).
/// Plain `Option<fn>` fields, no trait: one driver per process means one
/// implementation, and a single-implementation trait would be decoration
/// (same rule as `devman_client`'s `Option<BindCallback>`). Hook presence
/// is tested with `is_some`/`is_none` — function addresses are never
/// compared (their uniqueness is not guaranteed).
#[derive(Debug, Clone, Copy, Default)]
pub struct DriverHooks {
    /// Light callback (`idr_leds`), if the driver has lights.
    pub on_lights: Option<LightCallback>,
    /// Interrupt callback (`idr_intr`), if the driver owns hardware.
    pub on_interrupt: Option<InterruptCallback>,
    /// Alarm callback (`idr_alarm`), if the driver keeps timers.
    pub on_alarm: Option<AlarmCallback>,
    /// Catch-all (`idr_other`), if the driver wants the rest.
    pub on_other: Option<OtherCallback>,
}

// ── Incoming classification ──

/// Where one arrival goes in a driver's main loop.
///
/// C: `inputdriver_process` (`inputdriver.c:141-172`): notifications sort
/// by sender first (hardware → interrupt hook, clock → alarm hook, anything
/// else → catch-all); plain messages sort by number (configure, set-lights,
/// else catch-all). All one-way — no branch ever replies (C: "All messages
/// in the input protocol are one-way, so we never send a reply",
/// `inputdriver.c:139`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverIncoming {
    /// Hardware notification → interrupt hook (absent hook: absorbed).
    HardwareInterrupt,
    /// Clock notification → alarm hook (absent hook: absorbed).
    ClockAlarm,
    /// Other notification → catch-all (absent hook: absorbed).
    OtherNotify,
    /// Configuration → sender check, then store.
    Configure,
    /// Light request → sender check, then callback.
    SetLights,
    /// Anything else → catch-all (absent hook: absorbed).
    OtherMessage,
}

/// Source kinds for a notification arrival.
///
/// The transport maps the sender endpoint to one of these (hardware task,
/// clock task, anything else — same split as the character framework's,
/// document 02); classification stays pure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    /// From the hardware task.
    Hardware,
    /// From the clock task.
    Clock,
    /// From anyone else.
    Other,
}

/// Classifies one arrival for the hook dispatcher.
///
/// Pure over (message number, notify-ness, notify kind). A hook being
/// absent never changes the classification — absence only decides whether
/// the call happens, at the call site.
pub const fn classify_incoming(
    message_type: i32,
    is_notify: bool,
    notify_kind: NotifyKind,
) -> DriverIncoming {
    if is_notify {
        return match notify_kind {
            NotifyKind::Hardware => DriverIncoming::HardwareInterrupt,
            NotifyKind::Clock => DriverIncoming::ClockAlarm,
            NotifyKind::Other => DriverIncoming::OtherNotify,
        };
    }
    match message_type {
        INPUT_CONF => DriverIncoming::Configure,
        INPUT_SETLEDS => DriverIncoming::SetLights,
        _ => DriverIncoming::OtherMessage,
    }
}

/// Whether the terminal handshake numbers belong to a different number
/// family (a driver must never see them).
///
/// The TTY numbers (`0x1302/0x1303`, base `TTY_RQ_BASE`) live far from the
/// input numbers (`0x1500/0x1580`); a driver dispatcher matching only
/// `INPUT_CONF`/`INPUT_SETLEDS` routes everything else to the catch-all by
/// construction. Pinned here so a future number collision fails loudly.
pub const fn tty_numbers_disjoint() -> bool {
    TTY_RQ_BASE + 3 < INPUT_CONF
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::INPUT_EVENT;

    fn configured() -> DriverRegistration {
        let mut reg = DriverRegistration::new();
        reg.apply_conf(Endpoint(6), 1, -1);
        reg
    }

    #[test]
    fn test_announce_spelling_matches_c() {
        // C: "drv.inp.<label>" (inputdriver.c:23,32), shared prefix.
        assert_eq!(announce_key("kbd0"), "drv.inp.kbd0");
        assert!(announce_key("kbd0").starts_with(DRIVER_KEY_PREFIX));
        assert_eq!(announce_type(true, false), INPUT_DEV_KBD);
        assert_eq!(announce_type(false, true), INPUT_DEV_MOUSE);
        assert_eq!(announce_type(true, true), INPUT_DEV_KBD | INPUT_DEV_MOUSE);
        assert_eq!(announce_type(false, false), 0);
    }

    #[test]
    fn test_registration_lifecycle_matches_c() {
        // C: statics start NONE/INVALID (inputdriver.c:11-13).
        let fresh = DriverRegistration::new();
        assert!(!fresh.is_connected());
        assert_eq!(decide_report(fresh, false), ReportVerdict::DropUnconnected);
        // Configuration stores endpoint and slots, invalid included.
        let reg = configured();
        assert!(reg.is_connected());
        assert!(!reg.is_disabled());
        assert_eq!(decide_report(reg, false), ReportVerdict::Send { slot: 1 });
        assert_eq!(decide_report(reg, true), ReportVerdict::DropUnassigned);
        // Both invalid: alive but disabled (C logs "driver disabled").
        let mut reg = DriverRegistration::new();
        reg.apply_conf(Endpoint(6), -1, -1);
        assert!(reg.is_connected());
        assert!(reg.is_disabled());
        // Failed send forgets the server, keeps the slots.
        reg.note_server_lost();
        assert!(!reg.is_connected());
        assert_eq!(decide_report(reg, false), ReportVerdict::DropUnconnected);
    }

    #[test]
    fn test_conf_sender_check_matches_c() {
        // C: do_conf (inputdriver.c:88-101) — lookup, compare, store/ignore.
        let server = Endpoint(6);
        assert_eq!(
            verify_conf_sender(Some(server), server),
            ConfOutcome::Accept
        );
        assert_eq!(
            verify_conf_sender(Some(server), Endpoint(9)),
            ConfOutcome::IgnoreForeign
        );
        assert_eq!(
            verify_conf_sender(None, server),
            ConfOutcome::IgnoreLookupFailed
        );
        // Accepted configuration stores (do_conf inputdriver.c:103-106).
        let mut reg = DriverRegistration::new();
        if verify_conf_sender(Some(server), server) == ConfOutcome::Accept {
            reg.apply_conf(server, 2, 7);
        }
        assert_eq!(decide_report(reg, false), ReportVerdict::Send { slot: 2 });
        assert_eq!(decide_report(reg, true), ReportVerdict::Send { slot: 7 });
    }

    #[test]
    fn test_setleds_source_check_matches_c() {
        // C: do_setleds (inputdriver.c:124-129) — strangers ignored.
        let reg = configured();
        assert_eq!(
            accept_setleds(reg, Endpoint(6), 0x6),
            SetledsOutcome::Invoke { mask: 0x6 }
        );
        assert_eq!(
            accept_setleds(reg, Endpoint(9), 0x6),
            SetledsOutcome::Ignore
        );
        assert_eq!(
            accept_setleds(DriverRegistration::new(), Endpoint(6), 0x6),
            SetledsOutcome::Ignore
        );
    }

    #[test]
    fn test_classify_routes_like_inputdriver_process() {
        // C: inputdriver.c:145-171 — notify by sender, messages by number.
        assert_eq!(
            classify_incoming(INPUT_EVENT, true, NotifyKind::Hardware),
            DriverIncoming::HardwareInterrupt
        );
        assert_eq!(
            classify_incoming(INPUT_EVENT, true, NotifyKind::Clock),
            DriverIncoming::ClockAlarm
        );
        assert_eq!(
            classify_incoming(INPUT_EVENT, true, NotifyKind::Other),
            DriverIncoming::OtherNotify
        );
        assert_eq!(
            classify_incoming(INPUT_CONF, false, NotifyKind::Other),
            DriverIncoming::Configure
        );
        assert_eq!(
            classify_incoming(INPUT_SETLEDS, false, NotifyKind::Other),
            DriverIncoming::SetLights
        );
        assert_eq!(
            classify_incoming(0x1234, false, NotifyKind::Other),
            DriverIncoming::OtherMessage
        );
        // A hook-less driver absorbs everything without replying (one-way).
        let hooks = DriverHooks::default();
        assert!(hooks.on_lights.is_none());
        assert!(hooks.on_other.is_none());
    }

    #[test]
    fn test_number_families_stay_disjoint() {
        assert!(tty_numbers_disjoint());
    }
}
