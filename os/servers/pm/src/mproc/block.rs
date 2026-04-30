//! Process block state definition.
//!
//! Block states can be combined with lifecycle states.
//!
//! # Key Constraints
//! - `PROC_STOPPED` can combine with `Running` or `Exiting`
//! - `VFS_CALL` can combine with `Exiting`

use core::fmt;

/// Process block state.
///
/// Corresponds to block-related bits in Minix3's `mp_flags`:
/// - `PROC_STOPPED` → `stopped`
/// - `VFS_CALL` / `EVENT_CALL` / `DELAY_CALL` → `ipc_blocked`
/// - `UNPAUSED` → `unpaused`
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockState {
    /// Whether stopped in kernel (PROC_STOPPED).
    ///
    /// Can combine with `Running` / `Exiting`.
    pub stopped: bool,
    
    /// IPC block reason.
    ///
    /// Process is waiting for IPC response.
    pub ipc_blocked: Option<IpcBlockReason>,
    
    /// VFS has replied to unpause request (UNPAUSED).
    pub unpaused: bool,
}

/// IPC block reason (mutually exclusive).
///
/// Corresponds to three IPC block states in Minix3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcBlockReason {
    /// Waiting for VFS reply (VFS_CALL).
    ///
    /// Process is waiting for file system operation to complete.
    VfsCall,
    
    /// Waiting for process event subscriber (EVENT_CALL).
    ///
    /// Process is waiting for event notification.
    EventCall,
    
    /// Waiting for call completion before sending signal (DELAY_CALL).
    ///
    /// Signal needs to be delayed until IPC completes.
    DelayedSignal,
}

impl BlockState {
    /// Creates new block state (default: no block).
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Checks if process is blocked.
    ///
    /// Includes being stopped or waiting for IPC.
    pub fn is_blocked(&self) -> bool {
        self.stopped || self.ipc_blocked.is_some()
    }
    
    /// Checks if waiting for VFS.
    pub fn is_vfs_blocked(&self) -> bool {
        matches!(self.ipc_blocked, Some(IpcBlockReason::VfsCall))
    }
    
    /// Checks if waiting for event.
    pub fn is_event_blocked(&self) -> bool {
        matches!(self.ipc_blocked, Some(IpcBlockReason::EventCall))
    }
}

impl fmt::Display for BlockState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        
        if self.stopped {
            write!(f, "stopped")?;
            first = false;
        }
        if let Some(reason) = self.ipc_blocked {
            if !first {
                write!(f, ", ")?;
            }
            match reason {
                IpcBlockReason::VfsCall => write!(f, "vfs_blocked")?,
                IpcBlockReason::EventCall => write!(f, "event_blocked")?,
                IpcBlockReason::DelayedSignal => write!(f, "delayed_signal")?,
            }
            first = false;
        }
        if self.unpaused {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "unpaused")?;
        }
        
        if first {
            write!(f, "none")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_not_blocked() {
        let state = BlockState::default();
        assert!(!state.is_blocked());
    }
    
    #[test]
    fn test_stopped() {
        let mut state = BlockState::default();
        state.stopped = true;
        assert!(state.is_blocked());
    }
    
    #[test]
    fn test_vfs_blocked() {
        let mut state = BlockState::default();
        state.ipc_blocked = Some(IpcBlockReason::VfsCall);
        assert!(state.is_blocked());
        assert!(state.is_vfs_blocked());
        assert!(!state.is_event_blocked());
    }
    
    #[test]
    fn test_combined_state() {
        let mut state = BlockState::default();
        state.stopped = true;
        state.ipc_blocked = Some(IpcBlockReason::VfsCall);
        state.unpaused = true;
        assert!(state.is_blocked());
    }
}
