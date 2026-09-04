//! Character-device handlers: open, close, read, control, cancel, select.
//!
//! C: `input_open`/`input_close` (`minix3/minix/servers/input/input.c:85-125`),
//! `input_read` (`input.c:162-199`), `input_ioctl` (`input.c:241-277`),
//! `input_cancel` (`input.c:282-298`), `input_select` (`input.c:303-326`).
//!
//! Every handler is split in two halves, and the split is the design:
//!
//! - **Decide** (`decide_*`): a pure function of the device state and the
//!   request parameters. No transport, no grants, no side effects — fully
//!   testable with a hand-built device.
//! - **Apply** (`apply_*`): the state change for a decided outcome. Called
//!   only after the transport half succeeded (a failed grant copy must not
//!   advance the ring; a refused request must not park).
//!
//! The transport halves (minor lookup, grant copies, replies) belong to the
//! future dispatcher, which calls these functions in the order the C code
//! implies: map, decide, transport, apply, reply. Error outcomes use
//! [`crate::InputError`] so every refusal carries its Minix3 errno.
//!
//! Corresponding documents: `06-input-open-close.md` (open/close),
//! `07-input-read-suspend.md` (read), `08-input-ioctl-cancel-select.md`
//! (control/cancel/select).

use crate::error::InputError;
use crate::event::LedCode;
use crate::eventbuf::{apply_copy, plan_copy};
use crate::framework::{SELECT_ERROR, SELECT_NOTIFY, SELECT_READ, SELECT_WRITE};
use crate::structs::InputDevice;
use core::mem::size_of;
use minix_types::{Endpoint, KBD_LEDS_CAPS, KBD_LEDS_NUM, KBD_LEDS_SCROLL, KIOCSLEDS};

/// Wire size of one buffered event (20 bytes, see `event.rs`).
pub const EVENT_BYTES: usize = size_of::<crate::event::InputEvent>();

// ── Open / close (document 06) ──

/// Decides an open request against the device state.
///
/// C: `input_open` (`input.c:85-102`) after the minor lookup (a missed
/// lookup is [`InputError::UnknownMinor`], answered by the caller, not
/// here): an inactive device refuses with "no such device", an already
/// opened one with "busy". The checks run in C order — activity first —
/// so a device that is both inactive and opened reports inactive, exactly
/// as C does.
pub fn decide_open(device: &InputDevice) -> Result<(), InputError> {
    if !device.is_active() {
        return Err(InputError::DeviceNotActive);
    }
    if device.opened {
        return Err(InputError::DeviceBusy);
    }
    Ok(())
}

/// Applies a granted open: marks the device held.
///
/// C: `input_dev->opened = TRUE` (`input.c:99`).
pub fn apply_open(device: &mut InputDevice) {
    device.opened = true;
}

/// Decides a close request against the device state.
///
/// C: `input_close` (`input.c:107-125`) after the lookup: closing a device
/// nobody holds is "invalid argument" (C also logs the attempt — logging is
/// the transport's job, noted here so the line is not lost:
/// `printf("INPUT: closing already-closed device %d\n", minor)`).
pub const fn decide_close(device: &InputDevice) -> Result<(), InputError> {
    if !device.opened {
        return Err(InputError::NotOpened);
    }
    Ok(())
}

/// Applies a granted close: releases the hold, empties the queue, and parks
/// nothing.
///
/// C clears `opened`, `tail`, and `count` (`input.c:120-122`) — and stops
/// there. Stopping there is a bug, fixed here (see below): a suspended read
/// or a recorded selector left behind by a close outlives the reader that
/// registered it. The next opener then inherits a poisoned device — reads
/// fail, events are answered to a dead caller — with no request that can
/// clear it (a cancel only matches the previous caller).
///
/// MINIX3 BUG (input.c:107-125): close leaves `suspended` and `selector`
/// (plus the stale `caller`/`grant`/`req_id`) behind. Reachable: the file
/// system closes character devices without cancelling first
/// (`filedes.c:453` calls `cdev_close` directly), so a process that exits
/// — or a second thread that closes — while a blocking read is parked
/// wedges the device for every future opener. Rust fix: close clears the
/// whole reader state. In every run where C behaved (no parked reader at
/// close), clearing already-clear flags changes nothing observable.
pub fn apply_close(device: &mut InputDevice) {
    device.opened = false;
    device.tail = 0;
    device.count = 0;
    device.suspended = false;
    device.caller = Endpoint::NONE;
    device.grant = 0;
    device.request_id = 0;
    device.selector = Endpoint::NONE;
}

