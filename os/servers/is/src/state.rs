//! IS server inbox state (01-is-init-main.md §4.1).
//!
//! C: the four file-static globals — `static message m_in/m_out`,
//! `static endpoint_t who_e`, `static int callnr`
//! (`minix3/minix/servers/is/main.c:14-17`). `get_work` writes them,
//! the main loop reads them; there is a single thread, so no sharing
//! exists. The rewrite gathers them into one struct owned by
//! [`crate::IsServer`] instead of copying four `static mut` globals
//! (Rust 2024 deprecates `static mut`; a rewrite must not transliterate
//! them).

use minix_types::{Endpoint, Message};

/// The IS server's complete mutable state.
///
/// C: `m_in`/`m_out`/`who_e`/`callnr` — main.c:14-17. Single-threaded
/// event loop (user-space server model): `!Send` is correct, no
/// `Rc`/`RefCell` needed — nothing is shared.
#[derive(Debug, Default)]
pub struct IsServerState {
    /// Inbox. C: `m_in` — main.c:15.
    pub inbox: Message,
    /// Reply scratch buffer (only `m_type` is significant).
    /// C: `m_out` — main.c:16.
    pub reply_buf: Message,
    /// Sender of the message currently being handled.
    /// C: `who_e` — main.c:17.
    pub caller: Endpoint,
    /// Type of the message currently being handled.
    /// C: `callnr` — main.c:17.
    pub call_nr: i32,
}

impl IsServerState {
    /// Blank state (C statics are zero-initialised).
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_starts_blank() {
        let s = IsServerState::new();
        assert_eq!(s.call_nr, 0);
        assert_eq!(s.caller, Endpoint::default());
    }
}
