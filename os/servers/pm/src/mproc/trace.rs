//! Trace state definition.
//!
//! Independent from guardianship, describes the state of a process stopped due to tracing.

/// Trace state.
///
/// Corresponds to Minix3's `TRACE_STOPPED` flag and `TRACE_EXIT`.
///
/// # Note
/// `TRACE_STOPPED` is the state of a process stopped due to tracing,
/// can combine with `Running` or `Exiting`. `TRACE_EXIT` indicates tracer
/// forced exit (signal.c:695-698, 09-pm-exit.md).
#[derive(Debug, Clone, Default)]
pub struct TraceState {
    /// Whether stopped due to tracing (TRACE_STOPPED).
    pub stopped: bool,
    /// Whether tracer forced exit (TRACE_EXIT, `exit_proc` pending).
    pub exit_pending: bool,
}

impl TraceState {
    /// Creates new trace state (default: not stopped).
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Checks if stopped due to tracing.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_not_stopped() {
        let state = TraceState::default();
        assert!(!state.is_stopped());
    }
    
    #[test]
    fn test_stopped() {
        let mut state = TraceState::default();
        state.stopped = true;
        assert!(state.is_stopped());
    }
}
