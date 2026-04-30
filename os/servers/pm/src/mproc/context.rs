//! PM context structure definition.
//!
//! This is the Rust implementation of Minix3's `mp` macro, containing PM's private context management logic.
//!
//! # Minix3 Multi-Process Table Architecture
//! Minix3 uses a distributed process table design with 4 copies:
//! - **PM/mproc**: Process management, signals, permissions (this module)
//! - **VM/vmproc**: Virtual memory, page tables
//! - **VFS/fproc**: File descriptors, directories
//! - **Kernel/proc**: Scheduling, IPC, register saving
//!
//! # Design Decisions
//! - Uses `PmContext` to wrap process table and current process index
//! - C's implicit global state → Rust's explicit capability
//! - Uses borrow checker as "compile-time lock"
//!
//! # Why in PM crate, not minix-types?
//!
//! 1. **Separation of concerns**: PM context is PM's private concept
//! 2. **Invariant protection**: Context management binds PM internal state
//! 3. **Microkernel principle**: Other services don't need to know PM's context implementation

use minix_types::UserSlot;
use crate::mproc::{ProcTable, Process, Privilege, Credentials};

/// PM context.
///
/// Wraps process table and current process, provides context for system calls like do_fork.
///
/// # Design Philosophy
///
/// In Minix3 C code, `mp` is a global macro:
/// ```c
/// #define mp (&mproc[who_p])
/// ```
///
/// In Rust, we wrap it as `PmContext`:
/// - Explicit context passing, not implicit global state
/// - Uses borrow checker for safety
/// - Testable (can create arbitrary contexts)
///
/// # Borrow Checker as "Compile-Time Lock"
///
/// When you create a `PmContext`, you lock access to `ProcTable`:
/// - If you create `&mut ProcTable`, Rust guarantees that throughout the `PmContext` lifetime,
///   no other code can secretly read or write the process table
/// - This is a compile-time lock
pub struct PmContext<'a> {
    /// Process table reference.
    pub table: &'a mut ProcTable,
    /// Current process index.
    pub current: usize,
}

impl<'a> PmContext<'a> {
    /// Creates new PM context.
    pub fn new(table: &'a mut ProcTable, current: usize) -> Self {
        Self { table, current }
    }
    
    /// Gets current process reference.
    pub fn current_proc(&self) -> &Process {
        &self.table.procs[self.current]
    }
    
    /// Gets current process mutable reference.
    pub fn current_proc_mut(&mut self) -> &mut Process {
        &mut self.table.procs[self.current]
    }
    
    /// Gets process reference.
    pub fn get_proc(&self, index: usize) -> Option<&Process> {
        self.table.get(index)
    }
    
    /// Gets process mutable reference.
    pub fn get_proc_mut(&mut self, index: usize) -> Option<&mut Process> {
        self.table.get_mut(index)
    }
    
    /// Checks if current process is root.
    pub fn is_root(&self) -> bool {
        match &self.current_proc().resources.privilege {
            Privilege::User(creds) => creds.user.real == 0,
            Privilege::Kernel => true,
        }
    }
    
    /// Checks if slot can be allocated.
    pub fn can_alloc(&self) -> bool {
        self.table.can_alloc_for_user(self.is_root())
    }
    
    /// Gets parent process index.
    pub fn parent_index(&self) -> UserSlot {
        self.current_proc().state.guardianship.parent()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::Lifecycle;
    
    #[test]
    fn test_pm_context_new() {
        let mut table = ProcTable::new();
        let ctx = PmContext::new(&mut table, 0);
        assert_eq!(ctx.current, 0);
    }
    
    #[test]
    fn test_current_proc() {
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        let ctx = PmContext::new(&mut table, 0);
        assert!(ctx.current_proc().is_in_use());
    }
}
