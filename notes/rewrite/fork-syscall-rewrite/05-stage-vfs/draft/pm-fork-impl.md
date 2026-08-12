# VFS pm_fork 实现

> 本文档详细说明 VFS 层的 `pm_fork()` 实现，包括文件描述符复制和引用计数管理。

## C 源码位置

| 文件 | 路径 | 说明 |
|------|------|------|
| [`misc.c`](../../../../minix3/minix/servers/vfs/misc.c) | `minix3/minix/servers/vfs/misc.c` | VFS fork 主逻辑，`pm_fork()` 函数（约 50 行） |
| [`filedes.c`](../../../../minix3/minix/servers/vfs/filedes.c) | `minix3/minix/servers/vfs/filedes.c` | 文件描述符管理，`get_filp()`, `close_fd()` |
| [`vnode.c`](../../../../minix3/minix/servers/vfs/vnode.c) | `minix3/minix/servers/vfs/vnode.c` | VNode 管理，`dup_vnode()`, `put_vnode()` |
| [`vfs.h`](../../../../minix3/minix/servers/vfs/vfs.h) | `minix3/minix/servers/vfs/vfs.h` | `struct fproc`, `struct filp`, `struct vnode` 定义 |
| [`comm.c`](../../../../minix3/minix/servers/vfs/comm.c) | `minix3/minix/servers/vfs/comm.c` | VFS-PM 通信，`vfs_pm_fork_reply()` |

---

## 1. 函数签名

### 1.1 C 源码接口

```c
void pm_fork(endpoint_t pproc, endpoint_t cproc, pid_t cpid);
```

**参数**:
- `pproc`: 父进程 endpoint
- `cproc`: 子进程 endpoint
- `cpid`: 子进程 PID

### 1.2 Rust 接口设计

```rust
pub fn pm_fork(
    vfs: &mut VfsState,
    parent_ep: Endpoint,
    child_ep: Endpoint,
    child_pid: Pid,
) -> Result<(), Error>;
```

---

## 2. 核心数据结构

### 2.1 fproc

```c
struct fproc {
    unsigned fp_flags;
    pid_t fp_pid;
    endpoint_t fp_endpoint;
    vnode *fp_wd;                    // 工作目录
    vnode *fp_rd;                    // 根目录
    filp *fp_filp[OPEN_MAX];         // 文件描述符表
    fd_set fp_cloexec_set;           // FD_CLOEXEC 位图
    uid_t fp_realuid, fp_effuid;
    gid_t fp_realgid, fp_effgid;
    int fp_ngroups;
    gid_t fp_sgroups[NGROUPS_MAX];
    mode_t fp_umask;
    mutex_t fp_lock;                 // 属于 slot，不拷贝！
    char fp_name[PROC_NAME_LEN];
};
```

### 2.2 filp（全局文件表条目）

```c
struct filp {
    mode_t filp_mode;
    int filp_flags;
    int filp_count;                  // 引用计数！fork 时 ++
    vnode *filp_vno;
    off_t filp_pos;                  // 共享的文件偏移量
    mutex_t filp_lock;
};
```

### 2.3 vnode

```c
struct vnode {
    endpoint_t v_fs_e;
    ino_t v_inode_nr;
    mode_t v_mode;
    off_t v_size;
    int v_ref_count;                 // 引用计数！dup_vnode 时 ++
    int v_fs_count;
    vmnt *v_vmnt;
    tll_t v_lock;
};
```

---

## 3. 结构复制

### 3.1 pm_fork() 逐行分析

```c
void pm_fork(endpoint_t pproc, endpoint_t cproc, pid_t cpid) {
    // 1. 验证父进程 endpoint
    okendpt(pproc, &parentno);

    // 2. 提取子进程 slot 号（不能用 isokendpt，因为子进程 endpoint 还未设置）
    childno = _ENDPOINT_P(cproc);
    assert(fproc[childno].fp_pid == PID_FREE);

    // 3. 整体复制 fproc（但保留子进程自己的 mutex）
    c_fp_lock = fproc[childno].fp_lock;
    fproc[childno] = fproc[parentno];       // C 结构体赋值
    fproc[childno].fp_lock = c_fp_lock;     // 恢复子进程自己的 mutex

    // 4. 增加 filp 引用计数（核心！）
    cp = &fproc[childno];
    for (i = 0; i < OPEN_MAX; i++)
        if (cp->fp_filp[i] != NULL)
            cp->fp_filp[i]->filp_count++;   // 共享 filp，refcount++

    // 5. 设置子进程自己的 PID 和 endpoint
    cp->fp_pid = cpid;
    cp->fp_endpoint = cproc;

    // 6. 清除所有标志
    cp->fp_flags = FP_NOFLAGS;

    // 7. 增加目录 vnode 引用计数
    if (cp->fp_rd) dup_vnode(cp->fp_rd);   // v_ref_count++
    if (cp->fp_wd) dup_vnode(cp->fp_wd);   // v_ref_count++
}
```

