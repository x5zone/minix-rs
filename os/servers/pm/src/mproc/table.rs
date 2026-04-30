//! PM process table structure definition.
//!
//! This is the Rust implementation of Minix3's `mproc[NR_PROCS]`, containing PM's private process table management logic.
//!
//! # Minix3 Multi-Process Table Architecture
//! Minix3 uses a distributed process table design with 4 copies:
//! - **PM/mproc**: Process management, signals, permissions (this module)
//! - **VM/vmproc**: Virtual memory, page tables
//! - **VFS/fproc**: File descriptors, directories
//! - **Kernel/proc**: Scheduling, IPC, register saving
//!
//! # Design Decisions
//! - Uses static array `[Process; NR_PROCS]` to guarantee stable addresses
//! - Uses `Cell<usize>` for interior mutability (single-threaded safe)
//! - Preserves `IN_USE` semantics (`Lifecycle::Unused`)
//!
//! # Endpoint and Generation
//!
//! Minix3's Endpoint format:
//! ```text
//! endpoint = (generation << 15) + proc_nr
//! ```
//!
//! - **Low 15 bits**: process slot number
//! - **High 17 bits**: generation
//!
//! Generation's purpose:
//! - Prevents "stale messages sent to new processes"
//! - Each time a slot is released, generation +1
//! - Embedded in endpoint, **no separate storage needed**
//!
//! # Why in PM crate, not minix-types?
//!
//! 1. **Separation of concerns**: Process table slot allocation is PM's private logic
//! 2. **Invariant protection**: Slot allocation/release logic binds PM internal state
//! 3. **Microkernel principle**: Other services don't need to know PM's process table implementation

use core::cell::Cell;
use minix_types::{Endpoint, NR_PROCS, LAST_FEW};
use crate::mproc::{Process, Lifecycle, PidGenerator};

/// Endpoint generation shift.
///
/// Minix3 definition: `#define _ENDPOINT_GENERATION_SHIFT 15`
pub const ENDPOINT_GENERATION_SHIFT: u32 = 15;

/// PM process table.
///
/// Stores all PM process structures, provides slot allocation functionality.
///
/// # Memory Layout
/// ```text
/// ProcTable {
///     procs: [Process; 256],      // ~22.5 KB
///     procs_in_use: Cell<usize>,  // 8 bytes
///     next_child: Cell<usize>,    // 8 bytes
///     pid_generator: PidGenerator, // 4 bytes
/// }
/// ```
///
/// # Note
///
/// Generation is embedded in `Process.endpoint`, no separate storage needed.
/// This follows Minix3's design principle: **single truth**.
#[derive(Debug)]
pub struct ProcTable {
    /// Process array.
    pub procs: [Process; NR_PROCS],
    /// Number of processes currently in use.
    pub procs_in_use: Cell<usize>,
    /// Next child slot (round-robin algorithm).
    pub next_child: Cell<usize>,
    /// PID generator.
    ///
    /// Uses monotonic increment + conflict detection strategy.
    /// Corresponds to Minix3's `static pid_t next_pid`.
    pub pid_generator: PidGenerator,
}

impl ProcTable {
    /// Creates a new process table.
    ///
    /// All slots are initialized to `Lifecycle::Unused`.
    pub fn new() -> Self {
        Self {
            procs: core::array::from_fn(|_| Process::default()),
            procs_in_use: Cell::new(0),
            next_child: Cell::new(0),
            pid_generator: PidGenerator::new(),
        }
    }
    
    /// Gets a process reference.
    pub fn get(&self, index: usize) -> Option<&Process> {
        if index < NR_PROCS {
            Some(&self.procs[index])
        } else {
            None
        }
    }
    
