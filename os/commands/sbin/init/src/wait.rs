//! Child exit status decoding — thin re-export of the upstream
//! decoder in `minix-sys` (edge E-INITSYS ③ closed 2026-09-18).
//!
//! init adds nothing on top: the mapping helpers that turn a
//! [`WaitStatus`] onto init's own [`Signal`] authority live next to
//! their callers (see `single_user` and `runcom`).

pub use minix_sys::wait::{from_raw, EINTR, WNOHANG, WUNTRACED, WaitStatus};
