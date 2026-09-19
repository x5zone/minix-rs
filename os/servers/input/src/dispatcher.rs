//! The dispatcher: one arrival in — state changes and effects out.
//!
//! C: the loop body of `chardriver_task` plus `chardriver_process`
//! (`chardriver.c:455-573`) plus `input_other` (`input.c:608-641`), rendered
//! as pure decisions. The transport (a separate landing, edge todo
//! E-INWIRE) decodes raw messages into [`Arrival`] values — using the
//! framework's [`crate::framework::classify_request`] and the minix-types
//! decoders — calls [`handle_arrival`], performs the returned effects, and
//! completes any [`GrantCopy`] through [`complete_grant_copy`]. Nothing
//! here sends, receives, or copies: like every decision in this crate, the
//! whole dispatch is testable without a transport.
//!
//! Two arrivals deserve their naming explained. The data-store notification
//! ([`Arrival::DriverStoreChanged`]) is only a *prompt*: C's `input_check`
//! (`input.c:558-603`) then drives the store itself, so the Rust transport
//! drives the two-phase helpers instead ([`Server::driver_connect`] per
//! drained key, [`Server::departure_candidates`] plus
//! [`Server::driver_departed`] for the gone-away scan). And the light
//! request checks its sender against the terminal endpoint
//! ([`minix_types::Endpoint::TTY`]) because C does (`input.c:630-635`):
//! light commands from anyone else fall through to the unexpected-message
//! log, which is the fail-closed reading (architecture note A-10).
//!
//! Corresponding documents: `01-input-init-main.md` (the loop this serves),
//! `02-chardriver-framework.md` (gate and reply discipline),
//! `06`-`08-input-*.md` (request decisions), `09-input-event-processing.md`
//! (event intake), `10-input-setleds.md` and `11-input-driver-connect.md`
//! (the store-driven halves).

use crate::connect::{connect_driver, disconnect_device, labels_match, wants_from_typemask};
use crate::effects::{Effect, ReplyValue};
use crate::error::InputError;
use crate::eventbuf::{commit_read_copy, plan_read_copy, ReadCopyPlan};
use minix_chardriver::driver::{gate_character_request, GateVerdict};
use minix_chardriver::protocol::{CdevRequest, OpenDeviceSet};
use crate::handlers::{
    apply_close, apply_open, apply_select_record, cancel_parked_read, decide_close, decide_ioctl,
    decide_open, decide_read, decide_select, led_mask_from_kio_bits, park_read, IoctlVerdict,
    ReadVerdict,
};
use crate::produce::{
    complete_answered_reader, forward_to_terminal, route_event, stored_event, wake_on_event,
    WakeAction,
};
use crate::setleds::apply_light_save;
use crate::structs::{
    map_minor_to_index, DeviceIndex, InputTable, Minor, KEYBOARD_MULTIPLEXER_MINOR,
};
use alloc::vec;
use alloc::vec::Vec;
use minix_types::Endpoint;

/// The server as one piece of state: the device table plus the restart gate's
/// already-opened set.
///
/// C: the file-static `devs` array (`input.c:22`) plus the framework's
/// `open_devs` (`chardriver.c:54-94`). Assembled here so a test can build
/// "the server" in one call, and so the future `main` has exactly one thing
/// to own ([`Server::fresh`] mirrors `input_init`'s clearing loop plus the
/// announce-time set clear, `input.c:652-662`).
pub struct Server {
    /// The ten device slots (document 03).
    pub table: InputTable,
    /// Minors opened since the last restart (document 02).
    pub opened: OpenDeviceSet,
    /// F-key observer registry (C `fkey_obs`/`sfkey_obs`,
    /// `keyboard.c:72-73`). Starts gated off — the `debug_fkeys` boot flag
    /// comes from the environment the server does not parse yet.
    pub fkeys: crate::fkey::FkeyTable,
}

impl Server {
    /// A freshly started server: every slot cleared, nothing recorded.
    pub fn fresh() -> Server {
        Server {
            table: InputTable::fresh(),
            opened: OpenDeviceSet::new(),
            fkeys: crate::fkey::FkeyTable::new(false),
        }
    }