// ── Read (document 07) ──

/// The read decision: serve now, park for later, or refuse.
///
/// C: the branch structure of `input_read` (`input.c:162-199`), with the
/// transport (grant copies) factored out. `Serve` carries the event count
/// the copy half must move (already clamped to what is buffered).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadVerdict {
    /// Copy `event_count` oldest events to the caller now.
    Serve {
        /// How many events to move (≤ buffered, ≥ 1).
        event_count: u32,
    },
    /// Park the caller (`suspended` + contact details); the framework sends
    /// no reply now (`EDONTREPLY`), the wake-up answers later.
    Park,
    /// Refuse with an error (the framework replies immediately).
    Refuse(InputError),
}

/// Decides a read request.
///
/// `size_bytes` is the caller's buffer size; `nonblocking` mirrors the
/// `CDEV_NONBLOCK` transfer flag. In C order (`input.c:169-198`):
/// missed lookup is the caller's job ([`InputError::UnknownMinor`]); an
/// inactive device or an already-parked read refuses with "input/output
/// error"; a buffer too small for one event refuses the same way (the
/// caller asked for something unanswerable); an empty buffer parks — or
/// refuses with "try again" when the caller said not to wait; otherwise
/// serve, clamped to what is buffered.
pub fn decide_read(device: &InputDevice, size_bytes: usize, nonblocking: bool) -> ReadVerdict {
    if !device.is_active() || device.suspended {
        return ReadVerdict::Refuse(InputError::InputOutput);
    }
    let event_count = (size_bytes / EVENT_BYTES) as u32;
    if event_count == 0 {
        return ReadVerdict::Refuse(InputError::InputOutput);
    }
    if device.is_buffer_empty() {
        if nonblocking {
            return ReadVerdict::Refuse(InputError::WouldBlock);
        }
        return ReadVerdict::Park;
    }
    let clamped = if event_count > device.count {
        device.count
    } else {
        event_count
    };
    ReadVerdict::Serve {
        event_count: clamped,
    }
}

/// Parks a read: records who waits and how to answer them.
///
/// C: `input.c:186-189`. Faithful down to the omission: C does not touch
/// the selector here (with the self-deprecating comment "We should now wake
/// up any selector, but that's lame.."), so neither do we — a parked reader
/// and a recorded selector coexist until an event sorts them out
/// (document 09).
pub fn park_read(device: &mut InputDevice, caller: Endpoint, grant: i32, request_id: u32) {
    device.suspended = true;
    device.caller = caller;
    device.grant = grant;
    device.request_id = request_id;
}

/// Serves a decided copy: plans the segments and advances the device.
///
/// Thin glue over `eventbuf.rs` for the read path: `decide_read` promised
/// `event_count` (≤ buffered), the plan splits it, the advance fulfils it.
/// Once the dispatcher lands, the two grant copies go between the plan and
/// the advance (a failed copy must not advance); until then this function
/// keeps the promise (`Serve`) and its fulfilment from drifting apart.
pub fn serve_copy(device: &mut InputDevice, event_count: u32) -> Result<u32, InputError> {
    let plan = plan_copy(device.tail, device.count, event_count)?;
    // The transport copies `first_len` then `second_len` events through the
    // grant here (future dispatcher); advance only afterwards.
    apply_copy(device, &plan);
    Ok(plan.event_total())
}

