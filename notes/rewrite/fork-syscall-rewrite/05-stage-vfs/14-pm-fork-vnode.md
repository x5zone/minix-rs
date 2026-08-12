# 14-pm-fork-vnode: pm_fork — vnode 引用计数 (dup_vnode)

> 本文档分析 `minix3/minix/servers/vfs/misc.c` 中 `pm_fork()` 的 vnode 引用计数处理，以及 `vnode.c` 中的 `dup_vnode()` / `put_vnode()` 函数。

---

## 1. 概述

### 1.1 目录 vnode 的共享语义

fork 后，子进程继承父进程的根目录 (`fp_rd`) 和工作目录 (`fp_wd`)。POSIX 规定 fork 后子进程与父进程共享相同的根目录和工作目录。

共享的实现方式是**共享 vnode 指针 + 递增引用计数**：

```c
/* fproc.h */
struct vnode *fp_wd;    /* working directory; NULL during reboot */
struct vnode *fp_rd;    /* root directory; NULL during reboot */
```

fork 时，`pm_fork()` 先将父进程的 `fproc` 整体复制给子进程（包括 `fp_rd` 和 `fp_wd` 指针），然后对非 NULL 的 vnode 调用 `dup_vnode()` 递增 `v_ref_count`：

```c
/* misc.c: pm_fork() */
if (cp->fp_rd) dup_vnode(cp->fp_rd);
if (cp->fp_wd) dup_vnode(cp->fp_wd);
```

这与 filp 的处理方式不同。目录 vnode 在 `fproc` 中**直接引用**（`fp_rd`/`fp_wd` 是 `struct vnode*`），不经过 `filp` 中间层。而打开的文件通过 `fp_filp[]` → `filp` → `vnode` 的间接路径引用，fork 时递增的是 `filp_count` 而非 `v_ref_count`。

---

## 2. pm_fork 中的 vnode 处理

### 2.1 dup_vnode 调用

`pm_fork()` 在整体复制父进程的 `fproc` 之后，对子进程的目录 vnode 调用 `dup_vnode()`：

```c
/* misc.c: pm_fork() */

/* Copy the parent's fproc struct to the child. */
fproc[childno] = fproc[parentno];

/* Increase the counters in the 'filp' table. */
cp = &fproc[childno];
for (i = 0; i < OPEN_MAX; i++)
    if (cp->fp_filp[i] != NULL) cp->fp_filp[i]->filp_count++;

/* Record the fact that both root and working dir have another user. */
if (cp->fp_rd) dup_vnode(cp->fp_rd);
if (cp->fp_wd) dup_vnode(cp->fp_wd);
```

注意执行顺序：

1. **整体复制** `fproc[childno] = fproc[parentno]` — 子进程的 `fp_rd`/`fp_wd` 已经指向父进程的 vnode
2. **递增 filp_count** — 处理打开文件的引用计数
3. **递增 v_ref_count** — 处理目录 vnode 的引用计数

此时父子进程的 `fp_rd`/`fp_wd` 指向同一个 vnode，`dup_vnode()` 将 `v_ref_count` 从 1 递增到 2，正确反映了"两个进程共享同一目录"的事实。

### 2.2 NULL 检查

`if (cp->fp_rd)` 和 `if (cp->fp_wd)` 检查是必要的，因为 `fp_rd`/`fp_wd` 在特定时刻可能为 NULL：

```c
/* fproc.h */
struct vnode *fp_wd;    /* working directory; NULL during reboot */
struct vnode *fp_rd;    /* root directory; NULL during reboot */
```

**NULL 的场景**：系统启动期间（reboot 阶段），VFS 初始化时先将所有 `fproc` 的 `fp_rd`/`fp_wd` 设为 NULL：

```c
/* main.c: init_vfs() */
rfp->fp_rd = NULL;
rfp->fp_wd = NULL;
```

根目录 vnode 在 `do_init_root()` 中才被挂载并赋值。在此之前的窗口期，如果有进程 fork（虽然正常启动流程中不会发生），`fp_rd`/`fp_wd` 就是 NULL。`dup_vnode(NULL)` 会导致空指针解引用，因此必须先做 NULL 检查。

### 2.3 为什么不递增 filp 指向的 vnode 引用计数

`pm_fork()` 对 `fp_filp[]` 指向的 vnode **不调用** `dup_vnode()`，只递增 `filp_count`：

```c
for (i = 0; i < OPEN_MAX; i++)
    if (cp->fp_filp[i] != NULL) cp->fp_filp[i]->filp_count++;
```

原因在于 `filp` 和 `vnode` 之间的引用关系：

- **filp_count** 管理 `filp` 的生命周期：有多少个 `fp_filp[]` 条目指向这个 `filp`
- **v_ref_count** 管理 `vnode` 的生命周期：有多少个 `filp` 或直接引用指向这个 `vnode`

