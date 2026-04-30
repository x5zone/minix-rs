//! Process lifecycle state definition.
//!
//! Lifecycle states are mutually exclusive - a process can only be in one lifecycle state at a time.
//!
//! # State Transition Diagram
//! ```text
//! Unused ──────→ Running ──────→ Exiting ──────→ TraceZombie ──┐
//!                  │                │                  │       │
//!                  │                │                  ↓       │
//!                  │                └─────────────→ Zombie ←───┘
//!                  │                                      │
//!                  │                                      ↓
//!                  └──────────────────────────────→ ToldParent
//! ```

use core::fmt;

/// Process lifecycle state (mutually exclusive).
///
/// Corresponds to lifecycle-related bits in Minix3's `mp_flags`:
/// - `IN_USE` → `!Unused`
/// - `EXITING` → `Exiting`
/// - `TRACE_ZOMBIE` → `TraceZombie`
/// - `ZOMBIE` → `Zombie`
/// - `TOLD_PARENT` → `ToldParent`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// Slot not in use.
    ///
    /// Process table slot is free and can be allocated.
    Unused,
    
    /// Running normally.
    ///
    /// Process is executing, may have `BlockState::stopped = true` simultaneously.
    Running,
    
    /// Exiting.
    ///
    /// Process is in exit flow, may simultaneously have:
    /// - `VFS_CALL`: waiting for VFS cleanup
    /// - `PROC_STOPPED`: stopped by signal
    /// - `TRACE_EXIT`: tracer forced exit
    Exiting {
        /// Exit status code.
        exit_code: i8,
        /// Signal status (if killed by signal).
        sig_status: i8,
    },
    
    /// Trace zombie state.
    ///
    /// Process has exited, waiting for tracer to reap (when tracer != parent).
    /// Corresponds to `TRACE_ZOMBIE` flag.
    TraceZombie {
        exit_code: i8,
        sig_status: i8,
    },
    
    /// Zombie state.
    ///
    /// Process has exited, waiting for parent to reap.
    /// Corresponds to `ZOMBIE` flag.
    Zombie {
        exit_code: i8,
        sig_status: i8,
    },
    
    /// Parent notified.
    ///
    /// Parent has been notified of child exit, waiting for cleanup.
    /// Corresponds to `TOLD_PARENT` flag.
    ToldParent {
        exit_code: i8,
        sig_status: i8,
    },
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::Unused
    }
}

impl Lifecycle {
    /// Checks if slot is in use.
    ///
    /// Corresponds to `IN_USE` flag.
    pub fn is_in_use(&self) -> bool {
        !matches!(self, Self::Unused)
    }
    
    /// Gets exit status code.
    ///
    /// Returns `(exit_code, sig_status)`, or `None` if not an exit-related state.
    pub fn exit_code(&self) -> Option<(i8, i8)> {
        match self {
            Self::Exiting { exit_code, sig_status } 
            | Self::TraceZombie { exit_code, sig_status }
            | Self::Zombie { exit_code, sig_status }
            | Self::ToldParent { exit_code, sig_status } => {
                Some((*exit_code, *sig_status))
            }
            _ => None,
        }
    }
    
    /// Checks if in zombie state.
    ///
    /// Includes `Zombie` and `TraceZombie`.
    pub fn is_zombie(&self) -> bool {
        matches!(self, Self::Zombie { .. } | Self::TraceZombie { .. })
    }
    
    /// Checks if exiting.
    pub fn is_exiting(&self) -> bool {
        matches!(self, Self::Exiting { .. })
    }
}

impl fmt::Display for Lifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unused => write!(f, "Unused"),
            Self::Running => write!(f, "Running"),
            Self::Exiting { exit_code, sig_status } => {
                write!(f, "Exiting(exit={}, sig={})", exit_code, sig_status)
            }
            Self::TraceZombie { exit_code, sig_status } => {
                write!(f, "TraceZombie(exit={}, sig={})", exit_code, sig_status)
            }
            Self::Zombie { exit_code, sig_status } => {
                write!(f, "Zombie(exit={}, sig={})", exit_code, sig_status)
            }
            Self::ToldParent { exit_code, sig_status } => {
                write!(f, "ToldParent(exit={}, sig={})", exit_code, sig_status)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_is_unused() {
        let lifecycle = Lifecycle::default();
        assert!(matches!(lifecycle, Lifecycle::Unused));
        assert!(!lifecycle.is_in_use());
    }
    
    #[test]
    fn test_running_is_in_use() {
        let lifecycle = Lifecycle::Running;
        assert!(lifecycle.is_in_use());
        assert!(!lifecycle.is_zombie());
        assert!(!lifecycle.is_exiting());
    }
    
    #[test]
    fn test_exiting_state() {
        let lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 9 };
        assert!(lifecycle.is_in_use());
        assert!(lifecycle.is_exiting());
        assert!(!lifecycle.is_zombie());
        assert_eq!(lifecycle.exit_code(), Some((0, 9)));
    }
    
    #[test]
    fn test_zombie_states() {
        let zombie = Lifecycle::Zombie { exit_code: 42, sig_status: 0 };
        assert!(zombie.is_zombie());
        assert_eq!(zombie.exit_code(), Some((42, 0)));
        
        let trace_zombie = Lifecycle::TraceZombie { exit_code: 0, sig_status: 11 };
        assert!(trace_zombie.is_zombie());
        assert_eq!(trace_zombie.exit_code(), Some((0, 11)));
    }
    
    #[test]
    fn test_told_parent() {
        let state = Lifecycle::ToldParent { exit_code: 0, sig_status: 0 };
        assert!(state.is_in_use());
        assert!(!state.is_zombie());
        assert!(!state.is_exiting());
        assert_eq!(state.exit_code(), Some((0, 0)));
    }
}
