//! VM IPC handling module.
//!
//! Handles message dispatch and communication with other services.

mod dispatcher;

pub(crate) use dispatcher::*;
