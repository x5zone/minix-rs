//! Block and inode placement: group selectors, preallocation windows,
//! and reserved-block admission (`balloc.c`, `ialloc.c`).
//!
//! The second extended filesystem spreads inodes and blocks across
//! block groups, and placement decides performance: related files in one
//! group read sequentially, directories spread across groups balance
//! fullness. Four inode selectors compete (chosen by mount options):
//! the Orlov spread for directories, the parent-group hash for files,
//! the fullest-group scan for old trees, and the first-fit scan that
//! imitates the Minix allocator. Blocks allocate near a goal block with
//! an eight-block preallocation window for sequential writers, and the
//! last few blocks stay reserved unless the mounter opts in.
//!
//! Every selector here is pure: group statistics go in, a group index
//! comes out. Bitmap surgery stays with the caller, so tables of group
//! fullness drive every branch in tests.

/// Preallocated blocks per sequential file (`EXT2_PREALLOC_BLOCKS`,
/// eight: one returned at once, seven parked in the window).
pub const PREALLOC_WINDOW: usize = 8;
/// Root inode number (`ROOT_INODE`, two).
pub const ROOT_NUMBER: u64 = 2;

/// One group's load, as the selectors see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupLoad {
    /// Free inodes left.
    pub free_inodes: u32,
    /// Free blocks left.
    pub free_blocks: u32,
    /// Directories already placed here.
    pub directories: u32,
}

/// Inode placement policies (mount options, `main.c:15-24`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodePolicy {
    /// Spread directories, hash files (`orlov`, the default).
    Orlov,
    /// Old fullest-group scan for directories, hash for files.
    OldDirs,
    /// First group with a free slot, Minix style (`mfsalloc`).
    FirstFit,
}

/// Whether a group can host a directory under Orlov's top-level rule:
/// both counters at or above average, tracked while scanning for the
/// fewest directories (`ialloc.c:401-410`).
fn orlov_top_candidate(load: GroupLoad, average_inodes: u32, average_blocks: u32) -> bool {
    load.free_inodes >= average_inodes && load.free_blocks >= average_blocks
}

/// Pick a group for a top-level directory (`find_group_orlov`,
/// `ialloc.c:383-416`): groups below either average are skipped (but
/// remembered as fallback in scan order), and among survivors the one
/// with the fewest directories wins. Nothing surviving returns the best
/// average-qualified group, else the last fallback with any free inode.
pub fn orlov_top_group(groups: &[GroupLoad], average_inodes: u32, average_blocks: u32) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut best_average: Option<usize> = None;
    let mut fallback: Option<usize> = None;
    let mut fewest = u32::MAX;
    for (index, load) in groups.iter().enumerate() {
        if load.free_inodes == 0 {
            continue;
        }
        fallback = Some(index);
        if !orlov_top_candidate(*load, average_inodes, average_blocks) {
            continue;
        }
        best_average = Some(index);
        if load.directories < fewest {
            fewest = load.directories;
            best = Some(index);
        }
    }
    best.or(best_average).or(fallback)
}

/// Pick a group for a non-top directory (`find_group_orlov`,
/// `ialloc.c:417-442`): scan from the parent's group for the first one
/// holding at least half the average of both counters; otherwise the
/// last group with any free inode seen along the way.
pub fn orlov_child_group(
    groups: &[GroupLoad],
    parent_group: usize,
    average_inodes: u32,
    average_blocks: u32,
) -> Option<usize> {
    if groups.is_empty() {
        return None;
    }
    let minimum_inodes = average_inodes / 2;
    let minimum_blocks = average_blocks / 2;
    let mut fallback = None;
    for step in 0..groups.len() {
        let index = (parent_group + step) % groups.len();
        let load = groups[index];
        if load.free_inodes == 0 {
            continue;
        }
        fallback = Some(index);
        if load.free_inodes >= minimum_inodes && load.free_blocks >= minimum_blocks {
            return Some(index);
        }
    }
    fallback
}

/// Pick a group for a directory the old way (`find_group_dir`,
/// `ialloc.c:254-276`): above-average inodes required, most free blocks
/// wins.
pub fn fullest_dir_group(groups: &[GroupLoad], average_inodes: u32) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut most_blocks = 0u32;
    for (index, load) in groups.iter().enumerate() {
        if load.free_inodes == 0 || load.free_inodes < average_inodes {
            continue;
        }
        if best.is_none() || load.free_blocks > most_blocks {
            most_blocks = load.free_blocks;
            best = Some(index);
        }
    }
    best
}

/// Pick a group for a file by parent-group hashing (`find_group_hashalloc`,
/// `ialloc.c:284-335`): the parent's group when it holds both resources,
/// else quadratic probing from a parent-derived start, else a linear
/// scan for any free inode.
pub fn hashed_file_group(
    groups: &[GroupLoad],
    parent_group: usize,
    parent_number: u64,
) -> Option<usize> {
    let count = groups.len();
    if count == 0 || parent_group >= count {
        return None;
    }
    if groups[parent_group].free_inodes > 0 && groups[parent_group].free_blocks > 0 {
        return Some(parent_group);
    }
    let mut group = (parent_group + parent_number as usize) % count;
    let mut step = 1usize;
    while step < count {
        group += step;
        if group >= count {
            group -= count;
        }
        if groups[group].free_inodes > 0 && groups[group].free_blocks > 0 {
            return Some(group);
        }
        step <<= 1;
    }
    group = parent_group;
    for _ in 0..count {
        if groups[group].free_inodes > 0 {
            return Some(group);
        }
        group += 1;
        if group >= count {
            group = 0;
        }
    }
    None
}

