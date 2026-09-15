//! Event production: intake, enqueue, and wake-up decisions.
//!
//! C: `input_event` (`minix3/minix/servers/input/input.c:376-422`) and
//! `input_process` (`input.c:332-371`). A driver report arrives; the server
//! validates it, files it into the right queue, and wakes whoever waits —
//! or forwards it to the terminal driver when nobody holds the device.
//!
//! Like the handlers, this module splits decide from apply: routing and
//! wake-up choices are pure functions of the table state; the queue edit
//! and the transport replies happen in named apply steps. The terminal
//! forward is data, not transport: [`ForwardedEvent`] carries the five
//! lanes the sender transmits, built by [`forward_to_terminal`].
//!
//! Corresponding document: `09-input-event-processing.md`.

use crate::error::InputError;
use minix_types::InputEvent;
use crate::eventbuf::{ByteCount, EventCount, ReadCopyPlan, commit_read_copy, plan_read_copy};
use crate::structs::{
    DeviceIndex, InputDevice, InputTable, KEYBOARD_FIRST_MINOR, KEYBOARD_MINOR_COUNT,
    KEYBOARD_MULTIPLEXER_INDEX, MOUSE_MULTIPLEXER_INDEX, Minor,
};
use minix_types::Endpoint;

// ── Intake: route one driver report ──

/// Why a driver report was dropped.
///
/// C drops silently in both cases (`input.c:384-385`, `:389-390`): no log,
/// no reply — the protocol is one-way, so there is nobody to tell, and a
/// misbehaving driver must not be able to fill the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// The slot number is negative or past the table end.
    ///
    /// C: `if (id < 0 || id >= INPUT_DEV_MAX) return;` — note the comment
    /// above it: "Unlike minor numbers, device IDs are in fact array
    /// indices" (`input.c:382`). The number is used as an index directly,
    /// so it is validated as one.
    BadSlot,
    /// The sender does not own the slot it reports for.
    ///
    /// C: `if (input_dev->owner != m->m_source) return;` (`input.c:389`).
    ForeignSource,
}

/// Where one validated driver report goes.
///
/// C: `input_event` (`input.c:404-421`): the owning device when it is
/// opened, else the multiplexer when *it* is opened, else a terminal
/// forward. The order is a priority ladder — the specific device first,
/// the "any keyboard" fallback second, the terminal last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventIntake {
    /// File into this slot's queue (and consider waking its waiters).
    Deliver {
        /// The slot whose queue takes the event.
        target: DeviceIndex,
    },
    /// Nobody holds the device: relay the five lanes to the terminal.
    ForwardToTerminal,
    /// The report is unusable; drop it silently.
    Drop(DropReason),
}

/// Routes one driver report: slot number plus sender endpoint.
///
/// `slot_id` is the report's raw `id` lane (a signed `int` in C — negatives
/// are possible on the wire and must be rejected, not cast). `source` is
/// the message sender. Pure: no queue edits, no replies.
pub fn route_event(table: &InputTable, slot_id: i32, source: Endpoint) -> EventIntake {
    if slot_id < 0 {
        return EventIntake::Drop(DropReason::BadSlot);
    }
    let index = match DeviceIndex::new(slot_id as usize) {
        Ok(index) => index,
        Err(_) => return EventIntake::Drop(DropReason::BadSlot),
    };
    let device = &table.devices[index.0];
    if device.owner != source {
        return EventIntake::Drop(DropReason::ForeignSource);
    }
    if device.opened {
        return EventIntake::Deliver { target: index };
    }
    let mux = multiplexer_for(device.minor);
    if table.devices[mux.0].opened {
        return EventIntake::Deliver { target: mux };
    }
    EventIntake::ForwardToTerminal
}

/// Selects the multiplexer for a device's minor number.
///
/// C: `input.c:393-397` — a minor inside the keyboard window belongs to the
/// keyboard multiplexer; *everything else* belongs to the mouse
/// multiplexer. "Everything else" is literal: mouse minors, the multiplexer
/// minors themselves, even impossible values. Multiplexer slots are never
/// owned, so they never reach this function as event sources (the owner
/// check drops them first); the catch-all shape is preserved anyway, so the
/// function cannot disagree with C on any input.
pub const fn multiplexer_for(minor: Minor) -> DeviceIndex {
    if minor.0 >= KEYBOARD_FIRST_MINOR && minor.0 < KEYBOARD_FIRST_MINOR + KEYBOARD_MINOR_COUNT {
        DeviceIndex(KEYBOARD_MULTIPLEXER_INDEX)
    } else {
        DeviceIndex(MOUSE_MULTIPLEXER_INDEX)
    }
}

