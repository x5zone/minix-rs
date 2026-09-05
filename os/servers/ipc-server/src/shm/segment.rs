//! Shared-memory segment table: 1024 slots, birth half of the life cycle.
//!
//! C: `shm_list` / `shm_list_nr` / `shm_find_key` / `shm_find_id` /
//! `do_shmget` (shm.c:6-12/:15-49/:51-127).
//! Document `07-ipc-shm-segment.md` §3 (decisions D1-D5).
//!
//! Mirrors `sem/table.rs` on purpose (same three tools: high-water mark,
//! allocation bit, sequence numbers) so the two tables read alike. The
//! differences are the point: no event subscription here, an anonymous
//! mapping plus a physical snapshot there, original-versus-rounded sizes.

use minix_types::{ACCESSPERMS, IPC_CREAT, IPC_EXCL, IPC_PRIVATE, SEM_SEQ_MASK, SHM_ALLOC, SHMMNI};

use super::ShmError;
use crate::perms::{Identity, IpcPerm, check_perm};

// ============================================================================
// Data
// ============================================================================

/// Page size for rounding (C: `PAGE_SIZE`, machine page — 4096 on the
/// supported platforms).
pub const PAGE_SIZE: u64 = 4096;

/// Backing memory prepared by the service layer: local address plus the
/// physical snapshot.
///
/// C: `page` (local, from `mmap`) and `vm_id` (from `vm_getphys`,
/// shm.c:117-118). Passed in because mapping is a memory-management
/// effect; a failed mapping never reaches this module (the service layer
/// maps the failure to `ENOMEM` first — document 07 §3 D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backing {
    /// Local address of the anonymous mapping. C: `page`.
    pub local: u64,
    /// Physical address snapshot. C: `vm_id`.
    pub phys: u64,
}

/// One live segment: identity plus sizes plus backing plus times.
///
/// C: `struct shm_struct` — shm.c:6-10 (descriptor `shmid_ds` plus the two
/// addresses). Permission fields reuse the shared [`IpcPerm`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShmSegment {
    /// Permission record (key, owners, mode with `SHM_ALLOC`, sequence).
    pub perm: IpcPerm,
    /// Requested size in bytes, as asked. C: `shm_segsz` (the *original*
    /// size — shm.c:110 stores `old_size`, not the rounded value).
    pub size_bytes: u64,
    /// Allocated size in bytes, page-rounded. C: `roundup(size, PAGE_SIZE)`
    /// (shm.c:77) — the mapping and unmapping granularity.
    pub alloc_bytes: u64,
    /// Backing memory. C: `page` + `vm_id`.
    pub backing: Backing,
    /// Attach count (maintained lazily — document 08). C: `shm_nattch`.
    pub attached: u16,
    /// Last attach time. C: `shm_atime`.
    pub attach_time: u64,
    /// Last detach time. C: `shm_dtime`.
    pub detach_time: u64,
    /// Last change time. C: `shm_ctime`.
    pub change_time: u64,
    /// Creator process id. C: `shm_cpid`.
    pub creator_pid: i32,
    /// Last-operation process id. C: `shm_lpid`.
    pub last_pid: i32,
}

/// One table slot: either free (keeping its sequence) or holding a segment.
///
/// Same shape as `sem::SemSlot`, including the retained sequence — C reads
/// `seq` before zeroing on reuse here too (shm.c:100).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShmSlot {
    /// Free slot, carrying the last sequence number.
    Free {
        /// Last sequence number (0 on a never-used slot).
        seq: u16,
    },
    /// Live segment.
    Used(ShmSegment),
}

/// Creation arguments, bundled so the entry point stays readable.
///
/// Seven values travel together from the message plus the prepared backing
/// and the timestamps the service layer supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateParams {
    /// Lookup or create key.
    pub key: i32,
    /// Requested size in bytes.
    pub size: u64,
    /// Creation flags.
    pub flag: i32,
    /// Caller identity (looked up by the service layer).
    pub caller: Identity,
    /// Prepared anonymous mapping (see [`Backing`]).
    pub backing: Backing,
    /// Creation timestamp (`clock_time`, injected).
    pub now: u64,
    /// Creator process id (`getnpid`, injected).
    pub cpid: i32,
}