/// Pick the first group with a free inode from a search cursor
/// (`find_group_any`, `ialloc.c:341-358`, Minix style): the cursor
/// advances past the winner for next time.
pub fn first_fit_group(groups: &[GroupLoad], cursor: &mut usize) -> Option<usize> {
    while *cursor < groups.len() {
        let index = *cursor;
        if groups[index].free_inodes > 0 {
            *cursor = index;
            return Some(index);
        }
        *cursor += 1;
    }
    None
}

/// Admission for block allocation (`alloc_block`, `balloc.c:87-99`):
/// without the reserved-blocks option, allocation refuses once free
/// blocks reach the reserved count; with few blocks left it first drops
/// every preallocation window. Reports whether the caller may proceed
/// and whether windows must drop first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Proceed without dropping windows.
    Proceed,
    /// Drop all preallocation windows, then proceed.
    DropWindowsThenProceed,
    /// Refuse: only reserved blocks remain.
    Refuse,
}

pub fn admit_block(free_blocks: u64, reserved_blocks: u64, use_reserved: bool) -> Admission {
    if !use_reserved && free_blocks <= reserved_blocks {
        return Admission::Refuse;
    }
    if free_blocks == 0 {
        return Admission::Refuse;
    }
    if free_blocks <= PREALLOC_WINDOW as u64 {
        return Admission::DropWindowsThenProceed;
    }
    Admission::Proceed
}

/// Whether a sequential write may consume a parked window block
/// (`alloc_block`, `balloc.c:101-124`): the goal must be the parked
/// head or its immediate neighbor; anything else means a random write,
/// which disables preallocation for the file and drops its window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowUse {
    /// Consume the parked head block.
    Consume,
    /// Random access: disable windows and drop them.
    Disable,
}

pub fn use_window(goal: u64, parked_head: u64) -> WindowUse {
    if goal == parked_head || goal + 1 == parked_head {
        WindowUse::Consume
    } else {
        WindowUse::Disable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups() -> [GroupLoad; 4] {
        [
            GroupLoad { free_inodes: 100, free_blocks: 900, directories: 5 },
            GroupLoad { free_inodes: 10, free_blocks: 100, directories: 1 },
            GroupLoad { free_inodes: 0, free_blocks: 800, directories: 0 },
            GroupLoad { free_inodes: 200, free_blocks: 200, directories: 9 },
        ]
    }

    #[test]
    fn test_orlov_top_fewest_directories() {
        // Averages: inodes 77, blocks 500. Group 0 qualifies with five
        // directories; group 3 qualifies with nine. Fewest wins.
        assert_eq!(orlov_top_group(&groups(), 77, 500), Some(0));
        // Nothing above average: fallback is the last group with a free
        // inode in scan order.
        assert_eq!(orlov_top_group(&groups(), 1000, 9000), Some(3));
        // All full: no group at all.
        let full = [GroupLoad { free_inodes: 0, free_blocks: 0, directories: 0 }; 2];
        assert_eq!(orlov_top_group(&full, 1, 1), None);
    }

    #[test]
    fn test_orlov_child_half_average() {
        // Half averages: inodes 50, blocks 450. From group 1, group 1
        // fails, group 3 passes the block bar but not inodes... group 0
        // (wrapped) passes both.
        assert_eq!(orlov_child_group(&groups(), 1, 100, 900), Some(0));
        // Parent's own group serves when healthy.
        assert_eq!(orlov_child_group(&groups(), 0, 100, 900), Some(0));
    }

    #[test]
    fn test_fullest_dir_most_blocks() {
        assert_eq!(fullest_dir_group(&groups(), 77), Some(0));
        assert_eq!(fullest_dir_group(&groups(), 1000), None);
    }

    #[test]
    fn test_hashed_parent_then_probe() {
        // Parent group healthy: stays home.
        assert_eq!(hashed_file_group(&groups(), 0, 42), Some(0));
        // Parent group empty of inodes: probes from (parent + number)
        // mod groups, stepping before the first check, and lands on a
        // group with both resources.
        assert_eq!(hashed_file_group(&groups(), 2, 42), Some(1));
    }

    #[test]
    fn test_first_fit_advances_cursor() {
        let mut cursor = 0;
        assert_eq!(first_fit_group(&groups(), &mut cursor), Some(0));
        assert_eq!(cursor, 0);
        let mut cursor = 2;
        assert_eq!(first_fit_group(&groups(), &mut cursor), Some(3));
    }

    #[test]
    fn test_admission_reserved_and_windows() {
        assert_eq!(admit_block(5, 10, false), Admission::Refuse);
        assert_eq!(admit_block(0, 0, true), Admission::Refuse);
        assert_eq!(admit_block(8, 0, true), Admission::DropWindowsThenProceed);
        assert_eq!(admit_block(100, 10, false), Admission::Proceed);
        assert_eq!(PREALLOC_WINDOW, 8);
    }

    #[test]
    fn test_window_head_or_neighbor() {
        assert_eq!(use_window(50, 50), WindowUse::Consume);
        assert_eq!(use_window(49, 50), WindowUse::Consume);
        assert_eq!(use_window(30, 50), WindowUse::Disable);
    }
}
