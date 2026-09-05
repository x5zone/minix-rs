//! Slave-node storage: a fixed array behind an allocation bitmap
//! (`node.c` plus `node.h`).
//!
//! The table never grows: the system configuration fixes the terminal
//! count at thirty-two, small enough to preallocate everything up front
//! (`node.c:1-8`). Setting an index stores its metadata (refreshing an
//! allocated node keeps the slot, updating only the data); clearing an
//! index always succeeds, even when idle; reads report absent for idle
//! or out-of-range indexes. The upper bound for listings is the
//! configured count itself, not a tracked high-water mark
//! (`node.c:75-83`).

/// Configured terminal count (`NR_PTYS`, thirty-two).
pub const TERMINAL_COUNT: u32 = 32;

/// Metadata stored per slave node (`struct node_data`, `node.h:6-12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StoredNode {
    /// Device number of the slave side.
    pub device: u64,
    /// File mode.
    pub mode: u32,
    /// Owner user identifier.
    pub uid: u16,
    /// Owner group identifier.
    pub gid: u16,
    /// Creation time reported as all three timestamps.
    pub created: i64,
}

/// Fixed node table with an allocation bitmap.
pub struct NodeTable {
    /// Allocation bits, one per index.
    allocated: [u64; 1],
    /// Metadata per index.
    nodes: [StoredNode; TERMINAL_COUNT as usize],
}

impl NodeTable {
    /// Empty table (`init_nodes`, `node.c:20-25`).
    pub const fn new() -> Self {
        Self {
            allocated: [0],
            nodes: [StoredNode {
                device: 0,
                mode: 0,
                uid: 0,
                gid: 0,
                created: 0,
            }; TERMINAL_COUNT as usize],
        }
    }

    /// Store a node, allocating its index (`set_node`, `node.c:32-44`).
    /// Out-of-range indexes report no memory; refreshing an allocated
    /// index only updates the data.
    pub fn set(&mut self, index: u32, node: StoredNode) -> Result<(), super::PtyError> {
        if index >= TERMINAL_COUNT {
            return Err(super::PtyError::NoSpace);
        }
        self.allocated[0] |= 1 << index;
        self.nodes[index as usize] = node;
        Ok(())
    }

    /// Delete a node (`clear_node`, `node.c:51-55`): always succeeds.
    pub fn clear(&mut self, index: u32) {
        if index < TERMINAL_COUNT {
            self.allocated[0] &= !(1 << index);
        }
    }

    /// Read a node (`get_node`, `node.c:61-69`): absent when idle or
    /// out of range.
    pub fn get(&self, index: u32) -> Option<&StoredNode> {
        if index >= TERMINAL_COUNT || self.allocated[0] & (1 << index) == 0 {
            return None;
        }
        Some(&self.nodes[index as usize])
    }

    /// Read a node mutably.
    pub fn get_mut(&mut self, index: u32) -> Option<&mut StoredNode> {
        if index >= TERMINAL_COUNT || self.allocated[0] & (1 << index) == 0 {
            return None;
        }
        Some(&mut self.nodes[index as usize])
    }

    /// Upper bound for listing iterations (`get_max_node`).
    pub const fn upper_bound(&self) -> u64 {
        TERMINAL_COUNT as u64
    }
}

impl Default for NodeTable {
    fn default() -> Self {
        Self::new()
    }
}
