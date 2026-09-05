//! Process-slot arithmetic and the two-pass directory refresh (`tree.c`).
//!
//! The process server never trusts its cached tree: every listing or
//! lookup first reconciles the tree against a fresh process list from the
//! system-information service. Reconciliation runs in two passes so a
//! rapidly reused process identifier never collides with its own stale
//! entry (`tree.c:148-154`): the first pass deletes every directory whose
//! slot went idle or whose identifier or owner changed, and the second
//! pass creates every missing directory. Per-file entries inside one
//! process directory are built on demand instead: a lookup builds the one
//! named file, a listing builds every file not yet present.

use minix_vtreefs::{NodeId, NodeStat, Tree, TreeError};

use alloc::vec::Vec;

/// Owner and liveness of one kernel slot (the fields this server reads
/// from the system-information listing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotInfo {
    /// Whether the slot is in use.
    pub in_use: bool,
    /// Process identifier visible in the directory name.
    pub pid: i64,
    /// Effective owner user identifier.
    pub uid: u16,
    /// Effective owner group identifier.
    pub gid: u16,
}

/// Map a kernel slot to its process identifier (`pid_from_slot`,
/// `tree.c:12-26`): kernel tasks are always present and keep negative
/// identifiers derived from their slot; user slots report their stored
/// identifier when in use and zero when idle.
pub fn pid_from_slot(slot: usize, task_slots: usize, slots: &[SlotInfo]) -> i64 {
    if slot < task_slots {
        return slot as i64 - task_slots as i64;
    }
    match slots.get(slot - task_slots) {
        Some(info) if info.in_use => info.pid,
        _ => 0,
    }
}

/// Whether a directory's recorded owner still matches the slot
/// (`check_owner`, `tree.c:32-43`): kernel tasks always match; user slots
/// compare the stored owner pair. A changed owner rebuilds the whole
/// subtree, so no open file survives the ownership change
/// (`tree.c:183-186`).
pub fn owner_matches(recorded_uid: u16, recorded_gid: u16, slot: usize, task_slots: usize, slots: &[SlotInfo]) -> bool {
    if slot < task_slots {
        return true;
    }
    match slots.get(slot - task_slots) {
        Some(info) => info.uid == recorded_uid && info.gid == recorded_gid,
        None => false,
    }
}

/// Metadata for a slot's directory or one of its files (`make_stat`,
/// `tree.c:49-68`): directories take the world-accessible mode, files
/// take their registered mode; kernel tasks belong to the superuser,
/// user slots to their recorded owner; sizes stay zero because content
/// is generated on read.
pub fn slot_stat(mode: u16, slot: usize, task_slots: usize, slots: &[SlotInfo]) -> NodeStat {
    let (uid, gid) = if slot < task_slots {
        (0, 0)
    } else {
        match slots.get(slot - task_slots) {
            Some(info) => (info.uid, info.gid),
            None => (0, 0),
        }
    };
    NodeStat {
        mode,
        uid,
        gid,
        size: 0,
        device: 0,
    }
}

/// One directory the refresh must delete (slot, recorded identifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deletion {
    /// Kernel slot whose directory goes away.
    pub slot: usize,
}

/// One directory the refresh must create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Creation {
    /// Kernel slot getting a directory.
    pub slot: usize,
    /// Process identifier for the directory name.
    pub pid: i64,
}

/// Plan the two-pass refresh without touching the tree: compare every
/// slot against the directories the tree currently holds (given as
/// slot-to-identifier pairs), and report which directories to delete
/// first and which to create second. A directory is deleted when its slot
/// went idle, its identifier changed, or its owner changed; a directory
/// is created when its slot is live and no current directory stands for
/// it. Callers delete the whole `deletions` list before creating
/// anything, so a reused identifier never meets its stale self.
pub fn plan_refresh(
    task_slots: usize,
    slots: &[SlotInfo],
    current: &[(usize, i64, u16, u16)],
) -> (Vec<Deletion>, Vec<Creation>) {
    let mut deletions = Vec::new();
    let mut creations = Vec::new();
    let total = task_slots + slots.len();
    for slot in 0..total {
        let pid = pid_from_slot(slot, task_slots, slots);
        let held = current.iter().find(|entry| entry.0 == slot);
        match (pid == 0, held) {
            (true, Some(_)) => deletions.push(Deletion { slot }),
            (false, Some((_, old_pid, uid, gid))) => {
                if *old_pid != pid || !owner_matches(*uid, *gid, slot, task_slots, slots) {
                    deletions.push(Deletion { slot });
                    creations.push(Creation { slot, pid });
                }
            }
            (false, None) => creations.push(Creation { slot, pid }),
            (true, None) => {}
        }
    }
    (deletions, creations)
}