fork 时，父子进程的 `fp_filp[i]` 指向同一个 `filp`，`filp_count` 从 1 变为 2。但 `filp` 本身只持有**一个** `vnode` 引用，因此 `v_ref_count` 不需要改变。

换句话说：`filp` 是 vnode 的"代理"，fork 增加的是代理的引用者数量（`filp_count`），而非代理指向的目标的引用数量（`v_ref_count`）。只有当 `filp_count` 降到 0 导致 `filp` 被释放时，才会调用 `put_vnode()` 递减 `v_ref_count`。

---

## 3. dup_vnode() 函数

### 3.1 函数实现

`dup_vnode()` 的实现非常简单：

```c
/* vnode.c */
void dup_vnode(struct vnode *vp)
{
    ASSERTVP(vp);
    vp->v_ref_count++;
}
```

核心操作只有一行：`vp->v_ref_count++`。

**关于锁保护**：`dup_vnode()` 本身没有加锁。在 Minix3 VFS 中，每个 `fproc` 有自己的 `fp_lock` 互斥锁，`pm_fork()` 在持有相关锁的上下文中执行。此外，VFS 使用工作线程模型（worker thread），同一时刻只有一个线程操作特定进程的 `fproc`，因此不需要在 `dup_vnode()` 内部额外加锁。

`ASSERTVP(vp)` 是调试断言，验证 `vp` 指针落在 `vnode[]` 数组范围内，防止野指针。

### 3.2 调用场景

`dup_vnode()` 在 VFS 中的所有调用场景：

| 场景 | 文件 | 说明 |
|------|------|------|
| `pm_fork()` | misc.c | fork 时递增子进程的根目录/工作目录 vnode |
| `chdir()`/`fchdir()` | stadir.c | 改变工作目录，对新 vnode 递增引用 |
| 路径解析 | path.c | 解析路径时引用中间目录 vnode（3 处） |
| `chmod()`/`chown()` | protect.c | 修改文件属性时临时引用 vnode（2 处） |
| 管道创建 | pipe.c | 创建管道时引用 vnode |
| 挂载 | mount.c | 挂载文件系统时引用根 vnode |

其中与 fork 直接相关的是 `misc.c` 中的调用。其余场景的共同模式是：**获取 vnode 引用时递增，使用完毕后通过 `put_vnode()` 递减**。

---

## 4. put_vnode() 函数

### 4.1 函数实现

`put_vnode()` 比 `dup_vnode()` 复杂得多，因为需要处理引用计数归零时的清理：

```c
/* vnode.c */
void put_vnode(struct vnode *vp)
{
    ASSERTVP(vp);
    lock_vp = lock_vnode(vp, VNODE_OPCL);

    if (vp->v_ref_count > 1) {
        /* 简单递减 */
        vp->v_ref_count--;
        if (vp->v_fs_count > 256)
            vnode_clean_refs(vp);  /* 防止 v_fs_count 溢出 */
        if (lock_vp != EBUSY) unlock_vnode(vp);
        return;
    }

    /* v_ref_count == 1，即将归零 */
    upgrade_vnode_lock(vp);  /* 升级为排他锁 */

    /* 通知底层 FS 释放 inode */
    r = req_putnode(vp->v_fs_e, vp->v_inode_nr, vp->v_fs_count);

    /* 如果有映射 FS，也通知释放 */
    if (vp->v_mapfs_e != NONE && vp->v_mapfs_e != vp->v_fs_e)
        req_putnode(vp->v_mapfs_e, vp->v_mapinode_nr, vp->v_mapfs_count);

    /* 重置所有计数器 */
    vp->v_fs_count = 0;
    vp->v_ref_count = 0;
    vp->v_mapfs_count = 0;

    unlock_vnode(vp);
}
```

**关键逻辑**：

- `v_ref_count > 1`：简单递减，无需清理
- `v_ref_count == 1`：即将归零，需要通知底层 FS 释放 inode（`req_putnode`），并重置 vnode slot

**与 `dup_vnode()` 的锁差异**：`put_vnode()` 需要加锁，因为 `v_ref_count` 归零时涉及跨服务器通信（`req_putnode`），必须保证排他访问。

### 4.2 调用场景

`put_vnode()` 的调用场景远多于 `dup_vnode()`，几乎涉及所有 VFS 操作。与 fork/exit 直接相关的场景：

| 场景 | 文件 | 说明 |
|------|------|------|
| `free_proc()` | misc.c | 进程退出时递减 `fp_rd`/`fp_wd` |
| `close_fd()` → `close_filp()` | filedes.c | `filp_count` 降到 0 时递减 `filp_vno` |
| `chdir()` | stadir.c | 替换工作目录时递减旧 vnode |

