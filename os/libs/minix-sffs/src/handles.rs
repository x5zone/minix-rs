//! Host-handle lifecycle: open lazily, close silently (`handle.c`).
//!
//! Handles are expensive across the guest-host boundary, so each node
//! holds at most one, opened on first use and closed when the node
//! goes. Opening prefers read-write and falls back to read-only when
//! the mount is read-only or the first attempt fails (protection or
//! mount status may forbid writing, and the framework cannot tell
//! which, `handle.c:39-43`). Closing ignores errors: there is nothing
//! sensible left to do with a failing close.

/// Whether a node currently holds a handle (the `I_HANDLE` flag bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandleState {
    /// A handle is open.
    pub open: bool,
}

/// How to open (`get_handle`, `handle.c:18-52`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenPlan {
    /// Already open: nothing to do.
    AlreadyOpen,
    /// Open read-write first, fall back to read-only on failure.
    WriteThenRead,
    /// Open read-only at once (read-only mount).
    ReadOnly,
}

/// Plan opening for a node in a mount state.
pub const fn plan_open(handle_open: bool, read_only_mount: bool) -> OpenPlan {
    if handle_open {
        return OpenPlan::AlreadyOpen;
    }
    if read_only_mount {
        OpenPlan::ReadOnly
    } else {
        OpenPlan::WriteThenRead
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_plans() {
        assert_eq!(plan_open(true, false), OpenPlan::AlreadyOpen);
        assert_eq!(plan_open(false, true), OpenPlan::ReadOnly);
        assert_eq!(plan_open(false, false), OpenPlan::WriteThenRead);
        assert_eq!(HandleState { open: false }.open, false);
    }
}