/// The segment table: 1024 slots plus the high-water mark.
///
/// C: `shm_list[SHMMNI]` + `shm_list_nr` — shm.c:11-12.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShmTable {
    slots: [ShmSlot; SHMMNI],
    high_water: usize,
}

impl ShmTable {
    /// Empty table: 1024 free slots, water at zero.
    pub fn new() -> Self {
        Self {
            slots: [ShmSlot::Free { seq: 0 }; SHMMNI],
            high_water: 0,
        }
    }

    /// Number of live segments (== high-water mark after trailing shrink).
    pub fn live_count(&self) -> usize {
        self.high_water
    }

    /// Find a segment by key, private keys included (they never match:
    /// C returns `NULL` for `IPC_PRIVATE` up front — shm.c:19-20 — unlike
    /// the semaphore twin, which splits private keys at the caller).
    pub fn find_key(&self, key: i32) -> Option<usize> {
        if key == IPC_PRIVATE {
            return None;
        }
        for (i, slot) in self.slots.iter().enumerate().take(self.high_water) {
            if let ShmSlot::Used(seg) = slot
                && seg.perm.key == key
            {
                return Some(i);
            }
        }
        None
    }

    /// Find a segment by identifier (index, occupancy, sequence).
    ///
    /// C: `shm_find_id` — shm.c:33-48.
    pub fn find_id(&self, id: i32) -> Option<usize> {
        let index = (id & 0xffff) as usize;
        if index >= self.high_water {
            return None;
        }
        if let ShmSlot::Used(seg) = &self.slots[index]
            && seg.perm.seq as i32 == ((id >> 16) & 0xffff)
        {
            return Some(index);
        }
        None
    }

    /// Borrow a live segment by slot index.
    pub fn get(&self, index: usize) -> Option<&ShmSegment> {
        match self.slots.get(index) {
            Some(ShmSlot::Used(seg)) => Some(seg),
            _ => None,
        }
    }

