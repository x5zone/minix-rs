//! IPC message surface for devman (doc 05-devm-message-contract).
//!
//! - [`message`]: field views over the shared `m4` words + reply stamping
//!   + the RS-only gate.
//! - [`dispatch`]: single-handler routing ([ARCH:A-3] fall-through fix).

pub mod dispatch;
pub mod message;

pub use dispatch::{dispatch, Handler};
pub use message::{apply_reply, check_rs, device_id, grant_id, grant_size, request_endpoint, result};