    /// Gets a mutable process reference.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Process> {
        if index < NR_PROCS {
            Some(&mut self.procs[index])
        } else {
            None
        }
    }
    
    /// Gets the number of processes currently in use.
    pub fn count(&self) -> usize {
        self.procs_in_use.get()
    }
    
    /// Checks if the process table is full.
    pub fn is_full(&self) -> bool {
        self.procs_in_use.get() >= NR_PROCS
    }
    
    /// Iterates over all active processes.
    ///
    /// Returns an iterator containing only processes in use.
    ///
    /// # Usage
    ///
    /// Mainly used for PID conflict detection, iterating all active processes to check PID and process group ID conflicts.
    ///
    /// # Performance
    ///
    /// Uses Rust iterator's lazy evaluation and short-circuit evaluation:
    /// - **Lazy evaluation**: Only accesses processes when actually needed
    /// - **Short-circuit evaluation**: With `Iterator::any` etc., stops as soon as a match is found
    pub fn iter_active(&self) -> impl Iterator<Item = &Process> {
        self.procs.iter().filter(|p| p.is_in_use())
    }
    
    /// Checks if a non-root user can allocate a slot.
    ///
    /// Corresponds to Minix3's check:
    /// ```c
    /// if (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0)
    /// ```
    pub fn can_alloc_for_user(&self, is_root: bool) -> bool {
        let count = self.procs_in_use.get();
        if count >= NR_PROCS {
            return false;
        }
        if count >= NR_PROCS - LAST_FEW && !is_root {
            return false;
        }
        true
    }
    
    /// Finds a free slot (round-robin algorithm).
    ///
    /// Corresponds to the round-robin search in Minix3's `do_fork`:
    /// ```c
    /// do {
    ///     next_child = (next_child+1) % NR_PROCS;
    ///     n++;
    /// } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
    /// ```
    ///
    /// # Returns
    /// - `Some(usize)`: Found free slot index
    /// - `None`: Process table is full
    pub fn find_free_slot(&self) -> Option<usize> {
        let start = self.next_child.get();
        
        for i in 0..NR_PROCS {
            let idx = (start + i) % NR_PROCS;
            if !self.procs[idx].is_in_use() {
                self.next_child.set((idx + 1) % NR_PROCS);
                return Some(idx);
            }
        }
        
        None
    }
    
    /// Allocates a slot.
    ///
    /// Finds a free slot and marks it as in use.
    ///
    /// # Returns
    /// - `Some(usize)`: Allocated slot index
    /// - `None`: Process table is full
    pub fn alloc_slot(&self) -> Option<usize> {
        let slot = self.find_free_slot()?;
        self.procs_in_use.set(self.procs_in_use.get() + 1);
        Some(slot)
    }
    
    /// Releases a slot.
    ///
    /// Marks the slot as unused, increments generation.
    ///
    /// Note: This method doesn't check process state, only decrements the counter.
    /// Caller is responsible for ensuring process state has been properly reset.
    pub fn release_slot(&mut self, index: usize) {
        if index < NR_PROCS && self.procs_in_use.get() > 0 {
            self.procs_in_use.set(self.procs_in_use.get() - 1);
            
            let old_endpoint = self.procs[index].endpoint();
            let new_endpoint = Self::increment_endpoint_generation(old_endpoint);
            self.procs[index].identity.endpoint = new_endpoint;
        }
    }
    
    /// Calculates Endpoint.
    ///
    /// Minix3 formula: `endpoint = (generation << 15) + proc_nr`
    ///
    /// Note: This calculates the initial endpoint for a new process (generation = 0)
    pub fn calculate_endpoint(index: usize) -> Endpoint {
        if index < NR_PROCS {
            Endpoint(index as i32)
        } else {
            Endpoint::NONE
        }
    }

    /// Parses index from Endpoint.
    ///
    /// Uses Endpoint::slot() method.
    pub fn endpoint_to_index(endpoint: Endpoint) -> usize {
        endpoint.slot() as usize
    }

    /// Parses generation from Endpoint.
    ///
    /// Uses Endpoint::generation() method.
    pub fn endpoint_to_generation(endpoint: Endpoint) -> u32 {
        endpoint.generation() as u32
    }

    /// Increments Endpoint's generation.
    ///
    /// Used when releasing a slot, prevents stale messages from being sent to new processes.
    fn increment_endpoint_generation(endpoint: Endpoint) -> Endpoint {
        let generation = Self::endpoint_to_generation(endpoint);
        let index = Self::endpoint_to_index(endpoint);
        let new_gen = generation + 1;

        Endpoint::from_generation_slot(new_gen as i32, index as i32)
    }
    
    /// Validates if an Endpoint is valid.
    ///
    /// Checks if the endpoint's generation matches what's in the process table.
    pub fn validate_endpoint(&self, endpoint: Endpoint) -> bool {
        let index = Self::endpoint_to_index(endpoint);
        
        if index >= NR_PROCS {
            return false;
        }
        
        self.procs[index].endpoint() == endpoint
    }
}