// ── Control (document 08) ──

/// The control decision: set lights, or refuse.
///
/// C: `input_ioctl` (`input.c:241-277`) after lookup and activity check
/// (inactive → "input/output error", same as read): the only known request
/// is `KIOCSLEDS`; anything else is "not a typewriter control". The caller
/// bits arrive through a grant copy (transport's job — a failed copy
/// answers the transport's error, as C returns `r` at `input.c:258-260`);
/// this function routes on the request number, [`led_mask_from_kio_bits`]
/// translates the bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoctlVerdict {
    /// Set the indicator lights (the mask comes from
    /// [`led_mask_from_kio_bits`] applied to the transported caller bits;
    /// document 10 owns the sending).
    SetLeds,
    /// Refuse with an error.
    Refuse(InputError),
}

/// Routes a control request by number.
pub fn decide_ioctl(device: &InputDevice, request: u32) -> IoctlVerdict {
    if !device.is_active() {
        return IoctlVerdict::Refuse(InputError::InputOutput);
    }
    if request == KIOCSLEDS {
        return IoctlVerdict::SetLeds;
    }
    IoctlVerdict::Refuse(InputError::NotATypewriterControl)
}

/// Translates caller light bits into the server light mask.
///
/// C: `input.c:262-268`. Each caller bit (`KBD_LEDS_NUM/CAPS/SCROLL`)
/// raises the mask bit addressed by the matching light code
/// (`1 << INPUT_LED_*`, with codes 1/2/3 → mask bits 1/2/3). Unknown caller
/// bits are ignored: the caller may know lights this server does not.
pub const fn led_mask_from_kio_bits(kl_bits: u32) -> u32 {
    let mut mask = 0;
    if kl_bits & KBD_LEDS_NUM != 0 {
        mask |= 1 << LedCode::NumLock as u32;
    }
    if kl_bits & KBD_LEDS_CAPS != 0 {
        mask |= 1 << LedCode::CapsLock as u32;
    }
    if kl_bits & KBD_LEDS_SCROLL != 0 {
        mask |= 1 << LedCode::ScrollLock as u32;
    }
    mask
}

// ── Cancel (document 08) ──

/// The cancel decision: interrupt the parked reader, or ignore.
///
/// C: `input_cancel` (`input.c:282-298`): a cancel names its target by
/// (caller, request id). An exact match on a parked read unparks it and the
/// *original* read answers "interrupted" — note the subtlety: the cancel
/// request itself gets no special reply (the framework answers it normally),
/// it is the woken read that reports `EINTR`. Anything else — nothing
/// parked, wrong caller, wrong id — is met with "no reply" (`EDONTREPLY`):
/// silence, because the original request may already have finished, in
/// which case there is nobody to tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelVerdict {
    /// The parked read matches: unpark it; it answers `EINTR`.
    InterruptReader,
    /// No match: stay silent (`EDONTREPLY`).
    Ignore,
}

/// Matches a cancel against the parked read, if any.
pub const fn decide_cancel(
    device: &InputDevice,
    caller: Endpoint,
    request_id: u32,
) -> CancelVerdict {
    if device.suspended && device.caller.0 == caller.0 && device.request_id == request_id {
        return CancelVerdict::InterruptReader;
    }
    CancelVerdict::Ignore
}

/// Applies a matched cancel: unparks the read.
///
/// C: `input_dev->suspended = FALSE` (`input.c:292`) — and nothing else.
/// The stale contact fields stay, deliberately: every use of them is gated
/// on `suspended`, so once cleared they are unreachable until the next park
/// overwrites them (unlike close, where the next opener *can* reach a stale
/// `suspended` — hence the fix in [`apply_close`]).
pub fn apply_cancel(device: &mut InputDevice) {
    device.suspended = false;
}

// ── Select (document 08) ──

