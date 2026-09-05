//! Lazy reference-count sweep: ask, convert, destroy when due.
//!
//! C: `update_refcount_and_destroy` (shm.c:173-206).
//! Document `08-ipc-shm-attach.md` §3 (decisions D2/D3).
//!
//! The counting is lazy on purpose: attach and detach never touch the
//! count; every cycle end (plus detach tails and removals) re-asks the
//! virtual-memory service and converts. This module owns the conversion;
//! the queries (`vm_getrefcount`) and the unmaps (`munmap`) stay at the
//! boundary and arrive/leave as values.

use alloc::vec::Vec;

use super::segment::ShmTable;

// ============================================================================
// Boundary values
// ============================================================================

/// One answered reference-count query: slot plus what the virtual-memory
/// service said.
///
/// `count` is `None` when the region could not be found (C `rc == -1`,
/// shm.c:183-186): warned about and skipped, never fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefQuery {
    /// Table slot the query was asked for.
    pub slot: usize,
    /// Reference count including our own mapping, if found.
    pub count: Option<u8>,
}

/// One unmap the service layer must perform.
///
/// C: `munmap(page, roundup(segsz))` — shm.c:191-193.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnmapReq {
    /// Local address to unmap. C: `page`.
    pub addr: u64,
    /// Length in bytes, page-rounded. C: `roundup(segsz, PAGE_SIZE)`.
    pub len: u64,
}

/// Outcome of one sweep: what to unmap, what died, what was skipped.
///
/// Order matches the C loop (low slot to high): unmaps and deaths come out
/// in slot order so tests (and log readers) see a stable sequence.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SweepPlan {
    /// Unmaps to perform, in slot order.
    pub unmaps: Vec<UnmapReq>,
    /// Slots whose allocation bit was cleared, in slot order.
    pub freed: Vec<usize>,
    /// Slots skipped (region unknown), in slot order.
    pub skipped: Vec<usize>,
}

// ============================================================================
// Sweep
// ============================================================================

/// Refresh every live slot from answered queries; destroy what is due.
/// Afterwards pulls back the high-water mark over trailing free slots
/// (shm.c:203-205).
///
/// `queries` carries one answer per live slot the caller asked about
/// (missing answers are treated like unknown regions: skipped). For each
/// live slot, in slot order:
/// - unknown region → record `skipped`, leave the slot alone (shm.c:184-185);
/// - else store `count - 1` as the attach count — our own mapping counts
///   (shm.c:187);
/// - zero attaches plus the destroy mark → queue an unmap and clear the
///   allocation bit (shm.c:189-196).
///
/// Whether a slot carries the destroy mark is read from the mode bit
/// (`SHM_DEST 0x0400`); the mark is set by the remove command (document 08
/// §2.4) and never cleared here.
pub fn sweep(table: &mut ShmTable, queries: &[RefQuery]) -> SweepPlan {
    let mut plan = SweepPlan::default();
    let live = table.live_count();
    for index in 0..live {
        let count = queries
            .iter()
            .find(|q| q.slot == index)
            .and_then(|q| q.count);
        let Some(count) = count else {
            plan.skipped.push(index);
            continue;
        };
        let seg = table.get_mut(index).expect("sweep scans live slots");
        seg.attached = u16::from(count.saturating_sub(1));
        if seg.attached == 0 && seg.perm.mode & minix_types::SHM_DEST != 0 {
            plan.unmaps.push(UnmapReq {
                addr: seg.backing.local,
                len: super::segment::round_up(seg.size_bytes),
            });
            plan.freed.push(index);
        }
    }
    // Release after the scan (the plan keeps the list for the caller;
    // the copy is a handful of indices, not the table).
    let freed = plan.freed.clone();
    for index in freed {
        table.release(index);
    }
    plan
}

/// Destroy-mark bit (re-export for sweep readers).
pub const DESTROY_MARK: u32 = minix_types::SHM_DEST;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perms::Identity;
    use crate::shm::segment::{Backing, CreateParams, ShmTable};

    fn caller() -> Identity {
        Identity { uid: 100, gid: 200 }
    }

    fn table_with(marked: bool) -> ShmTable {
        let mut table = ShmTable::new();
        table
            .create(CreateParams {
                key: 1,
                size: 5000,
                flag: 0o1000 | 0o600,
                caller: caller(),
                backing: Backing {
                    local: 0x4000_0000,
                    phys: 0x0010_0000,
                },
                now: 0,
                cpid: 1,
            })
            .unwrap();
        if marked {
            table.get_mut(0).unwrap().perm.mode |= minix_types::SHM_DEST;
        }
        table
    }

    #[test]
    fn sweep_destroys_at_zero() {
        // C: shm.c:189-196 — marked plus zero attaches unmaps and frees.
        let mut table = table_with(true);
        let plan = sweep(
            &mut table,
            &[RefQuery {
                slot: 0,
                count: Some(1),
            }],
        );
        assert_eq!(plan.freed, [0]);
        assert_eq!(
            plan.unmaps,
            [UnmapReq {
                addr: 0x4000_0000,
                len: 8192
            }]
        );
        assert!(table.is_empty());
    }

    #[test]
    fn sweep_skips_unknown() {
        // C: shm.c:183-186 — unknown region warns and skips, never destroys.
        let mut table = table_with(true);
        let plan = sweep(
            &mut table,
            &[RefQuery {
                slot: 0,
                count: None,
            }],
        );
        assert_eq!(plan.skipped, [0]);
        assert!(plan.freed.is_empty() && plan.unmaps.is_empty());
        assert_eq!(table.live_count(), 1, "slot survives the skip");
    }

    #[test]
    fn sweep_keeps_attached() {
        // C: shm.c:187 — count minus our own share is stored, no destroy.
        let mut table = table_with(true);
        let plan = sweep(
            &mut table,
            &[RefQuery {
                slot: 0,
                count: Some(4),
            }],
        );
        assert!(plan.freed.is_empty() && plan.unmaps.is_empty());
        assert_eq!(table.get(0).unwrap().attached, 3);
        // Unmarked segments survive even at zero attaches.
        let mut plain = table_with(false);
        let plan = sweep(
            &mut plain,
            &[RefQuery {
                slot: 0,
                count: Some(1),
            }],
        );
        assert!(plan.freed.is_empty());
        assert_eq!(plain.get(0).unwrap().attached, 0);
    }

    #[test]
    fn sweep_shrinks_mark() {
        // C: shm.c:203-205 — trailing free slots pull the mark back.
        let mut table = ShmTable::new();
        for k in 1..=3 {
            table
                .create(CreateParams {
                    key: k,
                    size: 100,
                    flag: 0o1000,
                    caller: caller(),
                    backing: Backing {
                        local: 0,
                        phys: k as u64,
                    },
                    now: 0,
                    cpid: 1,
                })
                .unwrap();
        }
        table.get_mut(2).unwrap().perm.mode |= minix_types::SHM_DEST;
        let queries = alloc::vec![
            RefQuery {
                slot: 0,
                count: Some(2)
            },
            RefQuery {
                slot: 1,
                count: Some(2)
            },
            RefQuery {
                slot: 2,
                count: Some(1)
            },
        ];
        let plan = sweep(&mut table, &queries);
        assert_eq!(plan.freed, [2]);
        assert_eq!(table.live_count(), 2, "mark pulled back over slot 2");
    }
}
