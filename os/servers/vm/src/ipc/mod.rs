//! VM IPC handling module.
//!
//! Handles message dispatch and communication with other services.

pub(crate) mod cache_handlers;
pub(crate) mod dispatcher;
pub(crate) mod encode;
pub(crate) mod transport;