/// A terminal forward: the five relay lanes, ready to transmit.
///
/// C: the `fwd` message (`input.c:409-420`) copies the report lane-for-lane
/// into `m_input_tty_event`. This struct is that copy as data; the sender
/// transmits it (blocking send, failure logged — `input.c:419-420`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForwardedEvent {
    /// Source table slot.
    pub id: i32,
    /// Event family.
    pub page: i32,
    /// Family-specific code.
    pub code: i32,
    /// Press/release or motion value.
    pub value: i32,
    /// Absolute/relative flag.
    pub flags: i32,
}

/// Copies a report into a terminal forward, lane for lane.
///
/// C: `input.c:412-417`. No interpretation, no filtering here — the
/// terminal decides what it wants (document 13).
pub const fn forward_to_terminal(
    id: i32,
    page: i32,
    code: i32,
    value: i32,
    flags: i32,
) -> ForwardedEvent {
    ForwardedEvent {
        id,
        page,
        code,
        value,
        flags,
    }
}

// ── Enqueue: file one event ──

/// Files one event into a slot's queue; reports whether an old event was
/// overwritten to make room.
///
/// C: `input_process` (`input.c:338-355`). A full buffer advances the tail
/// first (the oldest event falls off), then the new event lands at
/// `(tail + count) % 32` and the count grows. The five lanes come from the
/// report; the source slot becomes `devid`; the reserved words are zeroed.
/// Returns `true` exactly when the buffer was full on entry.
pub fn enqueue(device: &mut InputDevice, event: InputEvent) -> bool {
    let overflowed = device.is_buffer_full();
    if overflowed {
        device.tail = (device.tail + 1) % crate::structs::EVENT_BUFFER_SIZE as u32;
        device.count -= 1;
    }
    let next = (device.tail + device.count) % crate::structs::EVENT_BUFFER_SIZE as u32;
    device.events[next as usize] = event;
    device.count += 1;
    overflowed
}

/// Builds the stored event from report lanes.
///
/// C: `input.c:348-354` — page/code/value/flags copied, `devid` set to the
/// report's slot number, reserved words zeroed. The `u16` lanes narrow the
/// message `int` lanes: ids are table indices (< 10) and pages/codes are
/// protocol-small by construction, the same narrowing C performs implicitly.
pub const fn stored_event(
    slot_id: i32,
    page: i32,
    code: i32,
    value: i32,
    flags: i32,
) -> InputEvent {
    InputEvent {
        page: page as u16,
        code: code as u16,
        value,
        flags: flags as u16,
        source_device: slot_id as u16,
        reserved: [0, 0],
    }
}

// ── Wake: answer whoever waits ──

/// Whom a fresh event wakes, if anyone.
///
/// C: `input_process` (`input.c:361-370`). A parked reader wins over a
/// recorded selector — never both: the reader takes the event (exactly one
/// is copied to it), the selector waits for the next one. The reader's
/// answer carries the copy result (bytes moved, or the copy error); the
/// selector's notification always says "readable".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeDirective {
    /// Answer the parked reader (`chardriver_reply_task` with the copy
    /// result, then unpark).
    AnswerReader {
        /// Who parked the read.
        caller: Endpoint,
        /// The request id its answer must echo.
        request_id: u32,
    },
    /// Notify the recorded selector (`chardriver_reply_select` with
    /// readable, then forget it).
    NotifySelector {
        /// Who asked to be told.
        selector: Endpoint,
        /// The minor the notification concerns.
        minor: Minor,
    },
    /// Nobody waits; nothing to do.
    Nobody,
}