其他主要场景（路径解析、文件操作等）都是"获取引用后释放"的配对模式，与 fork 无直接关系。

---

## 5. 引用计数关系

### 5.1 vnode 的双重引用计数

vnode 维护两个独立的引用计数：

```c
/* vnode.h */
int v_ref_count;    /* # times vnode used; 0 means slot is free */
int v_fs_count;     /* # reference at the underlying FS */
```

- **`v_ref_count`**（VFS 层）：记录 VFS 内部有多少引用指向此 vnode。引用来源包括 `fp_rd`/`fp_wd`（目录 vnode 直接引用）和 `filp`（文件 vnode 通过 filp 间接引用）。`v_ref_count == 0` 表示 vnode slot 空闲。

- **`v_fs_count`**（底层 FS 层）：记录 VFS 向底层 FS 报告的引用数量。底层 FS 用此计数管理 inode 的打开/关闭。

**`dup_vnode()` 只递增 `v_ref_count`，不递增 `v_fs_count`**。原因在 `put_vnode()` 的注释中说明：

```c
/* Decreasing the fs_count each time we decrease the ref count would lead
 * to poor performance. Instead, only decrease fs_count when the ref count
 * hits zero.
 */
```

每次 `dup_vnode()`/`put_vnode()` 都同步 `v_fs_count` 会导致频繁的跨服务器通信（`req_putnode`）。Minix3 的策略是：**`v_fs_count` 只在 `v_ref_count` 归零时才递减**，并设置 256 的阈值防止 `v_fs_count` 溢出（`vnode_clean_refs`）。

### 5.2 引用计数图

```
fork 前:
  fproc.parent
    fp_filp[0] ──→ filp (count=1) ──→ vnode_file (v_ref_count=1)
    fp_rd ──────────────────────────→ vnode_root (v_ref_count=1)
    fp_wd ──────────────────────────→ vnode_cwd  (v_ref_count=1)

fork 后:
  fproc.parent                    fproc.child
    fp_filp[0] ──┐                 fp_filp[0] ──┘  → filp (count=2) → vnode_file (ref=1, 不变!)
    fp_rd ───────┐                 fp_rd ───────┘   → vnode_root (ref=2)
    fp_wd ───────┐                 fp_wd ───────┘   → vnode_cwd  (ref=2)
```

**关键**: filp 指向的 vnode 的 v_ref_count 在 fork 时不改变，因为增加的是 filp_count。

---

## 6. 进程退出时的 vnode 清理

### 6.1 close 所有文件

`free_proc()` 是 `pm_exit()` 的核心，负责关闭进程的所有打开文件：

```c
/* misc.c: free_proc() */
for (i = 0; i < OPEN_MAX; i++) {
    (void) close_fd(fp, i, FALSE /*may_suspend*/);
}
```

`close_fd()` 的行为：
1. 获取 `fp->fp_filp[i]` 对应的 `filp`
2. 递减 `filp->filp_count`
3. 当 `filp_count` 降到 0 时，调用 `put_vnode(filp->filp_vno)` 递减 `v_ref_count`
4. 如果 `v_ref_count` 也降到 0，通知底层 FS 释放 inode

**fork 后父子进程共享 `filp`**，因此父进程 `close_fd()` 只是将 `filp_count` 从 2 递减到 1，`v_ref_count` 不变。只有最后一个引用者 close 时，vnode 才会被释放。

### 6.2 释放目录 vnode

关闭文件后，`free_proc()` 释放根目录和工作目录 vnode：

```c
if (fp->fp_rd) { put_vnode(fp->fp_rd); fp->fp_rd = NULL; }
if (fp->fp_wd) { put_vnode(fp->fp_wd); fp->fp_wd = NULL; }
```

与 `close_fd()` 不同，目录 vnode 的释放是直接的 `put_vnode()` 调用，不经过 `filp` 中间层。

**fork 后的场景**：`dup_vnode()` 将 `v_ref_count` 从 1 递增到 2。父进程 exit 时 `put_vnode()` 将 `v_ref_count` 从 2 递减到 1，vnode 不会被释放。子进程 exit 时 `put_vnode()` 将 `v_ref_count` 从 1 递减到 0，此时 vnode slot 被回收，并通知底层 FS 释放 inode。

---

## 7. C 源码

**文件**: `minix3/minix/servers/vfs/misc.c` (vnode 引用计数部分)

```c
/* Duplicate root and working directory vnodes */
if (cp->fp_rd) dup_vnode(cp->fp_rd);
if (cp->fp_wd) dup_vnode(cp->fp_wd);
```

**文件**: `minix3/minix/servers/vfs/vnode.c` (dup_vnode)

```c
void dup_vnode(struct vnode *vp)
{
    // ... 锁操作 ...
    vp->v_ref_count++;
    // ... 解锁 ...
}
```