/// A select answer: which operations are ready now, and whether to record
/// a waiter.
///
/// C: `input_select` (`input.c:303-326`) returns the ready mask directly;
/// recording the selector is a side effect inside the same function. Split
/// here into value (`ready_ops`) and effect (`record_selector`) so tests
/// can assert each half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectOutcome {
    /// Ready operations (`SELECT_*` bits, 0 = "nothing ready, come back
    /// later unless recorded").
    pub ready_ops: i32,
    /// Whether the caller asked for later notification with nothing ready
    /// now (record them as `selector`).
    pub record_selector: bool,
}

/// Answers a select query.
///
/// `ops` combines the queried operations with `SELECT_NOTIFY`
/// ("tell me later"). In C order (`input.c:314-323`): a read query on an
/// inactive or parked device reports ready — "ready" here means "asking
/// again is pointless, take the error now" (the `/* immediate error */`
/// comments); data buffered also reports ready; an empty buffer with
/// `SELECT_NOTIFY` records the waiter instead. A write query always reports
/// ready with the same "immediate error" meaning (input devices are
/// read-only; a write fails at once). Error queries (`SELECT_ERROR`) are
/// never reported ready — C has no branch for them.
pub fn decide_select(device: &InputDevice, ops: i32) -> SelectOutcome {
    let mut ready_ops = 0;
    let mut record_selector = false;
    if ops & SELECT_READ != 0 {
        // Ready means "an immediate answer exists": data buffered, or a
        // read that would fail at once (inactive device, parked reader —
        // both `/* immediate error */` in C, input.c:316). One bit covers
        // both; the follow-up read tells them apart (bytes vs error).
        if !device.is_active() || device.suspended || !device.is_buffer_empty() {
            ready_ops |= SELECT_READ;
        } else if ops & SELECT_NOTIFY != 0 {
            record_selector = true;
        }
    }
    if ops & SELECT_WRITE != 0 {
        ready_ops |= SELECT_WRITE;
    }
    // SELECT_ERROR has no C branch (input.c:314-323 tests only RD and WR):
    // error queries never report ready. Named so the omission reads as
    // deliberate, not as an oversight.
    let _ = SELECT_ERROR;
    SelectOutcome {
        ready_ops,
        record_selector,
    }
}

