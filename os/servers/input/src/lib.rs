#![cfg_attr(not(test), no_std)]

//! Minix-RS input server: keyboards and mice, as seen by everyone else.
//!
//! The input server sits between hardware drivers (which report raw key
//! presses and pointer motion) and readers (the terminal driver today).
//! Drivers push events in; readers pull events out; the server buffers,
//! multiplexes ("any keyboard" versus "this keyboard"), and remembers
//! indicator-light state across driver restarts.
//!
//! # Single-threaded model
//!
//! Like the data-store and device-manager servers, the whole crate assumes
//! **single-threaded event-loop execution**: one message at a time, handled
//! to completion. The device table is plain mutable state — no `Arc`, no
//! `Mutex`, no atomics. Do not share these types across threads.
//!
//! # Module map (documents 01-08)
//!
//! - [`init`] — startup order and the callback inventory (document 01).
//! - [`framework`] — the shared character-driver front door: message
//!   classification, the restart gate, reply discipline (document 02).
//! - [`structs`] — device slots, minor numbers, table indices (document 03).
//! - [`event`] — the event wire format and code tables (document 04).
//! - [`error`] — failures with their Minix3 errno numbers.
//! - [`key_codes`] — the 215 keyboard-page codes, mechanically derived.
//! - [`eventbuf`] — ring-buffer copy geometry (document 07).
//! - [`handlers`] — open/close/read/control/cancel/select decisions
//!   (documents 06-08).
//! - [`produce`] — event intake, enqueue, and wake-up decisions
//!   (document 09).
//! - [`setleds`] — light broadcast planning and mask memory (document 10).
//! - [`connect`] — driver slot allocation, connect reports, disconnect
//!   effects (document 11).
//!
//! Later documents (12-14) add the client library and the external
//! consumers (the client library lives in `minix-sys`, document 12).

extern crate alloc;

pub mod connect;
pub mod error;
pub mod event;
pub mod eventbuf;
pub mod framework;
pub mod handlers;
pub mod init;
pub mod key_codes;
pub mod produce;
pub mod setleds;
pub mod structs;

pub use connect::{
    ConnectReport, DisconnectEffects, NO_SLOT, alloc_id, connect_driver, disconnect_device,
    key_is_new_driver, wants_from_typemask,
};
pub use error::InputError;
pub use event::{
    ButtonCode, ConsumerCode, EventPage, GeneralDesktopCode, InputEvent, LedCode, PressState,
    ValueMode,
};
pub use eventbuf::{CopyPlan, apply_copy, drain_ordered, plan_copy};
pub use framework::{
    CharacterRequest, CharacterResponse, GateVerdict, Incoming, NotifySource, OpenDeviceSet,
    ReplyDecision,
};
pub use handlers::{
    CancelVerdict, EVENT_BYTES, IoctlVerdict, ReadVerdict, SelectOutcome, apply_cancel,
    apply_close, apply_open, apply_select_record, decide_cancel, decide_close, decide_ioctl,
    decide_open, decide_read, decide_select, led_mask_from_kio_bits, park_read, serve_copy,
};
pub use init::{HandlerSlot, InitStep, StartupRegistration};
pub use key_codes::KeyCode;
pub use produce::{
    DropReason, EventIntake, ForwardedEvent, WakeDirective, apply_wake_answered,
    apply_wake_notified, decide_wake, enqueue, forward_to_terminal, multiplexer_for, route_event,
    stored_event,
};
pub use setleds::{LightTarget, apply_light_save, plan_light_targets, remembered_lights};
pub use structs::{DeviceIndex, InputDevice, InputTable, Minor};

/// Crate-level init entry (called by `main.rs`; real init runs on startup).
pub fn init() {}
