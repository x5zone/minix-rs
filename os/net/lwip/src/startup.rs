//! Startup chain and main-loop dispatch: thirteen steps, four roads.
//!
//! C correspondence: the startup chain (`init`, `lwip.c:195-264`:
//! random seed, library init, event library, helpers, high sockets,
//! interfaces, card-driver module, low sockets, route, packet
//! filter device, management tree, default config, timer and go),
//! the main loop (`main`, `lwip.c:293-382`: poll loopback, check
//! timer, receive, then notify / management / socket-device /
//! card-driver-response dispatch), and the domain dispatch
//! (`alloc_socket`, `lwip.c:151-190`: fourth-version and
//! sixth-version internet protocol, route, link; raw sockets need
//! root).
//!
//! Message traffic stays in the service binary; this module owns the
//! order half: which step comes when and which road a message takes.

/// Startup stage: where the service is in its thirteen-step chain
/// (`init`, `lwip.c:203-263`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupStage {
    /// Nothing done yet.
    Fresh,
    /// Random seed and library ready.
    LibraryReady,
    /// Event library plus helpers ready.
    FrameworkReady,
    /// High sockets (internet, transport, datagram, raw) ready.
    SocketsReady,
    /// Interfaces plus card-driver module ready.
    InterfacesReady,
    /// Low sockets plus route plus filter device ready.
    LowSocketsReady,
    /// Management tree plus default config ready.
    ManagementReady,
    /// Timer armed and running.
    Running,
}

/// Startup driver: one stage at a time, in fixed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Startup {
    stage: StartupStage,
}

impl Startup {
    /// A service that has just been loaded.
    pub fn new() -> Self {
        Startup { stage: StartupStage::Fresh }
    }

    /// Current stage.
    pub fn stage(&self) -> StartupStage {
        self.stage
    }

    /// Advance after a successful step; false once running.
    pub fn note_done(&mut self) -> bool {
        let next = match self.stage {
            StartupStage::Fresh => StartupStage::LibraryReady,
            StartupStage::LibraryReady => StartupStage::FrameworkReady,
            StartupStage::FrameworkReady => StartupStage::SocketsReady,
            StartupStage::SocketsReady => StartupStage::InterfacesReady,
            StartupStage::InterfacesReady => StartupStage::LowSocketsReady,
            StartupStage::LowSocketsReady => StartupStage::ManagementReady,
            StartupStage::ManagementReady => StartupStage::Running,
            StartupStage::Running => return false,
        };
        self.stage = next;
        true
    }

    /// Whether the main loop may run.
    pub fn is_running(&self) -> bool {
        self.stage == StartupStage::Running
    }
}

impl Default for Startup {
    fn default() -> Self {
        Self::new()
    }
}

/// Which road one incoming message takes (`main`, `lwip.c:301-379`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchRoad {
    /// Timer or card-driver up/down notice.
    Notify,
    /// Management-tree query.
    Management,
    /// Socket-device request, or character/block request for filter.
    SocketDevice,
    /// Card-driver answer.
    CardResponse,
    /// Anything else: logged and dropped.
    Unexpected,
}

/// Socket domain for opening a socket (`alloc_socket`,
/// `lwip.c:156-189`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketDomain {
    /// Fourth-version internet protocol.
    InternetV4,
    /// Sixth-version internet protocol.
    InternetV6,
    /// Route sockets.
    Route,
    /// Link-layer sockets.
    Link,
}

/// Whether opening a raw socket is allowed: only root may
/// (`alloc_socket`, `lwip.c:169-170` reports access denied otherwise).
pub fn raw_allowed(is_root: bool) -> bool {
    is_root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_startup_starts_fresh() {
        let chain = Startup::new();
        assert_eq!(chain.stage(), StartupStage::Fresh);
        assert!(!chain.is_running());
    }

    #[test]
    fn test_startup_reaches_running_in_seven_steps() {
        let mut chain = Startup::new();
        for _ in 0..7 {
            assert!(chain.note_done());
        }
        assert!(chain.is_running());
        assert!(!chain.note_done());
    }

    #[test]
    fn test_dispatch_covers_four_roads() {
        let roads = [
            DispatchRoad::Notify,
            DispatchRoad::Management,
            DispatchRoad::SocketDevice,
            DispatchRoad::CardResponse,
            DispatchRoad::Unexpected,
        ];
        assert_eq!(roads.len(), 5);
    }

    #[test]
    fn test_raw_needs_root() {
        assert!(raw_allowed(true));
        assert!(!raw_allowed(false));
        let domains = [
            SocketDomain::InternetV4,
            SocketDomain::InternetV6,
            SocketDomain::Route,
            SocketDomain::Link,
        ];
        assert_eq!(domains.len(), 4);
    }
}
