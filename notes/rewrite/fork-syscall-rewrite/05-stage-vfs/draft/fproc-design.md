# VFS 进程结构体 (FProc) 设计

> 本文档详细说明 VFS (Virtual File System) 服务器的进程结构体设计和 pm_fork 实现。

---

## 1. C 源码分析

### 1.1 fproc 结构体

**文件**: `minix3/minix/servers/vfs/fproc.h`

```c
struct fproc {
  unsigned fp_flags;               /* 进程标志位 */
  pid_t fp_pid;                    /* 进程 ID */
  endpoint_t fp_endpoint;          /* 内核 endpoint 编号 */
  struct vnode *fp_wd;             /* 工作目录 */
  struct vnode *fp_rd;             /* 根目录 */
  struct filp *fp_filp[OPEN_MAX];  /* 文件描述符表 */
  fd_set fp_cloexec_set;           /* FD_CLOEXEC 位图 */
  dev_t fp_tty;                    /* 控制终端 */
  uid_t fp_realuid;                /* 真实用户 ID */
  uid_t fp_effuid;                 /* 有效用户 ID */
  gid_t fp_realgid;                /* 真实组 ID */
  gid_t fp_effgid;                 /* 有效组 ID */
  int fp_ngroups;                  /* 补充组数量 */
  gid_t fp_sgroups[NGROUPS_MAX];   /* 补充组 */
  mode_t fp_umask;                 /* umask */
  char fp_name[PROC_NAME_LEN];    /* 进程名 */
};
```

### 1.2 VFS pm_fork() 核心逻辑

**文件**: `minix3/minix/servers/vfs/misc.c`

```c
void pm_fork(endpoint_t pproc, endpoint_t cproc, pid_t cpid) {
  struct fproc *cp;
  int i, parentno, childno;
  mutex_t c_fp_lock;

  okendpt(pproc, &parentno);
  childno = _ENDPOINT_P(cproc);

  // 1. 整体复制 fproc
  c_fp_lock = fproc[childno].fp_lock;
  fproc[childno] = fproc[parentno];       /* C 结构体赋值 */
  fproc[childno].fp_lock = c_fp_lock;     /* 保留子进程自己的互斥锁 */

  // 2. 增加 filp 引用计数
  cp = &fproc[childno];
  for (i = 0; i < OPEN_MAX; i++)
    if (cp->fp_filp[i] != NULL) cp->fp_filp[i]->filp_count++;

  // 3. 设置新 PID 和 endpoint
  cp->fp_pid = cpid;
  cp->fp_endpoint = cproc;

  // 4. 清除标志
  cp->fp_flags = FP_NOFLAGS;

  // 5. 增加目录 vnode 引用计数
  if (cp->fp_rd) dup_vnode(cp->fp_rd);
  if (cp->fp_wd) dup_vnode(cp->fp_wd);
}
```

---

## 2. Rust 实现设计

### 2.1 fproc 结构体

**文件**: `os/servers/vfs/src/fproc.rs`

```rust
use minix_types::{Pid, Endpoint, Uid, Gid};
use bitflags::bitflags;

/// 最大打开文件数
pub const OPEN_MAX: usize = 128;

/// 最大补充组数
pub const NGROUPS_MAX: usize = 32;

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct FpFlags: u32 {
        const SRV_PROC = 0x0001;
        const REVIVED = 0x0002;
        const SESLDR = 0x0004;
        const PENDING = 0x0010;
        const EXITING = 0x0020;
        const PM_WORK = 0x0040;
    }
}

/// 文件描述符条目（引用计数）
#[derive(Debug, Clone)]
pub struct Filp {
    pub count: u32,
    pub pos: i64,       // 文件偏移量
    pub flags: u32,     // 文件标志
    pub vnode_index: usize,  // Mock: 用索引代替指针
}

/// VNode 引用（Mock 版本）
#[derive(Debug, Clone)]
pub struct VNodeRef {
    pub index: usize,
    pub ref_count: u32,
}

/// VFS 进程结构体
#[derive(Debug, Clone)]
pub struct FProc {
    pub flags: FpFlags,
    pub pid: Pid,
    pub endpoint: Endpoint,
    pub root_dir: Option<usize>,           // Mock: 根目录 vnode 索引
    pub work_dir: Option<usize>,           // Mock: 工作目录 vnode 索引
    pub filps: [Option<usize>; OPEN_MAX],  // Mock: filp 索引表
    pub cloexec_set: u128,                 // FD_CLOEXEC 位图
    pub real_uid: Uid,
    pub eff_uid: Uid,
    pub real_gid: Gid,
    pub eff_gid: Gid,
    pub ngroups: usize,
    pub supplemental_groups: [Gid; NGROUPS_MAX],
    pub umask: u32,
    pub name: [u8; 16],
}
```

### 2.2 FProcTable 进程表

```rust
/// VFS 进程表
pub struct FProcTable {
    slots: [Option<FProc>; NR_PROCS],
}

impl FProcTable {
    pub fn new() -> Self {
        Self {
            slots: [const { None }; NR_PROCS],
        }
    }

    /// 查找指定 endpoint 的 fproc
    pub fn find_by_endpoint(&self, ep: Endpoint) -> Option<&FProc> {
        self.slots.iter().flatten().find(|p| p.endpoint == ep)
    }

    /// 获取指定槽位的可变引用
    pub fn get_mut(&mut self, slot: usize) -> Option<&mut FProc> {
        self.slots[slot].as_mut()
    }

    /// 插入新的 fproc
    pub fn insert(&mut self, slot: usize, proc: FProc) {
        self.slots[slot] = Some(proc);
    }
}
```

### 2.3 VFS pm_fork 实现

**文件**: `os/servers/vfs/src/fork.rs`

```rust
impl FProc {
    /// VFS fork：从父进程创建子进程
    ///
    /// 对应 Minix3 的 vfs/misc.c pm_fork()
    pub fn fork_from(
        parent: &FProc,
        child_pid: Pid,
        child_endpoint: Endpoint,
        filps: &mut Vec<Filp>,
        vnodes: &mut Vec<VNodeRef>,
    ) -> Self {
        // 1. 复制文件描述符表（共享 filp，增加引用计数）
        let mut child_filps = parent.filps;
        for filp_idx in child_filps.iter_mut().flatten() {
            if *filp_idx < filps.len() {
                filps[*filp_idx].count += 1;
            }
        }

        // 2. 增加目录 vnode 引用计数
        if let Some(idx) = parent.root_dir {
            if idx < vnodes.len() { vnodes[idx].ref_count += 1; }
        }
        if let Some(idx) = parent.work_dir {
            if idx < vnodes.len() { vnodes[idx].ref_count += 1; }
        }

        // 3. 构造子进程 fproc
        FProc {
            flags: FpFlags::empty(),  // 清除所有标志
            pid: child_pid,
            endpoint: child_endpoint,
            root_dir: parent.root_dir,
            work_dir: parent.work_dir,
            filps: child_filps,
            cloexec_set: parent.cloexec_set,
            real_uid: parent.real_uid,
            eff_uid: parent.eff_uid,
            real_gid: parent.real_gid,
            eff_gid: parent.eff_gid,
            ngroups: parent.ngroups,
            supplemental_groups: parent.supplemental_groups,
            umask: parent.umask,
            name: parent.name,
        }
    }
}
```

---

## 3. 验证目标

- [ ] fproc 结构体字段与 Minix3 C 代码对应
- [ ] 文件描述符复制使用共享 filp + 引用计数
- [ ] FD_CLOEXEC 位图正确继承
- [ ] 目录 vnode 引用计数正确增加
- [ ] 子进程标志位清除