/// Apply one deletion to the tree.
pub fn apply_deletion(tree: &mut Tree, root: NodeId, slot: usize) -> Result<(), TreeError> {
    if let Some(node) = tree.child_by_index(root, slot as i32) {
        tree.delete(node)?;
    }
    Ok(())
}

/// Apply one creation to the tree.
pub fn apply_creation(
    tree: &mut Tree,
    root: NodeId,
    creation: Creation,
    dir_mode: u16,
    indexed_slots: i32,
) -> Result<(), TreeError> {
    if tree.child_by_index(root, creation.slot as i32).is_some() {
        return Ok(());
    }
    let mut name = [0u8; 24];
    let text = format_pid(creation.pid, &mut name);
    let stat = NodeStat {
        mode: dir_mode,
        uid: 0,
        gid: 0,
        size: 0,
        device: 0,
    };
    tree.add(root, text.as_bytes(), creation.slot as i32, stat, indexed_slots, creation.pid as u64)?;
    Ok(())
}

/// Render a process identifier as decimal text.
fn format_pid(pid: i64, out: &mut [u8; 24]) -> &str {
    let mut end = 24;
    let negative = pid < 0;
    let mut rest = pid.unsigned_abs();
    if rest == 0 {
        end -= 1;
        out[end] = b'0';
    } else {
        while rest > 0 {
            end -= 1;
            out[end] = b'0' + (rest % 10) as u8;
            rest /= 10;
        }
    }
    if negative {
        end -= 1;
        out[end] = b'-';
    }
    core::str::from_utf8(&out[end..]).unwrap_or("?")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots() -> Vec<SlotInfo> {
        use alloc::vec;
        vec![
            SlotInfo { in_use: true, pid: 100, uid: 10, gid: 10 },
            SlotInfo { in_use: false, pid: 0, uid: 0, gid: 0 },
            SlotInfo { in_use: true, pid: 101, uid: 11, gid: 11 },
        ]
    }

    #[test]
    fn test_pid_from_slot_tasks_and_users() {
        let slots = slots();
        // Kernel tasks are always present with slot-derived identifiers.
        assert_eq!(pid_from_slot(0, 2, &slots), -2);
        assert_eq!(pid_from_slot(1, 2, &slots), -1);
        // Live user slots report their identifier, idle slots zero.
        assert_eq!(pid_from_slot(2, 2, &slots), 100);
        assert_eq!(pid_from_slot(3, 2, &slots), 0);
        assert_eq!(pid_from_slot(4, 2, &slots), 101);
        assert_eq!(pid_from_slot(9, 2, &slots), 0);
    }

    #[test]
    fn test_owner_matches_tasks_always() {
        let slots = slots();
        assert!(owner_matches(999, 999, 0, 2, &slots));
        assert!(owner_matches(10, 10, 2, 2, &slots));
        assert!(!owner_matches(10, 99, 2, 2, &slots));
        assert!(!owner_matches(0, 0, 9, 2, &slots));
    }

    #[test]
    fn test_plan_refresh_two_passes() {
        let slots = slots();
        // Tree holds slot 2 as pid 100 (current), slot 3 as pid 55
        // (stale: slot idle now), and nothing for slot 4 (missing).
        let current = [(2usize, 100i64, 10u16, 10u16), (3usize, 55i64, 0u16, 0u16)];
        let (deletions, creations) = plan_refresh(2, &slots, &current);
        // Pass one deletes the stale slot-3 directory (plus task slots
        // 0 and 1 are live but unlisted, so they are created).
        assert!(deletions.contains(&Deletion { slot: 3 }));
        // Pass two creates the missing slot-4 directory and the two
        // task directories.
        let created: Vec<usize> = creations.iter().map(|entry| entry.slot).collect();
        assert!(created.contains(&4));
        assert!(created.contains(&0));
        assert!(!created.contains(&2));
    }

    #[test]
    fn test_plan_refresh_rebuilds_on_owner_change() {
        let slots = slots();
        // Slot 2 live as pid 100 but the recorded owner differs.
        let current = [(2usize, 100i64, 77u16, 77u16)];
        let (deletions, creations) = plan_refresh(2, &slots, &current);
        assert!(deletions.contains(&Deletion { slot: 2 }));
        assert!(creations.iter().any(|entry| entry.slot == 2 && entry.pid == 100));
    }

    #[test]
    fn test_format_pid_decimal() {
        let mut out = [0u8; 24];
        assert_eq!(format_pid(100, &mut out), "100");
        assert_eq!(format_pid(-2, &mut out), "-2");
        assert_eq!(format_pid(0, &mut out), "0");
    }
}