/// Chooses whom a fresh event wakes.
///
/// Reads the flags only; the queue edit (`enqueue`) and the copy are
/// separate steps. Order matters: reader before selector (C checks
/// `suspended` first).
pub const fn decide_wake(device: &InputDevice) -> WakeDirective {
    if device.suspended {
        return WakeDirective::AnswerReader {
            caller: device.caller,
            request_id: device.request_id,
        };
    }
    if device.has_selector() {
        return WakeDirective::NotifySelector {
            selector: device.selector,
            minor: device.minor,
        };
    }
    WakeDirective::Nobody
}

/// Applies an answered reader: unparks.
///
/// C: `input_dev->suspended = FALSE` (`input.c:365`). The copy itself (one
/// event, via the 07 machinery) and the reply travel through the transport
/// first; unpark only after they succeed.
pub fn apply_wake_answered(device: &mut InputDevice) {
    device.suspended = false;
}

/// Applies a notified selector: forgets it.
///
/// C: `input_dev->selector = NONE` (`input.c:369`). One-shot: the next
/// event needs a fresh select to be announced.
pub fn apply_wake_notified(device: &mut InputDevice) {
    device.selector = Endpoint::NONE;
}

// ── The runtime heart: file one event, answer whoever waits ──

/// What one fresh event set in motion, contact details included.
///
/// The composed answer side of `input_process`: everything after the queue
/// edit, with the reply payload ready for the transport. Reader and
/// selector never both fire — the reader takes the event, the selector
/// waits for the next one (C checks `suspended` first, `input.c:361`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeAction {
    /// The parked reader takes the event: perform the plan's one-event
    /// copy through the transport, then finish with
    /// [`complete_answered_reader`].
    AnswerReader {
        /// Who parked the read.
        caller: Endpoint,
        /// The request id the answer must echo.
        request_id: u32,
        /// The one-event copy the reader is answered with (`event_count
        /// = 1`, `input.c:362`): segments to transport, bytes for the
        /// reply. The queue is **not** advanced yet — see
        /// [`complete_answered_reader`].
        plan: ReadCopyPlan,
    },
    /// A recorded selector was told "readable", and forgotten.
    NotifySelector {
        /// Who asked to be told.
        selector: Endpoint,
        /// The minor the notification concerns.
        minor: Minor,
    },
    /// The one-event plan itself failed. Unreachable through the public
    /// flow (the queue just grew, so one event is always buffered) — kept
    /// because C answers the error instead of crashing (architecture
    /// evolution A-11): the reader is already unparked, its answer is the
    /// error.
    AnswerReaderFailed {
        /// Who parked the read.
        caller: Endpoint,
        /// The request id the answer must echo.
        request_id: u32,
        /// Why the plan failed.
        error: InputError,
    },
    /// Nobody waited; the event just sits in the queue.
    Nobody,
}

/// Files one event and answers whoever waits.
///
/// C: `input_process` (`input.c:332-371`) as one composition — the enqueue
/// (`input.c:347-355`), then the answer decision (`input.c:357-369`). The
/// reader branch plans the **exactly one** event copy (`event_count = 1`,
/// `input.c:362`) and hands it back with the contact details; the queue
/// keeps the event until the transport has actually moved it, because C
/// only advances after a successful copy (`input.c:153-154`) — a failed
/// copy leaves the event buffered, its reader still answered, with the
/// error as the answer value. The selector branch reports "readable" and
/// forgets the waiter right away (`input.c:367-369` — a plain reply send,
/// no transfer to fail).
pub fn wake_on_event(device: &mut InputDevice, event: InputEvent) -> WakeAction {
    enqueue(device, event);
    match decide_wake(device) {
        WakeDirective::AnswerReader {
            caller,
            request_id,
        } => match plan_read_copy(device, EventCount(1)) {
            Ok(plan) => WakeAction::AnswerReader {
                caller,
                request_id,
                plan,
            },
            // Unreachable on the wake path (the queue just grew, so one
            // event is always buffered), but C answers the error instead
            // of crashing (A-11); carry it as a failed transport round.
            Err(error) => {
                apply_wake_answered(device);
                WakeAction::AnswerReaderFailed { caller, request_id, error }
            }
        },
        WakeDirective::NotifySelector { selector, minor } => {
            apply_wake_notified(device);
            WakeAction::NotifySelector { selector, minor }
        }
        WakeDirective::Nobody => WakeAction::Nobody,
    }
}