---

## 4. 引用递增

### 4.1 引用计数关系图

```
fork 前:
  父进程 fproc
    fp_filp[0] ──→ filp (filp_count=1)
    fp_filp[1] ──→ filp (filp_count=1)
    fp_rd ──→ vnode (v_ref_count=1)
    fp_wd ──→ vnode (v_ref_count=1)

fork 后:
  父进程 fproc                   子进程 fproc
    fp_filp[0] ──┐                 fp_filp[0] ──┘  → filp (filp_count=2)
    fp_filp[1] ──┐                 fp_filp[1] ──┘  → filp (filp_count=2)
    fp_rd ───────┐                 fp_rd ───────┘   → vnode (v_ref_count=2)
    fp_wd ───────┐                 fp_wd ───────┘   → vnode (v_ref_count=2)
    fp_lock (自己的)               fp_lock (自己的，被保留)
    fp_pid = parent_pid            fp_pid = cpid
    fp_flags = (原值)              fp_flags = FP_NOFLAGS
```

### 4.2 close() 时的引用计数递减

```c
// close_fd() → close_filp()
if (--f->filp_count == 0) {
    // 最后一个引用消失，释放 vnode
    put_vnode(f->filp_vno);    // v_ref_count--
    f->filp_vno = NULL;
} else {
    // 还有其他引用，只解锁
    unlock_vnode(f->filp_vno);
}
```

**fork 后**: 父进程 close(fd) 只将 `filp_count` 从 2 减为 1，文件不会真正关闭。只有当 `filp_count` 降到 0 时才释放 vnode。

### 4.3 fp_lock 保留的原因

`fp_lock` 属于 fproc slot，不属于进程。VFS 是多线程的，fork 发生时可能有其他线程正在锁住子进程 slot 的 mutex。如果覆盖了这个 mutex，会导致死锁或崩溃。

---

## 5. 异步回复

VFS 处理完 `pm_fork()` 后，需要异步回复 PM：

```c
void vfs_pm_fork_reply(endpoint_t endpt) {
    message m;
    m.m_type = VFS_PM_FORK_REPLY;
    m.VFS_PM_ENDPT = endpt;
    asynsend(PM_PROC_NR, &m);
}
```

---

## 6. 错误处理

| 错误码 | 原因 | 处理 |
|--------|------|------|
| `EINVAL` | 无效的父进程 endpoint | 返回错误 |
| `EBUSY` | 子进程 slot 已被占用 | panic（不应该发生） |

---

## 7. Rust 结构定义

```rust
pub const OPEN_MAX: usize = 128;
pub const NGROUPS_MAX: usize = 32;

pub struct FProc {
    pub flags: FpFlags,
    pub pid: Pid,
    pub endpoint: Endpoint,
    pub root_dir: Option<VNodeRef>,
    pub work_dir: Option<VNodeRef>,
    pub filps: [Option<FilpRef>; OPEN_MAX],
    pub cloexec_set: u128,
    pub real_uid: Uid,
    pub eff_uid: Uid,
    pub real_gid: Gid,
    pub eff_gid: Gid,
    pub ngroups: usize,
    pub supplemental_groups: [Gid; NGROUPS_MAX],
    pub umask: u32,
    pub name: [u8; 16],
}

pub struct Filp {
    pub mode: u32,
    pub flags: i32,
    pub count: u32,           // 引用计数！
    pub vnode: Option<VNodeRef>,
    pub pos: i64,             // 共享的文件偏移量
}

pub struct VNodeRef {
    pub index: usize,
    pub ref_count: u32,       // 引用计数！
}

pub struct FilpRef {
    pub index: usize,         // 指向全局 filp 表的索引
}
```

---

## 8. 测试验证

### 8.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pm_fork_copies_fproc() {
        // 验证 fproc 被正确复制
    }

    #[test]
    fn test_pm_fork_increments_filp_count() {
        // 验证 filp_count 正确递增
    }

    #[test]
    fn test_pm_fork_preserves_fp_lock() {
        // 验证 fp_lock 被保留
    }
}
```

### 8.2 集成测试

```rust
#[test]
fn test_pm_fork_end_to_end() {
    // 完整 VFS fork 流程测试
}
```