    /// Phase one of the data-store arrival path: one drained key.
    ///
    /// C: `input_connect` (`input.c:475-528`). The transport supplies the
    /// key's label, the owner's label as the store reports it, the owner's
    /// endpoint, and the type mask. A label mismatch silently ignores the
    /// request (`input.c:488-495`); otherwise slots are allocated, the
    /// configuration reply goes out (allocation failures included —
    /// `input.c:500-523`), and a connected keyboard is relit from memory
    /// (`input.c:525-527`).
    pub fn driver_connect(
        &mut self,
        key_label: &[u8],
        store_label: &[u8],
        owner: Endpoint,
        typemask: u16,
    ) -> Vec<Effect> {
        if !labels_match(key_label, store_label) {
            return Vec::new();
        }
        let (wants_keyboard, wants_mouse) = wants_from_typemask(typemask);
        let report = connect_driver(&mut self.table, wants_keyboard, wants_mouse, owner, key_label);
        let mut effects = vec![Effect::input_conf(owner, &report)];
        if let Some((_minor, mask)) = report.restore_lights {
            // The mask *is* the slot's memory (it was read from there), so
            // only the send remains; the slot is owned now, and C's
            // broadcast-for-that-minor reduces to this one owner
            // (input.c:525-527 vs the general loop at :221-235).
            effects.push(Effect::setleds(owner, mask));
        }
        effects
    }

    /// Phase two of the data-store path: which owned slots to re-verify.
    ///
    /// C: the second loop of `input_check` (`input.c:588-602`) walks every
    /// owned slot and asks the store whether its label still resolves. The
    /// transport resolves each candidate's `label_bytes`; `ESRCH` means the
    /// driver is gone ([`Server::driver_departed`]).
    pub fn departure_candidates(&self) -> Vec<DeviceIndex> {
        self.table
            .devices
            .iter()
            .enumerate()
            .filter(|(_, device)| device.owner != Endpoint::NONE)
            .map(|(index, _)| DeviceIndex(index))
            .collect()
    }

    /// Applies one departure: the store answered that the label is gone.
    ///
    /// C: `input.c:597-598` routing into `input_disconnect`
    /// (`input.c:533-553`) — the parked reader is answered with an
    /// input/output error, the recorded selector is notified readable, and
    /// the slot is freed (its queue, opened flag, label, and light memory
    /// deliberately stay).
    pub fn driver_departed(&mut self, index: DeviceIndex) -> Vec<Effect> {
        let effects = disconnect_device(&mut self.table.devices[index.0]);
        let mut out = Vec::new();
        if let Some((caller, request_id)) = effects.answer_reader {
            out.push(Effect::reply_error(caller, request_id, InputError::InputOutput));
        }
        if let Some((selector, minor)) = effects.notify_selector {
            out.push(Effect::ReplySelect { selector, minor });
        }
        out
    }
}

/// One arrival, already decoded by the transport.
///
/// The transport classifies (framework `classify_request`, notify-by-sender,
/// the minix-types decoders) and hands over a value that carries everything
/// the decision needs — including data that in C arrives through a grant
/// copy (the ioctl light bits, `input.c:258`; copying them is the
/// transport's job, done once the request number is known).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// A data-store notification (`input.c:613-615`). Only a prompt: the
    /// transport then drives [`Server::driver_connect`] per drained key and
    /// the departure scan ([`Server::departure_candidates`],
    /// [`Server::driver_departed`]). The arrival itself produces nothing.
    DriverStoreChanged,
    /// A driver report (`INPUT_EVENT`, decoded).
    DriverReport {
        /// The sending driver.
        source: Endpoint,
        /// The report's slot number (array index, not minor — A-3).
        id: i32,
        /// The four report lanes.
        page: i32,
        code: i32,
        value: i32,
        flags: i32,
    },
    /// A light request decoded from `INPUT_SETLEDS`. The source check
    /// happens here, not in the transport: accepting or ignoring a sender
    /// is a decision (A-10), not mechanics.
    TerminalSetleds { source: Endpoint, mask: u32 },
    /// The IS debug-dump registry request (`TTY_FKEY_CONTROL`,
    /// `com.h:874`, decoded from `m_lsys_tty_fkey_ctl`).
    FkeyControl {
        /// Who asked (the IS server).
        source: Endpoint,
        /// `FKEY_MAP`/`FKEY_UNMAP`/`FKEY_EVENTS`.
        request: i32,
        /// F1..F12 bitmap, bits 1..=12 (bit 0 unused).
        fkeys: i32,
        /// Shift F1..F12 bitmap.
        sfkeys: i32,
    },
    /// A character-device request for one minor.
    Request(CdevCall),
}

