//! PM IPC handling module.
//!
//! Handles message dispatch and communication with other services.

mod calls;
mod dispatcher;
mod transport;
mod vfs;

pub use calls::*;
pub use dispatcher::*;
pub use transport::*;
pub use vfs::*;