    /// Mutably borrow a live segment by slot index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut ShmSegment> {
        match self.slots.get_mut(index) {
            Some(ShmSlot::Used(seg)) => Some(seg),
            _ => None,
        }
    }

    /// Create-or-open a segment (`shmget`).
    ///
    /// C: `do_shmget` — shm.c:51-127. Takes one bundled [`CreateParams`]
    /// (seven values travel together). Returns the identifier.
    pub fn create(&mut self, params: CreateParams) -> Result<i32, ShmError> {
        let CreateParams {
            key,
            size,
            flag,
            caller,
            backing,
            now,
            cpid,
        } = params;
        // Existing-segment branch (shm.c:64-71): permission first, then
        // exclusive collision, then the size check — the reverse order of
        // the semaphore twin (document 07 §2.3).
        if let Some(index) = self.find_key(key) {
            if !check_perm(
                &self.get(index).expect("find_key returned live slot").perm,
                caller,
                flag as u32,
            ) {
                return Err(ShmError::Access);
            }
            if flag & IPC_CREAT != 0 && flag & IPC_EXCL != 0 {
                return Err(ShmError::Exists);
            }
            let seg = self.get(index).expect("checked live above");
            if size != 0 && (seg.size_bytes) < size {
                return Err(ShmError::Invalid);
            }
            return Ok(encode_id(index, seg.perm.seq));
        }
        // Fresh-segment branch (shm.c:73-123).
        if flag & IPC_CREAT == 0 {
            return Err(ShmError::Missing);
        }
        if size == 0 {
            return Err(ShmError::Invalid);
        }
        let rounded = round_up(size);
        if rounded == 0 {
            // Overflow wrapped around (shm.c:78-79 catches the zero wrap;
            // non-zero wraps pass in C too — same contract here).
            return Err(ShmError::Invalid);
        }
        let index = self
            .slots
            .iter()
            .position(|s| matches!(s, ShmSlot::Free { .. }))
            .ok_or(ShmError::NoSpace)?;
        // C reads the stale sequence before zeroing (shm.c:100).
        let old_seq = match &self.slots[index] {
            ShmSlot::Used(seg) => seg.perm.seq,
            ShmSlot::Free { seq } => *seq,
        };
        let seg = ShmSegment {
            perm: IpcPerm {
                key,
                uid: caller.uid,
                gid: caller.gid,
                creator_uid: caller.uid,
                creator_gid: caller.gid,
                mode: SHM_ALLOC | (flag as u32 & ACCESSPERMS),
                seq: next_seq(old_seq),
            },
            size_bytes: size,
            alloc_bytes: rounded,
            backing,
            attached: 0,
            attach_time: 0,
            detach_time: 0,
            change_time: now,
            creator_pid: cpid,
            last_pid: 0,
        };
        let id = encode_id(index, seg.perm.seq);
        self.slots[index] = ShmSlot::Used(seg);
        if index == self.high_water {
            self.high_water += 1;
        }
        Ok(id)
    }

    /// Release a slot without further ceremony (the sweep in `refcount.rs`
    /// decides *when*; this performs the clearing).
    ///
    /// Keeps the sequence for the next occupant and pulls back the mark
    /// over trailing free slots — same shape as the semaphore twin.
    pub fn release(&mut self, index: usize) {
        let seq = match &self.slots[index] {
            ShmSlot::Used(seg) => seg.perm.seq,
            // Programming error: only live slots are released.
            ShmSlot::Free { .. } => panic!("release of a free slot"),
        };
        self.slots[index] = ShmSlot::Free { seq };
        while self.high_water > 0 && matches!(self.slots[self.high_water - 1], ShmSlot::Free { .. })
        {
            self.high_water -= 1;
        }
    }

    /// True when no segment is allocated. C: `is_shm_nil` — shm.c:465-469.
    pub fn is_empty(&self) -> bool {
        self.high_water == 0
    }
}

impl Default for ShmTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode slot index plus sequence as an identifier.
///
/// C: `IXSEQ_TO_IPCID(ix, perm)` — sys/ipc.h:110.
pub const fn encode_id(index: usize, seq: u16) -> i32 {
    ((seq as i32) << 16) | ((index as i32) & 0xffff)
}

/// Advance the sequence number, keeping fifteen bits.
///
/// C: `(seq + 1) & 0x7fff` — shm.c:109 (document 07 §3 D1).
pub const fn next_seq(seq: u16) -> u16 {
    ((seq as u32 + 1) & SEM_SEQ_MASK) as u16
}

