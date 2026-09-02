//! Guardianship relationship definition.
//!
//! Resolves the coupling issue of `mp_parent` and `mp_tracer` in Minix3.
//!
//! # Design Improvement
//! In `Normal` state there's no `tracer` field, preventing misuse.

use minix_types::UserSlot;
use bitflags::bitflags;

/// Guardianship.
///
/// Describes the parent and tracer relationship of a process.
///
/// # Minix3 Mapping
/// - `mp_parent` → `Normal { parent }` or `Traced { parent, .. }`
/// - `mp_tracer` → `Traced { tracer, .. }`
/// - `TRACE_EXIT` → `Traced { trace_exit: true, .. }`
/// - `mp_trace_flags` → `Traced { trace_options, .. }`
/// - `NO_TRACER (-1)` → `Normal`
#[derive(Debug, Clone)]
pub enum Guardianship {
    /// Normal state: only has a parent.
    Normal { 
        /// Parent process index.
        parent: UserSlot 
    },
    
    /// Debug state: hijacked by tracer.
    ///
    /// Tracer may not equal parent.
    Traced{
        /// Parent process index.
        parent: UserSlot,
        /// Tracer process index.
        tracer: UserSlot,
        /// TRACE_EXIT flag: tracer is forcing process exit.
        trace_exit: bool,
        /// Trace options (mp_trace_flags).
        trace_options: TraceOptions,
    },
}

impl Default for Guardianship {
    fn default() -> Self {
        Self::Normal {
            parent: UserSlot::new(0),
        }
    }
}

bitflags! {
    /// Trace options.
    ///
    /// Corresponds to Minix3's `mp_trace_flags` field.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TraceOptions: u32 {
        /// TO_TRACEFORK: Auto attach to forked child process.
        const TRACEFORK = 0x1;
        /// TO_ALTEXEC: Send SIGSTOP on successful exec.
        const ALTEXEC = 0x2;
        /// TO_NOEXEC: Don't send signal on successful exec.
        const NOEXEC = 0x4;
    }
}

impl Guardianship {
    /// Gets parent process index.
    ///
    /// Parent always exists regardless of being traced.
    pub fn parent(&self) -> UserSlot {
        match self {
            Self::Normal { parent } => *parent,
            Self::Traced { parent, .. } => *parent,
        }
    }
    
    /// Gets tracer process index.
    ///
    /// Returns `None` if process is not being traced.
    pub fn tracer(&self) -> Option<UserSlot> {
        match self {
            Self::Normal { .. } => None,
            Self::Traced { tracer, .. } => Some(*tracer),
        }
    }
    
    /// Checks if process is being traced.
    pub fn is_traced(&self) -> bool {
        matches!(self, Self::Traced { .. })
    }
    
    /// Gets TRACE_EXIT flag.
    ///
    /// Returns `false` if process is not being traced.
    pub fn trace_exit(&self) -> bool {
        match self {
            Self::Normal { .. } => false,
            Self::Traced { trace_exit, .. } => *trace_exit,
        }
    }

    /// Gets trace options.
    pub fn trace_options(&self) -> TraceOptions {
        match self {
            Self::Normal { .. } => TraceOptions::empty(),
            Self::Traced { trace_options, .. } => *trace_options,
        }
    }

    /// Sets trace options (`T_SETOPT`, `trace.c:162`).
    pub fn set_trace_options(&mut self, bits: u32) {
        if let Self::Traced { trace_options, .. } = self {
            *trace_options = TraceOptions::from_bits_truncate(bits);
        }
    }

    /// Tries to set tracer (`T_OK`, `trace.c:58`).
    pub fn try_set_tracer(&mut self, parent: UserSlot) -> Result<(), ()> {
        if self.tracer().is_some() {
            return Err(());
        }
        let p = self.parent();
        *self = Self::Traced {
            parent: p,
            tracer: parent,
            trace_exit: false,
            trace_options: TraceOptions::empty(),
        };
        Ok(())
    }

    /// Clears tracer (`T_DETACH`, `trace.c:194`).
    pub fn clear_tracer(&mut self) {
        let parent = self.parent();
        *self = Self::Normal { parent };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_is_normal() {
        let g = Guardianship::default();
        assert!(matches!(g, Guardianship::Normal { .. }));
        assert!(!g.is_traced());
        assert!(g.tracer().is_none());
    }
    
    #[test]
    fn test_normal_parent() {
        let g = Guardianship::Normal { parent: UserSlot::new(5) };
        assert_eq!(g.parent(), UserSlot::new(5));
        assert!(!g.is_traced());
        assert!(g.tracer().is_none());
        assert!(!g.trace_exit());
    }
    
    #[test]
    fn test_traced_state() {
        let g = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
            trace_exit: false,
            trace_options: TraceOptions::empty(),
        };
        assert!(g.is_traced());
        assert_eq!(g.parent(), UserSlot::new(1));
        assert_eq!(g.tracer(), Some(UserSlot::new(2)));
        assert!(!g.trace_exit());
    }
    
    #[test]
    fn test_trace_exit_flag() {
        let g = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
            trace_exit: true,
            trace_options: TraceOptions::empty(),
        };
        assert!(g.trace_exit());
    }
    
    #[test]
    fn test_trace_options() {
        let opts = TraceOptions::TRACEFORK | TraceOptions::ALTEXEC;
        assert!(opts.contains(TraceOptions::TRACEFORK));
        assert!(opts.contains(TraceOptions::ALTEXEC));
        assert!(!opts.contains(TraceOptions::NOEXEC));
    }
}
