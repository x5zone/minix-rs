//! The uniform output vocabulary: everything the server can *do*.
//!
//! Every decision in this crate ends as data — a verdict, a report, a wake
//! action, a plan. This module is the last step of that discipline: the
//! four kinds of outward action a dispatcher can be asked to perform, each
//! carrying its fully built wire message where one exists. The future
//! transport (a separate landing, edge todo E-INWIRE) executes effects and
//! nothing else: receive, run the decisions, perform the effects. Because
//! effects are data, a test can assert the whole outbound behavior of any
//! decision without a transport.
//!
//! The four kinds mirror the C send disciplines exactly:
//!
//! - **Task reply** ([`Effect::ReplyTask`]): `chardriver_reply_task`
//!   (`chardriver.c:129-151`) answers one parked or immediate request with
//!   `CDEV_REPLY`.
//! - **Select reply** ([`Effect::ReplySelect`]): `chardriver_reply_select`
//!   (`chardriver.c:153-174`) wakes one recorded waiter with
//!   `CDEV_SEL2_REPLY` readable.
//! - **Async driver send** ([`Effect::SendDriverAsync`]): `asynsend3` with
//!   `AMF_NOREPLY` (`input.c:231`, `:522`) — fire-and-forget downstream to
//!   a device driver.
//! - **Blocking terminal send** ([`Effect::SendTerminalBlocking`]):
//!   `ipc_send` (`input.c:419`, `:676`) — upstream to the terminal driver,
//!   blocking so a crashed terminal is noticed (the same two reasons the
//!   client library sends events blocking, `inputdriver.c:65-73`).
//!
//! Wire messages arrive ready to send ([`minix_types::conf_msg`] and
//! friends build them with `m_source` left `NONE`): the transport backfills
//! the sender endpoint, which is the one thing only it can know.
//!
//! Corresponding documents: `02-chardriver-framework.md` (the two reply
//! disciplines), `10-input-setleds.md` / `11-input-driver-connect.md`
//! (async sends), `13-tty-consumer.md` (blocking sends).

use crate::connect::ConnectReport;
use crate::error::InputError;
use crate::eventbuf::ByteCount;
use crate::handlers::CancelledRead;
use crate::produce::ForwardedEvent;
use crate::structs::Minor;
use minix_types::{
    conf_msg, setleds_msg, tty_event_msg, tty_up_msg, Endpoint, Message, INVALID_INPUT_ID,
};

/// What a task reply says: bytes moved, or a plain status code.
///
/// C collapses the two into one `int` (`chardriver.c:129-151` sends the
/// status verbatim). Two constructors keep the units honest at the call
/// site — a read completion answers in bytes (`input.c:156`), everything
/// else answers with `OK` (zero) or an errno. Status codes follow this
/// crate's convention (positive errno constants, matching
/// `crate::error::InputError::to_errno`); the wire sign, if any, is the
/// transport's concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyValue {
    /// Bytes moved: a read completion (`input.c:156`).
    Bytes(ByteCount),
    /// A plain status: `OK` (zero) or a negative errno.
    Code(i32),
}

impl ReplyValue {
    /// The success answer for non-transfer requests (`OK`).
    pub const fn ok() -> ReplyValue {
        ReplyValue::Code(0)
    }

    /// The failure answer for a domain error (its Minix3 errno).
    pub const fn from_error(error: InputError) -> ReplyValue {
        ReplyValue::Code(error.to_errno())
    }

    /// The answer for a read copy outcome: bytes or the copy errno.
    pub const fn from_copy_outcome(outcome: Result<ByteCount, InputError>) -> ReplyValue {
        match outcome {
            Ok(bytes) => ReplyValue::Bytes(bytes),
            Err(error) => ReplyValue::Code(error.to_errno()),
        }
    }
}