/// One character-device request, with every field its decisions need.
///
/// `request_id` is the id the wire carries (zero on the id-less requests —
/// C zeroes its messages before use, so an unset id echoes as zero). For
/// [`CdevCall::Ioctl`], `kl_bits` are the caller's light bits the transport
/// copied after seeing the request number was `KIOCSLEDS` (C copies inside
/// the case, `input.c:257-260`; copying them unconditionally beforehand
/// reads caller memory no differently).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdevCall {
    /// Open a minor (`input_open`, document 06).
    Open {
        minor: Minor,
        access: i32,
        user: i64,
        caller: Endpoint,
        request_id: u32,
    },
    /// Close a minor (document 06).
    Close {
        minor: Minor,
        caller: Endpoint,
        request_id: u32,
    },
    /// Read a minor (document 07). `size` is the caller buffer's byte size;
    /// `grant` is where served events go.
    Read {
        minor: Minor,
        caller: Endpoint,
        grant: i32,
        size: usize,
        nonblocking: bool,
        request_id: u32,
    },
    /// Control a minor (document 08) — only ever `KIOCSLEDS` in effect.
    Ioctl {
        minor: Minor,
        request: u32,
        kl_bits: u32,
        caller: Endpoint,
        request_id: u32,
    },
    /// Cancel a parked read (document 08).
    Cancel {
        minor: Minor,
        caller: Endpoint,
        request_id: u32,
    },
    /// Query a minor (document 08).
    Select {
        minor: Minor,
        ops: i32,
        caller: Endpoint,
    },
}

impl CdevCall {
    /// The framework's name for this request (drives the restart gate).
    pub fn request(&self) -> CdevRequest {
        match self {
            CdevCall::Open { .. } => CdevRequest::Open,
            CdevCall::Close { .. } => CdevRequest::Close,
            CdevCall::Read { .. } => CdevRequest::Read,
            CdevCall::Ioctl { .. } => CdevRequest::Ioctl,
            CdevCall::Cancel { .. } => CdevRequest::Cancel,
            CdevCall::Select { .. } => CdevRequest::Select,
        }
    }

    /// Who asked (the reply destination).
    pub fn caller(&self) -> Endpoint {
        match self {
            CdevCall::Open { caller, .. }
            | CdevCall::Close { caller, .. }
            | CdevCall::Read { caller, .. }
            | CdevCall::Ioctl { caller, .. }
            | CdevCall::Cancel { caller, .. }
            | CdevCall::Select { caller, .. } => *caller,
        }
    }

    /// The id the reply must echo.
    pub fn request_id(&self) -> u32 {
        match self {
            CdevCall::Open { request_id, .. }
            | CdevCall::Close { request_id, .. }
            | CdevCall::Read { request_id, .. }
            | CdevCall::Ioctl { request_id, .. }
            | CdevCall::Cancel { request_id, .. } => *request_id,
            CdevCall::Select { .. } => 0,
        }
    }

    /// The addressed minor.
    pub fn minor(&self) -> Minor {
        match self {
            CdevCall::Open { minor, .. }
            | CdevCall::Close { minor, .. }
            | CdevCall::Read { minor, .. }
            | CdevCall::Ioctl { minor, .. }
            | CdevCall::Cancel { minor, .. }
            | CdevCall::Select { minor, .. } => *minor,
        }
    }
}

/// What one arrival produced.
///
/// Most arrivals finish immediately: state is final and the effects are
/// ready to perform. A read with data ([`Outcome::GrantCopy`]) stops half
/// way — its grant copy stands between the decision and the commit (C does
/// the copy inside the handler, `input.c:144-151`; the transport performs
/// it, then [`complete_grant_copy`] finishes the bookkeeping either way).
#[derive(Debug)]
pub enum Outcome {
    /// State is final; perform the effects in order.
    Done(Vec<Effect>),
    /// Copy the planned events through the grant, then complete. The
    /// leading effects run before the copy (an F-key trap on the same
    /// press must notify before the reader's answer matters).
    GrantCopy {
        /// The copy plan itself.
        copy: GrantCopy,
        /// Effects that precede the copy.
        leading: Vec<Effect>,
    },
}

/// A grant copy standing between decision and completion.
#[derive(Debug, Clone, Copy)]
pub struct GrantCopy {
    /// The slot whose queue the events come from.
    pub slot: DeviceIndex,
    /// The planned events: segments to move, byte answer on success.
    pub plan: ReadCopyPlan,
    /// The reader's grant (where the events go).
    pub grant: i32,
    /// The reply destination.
    pub caller: Endpoint,
    /// The request id the reply must echo.
    pub request_id: u32,
    /// Whether completion also unparks the reader (a woken parked read:
    /// `input.c:365` clears `suspended` unconditionally). A served read was
    /// never parked.
    pub unpark: bool,
}

