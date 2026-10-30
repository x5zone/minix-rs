//! SysV IPC wire layouts — `struct ipc_perm` / `struct semid_ds` /
//! `struct shmid_ds` (E-IPCWIRE item 8).
//!
//! C ground truth: `minix3/sys/sys/ipc.h:54-66` (ipc_perm),
//! `minix3/sys/sys/sem.h:55-66` (semid_ds),
//! `minix3/minix/../sys/sys/shm.h:99-114` (shmid_ds).
//!
//! IPC_STAT copies out and IPC_SET copies in across these bytes (user-space
//! ipcs(1) relies on the same layout); the layout is pinned to this rewrite's
//! x86_64 LP64 target (time_t is 8 bytes, pointers are 8 bytes — the C headers
//! were written for i386, and this rewrite follows the 64-bit-overlay precedent
//! of MessVmmcpReply).
//!
//! The private pointer members of the C structures (`_sem_base`/`_shm_internal`)
//! keep placeholder slots: they are internal bookkeeping of the C implementation
//! and the values IPC_STAT returns to user space have no cross-process meaning,
//! but the slots themselves are part of the layout contract (user-space sizeof
//! assertions depend on them).

/// `struct ipc_perm` — the permission structure of an IPC object (C ipc.h:54-66).
///
/// LP64: five 4-byte fields + u16 + 2 alignment pad = 24 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IpcPerm {
    /// Owning user id. C: `uid_t uid`.
    pub uid: u32,
    /// Owning group id. C: `gid_t gid`.
    pub gid: u32,
    /// Creator user id. C: `uid_t cuid`.
    pub cuid: u32,
    /// Creator group id. C: `gid_t cgid`.
    pub cgid: u32,
    /// Read/write permission bits. C: `mode_t mode` (u32).
    pub mode: u32,
    /// Sequence number (used to generate msg/sem/shm ids). C:
    /// `unsigned short _seq`.
    pub seq: u16,
    /// Alignment pad (structure alignment 4, member sum 22 -> 2 bytes of pad).
    pub _pad: u16,
}

/// `struct semid_ds` — the IPC_STAT/IPC_SET payload of a semaphore set
/// (C sem.h:55-66).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SemidDs {
    /// Permission structure. C: `struct ipc_perm sem_perm`.
    pub sem_perm: IpcPerm,
    /// Number of semaphores in the set. C: `unsigned short sem_nsems`.
    pub sem_nsems: u16,
    /// Alignment pad (time_t 8-byte alignment).
    pub _pad0: u16,
    /// Alignment pad.
    pub _pad1: u32,
    /// Time of the last semop. C: `time_t sem_otime`.
    pub sem_otime: u64,
    /// Time of the last semctl change. C: `time_t sem_ctime`.
    pub sem_ctime: u64,
    /// C private pointer slot (`struct __sem *_sem_base` — a layout-contract
    /// placeholder with no cross-process semantics).
    pub sem_base_ptr: u64,
}

/// `struct shmid_ds` — the IPC_STAT/IPC_SET payload of a shared-memory segment
/// (C shm.h:99-114).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShmidDs {
    /// Permission structure. C: `struct ipc_perm shm_perm`.
    pub shm_perm: IpcPerm,
    /// Segment size in bytes. C: `size_t shm_segsz`.
    pub shm_segsz: u64,
    /// pid of the process of the last shm operation. C: `pid_t shm_lpid`.
    pub shm_lpid: i32,
    /// Creator pid. C: `pid_t shm_cpid`.
    pub shm_cpid: i32,
    /// Current attach count. C: `shmatt_t shm_nattch` (u32).
    pub shm_nattch: u32,
    /// Alignment pad (time_t 8-byte alignment).
    pub _pad: u32,
    /// Time of the last shmat. C: `time_t shm_atime`.
    pub shm_atime: u64,
    /// Time of the last shmdt. C: `time_t shm_dtime`.
    pub shm_dtime: u64,
    /// Time of the last change. C: `time_t shm_ctime`.
    pub shm_ctime: u64,
    /// C private pointer slot (`void *_shm_internal` — a layout-contract
    /// placeholder).
    pub internal_ptr: u64,
}

#[cfg(test)]
mod sysvipc_layout_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// Layout witness: ipc_perm is 24 bytes (five u32 fields + seq + pad).
    #[test]
    fn test_ipc_perm_layout() {
        assert_eq!(size_of::<IpcPerm>(), 24);
        assert_eq!(offset_of!(IpcPerm, uid), 0);
        assert_eq!(offset_of!(IpcPerm, gid), 4);
        assert_eq!(offset_of!(IpcPerm, cuid), 8);
        assert_eq!(offset_of!(IpcPerm, cgid), 12);
        assert_eq!(offset_of!(IpcPerm, mode), 16);
        assert_eq!(offset_of!(IpcPerm, seq), 20);
    }

    /// Layout witness: semid_ds is 56 bytes — perm@0, nsems@24, otime@32,
    /// ctime@40, private pointer slot@48 (sem.h:55-66, LP64).
    #[test]
    fn test_semid_ds_layout() {
        assert_eq!(size_of::<SemidDs>(), 56);
        assert_eq!(offset_of!(SemidDs, sem_perm), 0);
        assert_eq!(offset_of!(SemidDs, sem_nsems), 24);
        assert_eq!(offset_of!(SemidDs, sem_otime), 32);
        assert_eq!(offset_of!(SemidDs, sem_ctime), 40);
        assert_eq!(offset_of!(SemidDs, sem_base_ptr), 48);
    }

    /// Layout witness: shmid_ds is 80 bytes — perm@0, segsz@24, lpid@32,
    /// cpid@36, nattch@40, atime@48, dtime@56, ctime@64, private pointer
    /// slot@72 (shm.h:99-114, LP64).
    #[test]
    fn test_shmid_ds_layout() {
        assert_eq!(size_of::<ShmidDs>(), 80);
        assert_eq!(offset_of!(ShmidDs, shm_perm), 0);
        assert_eq!(offset_of!(ShmidDs, shm_segsz), 24);
        assert_eq!(offset_of!(ShmidDs, shm_lpid), 32);
        assert_eq!(offset_of!(ShmidDs, shm_cpid), 36);
        assert_eq!(offset_of!(ShmidDs, shm_nattch), 40);
        assert_eq!(offset_of!(ShmidDs, shm_atime), 48);
        assert_eq!(offset_of!(ShmidDs, shm_dtime), 56);
        assert_eq!(offset_of!(ShmidDs, shm_ctime), 64);
        assert_eq!(offset_of!(ShmidDs, internal_ptr), 72);
    }
}