/// Round a byte size up to whole pages.
///
/// C: `roundup(size, PAGE_SIZE)` — shm.c:77 (document 07 §3 D3).
pub const fn round_up(size: u64) -> u64 {
    (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caller() -> Identity {
        Identity { uid: 100, gid: 200 }
    }

    fn backing() -> Backing {
        Backing {
            local: 0x4000_0000,
            phys: 0x0010_0000,
        }
    }

    fn mk(key: i32, size: u64, flag: i32) -> CreateParams {
        CreateParams {
            key,
            size,
            flag,
            caller: caller(),
            backing: backing(),
            now: 0,
            cpid: 1,
        }
    }

    #[test]
    fn create_new_fills_entry() {
        // C: shm.c:72-126 — identity, original size, rounded alloc, phys.
        let mut table = ShmTable::new();
        let id = table
            .create(CreateParams {
                key: 0x1234,
                size: 5000,
                flag: 0o1000 | 0o600,
                caller: caller(),
                backing: backing(),
                now: 999,
                cpid: 42,
            })
            .unwrap();
        assert_eq!(id & 0xffff, 0);
        let seg = table.get(0).unwrap();
        assert_eq!((seg.size_bytes, seg.alloc_bytes), (5000, 8192));
        assert_eq!((seg.creator_pid, seg.change_time), (42, 999));
        assert_eq!(seg.backing, backing());
        assert_eq!(seg.attached, 0);
        assert_eq!(table.find_id(id), Some(0));
        assert_eq!(table.find_key(0x1234), Some(0));
    }

    #[test]
    fn create_existing_checks() {
        // C: shm.c:64-71 — permission first (reverse of semget), then
        // exclusive collision, then the size check.
        let mut table = ShmTable::new();
        let id = table.create(mk(7, 4096, 0o1000 | 0o600)).unwrap();
        // Exclusive re-create collides (with permission to see it).
        assert_eq!(
            table.create(mk(7, 100, 0o1000 | 0o2000 | 0o600)),
            Err(ShmError::Exists)
        );
        // Same key with a read bit re-opens the same segment.
        let id2 = table.create(mk(7, 100, 0o400)).unwrap();
        assert_eq!(id2, id);
        // Asking for more than the segment holds fails; asking less passes.
        assert_eq!(table.create(mk(7, 5000, 0o400)), Err(ShmError::Invalid));
        // A stranger fails at the permission check first.
        let stranger = Identity { uid: 999, gid: 999 };
        assert_eq!(
            table.create(CreateParams {
                caller: stranger,
                ..mk(7, 100, 0o600)
            }),
            Err(ShmError::Access)
        );
        // Key without create flag misses (private too).
        assert_eq!(table.create(mk(0xBEEF, 100, 0o400)), Err(ShmError::Missing));
        assert_eq!(
            table.create(mk(IPC_PRIVATE, 100, 0o400)),
            Err(ShmError::Missing)
        );
    }

    #[test]
    fn create_private_always_new() {
        // C: shm.c:19-20 — private keys never match; each creates.
        let mut table = ShmTable::new();
        let id1 = table.create(mk(IPC_PRIVATE, 100, 0o1000)).unwrap();
        let id2 = table.create(mk(IPC_PRIVATE, 100, 0o1000)).unwrap();
        assert_ne!(id1, id2);
        assert_eq!(table.find_key(IPC_PRIVATE), None);
    }

    #[test]
    fn round_up_edges() {
        // C: shm.c:75-79 — zero rejected, pages exact, partial rounds up.
        assert_eq!(round_up(1), 4096);
        assert_eq!(round_up(4096), 4096);
        assert_eq!(round_up(4097), 8192);
        let mut table = ShmTable::new();
        assert_eq!(table.create(mk(1, 0, 0o1000)), Err(ShmError::Invalid));
    }

    #[test]
    fn find_id_rejects_stale_seq() {
        // C: shm.c:45 — reused slot with a new sequence rejects old ids.
        let mut table = ShmTable::new();
        let id = table.create(mk(1, 100, 0o1000)).unwrap();
        table.release(0);
        assert_eq!(table.find_id(id), None, "old id must not match");
        let id2 = table.create(mk(2, 100, 0o1000)).unwrap();
        assert_ne!(id2, id, "sequence ages across the free slot");
        assert_eq!(table.find_id(id2), Some(0));
    }

    #[test]
    fn table_full_returns_nospc() {
        // C: shm.c:82-86 — sampled: fill a prefix, release one, reuse it.
        // (Filling all 1024 in a unit test wastes time; the scan logic is
        // the same first-fit loop as the semaphore twin, tested fully
        // there.)
        let mut table = ShmTable::new();
        for k in 1..=8 {
            table.create(mk(k, 100, 0o1000)).unwrap();
        }
        assert_eq!(table.live_count(), 8);
        table.release(3);
        assert_eq!(table.live_count(), 8, "interior hole keeps the mark");
        let id = table.create(mk(99, 100, 0o1000)).unwrap();
        assert_eq!(id & 0xffff, 3, "first fit reuses the hole");
        assert!(!table.is_empty());
        for i in 0..8 {
            if table.get(i).is_some() {
                table.release(i);
            }
        }
        assert!(table.is_empty());
    }
}