/// Dispatches one arrival.
pub fn handle_arrival(server: &mut Server, arrival: Arrival) -> Outcome {
    match arrival {
        Arrival::DriverStoreChanged => Outcome::Done(Vec::new()),
        Arrival::DriverReport {
            source,
            id,
            page,
            code,
            value,
            flags,
        } => handle_report(server, source, id, page, code, value, flags),
        Arrival::TerminalSetleds { source, mask } => {
            Outcome::Done(terminal_setleds(server, source, mask))
        }
        Arrival::FkeyControl {
            source,
            request,
            fkeys,
            sfkeys,
        } => Outcome::Done(fkey_control(server, source, request, fkeys, sfkeys)),
        Arrival::Request(call) => handle_request(server, call),
    }
}

/// Completes a grant copy after the transport attempted it.
///
/// Both kinds share the same tail: commit only on success (the events stay
/// buffered otherwise, `input.c:144-151`), then answer the reader in bytes
/// or the error. The woken-reader kind additionally unparks —
/// unconditionally, C's `input.c:365` — so a failed copy still ends the
/// wait.
pub fn complete_grant_copy(
    server: &mut Server,
    copy: GrantCopy,
    transported: Result<(), InputError>,
) -> Vec<Effect> {
    let device = &mut server.table.devices[copy.slot.0];
    let outcome = if copy.unpark {
        complete_answered_reader(device, copy.plan, transported)
    } else {
        match transported {
            Ok(()) => {
                let bytes = copy.plan.bytes;
                commit_read_copy(device, copy.plan);
                Ok(bytes)
            }
            Err(error) => Err(error),
        }
    };
    vec![Effect::reply_outcome(copy.caller, copy.request_id, outcome)]
}

/// Routes a driver report: file it (and wake whoever waits), or forward it.
fn handle_report(
    server: &mut Server,
    source: Endpoint,
    id: i32,
    page: i32,
    code: i32,
    value: i32,
    flags: i32,
) -> Outcome {
    // The F-key trap fires on key presses only (C: `if (scode &
    // RELEASE_BIT) return FALSE` — keyboard.c:539-541; `INPUT_PRESS = 1`).
    let mut fkey_notify = None;
    if value == 1
        && let Some(target) = server.fkeys.key_press(page, code)
    {
        fkey_notify = Some(target);
    }
    let mut leading = Vec::new();
    if let Some(target) = fkey_notify {
        leading.push(Effect::NotifyFkeyObserver { target });
    }
    let outcome = match route_event(&server.table, id, source) {
        crate::produce::EventIntake::Drop(_) => Outcome::Done(Vec::new()),
        crate::produce::EventIntake::ForwardToTerminal => Outcome::Done(vec![Effect::tty_event(
            forward_to_terminal(id, page, code, value, flags),
        )]),
        crate::produce::EventIntake::Deliver { target } => {
            let event = stored_event(id, page, code, value, flags);
            match wake_on_event(&mut server.table.devices[target.0], event) {
                WakeAction::AnswerReader {
                    caller,
                    request_id,
                    plan,
                } => Outcome::GrantCopy {
                    copy: GrantCopy {
                        slot: target,
                        plan,
                        grant: server.table.devices[target.0].grant,
                        caller,
                        request_id,
                        unpark: true,
                    },
                    leading: core::mem::take(&mut leading),
                },
                WakeAction::AnswerReaderFailed {
                    caller,
                    request_id,
                    error,
                } => Outcome::Done(vec![Effect::reply_error(caller, request_id, error)]),
                WakeAction::NotifySelector { selector, minor } => {
                    Outcome::Done(vec![Effect::ReplySelect { selector, minor }])
                }
                WakeAction::Nobody => Outcome::Done(Vec::new()),
            }
        }
    };
    // The F-key notification precedes whatever the report itself produced
    // (the IS dump runs before the reader's answer matters).
    match outcome {
        Outcome::Done(mut effects) => {
            let mut all = core::mem::take(&mut leading);
            all.append(&mut effects);
            Outcome::Done(all)
        }
        other => other,
    }
}

/// Applies a terminal light request, or ignores a non-terminal sender./// Runs the IS F-key registry request and shapes the reply effect.
///
/// The registry decision is all [`crate::fkey::FkeyTable::control`]; this
/// wrapper only carries its result into the reply effect (C `do_fkey_ctl`
/// answers via one sendnb with `m_type = result`,
/// `keyboard.c:523-526`).
fn fkey_control(
    server: &mut Server,
    source: Endpoint,
    request: i32,
    fkeys: i32,
    sfkeys: i32,
) -> Vec<Effect> {
    let (result, leftover_fkeys, leftover_sfkeys) =
        server
            .fkeys
            .control(source, request, fkeys, sfkeys);
    vec![Effect::FkeyControlReply {
        caller: source,
        result,
        fkeys: leftover_fkeys,
        sfkeys: leftover_sfkeys,
    }]
}


