//! Hub port management: connection states plus reset discipline.
//!
//! C correspondence: the port limits (`PORT_LIMIT 8`, `POLLING_INTERVAL`
//! 1000 milliseconds, `MAX_TRIES 3`, `RESET_DELAY` 200 milliseconds,
//! `usb_hub.c:47-56`), the port state (`hub_state` with per-port
//! connection marks, `usb_hub.c:183-190`), the port task loop
//! (`hub_task`, `usb_hub.c:390-512`: power each port, then poll every
//! port once per second), the change truth table
//! (`hub_handle_change`, `usb_hub.c:693`), and the connection path
//! (`hub_handle_connection`, `usb_hub.c:826`: reset the port, wait
//! for the reset flag up to three times, clear it, require connection
//! plus enable, translate speed, report up).
//!
//! Control traffic stays in the service binary; this module owns the
//! bookkeeping half: what each port believes and when a port is
//! declared broken.

/// Most ports one hub tracks (`PORT_LIMIT`, `usb_hub.c:49`).
pub const PORT_LIMIT: usize = 8;

/// Reset attempts before a port is declared broken (`MAX_TRIES`,
/// `usb_hub.c:51`).
pub const MAX_RESET_TRIES: u8 = 3;

/// What one port currently believes (`hub_state.conn`, `usb_hub.c:190`,
/// with `port_conn`, `usb_hub.c:141-144`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortBelief {
    /// Nothing attached.
    Disconnected,
    /// A device is attached and enabled.
    Connected,
    /// The port failed; only removal clears it.
    Broken,
}

/// What a poll observed on one port (`port_change`, `usb_hub.c:146-158`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortObservation {
    /// No change since the last poll.
    None,
    /// A device arrived.
    Arrived,
    /// A device left.
    Left,
    /// The port reports a status error (power it off, mark broken).
    StatusError,
    /// The port stopped answering (hangs the task in C).
    CommError,
}

/// One hub port: belief plus reset budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HubPort {
    belief: PortBelief,
    resets_used: u8,
}

impl HubPort {
    /// A fresh port starts disconnected with a full reset budget.
    pub fn new() -> Self {
        HubPort { belief: PortBelief::Disconnected, resets_used: 0 }
    }

    /// Current belief about this port.
    pub fn belief(&self) -> PortBelief {
        self.belief
    }

    /// Fold one poll observation into the belief (`hub_handle_change`,
    /// `usb_hub.c:693`: arrivals connect, departures disconnect,
    /// status errors break the port permanently).
    pub fn observe(&mut self, observation: PortObservation) {
        match observation {
            PortObservation::None => {}
            PortObservation::Arrived => {
                if self.belief != PortBelief::Broken {
                    self.belief = PortBelief::Connected;
                    self.resets_used = 0;
                }
            }
            PortObservation::Left => {
                self.belief = PortBelief::Disconnected;
                self.resets_used = 0;
            }
            PortObservation::StatusError => {
                self.belief = PortBelief::Broken;
            }
            PortObservation::CommError => {
                self.belief = PortBelief::Broken;
            }
        }
    }

    /// Spend one reset attempt (`hub_handle_connection`, `usb_hub.c:826`):
    /// true while attempts remain, false once the budget is exhausted.
    pub fn try_reset(&mut self) -> bool {
        if self.resets_used >= MAX_RESET_TRIES {
            self.belief = PortBelief::Broken;
            return false;
        }
        self.resets_used += 1;
        true
    }
}

impl Default for HubPort {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_starts_disconnected() {
        let port = HubPort::new();
        assert_eq!(port.belief(), PortBelief::Disconnected);
        assert_eq!(PORT_LIMIT, 8);
    }

    #[test]
    fn test_arrival_and_departure_flip_belief() {
        let mut port = HubPort::new();
        port.observe(PortObservation::None);
        assert_eq!(port.belief(), PortBelief::Disconnected);
        port.observe(PortObservation::Arrived);
        assert_eq!(port.belief(), PortBelief::Connected);
        port.observe(PortObservation::Left);
        assert_eq!(port.belief(), PortBelief::Disconnected);
    }

    #[test]
    fn test_status_error_breaks_port_permanently() {
        let mut port = HubPort::new();
        port.observe(PortObservation::Arrived);
        port.observe(PortObservation::StatusError);
        assert_eq!(port.belief(), PortBelief::Broken);
        port.observe(PortObservation::Arrived);
        assert_eq!(port.belief(), PortBelief::Broken);
        port.observe(PortObservation::Left);
        assert_eq!(port.belief(), PortBelief::Disconnected);
    }

    #[test]
    fn test_reset_budget_runs_out_after_three_tries() {
        let mut port = HubPort::new();
        assert!(port.try_reset());
        assert!(port.try_reset());
        assert!(port.try_reset());
        assert!(!port.try_reset());
        assert_eq!(port.belief(), PortBelief::Broken);
        assert_eq!(MAX_RESET_TRIES, 3);
    }
}