/// Records a select waiter for later notification.
///
/// C: `input_dev->selector = endpt` (`input.c:320`). Overwrites any previous
/// waiter — the device remembers one selector, the latest asker (a second
/// asker displaces the first; C keeps no queue here).
pub fn apply_select_record(device: &mut InputDevice, selector: Endpoint) {
    device.selector = selector;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structs::InputTable;

    fn keyboard() -> InputDevice {
        // Slot 1: first keyboard, driver-owned, opened, empty queue.
        let mut device = InputTable::fresh().devices[1];
        device.owner = Endpoint(3);
        device.opened = true;
        device
    }

    fn closed_keyboard() -> InputDevice {
        // Same, but nobody holds it yet: the state an open expects.
        let mut device = InputTable::fresh().devices[1];
        device.owner = Endpoint(3);
        device
    }

    #[test]
    fn test_event_bytes_match_c() {
        // C: sizeof(struct input_event) = 20 (input.h:25-32).
        assert_eq!(EVENT_BYTES, 20);
    }

    #[test]
    fn test_open_accepts_active_closed_device() {
        let mut device = closed_keyboard();
        assert_eq!(decide_open(&device), Ok(()));
        apply_open(&mut device);
        assert!(device.opened);
    }

    #[test]
    fn test_open_refusals_follow_c_order() {
        // C: input.c:90-97 — activity before already-open.
        let mut idle = InputTable::fresh().devices[1];
        assert_eq!(decide_open(&idle), Err(InputError::DeviceNotActive));
        idle.owner = Endpoint(3);
        idle.opened = true;
        assert_eq!(decide_open(&idle), Err(InputError::DeviceBusy));
        // Multiplexers are active with no driver (input.c:24-26).
        let mux = InputTable::fresh().devices[0];
        assert_eq!(decide_open(&mux), Ok(()));
    }

    #[test]
    fn test_close_releases_and_empties() {
        // C: input.c:120-122 (opened/tail/count) + Rust close fix.
        let mut device = keyboard();
        device.tail = 9;
        device.count = 4;
        assert_eq!(decide_close(&device), Ok(()));
        apply_close(&mut device);
        assert!(!device.opened);
        assert_eq!((device.tail, device.count), (0, 0));
        // Closing a device nobody holds is EINVAL (input.c:115-118).
        assert_eq!(decide_close(&device), Err(InputError::NotOpened));
    }

    #[test]
    fn test_close_clears_parked_reader_and_selector() {
        // MINIX3 BUG (input.c:107-125 leaves both behind): a close with a
        // parked reader and a recorded selector must leave neither.
        let mut device = keyboard();
        park_read(&mut device, Endpoint(7), 11, 13);
        device.selector = Endpoint(9);
        apply_close(&mut device);
        assert!(!device.suspended);
        assert!(!device.has_selector());
        assert_eq!(device.caller, Endpoint::NONE);
    }

    #[test]
    fn test_read_serves_clamped_to_buffered() {
        // C: input.c:195-198 — clamp, then copy.
        let mut device = keyboard();
        device.count = 3;
        assert_eq!(
            decide_read(&device, 10 * EVENT_BYTES, false),
            ReadVerdict::Serve { event_count: 3 }
        );
        assert_eq!(
            decide_read(&device, 2 * EVENT_BYTES, false),
            ReadVerdict::Serve { event_count: 2 }
        );
    }

    #[test]
    fn test_read_refusals_match_c() {
        // C: input.c:172-179 — inactive/parked/too-small all refuse EIO.
        let idle = InputTable::fresh().devices[1];
        assert_eq!(
            decide_read(&idle, EVENT_BYTES, false),
            ReadVerdict::Refuse(InputError::InputOutput)
        );
        let mut parked = keyboard();
        park_read(&mut parked, Endpoint(7), 11, 13);
        assert_eq!(
            decide_read(&parked, EVENT_BYTES, false),
            ReadVerdict::Refuse(InputError::InputOutput)
        );
        assert_eq!(
            decide_read(&keyboard(), EVENT_BYTES - 1, false),
            ReadVerdict::Refuse(InputError::InputOutput)
        );
        assert_eq!(
            decide_read(&keyboard(), 0, false),
            ReadVerdict::Refuse(InputError::InputOutput)
        );
    }

    #[test]
    fn test_read_parks_or_refuses_when_empty() {
        // C: input.c:182-193 — empty parks, unless nonblocking (EAGAIN).
        let device = keyboard();
        assert_eq!(decide_read(&device, EVENT_BYTES, false), ReadVerdict::Park);
        assert_eq!(
            decide_read(&device, EVENT_BYTES, true),
            ReadVerdict::Refuse(InputError::WouldBlock)
        );
        let mut parked = device;
        park_read(&mut parked, Endpoint(7), 11, 13);
        assert!(parked.suspended);
        assert_eq!(parked.caller, Endpoint(7));
        assert_eq!(parked.grant, 11);
        assert_eq!(parked.request_id, 13);
    }

    #[test]
    fn test_serve_copy_moves_and_advances() {
        let mut device = keyboard();
        device.tail = 30;
        device.count = 8;
        let moved = serve_copy(&mut device, 5).unwrap();
        assert_eq!(moved, 5);
        assert_eq!((device.tail, device.count), (3, 3));
        assert!(serve_copy(&mut device, 4).is_err());
    }

    #[test]
    fn test_ioctl_routes_setleds_only() {
        // C: input.c:253-276 — active check, KIOCSLEDS, default ENOTTY.
        let device = keyboard();
        assert_eq!(decide_ioctl(&device, KIOCSLEDS), IoctlVerdict::SetLeds);
        assert_eq!(
            decide_ioctl(&device, 0x1234),
            IoctlVerdict::Refuse(InputError::NotATypewriterControl)
        );
        let idle = InputTable::fresh().devices[1];
        assert_eq!(
            decide_ioctl(&idle, KIOCSLEDS),
            IoctlVerdict::Refuse(InputError::InputOutput)
        );
    }

    #[test]
    fn test_led_mask_bits_match_c() {
        // C: input.c:262-268 — (1 << INPUT_LED_*) per caller bit.
        assert_eq!(led_mask_from_kio_bits(0), 0);
        assert_eq!(led_mask_from_kio_bits(KBD_LEDS_NUM), 0x2);
        assert_eq!(led_mask_from_kio_bits(KBD_LEDS_CAPS), 0x4);
        assert_eq!(led_mask_from_kio_bits(KBD_LEDS_SCROLL), 0x8);
        assert_eq!(
            led_mask_from_kio_bits(KBD_LEDS_NUM | KBD_LEDS_CAPS | KBD_LEDS_SCROLL),
            0xE
        );
        // Unknown caller bits are ignored.
        assert_eq!(led_mask_from_kio_bits(0xFFFF_FFF0), 0);
    }

    #[test]
    fn test_cancel_matches_exactly_or_ignores() {
        // C: input.c:290-297 — triple match interrupts, else silence.
        let mut device = keyboard();
        park_read(&mut device, Endpoint(7), 11, 13);
        assert_eq!(
            decide_cancel(&device, Endpoint(7), 13),
            CancelVerdict::InterruptReader
        );
        apply_cancel(&mut device);
        assert!(!device.suspended);
        // Wrong caller, wrong id, nothing parked: all ignored.
        park_read(&mut device, Endpoint(7), 11, 13);
        assert_eq!(
            decide_cancel(&device, Endpoint(8), 13),
            CancelVerdict::Ignore
        );
        assert_eq!(
            decide_cancel(&device, Endpoint(7), 14),
            CancelVerdict::Ignore
        );
        apply_cancel(&mut device);
        assert_eq!(
            decide_cancel(&device, Endpoint(7), 13),
            CancelVerdict::Ignore
        );
    }

    #[test]
    fn test_select_reports_ready_data_and_errors() {
        // C: input.c:314-323.
        let mut device = keyboard();
        // Empty, no notify: nothing ready.
        let out = decide_select(&device, SELECT_READ);
        assert_eq!(
            out,
            SelectOutcome {
                ready_ops: 0,
                record_selector: false
            }
        );
        // Empty with notify: record the waiter.
        let out = decide_select(&device, SELECT_READ | SELECT_NOTIFY);
        assert_eq!(
            out,
            SelectOutcome {
                ready_ops: 0,
                record_selector: true
            }
        );
        apply_select_record(&mut device, Endpoint(9));
        assert!(device.has_selector());
        // Buffered: ready.
        device.count = 2;
        let out = decide_select(&device, SELECT_READ);
        assert_eq!(out.ready_ops & SELECT_READ, SELECT_READ);
        // Parked reader or inactive device: "ready" means error now.
        park_read(&mut device, Endpoint(7), 11, 13);
        let out = decide_select(&device, SELECT_READ);
        assert_eq!(out.ready_ops & SELECT_READ, SELECT_READ);
        let idle = InputTable::fresh().devices[1];
        let out = decide_select(&idle, SELECT_READ);
        assert_eq!(out.ready_ops & SELECT_READ, SELECT_READ);
    }

    #[test]
    fn test_select_write_always_ready() {
        // C: input.c:323 — write queries are answered ready ("immediate
        // error": input devices take no writes).
        let device = keyboard();
        let out = decide_select(&device, SELECT_WRITE);
        assert_eq!(out.ready_ops & SELECT_WRITE, SELECT_WRITE);
        assert!(!out.record_selector);
        // Error queries alone never report ready (no C branch).
        let out = decide_select(&device, SELECT_ERROR);
        assert_eq!(out.ready_ops, 0);
    }
}