///
/// C: `input.c:630-635` accepts `INPUT_SETLEDS` only from the terminal and
/// falls through to the unexpected-message log otherwise (A-10); then
/// `input_set_leds` (`input.c:204-240`) saves the mask on every addressed
/// keyboard slot and sends only to the owned ones.
fn terminal_setleds(server: &mut Server, source: Endpoint, mask: u32) -> Vec<Effect> {
    if source != Endpoint::TTY {
        return Vec::new();
    }
    let minor = Minor(KEYBOARD_MULTIPLEXER_MINOR);
    let targets = crate::setleds::plan_light_targets(&server.table, minor);
    let mut effects = Vec::new();
    for target in targets.into_iter().flatten() {
        apply_light_save(&mut server.table.devices[target.index.0], mask);
        if target.owned {
            effects.push(Effect::setleds(
                server.table.devices[target.index.0].owner,
                mask,
            ));
        }
    }
    effects
}

/// Dispatches one character-device request.
fn handle_request(server: &mut Server, call: CdevCall) -> Outcome {
    let slot = match map_minor_to_index(call.minor()) {
        Some(slot) => slot,
        None => {
            return Outcome::Done(vec![Effect::reply_error(
                call.caller(),
                call.request_id(),
                InputError::UnknownMinor,
            )])
        }
    };
    // The restart gate (chardriver.c:503-513): unrecorded minors serve only
    // an open; everything else for them is a stale request from before the
    // restart, dropped without a reply.
    let verdict = gate_character_request(call.request(), server.opened.contains_raw(call.minor().0 as u32));
    match verdict {
        GateVerdict::DropAsStale => return Outcome::Done(Vec::new()),
        GateVerdict::RecordAndServe => {
            if !server.opened.insert_raw(call.minor().0 as u32) {
                // The 256-entry set is full (unreachable in practice for
                // ten devices, but the framework's own note, framework.rs
                // record(), demands an error rather than an untracked
                // serve). "Try again" is the honest answer: the restart
                // that clears the set makes room.
                return Outcome::Done(vec![Effect::reply_error(
                    call.caller(),
                    call.request_id(),
                    InputError::WouldBlock,
                )]);
            }
        }
        GateVerdict::Serve => {}
    }
    let device = &mut server.table.devices[slot.0];
    match call {
        CdevCall::Open { .. } => match decide_open(device) {
            Ok(()) => {
                apply_open(device);
                Outcome::Done(vec![Effect::reply_ok(call.caller(), call.request_id())])
            }
            Err(error) => Outcome::Done(vec![Effect::reply_error(
                call.caller(),
                call.request_id(),
                error,
            )]),
        },
        CdevCall::Close { .. } => match decide_close(device) {
            Ok(()) => {
                apply_close(device);
                Outcome::Done(vec![Effect::reply_ok(call.caller(), call.request_id())])
            }
            Err(error) => Outcome::Done(vec![Effect::reply_error(
                call.caller(),
                call.request_id(),
                error,
            )]),
        },
        CdevCall::Read {
            caller,
            grant,
            size,
            nonblocking,
            request_id,
            ..
        } => match decide_read(device, size, nonblocking) {
            ReadVerdict::Refuse(error) => {
                Outcome::Done(vec![Effect::reply_error(caller, request_id, error)])
            }
            ReadVerdict::Park => {
                park_read(device, caller, grant, request_id);
                // No reply: the wake-up answers later (EDONTREPLY).
                Outcome::Done(Vec::new())
            }
            ReadVerdict::Serve { event_count } => match plan_read_copy(device, event_count) {
                Ok(plan) => Outcome::GrantCopy {
                    copy: GrantCopy {
                        slot,
                        plan,
                        grant,
                        caller,
                        request_id,
                        unpark: false,
                    },
                    leading: Vec::new(),
                },
                Err(error) => Outcome::Done(vec![Effect::reply_error(caller, request_id, error)]),
            },
        },
        CdevCall::Ioctl {
            request,
            kl_bits,
            caller,
            request_id,
            ..
        } => match decide_ioctl(device, request) {
            IoctlVerdict::Refuse(error) => {
                Outcome::Done(vec![Effect::reply_error(caller, request_id, error)])
            }
            IoctlVerdict::SetLeds => {
                let mask = led_mask_from_kio_bits(kl_bits);
                let minor = call.minor();
                let mut effects = broadcast_lights(server, minor, mask);
                effects.push(Effect::reply_ok(caller, request_id));
                Outcome::Done(effects)
            }
        },
        CdevCall::Cancel { caller, request_id, .. } => {
            match cancel_parked_read(device, caller, request_id) {
                Some(read) => Outcome::Done(vec![Effect::reply_interrupted(read)]),
                None => Outcome::Done(Vec::new()),
            }
        }
        CdevCall::Select { ops, caller, .. } => {
            let outcome = decide_select(device, ops);
            if outcome.record_selector {
                apply_select_record(device, caller);
            }
            // The ready mask rides in the reply's status lane — C's
            // `do_select` returns it the same way (input.c:325).
            Outcome::Done(vec![Effect::ReplyTask {
                caller,
                request_id: call.request_id(),
                value: ReplyValue::Code(outcome.ready_ops),
            }])
        }
    }
}

