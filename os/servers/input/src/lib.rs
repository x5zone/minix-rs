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
//! # Module map (documents 01-04)
//!
//! - [`init`] — startup order and the callback inventory (document 01).
//! - [`framework`] — the shared character-driver front door: message
//!   classification, the restart gate, reply discipline (document 02).
//! - [`structs`] — device slots, minor numbers, table indices (document 03).
//! - [`event`] — the event wire format and code tables (document 04).
//! - [`error`] — failures with their Minix3 errno numbers.
//! - [`key_codes`] — the 215 keyboard-page codes, mechanically derived.
//!
//! Later documents (05-14) add handlers, buffering behavior, driver
//! lifecycle, the client library, and the external consumers.

extern crate alloc;

pub mod error;
pub mod event;
pub mod framework;
pub mod init;
pub mod key_codes;
pub mod structs;

pub use error::InputError;
pub use event::{
    ButtonCode, ConsumerCode, EventPage, GeneralDesktopCode, InputEvent, LedCode, PressState,
    ValueMode,
};
pub use framework::{
    CharacterRequest, CharacterResponse, GateVerdict, Incoming, NotifySource, OpenDeviceSet,
    ReplyDecision,
};
pub use init::{HandlerSlot, InitStep, StartupRegistration};
pub use key_codes::KeyCode;
pub use structs::{DeviceIndex, InputDevice, InputTable, Minor};

/// Crate-level init entry (called by `main.rs`; real init runs on startup).
pub fn init() {}
