//! Filesystem sidecar: slave nodes on the pseudo-terminal filesystem.
//!
//! C correspondence: `ptyfs_set`, `ptyfs_clear` (named `ptyfs_del` in the
//! plan), and the synchronous send helper in
//! `minix3/minix/drivers/tty/pty/ptyfs.c:1-112`, with request numbers
//! `PTYFS_SET` and `PTYFS_CLEAR` in `minix3/minix/include/minix/com.h:901
//! -902`.
//!
//! The filesystem names Unix98 slave nodes; the driver only asks it to add
//! or remove one node at a time. Endpoint lookup and message transport
//! stay in the service crate; this trait is the policy seam: the clone
//! path needs "clear, then proceed only on success".

/// Slave node attributes for one filesystem entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlaveNode {
    /// Pair index on the filesystem.
    pub index: u32,
    /// Node mode (character device, owner read-write).
    pub mode: u32,
    /// Owning user and group.
    pub owner: (u32, u32),
}

/// Filesystem behavior for slave nodes.
pub trait PtyFs {
    /// Add or update the node for this pair index.
    fn set(&mut self, node: SlaveNode) -> Result<(), FsError>;
    /// Remove the node for this pair index (missing is success).
    fn clear(&mut self, index: u32) -> Result<(), FsError>;
}

/// Filesystem failure: unavailable or refusing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    /// Filesystem not running (clone allocation must fail).
    Unavailable,
    /// Request refused.
    Refused,
}

/// Absent filesystem: every operation fails as unavailable.
///
/// C: `ptyfs_sendrec` fails when the data-store lookup finds no filesystem
/// (`ptyfs.c`), and the clone caller answers "try again".
#[derive(Debug, Default, Clone, Copy)]
pub struct NoPtyFs;

impl PtyFs for NoPtyFs {
    fn set(&mut self, _node: SlaveNode) -> Result<(), FsError> {
        Err(FsError::Unavailable)
    }

    fn clear(&mut self, _index: u32) -> Result<(), FsError> {
        Err(FsError::Unavailable)
    }
}

/// In-memory filesystem for tests: records nodes in a vector.
#[derive(Debug, Default, Clone)]
pub struct MemPtyFs {
    nodes: alloc::vec::Vec<SlaveNode>,
}

impl MemPtyFs {
    /// Fresh empty filesystem.
    pub fn new() -> MemPtyFs {
        MemPtyFs {
            nodes: alloc::vec::Vec::new(),
        }
    }

    /// Node for this index, if present.
    pub fn get(&self, index: u32) -> Option<SlaveNode> {
        self.nodes.iter().find(|node| node.index == index).copied()
    }
}

impl PtyFs for MemPtyFs {
    fn set(&mut self, node: SlaveNode) -> Result<(), FsError> {
        if let Some(slot) = self.nodes.iter_mut().find(|slot| slot.index == node.index) {
            *slot = node;
        } else {
            self.nodes.push(node);
        }
        Ok(())
    }

    fn clear(&mut self, index: u32) -> Result<(), FsError> {
        self.nodes.retain(|node| node.index != index);
        Ok(())
    }
}

/// Clone allocation gate: clear the stale node, proceed only on success.
///
/// C: the clone branch clears first and answers "try again" on failure
/// (`pty.c:151-159`), which also sweeps stale nodes after a restart.
pub fn clone_gate<F: PtyFs>(fs: &mut F, index: u32) -> bool {
    fs.clear(index).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(index: u32) -> SlaveNode {
        SlaveNode {
            index,
            mode: 0o620,
            owner: (0, 0),
        }
    }

    #[test]
    fn test_absent_filesystem_blocks_clone() {
        let mut fs = NoPtyFs;
        assert_eq!(fs.set(node(0)), Err(FsError::Unavailable));
        assert!(!clone_gate(&mut fs, 0));
    }

    #[test]
    fn test_memory_filesystem_sets_and_clears() {
        let mut fs = MemPtyFs::new();
        assert!(clone_gate(&mut fs, 3));
        fs.set(node(3)).unwrap();
        assert_eq!(fs.get(3), Some(node(3)));
        fs.clear(3).unwrap();
        assert_eq!(fs.get(3), None);
    }

    #[test]
    fn test_clear_missing_is_success() {
        let mut fs = MemPtyFs::new();
        assert!(fs.clear(9).is_ok());
    }
}
