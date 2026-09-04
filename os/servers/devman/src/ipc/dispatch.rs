//! DEVMAN message dispatch: single-handler routing (doc 05).
//!
//! C: `message_hook` (main.c:46-58) — a switch with **no `break`s**: every
//! message falls through all four handlers (ADD runs add→del→bind→unbind).
//! [ARCH:A-3] Rust routes each message to **exactly one** handler.
//! Evidence the C behavior is a bug, not a protocol (05 §3.2):
//! libdevman clients observe added devices as present and bindable
//! (10), which fall-through immediately violates (add→instant del).
//!
//! Unknown types — including the five declared-but-never-cased codes
//! (`ADD_BUS`/`DEL_BUS`/`ADD_DEVFILE`/`DEL_DEVFILE`/`REQUEST`, A-6) —
//! map to `Ignored`: no handler runs, **no reply is sent**. That is
//! C-identical (an unmatched switch case does nothing) and fail-closed.

use minix_types::{DEVMAN_ADD_DEV, DEVMAN_BIND, DEVMAN_DEL_DEV, DEVMAN_UNBIND};

/// The single handler owning a message. Handlers live in 07–09;
/// this enum is the routing contract between the loop and the handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    /// `DEVMAN_ADD_DEV` → 07 `do_add_device`.
    Add,
    /// `DEVMAN_DEL_DEV` → 08 `do_del_device`.
    Del,
    /// `DEVMAN_BIND` → 09 `do_bind_device` (RS-only gate inside).
    Bind,
    /// `DEVMAN_UNBIND` → 09 `do_unbind_device` (RS-only gate inside).
    Unbind,
    /// Anything else (incl. the five A-6 codes): run nothing, reply
    /// nothing. The RS-only gate is *not* consulted — there is no
    /// handler to protect.
    Ignored,
}

/// Route one message type to its single handler ([ARCH:A-3]).
/// Total: every `i32` maps somewhere; no fall-through exists to forget.
pub fn dispatch(m_type: i32) -> Handler {
    match m_type {
        t if t == DEVMAN_ADD_DEV => Handler::Add,
        t if t == DEVMAN_DEL_DEV => Handler::Del,
        t if t == DEVMAN_BIND => Handler::Bind,
        t if t == DEVMAN_UNBIND => Handler::Unbind,
        _ => Handler::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{
        DEVMAN_ADD_BUS, DEVMAN_ADD_DEVFILE, DEVMAN_DEL_BUS, DEVMAN_DEL_DEVFILE,
        DEVMAN_REPLY, DEVMAN_REQUEST,
    };

    #[test]
    fn four_messages_route_singly() {
        // The A-3 fix: one message, one handler — never four.
        assert_eq!(dispatch(DEVMAN_ADD_DEV), Handler::Add);
        assert_eq!(dispatch(DEVMAN_DEL_DEV), Handler::Del);
        assert_eq!(dispatch(DEVMAN_BIND), Handler::Bind);
        assert_eq!(dispatch(DEVMAN_UNBIND), Handler::Unbind);
    }

    #[test]
    fn unknown_is_ignored_without_reply() {
        // A-6 codes + REPLY itself + garbage: C switch matches nothing,
        // sends nothing. Rust: Ignored (fail-closed, 05 §3.5).
        for t in [
            DEVMAN_ADD_BUS,
            DEVMAN_DEL_BUS,
            DEVMAN_ADD_DEVFILE,
            DEVMAN_DEL_DEVFILE,
            DEVMAN_REQUEST,
            DEVMAN_REPLY,
            0,
            -1,
            i32::MAX,
        ] {
            assert_eq!(dispatch(t), Handler::Ignored, "type {t}");
        }
    }
}
