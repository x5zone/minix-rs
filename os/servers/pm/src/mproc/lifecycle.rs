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
#[derive(Default)]
pub enum Lifecycle {
    /// Slot not in use.
    ///
    /// Process table slot is free and can be allocated.
    #[default]
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
        ///
        /// bit7 = `WCOREFLAG`（0o200，core dumped，`main.c:357-358` 的
        /// `mp_sigstatus |= WCOREFLAG` 等价物，经 `set_core_flag` 置位）；
        /// 低 7 位为终止信号号。位语义在 u8 域成立——组合 wait status 时
        /// 经 `as u8` 取字节（wait.rs 的 ZOMBIE 环）。
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
    
    /// Checks if the C `EXITING` bit is set — true for the whole death
    /// window from `exit_proc` mark until slot release.
    ///
    /// C models `mp_flags` as a bitmask and `zombify` only ORs in
    /// `ZOMBIE`/`TRACE_ZOMBIE` (`forkexit.c:619/621`), never dropping
    /// `EXITING` — the bit dies only in `cleanup()` when the slot is
    /// freed. So an exiting process passes through Exiting →
    /// (Trace)Zombie → ToldParent with `EXITING` continuously true. The
    /// exclusive-enum lifecycle must reproduce that: every post-exit
    /// variant reports here, otherwise the VFS EXIT/CORE reply — which
    /// arrives AFTER `exit_proc` synchronously zombified the process —
    /// fails `assert(mp_flags & EXITING)` (`main.c:362`, NK4-C 1.54 B28:
    /// slot 13's normal exit panicked PM exactly here under the old
    /// Exiting-only semantics). Call sites mapping C bit tests
    /// (`& EXITING` / `(IN_USE|EXITING) != IN_USE`) all want this
    /// inclusive reading; variant-strict checks stay as explicit
    /// `matches!` on the concrete arm.
    pub fn is_exiting(&self) -> bool {
        matches!(
            self,
            Self::Exiting { .. }
                | Self::TraceZombie { .. }
                | Self::Zombie { .. }
                | Self::ToldParent { .. }
        )
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
        // C: tell_parent 只 OR TOLD_PARENT，EXITING 位直到 cleanup 才清
        //（forkexit.c:711-716 + 位定义）——ToldParent 仍在死亡窗口内。
        assert!(state.is_exiting());
        assert_eq!(state.exit_code(), Some((0, 0)));
    }
}