impl Default for ProcTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_proc_table_new() {
        let table = ProcTable::new();
        assert_eq!(table.count(), 0);
        assert!(!table.is_full());
    }
    
    #[test]
    fn test_find_free_slot() {
        let table = ProcTable::new();
        let slot = table.find_free_slot().unwrap();
        assert!(slot < NR_PROCS);
    }
    
    #[test]
    fn test_alloc_slot() {
        let table = ProcTable::new();
        let slot = table.alloc_slot().unwrap();
        assert!(slot < NR_PROCS);
        assert_eq!(table.count(), 1);
    }
    
    #[test]
    fn test_release_slot() {
        let mut table = ProcTable::new();
        
        let slot = table.alloc_slot().unwrap();
        assert_eq!(table.count(), 1);
        
        table.procs[slot].identity.endpoint = ProcTable::calculate_endpoint(slot);
        let gen_before = ProcTable::endpoint_to_generation(table.procs[slot].endpoint());
        
        table.release_slot(slot);
        assert_eq!(table.count(), 0);
        
        let gen_after = ProcTable::endpoint_to_generation(table.procs[slot].endpoint());
        assert_eq!(gen_after, gen_before + 1);
    }
    
    #[test]
    fn test_can_alloc_for_user() {
        let table = ProcTable::new();
        
        assert!(table.can_alloc_for_user(false));
        assert!(table.can_alloc_for_user(true));
    }
    
    #[test]
    fn test_endpoint_calculation() {
        let endpoint = ProcTable::calculate_endpoint(5);
        let index = ProcTable::endpoint_to_index(endpoint);
        let gen_val = ProcTable::endpoint_to_generation(endpoint);
        
        assert_eq!(index, 5);
        assert_eq!(gen_val, 0);
    }
    
    #[test]
    fn test_endpoint_generation_increment() {
        let endpoint1 = ProcTable::calculate_endpoint(5);
        assert_eq!(ProcTable::endpoint_to_generation(endpoint1), 0);
        
        let endpoint2 = ProcTable::increment_endpoint_generation(endpoint1);
        assert_eq!(ProcTable::endpoint_to_generation(endpoint2), 1);
        assert_eq!(ProcTable::endpoint_to_index(endpoint2), 5);
        
        let endpoint3 = ProcTable::increment_endpoint_generation(endpoint2);
        assert_eq!(ProcTable::endpoint_to_generation(endpoint3), 2);
        assert_eq!(ProcTable::endpoint_to_index(endpoint3), 5);
    }
    
    #[test]
    fn test_endpoint_after_release() {
        let mut table = ProcTable::new();
        
        let slot = table.alloc_slot().unwrap();
        table.procs[slot].identity.endpoint = ProcTable::calculate_endpoint(slot);
        
        let endpoint_before = table.procs[slot].endpoint();
        
        table.release_slot(slot);
        
        let endpoint_after = table.procs[slot].endpoint();
        assert_ne!(endpoint_before, endpoint_after);
        
        assert!(!table.validate_endpoint(endpoint_before));
        assert!(table.validate_endpoint(endpoint_after));
    }
}