/// Finishes an answered reader after the transport attempt.
///
/// C: `input.c:361-365` in order — copy (the transport's part, reported
/// here as `transported`), reply with the outcome, unpark. The unpark runs
/// **unconditionally**: C clears `suspended` after the reply whether the
/// copy moved bytes or returned an error, so a failed transport still ends
/// the suspended wait — the events stay buffered for the next reader, and
/// this one walks away with the error.
pub fn complete_answered_reader(
    device: &mut InputDevice,
    plan: ReadCopyPlan,
    transported: Result<(), InputError>,
) -> Result<ByteCount, InputError> {
    let outcome = match transported {
        Ok(()) => {
            let bytes = plan.bytes;
            commit_read_copy(device, plan);
            Ok(bytes)
        }
        Err(error) => Err(error),
    };
    apply_wake_answered(device);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structs::InputTable;

    fn owned_open(slot: usize, owner: Endpoint) -> InputTable {
        let mut table = InputTable::fresh();
        table.devices[slot].owner = owner;
        table.devices[slot].opened = true;
        table
    }

    #[test]
    fn test_route_drops_bad_slots() {
        // C: input.c:384-385 — negatives and past-the-end silently dropped.
        let table = InputTable::fresh();
        assert_eq!(
            route_event(&table, -1, Endpoint(3)),
            EventIntake::Drop(DropReason::BadSlot)
        );
        assert_eq!(
            route_event(&table, -2147483648, Endpoint(3)),
            EventIntake::Drop(DropReason::BadSlot)
        );
        assert_eq!(
            route_event(&table, 10, Endpoint(3)),
            EventIntake::Drop(DropReason::BadSlot)
        );
        assert_eq!(
            route_event(&table, i32::MAX, Endpoint(3)),
            EventIntake::Drop(DropReason::BadSlot)
        );
    }

    #[test]
    fn test_route_drops_foreign_source() {
        // C: input.c:389-390 — sender must own the slot.
        let table = owned_open(2, Endpoint(3));
        assert_eq!(
            route_event(&table, 2, Endpoint(4)),
            EventIntake::Drop(DropReason::ForeignSource)
        );
        assert_eq!(
            route_event(&table, 2, Endpoint::NONE),
            EventIntake::Drop(DropReason::ForeignSource)
        );
    }

    #[test]
    fn test_route_prefers_device_then_mux_then_terminal() {
        // C: input.c:404-421 — the priority ladder.
        let owner = Endpoint(3);
        // Device opened: delivers to the device itself.
        let table = owned_open(2, owner);
        assert_eq!(
            route_event(&table, 2, owner),
            EventIntake::Deliver {
                target: DeviceIndex(2)
            }
        );
        // Device closed, keyboard multiplexer opened: delivers to mux 0.
        let mut table = InputTable::fresh();
        table.devices[2].owner = owner;
        table.devices[0].opened = true;
        assert_eq!(
            route_event(&table, 2, owner),
            EventIntake::Deliver {
                target: DeviceIndex(0)
            }
        );
        // Mouse device closed, mouse multiplexer opened: delivers to mux 5.
        let mut table = InputTable::fresh();
        table.devices[7].owner = owner;
        table.devices[5].opened = true;
        assert_eq!(
            route_event(&table, 7, owner),
            EventIntake::Deliver {
                target: DeviceIndex(5)
            }
        );
        // Nothing opened: forwards to the terminal.
        let mut table = InputTable::fresh();
        table.devices[2].owner = owner;
        assert_eq!(
            route_event(&table, 2, owner),
            EventIntake::ForwardToTerminal
        );
    }

    #[test]
    fn test_multiplexer_selection_matches_c() {
        // C: input.c:393-397 — keyboard window → mux 0, all else → mux 5.
        for minor in [1, 2, 3, 4] {
            assert_eq!(multiplexer_for(Minor(minor)), DeviceIndex(0));
        }
        for minor in [0, 5, 63, 64, 65, 66, 67, 68, -1, 100] {
            assert_eq!(multiplexer_for(Minor(minor)), DeviceIndex(5));
        }
    }

    #[test]
    fn test_forward_copies_lanes() {
        // C: input.c:412-417 — lane-for-lane copy, no interpretation.
        let fwd = forward_to_terminal(2, 0x0007, 0x0004, 1, 0);
        assert_eq!(
            fwd,
            ForwardedEvent {
                id: 2,
                page: 0x0007,
                code: 0x0004,
                value: 1,
                flags: 0
            }
        );
    }

    #[test]
    fn test_enqueue_appends_and_reports_overflow() {
        // C: input.c:338-355 — append at (tail+count)%32; full overwrites oldest.
        let mut table = InputTable::fresh();
        let device = &mut table.devices[1];
        let first = stored_event(1, 7, 4, 1, 0);
        assert!(!enqueue(device, first));
        assert_eq!(device.count, 1);
        assert_eq!(device.events[0].code, 4);
        assert_eq!(device.events[0].source_device, 1);
        assert!(device.events[0].reserved_is_zero());
        // Fill to 32, then one more: tail advances, count stays 32.
        for k in 1..32 {
            let event = stored_event(1, 7, 100 + k, 1, 0);
            assert!(!enqueue(device, event));
        }
        assert_eq!(device.count, 32);
        let event = stored_event(1, 7, 999, 1, 0);
        assert!(enqueue(device, event));
        assert_eq!(device.count, 32);
        assert_eq!(device.tail, 1);
        // Oldest (code 4) fell off; newest (code 999) landed at slot 0.
        assert_eq!(device.events[0].code, 999);
        assert_eq!(device.events[1].code, 101);
    }

    #[test]
    fn test_wake_prefers_reader_over_selector() {
        // C: input.c:361-370 — suspended wins; selector only when no reader.
        let mut table = InputTable::fresh();
        let device = &mut table.devices[1];
        assert_eq!(decide_wake(device), WakeDirective::Nobody);
        device.selector = Endpoint(9);
        assert_eq!(
            decide_wake(device),
            WakeDirective::NotifySelector {
                selector: Endpoint(9),
                minor: device.minor,
            }
        );
        device.suspended = true;
        device.caller = Endpoint(7);
        device.request_id = 13;
        assert_eq!(
            decide_wake(device),
            WakeDirective::AnswerReader {
                caller: Endpoint(7),
                request_id: 13
            }
        );
        apply_wake_answered(device);
        assert!(!device.suspended);
        // Reader gone: the selector is still owed its notification.
        assert_eq!(
            decide_wake(device),
            WakeDirective::NotifySelector {
                selector: Endpoint(9),
                minor: device.minor,
            }
        );
        apply_wake_notified(device);
        assert!(!device.has_selector());
        assert_eq!(decide_wake(device), WakeDirective::Nobody);
    }

    #[test]
    fn test_wake_on_event_answers_reader_with_exactly_one_event() {
        // C: input.c:357-365 — the reader's answer plans event_count = 1;
        // the queue keeps the event until the transport has moved it.
        let mut table = InputTable::fresh();
        let device = &mut table.devices[1];
        park_read_helper(device, Endpoint(7), 13);
        device.count = 2;
        device.tail = 10;
        device.events[10].code = 100;
        device.events[11].code = 101;
        // A third event arrives; the reader is answered with the oldest.
        let action = wake_on_event(device, stored_event(1, 7, 102, 1, 0));
        let plan = match action {
            WakeAction::AnswerReader {
                caller,
                request_id,
                plan,
            } => {
                assert_eq!((caller, request_id), (Endpoint(7), 13));
                plan
            }
            _ => panic!("expected AnswerReader"),
        };
        assert_eq!(plan.plan.event_total(), 1);
        // Nothing committed yet (input.c:153-154 runs only after a
        // successful transport copy), nobody unparked yet.
        assert_eq!(device.count, 3);
        assert!(device.suspended);
        // The transport moves the single oldest event; the completion then
        // commits and unparks (input.c:362-365).
        let outcome = complete_answered_reader(device, plan, Ok(()));
        assert_eq!(outcome, Ok(ByteCount(20)));
        assert_eq!((device.tail, device.count), (11, 2));
        assert!(!device.suspended);
        assert_eq!(device.events[11].code, 101);
        assert_eq!(device.events[12].code, 102);
    }

    #[test]
    fn test_wake_on_event_reader_takes_event_from_empty_queue() {
        // C: input.c:182-193 parks on empty; input.c:361-364 hands the
        // first arrival straight over — the queue empties on completion.
        let mut table = InputTable::fresh();
        let device = &mut table.devices[1];
        park_read_helper(device, Endpoint(7), 13);
        let action = wake_on_event(device, stored_event(1, 7, 55, 1, 0));
        let plan = match action {
            WakeAction::AnswerReader { plan, .. } => plan,
            _ => panic!("expected AnswerReader"),
        };
        assert_eq!(device.count, 1);
        let outcome = complete_answered_reader(device, plan, Ok(()));
        assert_eq!(outcome, Ok(ByteCount(20)));
        assert!(!device.suspended);
        assert!(device.is_buffer_empty());
    }

    #[test]
    fn test_wake_completion_failure_keeps_events_and_unparks() {
        // C: input.c:144-151 (failed copy: queue untouched) + input.c:365
        // (unpark runs unconditionally) — the reader walks away with the
        // error, the event stays for the next one.
        let mut table = InputTable::fresh();
        let device = &mut table.devices[1];
        park_read_helper(device, Endpoint(7), 13);
        let action = wake_on_event(device, stored_event(1, 7, 55, 1, 0));
        let plan = match action {
            WakeAction::AnswerReader { plan, .. } => plan,
            _ => panic!("expected AnswerReader"),
        };
        let outcome = complete_answered_reader(device, plan, Err(InputError::InputOutput));
        assert_eq!(outcome, Err(InputError::InputOutput));
        assert!(!device.suspended);
        assert_eq!((device.tail, device.count), (0, 1));
        assert_eq!(device.events[0].code, 55);
    }

    #[test]
    fn test_wake_on_event_selector_notified_and_forgotten() {
        // C: input.c:366-369 — no reader: the selector hears "readable" once.
        let mut table = InputTable::fresh();
        let device = &mut table.devices[1];
        device.selector = Endpoint(9);
        let action = wake_on_event(device, stored_event(1, 7, 55, 1, 0));
        assert_eq!(
            action,
            WakeAction::NotifySelector {
                selector: Endpoint(9),
                minor: device.minor,
            }
        );
        assert!(!device.has_selector());
        assert_eq!(device.count, 1);
    }

    #[test]
    fn test_wake_on_event_nobody_just_files() {
        // C: input.c:347-355 with no waiter — the event waits in the queue.
        let mut table = InputTable::fresh();
        let device = &mut table.devices[1];
        let action = wake_on_event(device, stored_event(1, 7, 55, 1, 0));
        assert_eq!(action, WakeAction::Nobody);
        assert_eq!(device.count, 1);
    }

    #[test]
    fn test_wake_on_event_full_buffer_reader_still_gets_one() {
        // C: input.c:338-346 (overflow drops oldest) + input.c:361-364
        // (reader takes the oldest survivor of the drop) — together the
        // reader sees the freshest events, never a starved queue.
        let mut table = InputTable::fresh();
        let device = &mut table.devices[1];
        park_read_helper(device, Endpoint(7), 13);
        for code in 0..32u16 {
            enqueue(device, stored_event(1, 7, code as i32, 1, 0));
        }
        assert!(device.is_buffer_full());
        let action = wake_on_event(device, stored_event(1, 7, 999, 1, 0));
        let plan = match action {
            WakeAction::AnswerReader { plan, .. } => plan,
            _ => panic!("expected AnswerReader"),
        };
        // Overflow already applied (tail moved past code 0), nothing committed.
        assert_eq!(device.count, 32);
        let outcome = complete_answered_reader(device, plan, Ok(()));
        assert_eq!(outcome, Ok(ByteCount(20)));
        // 32 full, overflow drops code 0, the reader takes code 1: 31 left,
        // the oldest survivor now is code 2.
        assert_eq!(device.count, 31);
        assert_eq!(device.events[device.tail as usize].code, 2);
    }

    /// Parks a reader the way `park_read` does (test helper; keeps the
    /// wake tests readable without importing handlers here).
    fn park_read_helper(device: &mut InputDevice, caller: Endpoint, request_id: u32) {
        device.suspended = true;
        device.caller = caller;
        device.grant = 11;
        device.request_id = request_id;
    }
}
