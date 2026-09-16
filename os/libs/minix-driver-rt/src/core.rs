//! Shared server machinery: open tracking, loop actions, server state.
//!
//! C correspondence: the open-device bookkeeping (`open_devs` +
//! `clear_open_devs`/`is_open_dev`/`set_open_dev`, `chardriver.c:54-94`,
//! duplicated verbatim by libblockdriver), the `running` flag and its
//! announce/terminate transitions (`chardriver.c:52,99-124,537-544`), and
//! the receive-outcome policy (`chardriver_task`'s loop body,
//! `chardriver.c:560-569`). The three framework libraries carried
//! identical copies of this machinery; this module is the single
//! original. What deliberately stays per-family: the `Route` enums and
//! their classify functions — the character family routes block-open
//! interlopers, the net family gates on initialization instead of opens,
//! and those differences are protocol, not boilerplate.

use minix_types::EINTR;

/// Maximum minor devices remembered as opened.
///
/// C: `MAX_NR_OPEN_DEVICES 256` (`driver.h:41`), shared by the character
/// and block frameworks.
pub const MAX_OPEN_DEVICES: usize = 256;

/// Set of minor devices opened since the last announce.
///
/// C: the open table plus its three helpers (`chardriver.c:54-94`). The
/// C code panics when the table overflows; `insert` reports the same
/// situation as a boolean so the server — which has the message context —
/// decides whether a full table is fatal.
#[derive(Debug, Clone)]
pub struct OpenSet {
    slots: [u32; MAX_OPEN_DEVICES],
    len: usize,
}

impl OpenSet {
    /// Empty set, as right after an announce.
    pub const fn new() -> OpenSet {
        OpenSet {
            slots: [0; MAX_OPEN_DEVICES],
            len: 0,
        }
    }

    /// Forget every recorded device (fresh start or restart).
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Number of recorded devices.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// True when the set holds no device.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// True when the raw minor was recorded before.
    ///
    /// C: `is_open_dev` (`chardriver.c:70-80`), a linear scan — fine for
    /// at most a few hundred entries on a slow control path.
    pub fn contains_raw(&self, minor: u32) -> bool {
        self.slots[..self.len].contains(&minor)
    }

    /// Record a raw minor; false when the table is already full.
    ///
    /// C: `set_open_dev` (`chardriver.c:85-94`).
    pub fn insert_raw(&mut self, minor: u32) -> bool {
        if self.contains_raw(minor) {
            return true;
        }
        if self.len >= MAX_OPEN_DEVICES {
            return false;
        }
        self.slots[self.len] = minor;
        self.len += 1;
        true
    }
}

impl Default for OpenSet {
    fn default() -> Self {
        Self::new()
    }
}

/// What the event loop does next after one receive outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopAction {
    /// A message arrived: route it.
    Dispatch,
    /// Termination was requested while blocked: leave the loop.
    Stop,
    /// The receive itself failed: stop immediately.
    Abort,
}

/// The receive-outcome policy, shared by every framework loop.
///
/// C: the loop body (`chardriver.c:560-569`): a transport error of
/// "interrupted" while stopping ends the loop quietly; any other
/// transport error is fatal — a failed receive means unknown state, and
/// the framework stops rather than serving blindly.
pub const fn loop_action(result: Result<(), i32>, running: bool) -> LoopAction {
    match result {
        Ok(()) => LoopAction::Dispatch,
        Err(code) if code == EINTR && !running => LoopAction::Stop,
        Err(_) => LoopAction::Abort,
    }
}

/// Server state: run flag plus open tracking.
///
/// C: the file-static `running` flag and the open table. The receive
/// loop itself stays in the service crate (only it owns the transport);
/// this type owns the policy: when to keep looping, when a receive error
/// is fatal, and when an interrupt means "stop now".
#[derive(Debug, Clone)]
pub struct ServerState {
    running: bool,
    opened: OpenSet,
}

impl ServerState {
    /// Fresh server, as before the first announce.
    pub const fn new() -> ServerState {
        ServerState {
            running: false,
            opened: OpenSet::new(),
        }
    }

    /// Announce readiness: start running and forget pre-restart opens.
    ///
    /// C: the announce half (`chardriver.c:99-124`); the data-store
    /// publication stays with the transport.
    pub fn announce(&mut self) {
        self.running = true;
        self.opened.clear();
    }

    /// Stop after the current request (`chardriver_terminate`,
    /// `blockdriver_terminate`).
    pub fn terminate(&mut self) {
        self.running = false;
    }

    /// True while the event loop should keep receiving.
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// Open-device set for the router.
    pub const fn opened(&self) -> &OpenSet {
        &self.opened
    }

    /// Mutable open-device set (record opens, clear on restart).
    pub fn opened_mut(&mut self) -> &mut OpenSet {
        &mut self.opened
    }

    /// Handle one receive outcome from the transport.
    pub fn note_receive(&mut self, result: Result<(), i32>) -> LoopAction {
        loop_action(result, self.running)
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_set_tracks_and_fills() {
        let mut set = OpenSet::new();
        assert!(set.is_empty());
        assert!(set.insert_raw(3));
        assert!(set.contains_raw(3));
        assert!(!set.contains_raw(4));
        assert!(set.insert_raw(3)); // idempotent, no growth
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_open_set_refuses_when_full() {
        let mut set = OpenSet::new();
        for minor in 0..MAX_OPEN_DEVICES as u32 {
            assert!(set.insert_raw(minor));
        }
        assert!(!set.insert_raw(MAX_OPEN_DEVICES as u32));
        set.clear();
        assert!(set.is_empty());
    }

    #[test]
    fn test_loop_action_policy() {
        assert_eq!(loop_action(Ok(()), true), LoopAction::Dispatch);
        // Interrupt while terminating: leave quietly. While running, the
        // shared policy treats ANY failed receive as fatal (both original
        // copies agreed) — the core preserves that verbatim.
        assert_eq!(loop_action(Err(EINTR), false), LoopAction::Stop);
        assert_eq!(loop_action(Err(EINTR), true), LoopAction::Abort);
        assert_eq!(loop_action(Err(-1), false), LoopAction::Abort);
    }

    #[test]
    fn test_server_announce_forgets_pre_restart_opens() {
        let mut server = ServerState::new();
        assert!(!server.is_running());
        server.opened_mut().insert_raw(9);
        server.announce();
        assert!(server.is_running());
        assert!(!server.opened().contains_raw(9)); // restart gate cleared
        server.terminate();
        assert!(!server.is_running());
    }
}