/// Saves and sends one light mask: `input_set_leds` as an effect list.
///
/// C: `input.c:204-240` — the mask is saved on every addressed keyboard
/// slot (owned or not: a driver arriving later must find the lights
/// waiting) and sent only to the owned ones. A mouse minor addresses
/// nothing, which is how C "discards" light requests aimed at mice
/// (`input.c:217-220`).
fn broadcast_lights(server: &mut Server, minor: Minor, mask: u32) -> Vec<Effect> {
    let targets = crate::setleds::plan_light_targets(&server.table, minor);
    let mut effects = Vec::new();
    for target in targets.into_iter().flatten() {
        apply_light_save(&mut server.table.devices[target.index.0], mask);
        if target.owned {
            effects.push(Effect::setleds(
                server.table.devices[target.index.0].owner,
                mask,
            ));
        }
    }
    effects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::produce::enqueue;
    use crate::structs::KEYBOARD_FIRST_MINOR;
    use minix_types::{
        decode_conf, decode_input_event, KBD_LEDS_NUM, KIOCSLEDS, INPUT_CONF, TTY_INPUT_EVENT,
    };

    const DRIVER: Endpoint = Endpoint(9);
    const READER: Endpoint = Endpoint(7);

    fn owned(slot: usize) -> Server {
        let mut server = Server::fresh();
        server.table.devices[slot].owner = DRIVER;
        server
    }

    /// A server whose slot has a driver and has been opened — the state a
    /// device reaches after VFS opens it (the gate only serves such minors).
    fn opened(slot: usize) -> Server {
        let mut server = owned(slot);
        let outcome = handle_arrival(
            &mut server,
            Arrival::Request(CdevCall::Open {
                minor: Minor(slot as i32),
                access: 0,
                user: 0,
                caller: READER,
                request_id: 0,
            }),
        );
        match outcome {
            Outcome::Done(effects) => assert_eq!(effects.len(), 1),
            _ => panic!("open must finish immediately"),
        }
        server
    }

    fn stored(server: &mut Server, slot: usize, code: i32) {
        enqueue(
            &mut server.table.devices[slot],
            stored_event(slot as i32, 7, code, 1, 0),
        );
    }

    fn read_call(minor: i32, size: usize) -> Arrival {
        Arrival::Request(CdevCall::Read {
            minor: Minor(minor),
            caller: READER,
            grant: 77,
            size,
            nonblocking: false,
            request_id: 5,
        })
    }

    #[test]
    fn test_open_records_and_answers_ok() {
        // C: input.c:85-102 + chardriver.c:503-513 — first open passes the
        // gate, records, and answers OK (the slot has a driver, so the
        // activity check passes).
        let mut server = owned(1);
        let outcome = handle_arrival(
            &mut server,
            Arrival::Request(CdevCall::Open {
                minor: Minor(1),
                access: 0,
                user: 0,
                caller: READER,
                request_id: 0,
            }),
        );
        match outcome {
            Outcome::Done(effects) => {
                assert_eq!(effects.len(), 1);
                match effects[0] {
                    Effect::ReplyTask {
                        caller,
                        request_id,
                        value: ReplyValue::Code(0),
                    } => {
                        assert_eq!((caller, request_id), (READER, 0));
                    }
                    other => panic!("expected ReplyTask OK, got {:?}", other),
                }
            }
            _ => panic!("expected Done"),
        }
        assert!(server.opened.contains_raw(1));
        assert!(server.table.devices[1].opened);
    }

    #[test]
    fn test_stale_read_before_any_open_is_dropped_silently() {
        // C: chardriver.c:503-513 — an unrecorded minor serves only an open.
        let mut server = Server::fresh();
        let outcome = handle_arrival(&mut server, read_call(1, 20));
        match outcome {
            Outcome::Done(effects) => assert!(effects.is_empty()),
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn test_unknown_minor_answers_enxio() {
        // C: input.c:60-61 + :90-91 — no such minor: ENXIO.
        let mut server = Server::fresh();
        let outcome = handle_arrival(
            &mut server,
            Arrival::Request(CdevCall::Close {
                minor: Minor(99),
                caller: READER,
                request_id: 3,
            }),
        );
        match outcome {
            Outcome::Done(effects) => match effects[0] {
                Effect::ReplyTask {
                    caller,
                    request_id,
                    value: ReplyValue::Code(code),
                } => {
                    assert_eq!((caller, request_id), (READER, 3));
                    assert_eq!(code, minix_types::ENXIO);
                }
                other => panic!("expected ReplyTask ENXIO, got {:?}", other),
            },
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn test_read_with_data_yields_grant_copy_then_commit_answers_bytes() {
        // C: input.c:162-198 — data buffered: clamp, copy (transport),
        // commit, answer in bytes.
        let mut server = opened(1);
        stored(&mut server, 1, 42);
        let outcome = handle_arrival(&mut server, read_call(1, 40));
        let copy = match outcome {
            Outcome::GrantCopy { copy, .. } => copy,
            _ => panic!("expected GrantCopy"),
        };
        assert_eq!(copy.slot, DeviceIndex(1));
        assert_eq!(copy.grant, 77);
        // The buffer holds one event; the plan clamps to it.
        assert_eq!(copy.plan.plan.event_total(), 1);
        let effects = complete_grant_copy(&mut server, copy, Ok(()));
        assert_eq!(effects.len(), 1);
        match effects[0] {
            Effect::ReplyTask {
                caller,
                request_id,
                value: ReplyValue::Bytes(bytes),
            } => {
                assert_eq!((caller, request_id), (READER, 5));
                assert_eq!(bytes, crate::eventbuf::ByteCount(20));
            }
            other => panic!("expected ReplyTask bytes, got {:?}", other),
        }
        assert_eq!(server.table.devices[1].count, 0);
    }

    #[test]
    fn test_read_on_empty_parks_then_wake_completes_the_roundtrip() {
        // C: input.c:182-193 (park, no reply) + input.c:361-365 (wake:
        // copy one, answer, unpark).
        let mut server = opened(1);
        let outcome = handle_arrival(&mut server, read_call(1, 20));
        match outcome {
            Outcome::Done(effects) => assert!(effects.is_empty()),
            _ => panic!("expected Done"),
        }
        assert!(server.table.devices[1].suspended);
        let outcome = handle_arrival(
            &mut server,
            Arrival::DriverReport {
                source: DRIVER,
                id: 1,
                page: 7,
                code: 42,
                value: 1,
                flags: 0,
            },
        );
        let copy = match outcome {
            Outcome::GrantCopy { copy, .. } => copy,
            _ => panic!("expected GrantCopy"),
        };
        assert!(copy.unpark);
        let effects = complete_grant_copy(&mut server, copy, Ok(()));
        assert_eq!(effects.len(), 1);
        assert!(!server.table.devices[1].suspended);
        assert_eq!(server.table.devices[1].count, 0);
    }

    #[test]
    fn test_unowned_report_is_forwarded_to_the_terminal() {
        // C: input.c:388-390 (the sender must own the slot) + :404-421 —
        // owned but unopened (device and mux): forward lanes intact.
        let mut server = owned(1);
        let outcome = handle_arrival(
            &mut server,
            Arrival::DriverReport {
                source: DRIVER,
                id: 1,
                page: 7,
                code: 42,
                value: 1,
                flags: 0,
            },
        );
        match outcome {
            Outcome::Done(effects) => {
                assert_eq!(effects.len(), 1);
                match effects[0] {
                    Effect::SendTerminalBlocking { message } => {
                        assert_eq!(message.m_type, TTY_INPUT_EVENT);
                        assert_eq!(decode_input_event(&message), None);
                        assert_eq!(
                            minix_types::decode_tty_event(&message),
                            Some((1, 7, 42, 1, 0))
                        );
                    }
                    _ => panic!("expected SendTerminalBlocking"),
                }
            }
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn test_terminal_setleds_requires_the_terminal_and_broadcasts() {
        // C: input.c:630-635 (source check) + :204-240 (save all, send owned).
        let mut server = owned(1);
        // A non-terminal sender is ignored, nothing saved.
        let effects = terminal_setleds(&mut server, Endpoint(42), 0b111);
        assert!(effects.is_empty());
        assert_eq!(server.table.devices[1].leds, 0);
        // The terminal's request saves on all four keyboards, sends to owned.
        let effects = terminal_setleds(&mut server, Endpoint::TTY, 0b111);
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], Effect::SendDriverAsync { to: DRIVER, .. }));
        for slot in 1..=4 {
            assert_eq!(server.table.devices[slot].leds, 0b111);
        }
    }

    #[test]
    fn test_ioctl_translates_bits_replies_ok_after_broadcast() {
        // C: input.c:241-277 — route, translate, broadcast, answer OK.
        let mut server = opened(1);
        let outcome = handle_arrival(
            &mut server,
            Arrival::Request(CdevCall::Ioctl {
                minor: Minor(1),
                request: KIOCSLEDS,
                kl_bits: KBD_LEDS_NUM,
                caller: READER,
                request_id: 8,
            }),
        );
        match outcome {
            Outcome::Done(effects) => {
                assert_eq!(effects.len(), 2);
                assert!(matches!(
                    effects[0],
                    Effect::SendDriverAsync { to: DRIVER, .. }
                ));
                match effects[1] {
                    Effect::ReplyTask {
                        caller,
                        request_id,
                        value: ReplyValue::Code(0),
                    } => assert_eq!((caller, request_id), (READER, 8)),
                    other => panic!("expected ReplyTask OK, got {:?}", other),
                }
            }
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn test_driver_connect_checks_labels_allocates_and_replies() {
        // C: input.c:488-527 — mismatch ignored silently; match allocates,
        // replies INPUT_CONF, and the keyboard slot is owned.
        let mut server = Server::fresh();
        let key = b"pckbd";
        let wrong = b"impostor";
        let effects = server.driver_connect(key, wrong, DRIVER, minix_types::INPUT_DEV_KBD);
        assert!(effects.is_empty());
        assert_eq!(server.table.devices[1].owner, Endpoint::NONE);
        let effects = server.driver_connect(key, key, DRIVER, minix_types::INPUT_DEV_KBD);
        assert_eq!(effects.len(), 2); // configuration reply + initial lights
        match effects[0] {
            Effect::SendDriverAsync { to, message } => {
                assert_eq!(to, DRIVER);
                assert_eq!(message.m_type, INPUT_CONF);
                assert_eq!(decode_conf(&message), Some((1, minix_types::INVALID_INPUT_ID)));
            }
            _ => panic!("expected SendDriverAsync"),
        }
        assert_eq!(server.table.devices[1].owner, DRIVER);
    }

    #[test]
    fn test_driver_departure_answers_waiters_and_frees_the_slot() {
        // C: input.c:533-553 + :588-602 — ESRCH scan finds the dead label;
        // the reader gets EIO, the selector a readable nudge, the slot frees.
        let mut server = owned(1);
        server.table.devices[1].suspended = true;
        server.table.devices[1].caller = READER;
        server.table.devices[1].request_id = 5;
        server.table.devices[1].selector = Endpoint(11);
        let candidates = server.departure_candidates();
        assert_eq!(candidates, vec![DeviceIndex(1)]);
        let effects = server.driver_departed(DeviceIndex(1));
        assert_eq!(effects.len(), 2);
        match effects[0] {
            Effect::ReplyTask {
                caller,
                request_id,
                value: ReplyValue::Code(code),
            } => {
                assert_eq!((caller, request_id), (READER, 5));
                assert_eq!(code, minix_types::EIO);
            }
            other => panic!("expected ReplyTask EIO, got {:?}", other),
        }
        assert!(matches!(
            effects[1],
            Effect::ReplySelect { selector: Endpoint(11), .. }
        ));
        assert_eq!(server.table.devices[1].owner, Endpoint::NONE);
    }

    #[test]
    fn test_cancel_through_the_dispatcher_answers_eintr() {
        // C: input.c:282-298 + chardriver.c:255-261.
        let mut server = opened(1);
        park_read(&mut server.table.devices[1], READER, 77, 5);
        let outcome = handle_arrival(
            &mut server,
            Arrival::Request(CdevCall::Cancel {
                minor: Minor(KEYBOARD_FIRST_MINOR),
                caller: READER,
                request_id: 5,
            }),
        );
        match outcome {
            Outcome::Done(effects) => match effects[0] {
                Effect::ReplyTask {
                    caller,
                    request_id,
                    value: ReplyValue::Code(code),
                } => {
                    assert_eq!((caller, request_id), (READER, 5));
                    assert_eq!(code, minix_types::EINTR);
                }
                other => panic!("expected ReplyTask EINTR, got {:?}", other),
            },
            _ => panic!("expected Done"),
        }
        assert!(!server.table.devices[1].suspended);
    }
}