/// One outward action, fully armed.
///
/// Not `PartialEq`: a `Message` wraps a union of wire payloads, for which
/// field equality is meaningless (padding lanes differ by construction).
/// Tests assert on the decoded payload instead, which is the part with
/// semantics.
#[derive(Debug, Clone, Copy)]
pub enum Effect {
    /// Complete one request: `CDEV_REPLY` to the caller, echoing its
    /// request id (`chardriver.c:129-151`).
    ReplyTask {
        /// Who asked (the reply destination).
        caller: Endpoint,
        /// The request id to echo.
        request_id: u32,
        /// What the answer says.
        value: ReplyValue,
    },
    /// Wake one recorded select waiter: `CDEV_SEL2_REPLY` readable
    /// (`chardriver.c:153-174`; the server only ever reports read-ready,
    /// `input.c:367-368`).
    ReplySelect {
        /// Who asked to be told.
        selector: Endpoint,
        /// The minor the notification concerns.
        minor: Minor,
    },
    /// Fire-and-forget send downstream to a device driver: the
    /// configuration reply (`input.c:522`) or a light command
    /// (`input.c:231`). `asynsend3` with `AMF_NOREPLY`; a send failure is
    /// logged by the transport, never retried here (C logs and moves on,
    /// `input.c:230-234` — the reconnect-time light restore, `input.c:527`,
    /// is the recovery path).
    SendDriverAsync {
        /// The driver's endpoint.
        to: Endpoint,
        /// The wire message, ready to send.
        message: Message,
    },
    /// Blocking one-way send upstream to the terminal driver: an event
    /// forward (`input.c:412-420`) or the startup handshake
    /// (`input.c:672-677`). Blocking on purpose: backpressure plus crash
    /// detection (`inputdriver.c:65-73` states the same two reasons from
    /// the driver side). A failure is logged by the transport.
    SendTerminalBlocking {
        /// The wire message, ready to send.
        message: Message,
    },
}

impl Effect {
    /// Answers a read completion (bytes moved) or any error.
    pub fn reply_outcome(
        caller: Endpoint,
        request_id: u32,
        outcome: Result<ByteCount, InputError>,
    ) -> Effect {
        Effect::ReplyTask {
            caller,
            request_id,
            value: ReplyValue::from_copy_outcome(outcome),
        }
    }

    /// Answers a plain successful request (`OK`).
    pub fn reply_ok(caller: Endpoint, request_id: u32) -> Effect {
        Effect::ReplyTask {
            caller,
            request_id,
            value: ReplyValue::ok(),
        }
    }

    /// Answers a plain failed request (the error's errno).
    pub fn reply_error(caller: Endpoint, request_id: u32, error: InputError) -> Effect {
        Effect::ReplyTask {
            caller,
            request_id,
            value: ReplyValue::from_error(error),
        }
    }

    /// Answers a matched cancel: EINTR to the original read.
    ///
    /// C: the cancel handler's `EINTR` return is sent under the cancel's
    /// request id, which equals the parked read's own — one reply completes
    /// both (`chardriver.c:255-261`, `input.c:290-295`).
    pub fn reply_interrupted(read: CancelledRead) -> Effect {
        Effect::ReplyTask {
            caller: read.caller,
            request_id: read.request_id,
            value: ReplyValue::Code(InputError::Interrupted.to_errno()),
        }
    }

    /// Builds the configuration reply to a freshly connected driver.
    ///
    /// C: `input.c:514-523`. Unassigned slots travel as the invalid id —
    /// a partly disabled driver (the allocation-failure semantics, doc 11).
    pub fn input_conf(to: Endpoint, report: &ConnectReport) -> Effect {
        let kbd = report
            .keyboard_slot
            .map_or(INVALID_INPUT_ID, |slot| slot.0 as i32);
        let mouse = report
            .mouse_slot
            .map_or(INVALID_INPUT_ID, |slot| slot.0 as i32);
        Effect::SendDriverAsync {
            to,
            message: conf_msg(kbd, mouse),
        }
    }

    /// Builds a light command to one owned keyboard slot.
    ///
    /// C: `input.c:214-234` — one message per addressed owned slot.
    pub fn setleds(to: Endpoint, mask: u32) -> Effect {
        Effect::SendDriverAsync {
            to,
            message: setleds_msg(mask),
        }
    }

    /// Builds the terminal forward of one unrouted event.
    ///
    /// C: `input.c:408-421` — the five report lanes, lane for lane.
    pub fn tty_event(event: ForwardedEvent) -> Effect {
        Effect::SendTerminalBlocking {
            message: tty_event_msg(event.id, event.page, event.code, event.value, event.flags),
        }
    }

