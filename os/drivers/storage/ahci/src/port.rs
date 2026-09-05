//! Port state machine: stop, start, commands, timeouts, resets.
//!
//! C correspondence: `port_start`, `port_stop`, `port_restart`,
//! `port_issue`, `port_exec`, `port_wait`, `port_timeout`,
//! `port_hardreset`, `port_connect`, `port_disconnect`
//! (`ahci.c:1220-1523,1715-1839`) and the command/status bits
//! (`AHCI_PORT_CMD_ST/CR/FR/FRE/CLO`, `ahci.h:191-197`).
//!
//! Register reads and writes stay in the service crate; this module owns
//! the order (stop before reprogramming, start before issuing, timeout
//! then fail, reset on unrecoverable errors).

/// Command slots per port.
///
/// The C driver tracks in-flight commands per port (`port_find_cmd`,
/// `ahci.c:965-985`); thirty-two slots match the hardware command list.
pub const COMMAND_SLOTS: usize = 32;

/// Port running state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortState {
    /// Stopped: lists may be reprogrammed.
    Stopped,
    /// Started: commands may issue.
    Started,
    /// Timed out: commands failed, reset needed.
    TimedOut,
}

/// Outcome of issuing one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueOutcome {
    /// Command accepted (a slot was free).
    Accepted(usize),
    /// No free slot: caller retries later.
    NoSlot,
    /// Port not started: start it first.
    NotStarted,
}

/// One port: state plus in-flight slot bitmap.
///
/// C: `struct port_state` command tracking (`ahci.c`) reduced to the
/// policy half (which slots are busy); FIS contents and PRD tables are
/// transport owned by the service crate.
#[derive(Debug, Clone)]
pub struct Port {
    state: PortState,
    busy: [bool; COMMAND_SLOTS],
}

impl Port {
    /// Fresh port (stopped, nothing in flight).
    pub const fn new() -> Port {
        Port {
            state: PortState::Stopped,
            busy: [false; COMMAND_SLOTS],
        }
    }

    /// Current state.
    pub const fn state(&self) -> PortState {
        self.state
    }

    /// Start the port (lists already programmed).
    ///
    /// C: `port_start` (`ahci.c:1254-...`).
    pub fn start(&mut self) {
        if self.state == PortState::Stopped {
            self.state = PortState::Started;
        }
    }

    /// Stop the port (before reprogramming lists).
    ///
    /// C: `port_stop` (`ahci.c:1275-...`).
    pub fn stop(&mut self) {
        self.state = PortState::Stopped;
        self.busy = [false; COMMAND_SLOTS];
    }

    /// Issue one command; returns its slot, if any.
    ///
    /// C: `port_issue` plus `port_find_cmd` (`ahci.c:965-985,1810-...`).
    pub fn issue(&mut self) -> IssueOutcome {
        if self.state != PortState::Started {
            return IssueOutcome::NotStarted;
        }
        match self.busy.iter().position(|slot| !slot) {
            Some(slot) => {
                self.busy[slot] = true;
                IssueOutcome::Accepted(slot)
            }
            None => IssueOutcome::NoSlot,
        }
    }

    /// Complete one slot (success or device error alike: the slot frees).
    ///
    /// C: `port_finish_cmd` (`ahci.c:900-926`).
    pub fn finish(&mut self, slot: usize) -> bool {
        match self.busy.get_mut(slot) {
            Some(busy) if *busy => {
                *busy = false;
                true
            }
            _ => false,
        }
    }

    /// Note a timeout: all in-flight commands fail, reset is next.
    ///
    /// C: `port_timeout` plus `port_fail_cmds` (`ahci.c:927-940,1715-...`).
    pub fn note_timeout(&mut self) {
        self.busy = [false; COMMAND_SLOTS];
        self.state = PortState::TimedOut;
    }

    /// Reset after a timeout: back to stopped, ready to reprogram.
    ///
    /// C: `port_hardreset` plus `port_restart` (`ahci.c:1220-...,
    /// 1296-...`).
    pub fn reset(&mut self) {
        self.busy = [false; COMMAND_SLOTS];
        self.state = PortState::Stopped;
    }

    /// In-flight command count.
    pub fn in_flight(&self) -> usize {
        self.busy.iter().filter(|slot| **slot).count()
    }
}

impl Default for Port {
    fn default() -> Self {
        Port::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_stop_cycle() {
        let mut port = Port::new();
        assert_eq!(port.issue(), IssueOutcome::NotStarted);
        port.start();
        assert_eq!(port.state(), PortState::Started);
        port.stop();
        assert_eq!(port.state(), PortState::Stopped);
    }

    #[test]
    fn test_slots_fill_then_refuse() {
        let mut port = Port::new();
        port.start();
        for _ in 0..COMMAND_SLOTS {
            assert!(matches!(port.issue(), IssueOutcome::Accepted(_)));
        }
        assert_eq!(port.issue(), IssueOutcome::NoSlot);
        assert_eq!(port.in_flight(), COMMAND_SLOTS);
        assert!(port.finish(0));
        assert!(!port.finish(0));
        assert_eq!(port.in_flight(), COMMAND_SLOTS - 1);
    }

    #[test]
    fn test_timeout_clears_and_needs_reset() {
        let mut port = Port::new();
        port.start();
        port.issue();
        port.note_timeout();
        assert_eq!(port.state(), PortState::TimedOut);
        assert_eq!(port.in_flight(), 0);
        assert_eq!(port.issue(), IssueOutcome::NotStarted);
        port.reset();
        port.start();
        assert!(matches!(port.issue(), IssueOutcome::Accepted(_)));
    }
}
