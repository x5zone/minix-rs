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
/// - `VFS_CALL` → `ipc_blocked = Some(IpcBlockReason::VfsCall { .. })`
/// - `EVENT_CALL` → `ipc_blocked = Some(IpcBlockReason::EventCall)`
/// - `DELAY_CALL` → `ipc_blocked = Some(IpcBlockReason::DelayedSignal)`
/// - `NEW_PARENT` → `IpcBlockReason::VfsCall { reply_to_new_parent: true }`
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

/// Cursor into `EventRegistry::subs` (0..NR_SUBS).
///
/// C: `mp_eventsub` (`char`, `0..nsubs-1` or `-1 = NO_EVENTSUB`,
/// `const.h:13` / `mproc.h:27`). Rust encodes `NO_EVENTSUB` as `None`
/// (no `EventCall`), and `0..nsubs-1` as `Some(EventCursor(n))` inside
/// `EventCall`. [ARCH: A-2] `EVENT_CALL + mp_eventsub → EventCall { cursor }`
/// makes the two C fields' "born/die together" invariant unrepresentable
/// when violated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventCursor(pub usize);

/// IPC block reason (mutually exclusive).
///
/// Corresponds to three IPC block states in Minix3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcBlockReason {
    /// Waiting for VFS reply (VFS_CALL).
    ///
    /// Process is waiting for file system operation to complete.
    ///
    /// `reply_to_new_parent` corresponds to Minix3's `NEW_PARENT` flag:
    /// the process's parent changed (was adopted by INIT) while the VFS call
    /// was in flight, so the reply must be delivered to the new parent.
    /// In C, `NEW_PARENT` is only ever set while `VFS_CALL` is set
    /// (forkexit.c:402-404) and both are cleared together
    /// (main.c:327-328); encoding it as a payload makes the combination
    /// unrepresentable when invalid.
    VfsCall {
        /// NEW_PARENT: reply should go to the new parent (INIT).
        reply_to_new_parent: bool,
    },

    /// Waiting for process event subscriber (EVENT_CALL).
    ///
    /// Process is waiting for event notification.
    ///
    /// `cursor` is `mp_eventsub` (next subscriber to try, `event.c:97`).
    /// C: `mp_flags & EVENT_CALL` + `mp_eventsub` born/die together
    /// (`event.c:116-117 / 349-350`); Rust merges them.
    EventCall {
        /// Next subscriber index to try (0..NR_SUBS).
        cursor: EventCursor,
    },

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
        matches!(self.ipc_blocked, Some(IpcBlockReason::VfsCall { .. }))
    }
    
    /// Checks if waiting for event.
    pub fn is_event_blocked(&self) -> bool {
        matches!(self.ipc_blocked, Some(IpcBlockReason::EventCall { .. }))
    }

    /// Returns event cursor if `EVENT_CALL` is set.
    pub fn event_cursor(&self) -> Option<EventCursor> {
        match self.ipc_blocked {
            Some(IpcBlockReason::EventCall { cursor }) => Some(cursor),
            _ => None,
        }
    }

    /// Sets `EVENT_CALL` with given cursor.
    pub fn set_event_blocked(&mut self, cursor: EventCursor) {
        self.ipc_blocked = Some(IpcBlockReason::EventCall { cursor });
    }

    /// Clears `EVENT_CALL` (if set).
    pub fn clear_event_blocked(&mut self) {
        if matches!(self.ipc_blocked, Some(IpcBlockReason::EventCall { .. })) {
            self.ipc_blocked = None;
        }
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
                IpcBlockReason::VfsCall { reply_to_new_parent } => {
                    write!(f, "vfs_blocked")?;
                    if reply_to_new_parent {
                        write!(f, "(new_parent)")?;
                    }
                }
                IpcBlockReason::EventCall { cursor } => {
                    write!(f, "event_blocked({})", cursor.0)?;
                }
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
        state.ipc_blocked = Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
        assert!(state.is_blocked());
        assert!(state.is_vfs_blocked());
        assert!(!state.is_event_blocked());
    }
    
    #[test]
    fn test_vfs_call_new_parent_payload() {
        // NEW_PARENT only exists during a VFS call: the flag combination
        // (VFS_CALL | NEW_PARENT) is encoded as a single enum payload.
        let mut state = BlockState::default();
        state.ipc_blocked = Some(IpcBlockReason::VfsCall { reply_to_new_parent: true });
        assert!(state.is_vfs_blocked());
        assert!(matches!(
            state.ipc_blocked,
            Some(IpcBlockReason::VfsCall { reply_to_new_parent: true })
        ));
    }
    
    #[test]
    fn test_combined_state() {
        let mut state = BlockState::default();
        state.stopped = true;
        state.ipc_blocked = Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
        state.unpaused = true;
        assert!(state.is_blocked());
    }
}