    /// Builds the startup handshake to the terminal driver.
    ///
    /// C: `input.c:672-677` — no payload, presence is the message.
    pub fn tty_up() -> Effect {
        Effect::SendTerminalBlocking {
            message: tty_up_msg(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventbuf::ByteCount;
    use crate::structs::DeviceIndex;
    use minix_types::{
        decode_conf, decode_setleds, decode_tty_event, INPUT_CONF, INPUT_SETLEDS,
        TTY_INPUT_EVENT, TTY_INPUT_UP,
    };

    #[test]
    fn test_reply_values_encode_c_statuses() {
        // C: chardriver.c:129-151 sends the status verbatim; a read answers
        // in bytes, everything else in OK or an errno (crate convention:
        // positive constants, as in `error.rs`'s own tests).
        assert_eq!(ReplyValue::ok(), ReplyValue::Code(0));
        assert_eq!(
            ReplyValue::from_error(InputError::UnknownMinor),
            ReplyValue::Code(minix_types::ENXIO)
        );
        assert_eq!(
            ReplyValue::from_copy_outcome(Ok(ByteCount(20))),
            ReplyValue::Bytes(ByteCount(20))
        );
        assert_eq!(
            ReplyValue::from_copy_outcome(Err(InputError::Interrupted)),
            ReplyValue::Code(minix_types::EINTR)
        );
    }

    #[test]
    fn test_reply_interrupted_targets_the_original_read() {
        // C: chardriver.c:255-261 — the cancel's EINTR goes out under the
        // shared request id, completing the parked read.
        let effect = Effect::reply_interrupted(CancelledRead {
            caller: Endpoint(7),
            request_id: 13,
        });
        match effect {
            Effect::ReplyTask {
                caller,
                request_id,
                value,
            } => {
                assert_eq!((caller, request_id), (Endpoint(7), 13));
                assert_eq!(value, ReplyValue::Code(minix_types::EINTR));
            }
            _ => panic!("expected ReplyTask"),
        }
    }

    #[test]
    fn test_input_conf_carries_slots_and_reserved_invalids() {
        // C: input.c:514-523 — assigned ids plus rsvd1/rsvd2 = invalid.
        let report = ConnectReport {
            keyboard_slot: Some(DeviceIndex(2)),
            mouse_slot: None,
            restore_lights: None,
        };
        let effect = Effect::input_conf(Endpoint(5), &report);
        match effect {
            Effect::SendDriverAsync { to, message } => {
                assert_eq!(to, Endpoint(5));
                assert_eq!(message.m_type, INPUT_CONF);
                assert_eq!(decode_conf(&message), Some((2, INVALID_INPUT_ID)));
            }
            _ => panic!("expected SendDriverAsync"),
        }
    }

    #[test]
    fn test_setleds_message_round_trips_the_mask() {
        // C: input.c:214-231 — one INPUT_SETLEDS per owned slot.
        let effect = Effect::setleds(Endpoint(6), 0b111);
        match effect {
            Effect::SendDriverAsync { to, message } => {
                assert_eq!(to, Endpoint(6));
                assert_eq!(message.m_type, INPUT_SETLEDS);
                assert_eq!(decode_setleds(&message), Some(0b111));
            }
            _ => panic!("expected SendDriverAsync"),
        }
    }

    #[test]
    fn test_tty_event_forward_maps_lanes_lane_for_lane() {
        // C: input.c:408-421 — five lanes, no interpretation.
        let effect = Effect::tty_event(ForwardedEvent {
            id: 3,
            page: 7,
            code: 42,
            value: 1,
            flags: 0,
        });
        match effect {
            Effect::SendTerminalBlocking { message } => {
                assert_eq!(message.m_type, TTY_INPUT_EVENT);
                assert_eq!(decode_tty_event(&message), Some((3, 7, 42, 1, 0)));
            }
            _ => panic!("expected SendTerminalBlocking"),
        }
    }

    #[test]
    fn test_tty_up_is_a_payload_free_announcement() {
        // C: input.c:672-677 — zeroed message, only the type set.
        let effect = Effect::tty_up();
        match effect {
            Effect::SendTerminalBlocking { message } => {
                assert_eq!(message.m_type, TTY_INPUT_UP);
                assert_eq!(message.m_source, Endpoint::NONE);
                // SAFETY: reading the raw lanes of the payload union; the
                // constructor zero-initialized them (`tty_up_msg`), and any
                // union lane views the same bytes.
                let raw = unsafe { message.m_u.raw };
                assert!(raw.iter().all(|byte| *byte == 0));
            }
            _ => panic!("expected SendTerminalBlocking"),
        }
    }
}
