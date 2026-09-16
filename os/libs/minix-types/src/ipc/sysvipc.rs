//! SysV IPC wire layouts — `struct ipc_perm` / `struct semid_ds` /
//! `struct shmid_ds` (E-IPCWIRE 第 8 项)。
//!
//! C ground truth: `minix3/sys/sys/ipc.h:54-66`(ipc_perm)、
//! `minix3/sys/sys/sem.h:55-66`(semid_ds)、
//! `minix3/minix/../sys/sys/shm.h:99-114`(shmid_ds)。
//!
//! IPC_STAT 的拷出与 IPC_SET 的拷入按这些字节搬运(用户态 ipcs(1)
//! 依赖同一布局);布局按本重写的 x86_64 LP64 目标钉死(time_t 8 字节、
//! 指针 8 字节——C 头按 i386 书写,本重写沿用 MessVmmcpReply 的
//! 64-bit-overlay 判例)。
//!
//! C 结构里的私有指针成员(`_sem_base`/`_shm_internal`)保留占位槽:
//! 它们是 C 实现的内部簿记,IPC_STAT 对用户态返回的值无跨进程语义,
//! 但槽位本身是布局契约的一部分(用户态 sizeof 断言依赖)。

/// `struct ipc_perm` — IPC 对象的权限结构(C ipc.h:54-66)。
///
/// LP64:五个 4 字节域 + u16 + 2 对齐垫 = 24 字节。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IpcPerm {
    /// 属主 user id。C: `uid_t uid`。
    pub uid: u32,
    /// 属主 group id。C: `gid_t gid`。
    pub gid: u32,
    /// 创建者 user id。C: `uid_t cuid`。
    pub cuid: u32,
    /// 创建者 group id。C: `gid_t cgid`。
    pub cgid: u32,
    /// 读写权限位。C: `mode_t mode`(u32)。
    pub mode: u32,
    /// 序号(msg/sem/shm id 生成用)。C: `unsigned short _seq`。
    pub seq: u16,
    /// 对齐垫(结构对齐 4,成员和 22 → 垫 2)。
    pub _pad: u16,
}

/// `struct semid_ds` — 信号量集的 IPC_STAT/IPC_SET 载荷
/// (C sem.h:55-66)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SemidDs {
    /// 权限结构。C: `struct ipc_perm sem_perm`。
    pub sem_perm: IpcPerm,
    /// 集内信号量个数。C: `unsigned short sem_nsems`。
    pub sem_nsems: u16,
    /// 对齐垫(time_t 8 对齐)。
    pub _pad0: u16,
    /// 对齐垫。
    pub _pad1: u32,
    /// 最近 semop 时间。C: `time_t sem_otime`。
    pub sem_otime: u64,
    /// 最近 semctl 变更时间。C: `time_t sem_ctime`。
    pub sem_ctime: u64,
    /// C 私有指针槽(`struct __sem *_sem_base`——布局契约占位,
    /// 跨进程无语义)。
    pub sem_base_ptr: u64,
}

/// `struct shmid_ds` — 共享内存段的 IPC_STAT/IPC_SET 载荷
/// (C shm.h:99-114)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShmidDs {
    /// 权限结构。C: `struct ipc_perm shm_perm`。
    pub shm_perm: IpcPerm,
    /// 段大小(字节)。C: `size_t shm_segsz`。
    pub shm_segsz: u64,
    /// 最近 shm 操作的进程 pid。C: `pid_t shm_lpid`。
    pub shm_lpid: i32,
    /// 创建者 pid。C: `pid_t shm_cpid`。
    pub shm_cpid: i32,
    /// 当前附着数。C: `shmatt_t shm_nattch`(u32)。
    pub shm_nattch: u32,
    /// 对齐垫(time_t 8 对齐)。
    pub _pad: u32,
    /// 最近 shmat 时间。C: `time_t shm_atime`。
    pub shm_atime: u64,
    /// 最近 shmdt 时间。C: `time_t shm_dtime`。
    pub shm_dtime: u64,
    /// 最近变更时间。C: `time_t shm_ctime`。
    pub shm_ctime: u64,
    /// C 私有指针槽(`void *_shm_internal`——布局契约占位)。
    pub internal_ptr: u64,
}

#[cfg(test)]
mod sysvipc_layout_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// 布局见证:ipc_perm 24 字节(五 u32 域 + seq + 垫)。
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

    /// 布局见证:semid_ds 56 字节——perm@0、nsems@24、otime@32、
    /// ctime@40、私有指针槽@48(sem.h:55-66,LP64)。
    #[test]
    fn test_semid_ds_layout() {
        assert_eq!(size_of::<SemidDs>(), 56);
        assert_eq!(offset_of!(SemidDs, sem_perm), 0);
        assert_eq!(offset_of!(SemidDs, sem_nsems), 24);
        assert_eq!(offset_of!(SemidDs, sem_otime), 32);
        assert_eq!(offset_of!(SemidDs, sem_ctime), 40);
        assert_eq!(offset_of!(SemidDs, sem_base_ptr), 48);
    }

    /// 布局见证:shmid_ds 80 字节——perm@0、segsz@24、lpid@32、
    /// cpid@36、nattch@40、atime@48、dtime@56、ctime@64、
    /// 私有指针槽@72(shm.h:99-114,LP64)。
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
