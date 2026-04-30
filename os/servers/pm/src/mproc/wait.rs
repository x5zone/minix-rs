//! Parent wait state definition.
//!
//! **Important**: `WAITING` is the parent process's state, not the child's!
//!
//! When parent calls `wait()` or `waitpid()`, the parent's `WaitState` is set.

use minix_types::{Pid, VirBytes};

/// Parent wait state.
///
/// Corresponds to Minix3's `WAITING` flag and `mp_wpid`, `mp_waddr` fields.
#[derive(Debug, Clone, Default)]
pub struct WaitState {
    /// Whether waiting for child process (WAITING).
    pub waiting: bool,
    
    /// Wait target (mp_wpid).
    pub target: WaitTarget,
    
    /// rusage address (mp_waddr).
    ///
    /// Used to store child process resource usage.
    pub rusage_addr: VirBytes,
}

/// Wait target (mutually exclusive).
///
/// Corresponds to the first parameter of `waitpid()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitTarget {
    /// `wait()` - wait for any child.
    ///
    /// Corresponds to `pid == -1`.
    AnyChild,
    
    /// `waitpid(pid)` - wait for specific child.
    ///
    /// Corresponds to `pid > 0`.
    SpecificChild(Pid),
    
    /// `waitpid(-pgrp)` - wait for process group.
    ///
    /// Corresponds to `pid < -1`, wait for any child in process group `-pid`.
    Group(Pid),
}

impl Default for WaitTarget {
    fn default() -> Self {
        Self::AnyChild
    }
}

impl WaitState {
    /// Creates new wait state (default: not waiting).
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Checks if waiting for specified child.
    ///
    /// # Parameters
    /// - `child_pid`: Child process PID
    /// - `child_procgrp`: Child process's process group
    ///
    /// # Returns
    /// Returns `true` if parent is waiting for this child.
    pub fn is_waiting_for(&self, child_pid: Pid, child_procgrp: Pid) -> bool {
        if !self.waiting {
            return false;
        }
        
        match self.target {
            WaitTarget::AnyChild => true,
            WaitTarget::SpecificChild(pid) => child_pid == pid,
            WaitTarget::Group(pgrp) => child_procgrp == -pgrp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_not_waiting() {
        let state = WaitState::default();
        assert!(!state.waiting);
    }
    
    #[test]
    fn test_waiting_for_any_child() {
        let mut state = WaitState::default();
        state.waiting = true;
        state.target = WaitTarget::AnyChild;
        
        assert!(state.is_waiting_for(1234, 100));
        assert!(state.is_waiting_for(5678, 200));
    }
    
    #[test]
    fn test_waiting_for_specific_child() {
        let mut state = WaitState::default();
        state.waiting = true;
        state.target = WaitTarget::SpecificChild(1234);
        
        assert!(state.is_waiting_for(1234, 100));
        assert!(!state.is_waiting_for(5678, 100));
    }
    
    #[test]
    fn test_waiting_for_group() {
        let mut state = WaitState::default();
        state.waiting = true;
        state.target = WaitTarget::Group(-100);
        
        assert!(state.is_waiting_for(1234, 100));
        assert!(!state.is_waiting_for(5678, 200));
    }
}
