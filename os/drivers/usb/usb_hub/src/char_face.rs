//! minix-usb wiring: the hub's port walk over the URB bookkeeping.
//!
//! C correspondence: `hub_task`'s poll walk (`usb_hub.c:440-491`) — read
//! each port's status, fold the change, and reset new arrivals; plus the
//! status-report urb the hub uses to talk to the HCD.
//!
//! The face composes the port beliefs over minix-usb's roster: each poll
//! round is framed as a control urb (submitted, then completed), and the
//! wake plan per round carries what the service must report. The actual
//! control transfers stay with the service.

use minix_usb::urb::{Direction, PendingUrb, TransferKind};

use crate::ports::{HubPort, PortBelief, PortObservation, PortEffect, PORT_LIMIT};

/// The hub's poll-round outcome: what the service does between rounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundEffect {
    /// Keep polling after the interval.
    Continue,
    /// A communication error hung the task (usb_hub.c:465-468).
    SuspendTask,
}

/// The usb_hub face: per-port beliefs plus the control-urb roster.
pub struct HubFace {
    /// One belief per port.
    pub ports: alloc::vec::Vec<HubPort>,
    /// In-flight control urbs.
    pub pending: alloc::vec::Vec<PendingUrb>,
}

impl HubFace {
    /// A face with `PORT_LIMIT` fresh ports.
    pub fn new() -> Self {
        HubFace {
            ports: {
                let mut v = alloc::vec::Vec::new();
                v.resize_with(PORT_LIMIT, HubPort::new);
                v
            },
            pending: alloc::vec::Vec::new(),
        }
    }

    /// Submit the round's status-report urb (control transfer).
    pub fn submit_round(&mut self) -> u32 {
        let id = 1; // the hub uses a single fixed control urb
        self.pending.push(PendingUrb {
            id,
            endpoint: 0,
            kind: TransferKind::Control,
            direction: Direction::In,
        });
        id
    }

    /// Complete the round's urb (the HCD answered).
    pub fn complete_round(&mut self) {
        minix_usb::urb::remove_pending(&mut self.pending, 1);
    }

    /// Fold one port's observation into the walk.
    ///
    /// Returns the round effect: a communication error suspends the
    /// whole task (the port's belief is untouched); everything else
    /// continues.
    pub fn observe(&mut self, port: usize, observation: PortObservation) -> RoundEffect {
        let Some(slot) = self.ports.get_mut(port) else {
            return RoundEffect::Continue;
        };
        match slot.observe(observation) {
            PortEffect::Continue => RoundEffect::Continue,
            PortEffect::SuspendTask => RoundEffect::SuspendTask,
        }
    }

    /// Whether a port still wants its reset budget.
    pub fn try_reset(&mut self, port: usize) -> bool {
        self.ports.get_mut(port).map(|p| p.try_reset()).unwrap_or(false)
    }

    /// Belief of one port.
    pub fn belief(&self, port: usize) -> Option<PortBelief> {
        self.ports.get(port).map(|p| p.belief())
    }
}

impl Default for HubFace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_urb_submits_and_completes() {
        let mut face = HubFace::new();
        assert_eq!(face.submit_round(), 1);
        assert_eq!(face.pending.len(), 1);
        face.complete_round();
        assert!(face.pending.is_empty());
    }

    #[test]
    fn test_arrival_connects_and_comm_error_suspends() {
        let mut face = HubFace::new();
        assert_eq!(
            face.observe(0, PortObservation::Arrived),
            RoundEffect::Continue
        );
        assert_eq!(face.belief(0), Some(PortBelief::Connected));
        // Communication error suspends the task, leaves the belief alone.
        assert_eq!(
            face.observe(0, PortObservation::CommError),
            RoundEffect::SuspendTask
        );
        assert_eq!(face.belief(0), Some(PortBelief::Connected));
    }

    #[test]
    fn test_status_error_blocks_the_port() {
        let mut face = HubFace::new();
        face.observe(2, PortObservation::Arrived);
        face.observe(2, PortObservation::StatusError);
        assert_eq!(face.belief(2), Some(PortBelief::Broken));
        // Blocked ports skip the walk: departures cannot reach them.
        face.observe(2, PortObservation::Left);
        assert_eq!(face.belief(2), Some(PortBelief::Broken));
    }

    #[test]
    fn test_reset_budget_runs_out() {
        let mut face = HubFace::new();
        assert!(face.try_reset(1));
        assert!(face.try_reset(1));
        assert!(face.try_reset(1));
        assert!(!face.try_reset(1));
    }
}
