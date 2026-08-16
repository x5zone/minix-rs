# 17-filedes: 文件描述符管理 (close 路径)

> 本文档分析 `minix3/minix/servers/vfs/filedes.c` 中的文件描述符管理，重点关注 close 路径——fork 的逆操作。

---

## 1. 概述

### 1.1 filedes.c 的角色

`filedes.c` 负责文件描述符的分配、查找和释放，是 VFS 文件描述符管理的核心模块。

与 fork 的关系是**对称的**：
- **fork** 增加 `filp` 引用计数（`filp_count++`），建立共享
- **close** 减少引用计数（`filp_count--`），解除共享

理解 close 路径对理解 fork 语义至关重要，因为 fork 建立的共享关系最终由 close 解除。fork 时 `filp_count` 从 1 变为 2，close 时从 2 变回 1——只有最后一个引用者 close 时，文件才真正关闭。

---

## 2. 文件描述符分配

### 2.1 get_fd()

```c
int get_fd(struct fproc *rfp, int start, mode_t bits, int *k, struct filp **fpt)
{
    /* Search the fproc fp_filp table for a free file descriptor. */
    for (i = start; i < OPEN_MAX; i++) {
        if (rfp->fp_filp[i] == NULL) {
            *k = i;
            break;
        }
    }
    if (i >= OPEN_MAX) return(EMFILE);

    /* Now look for a free filp slot. */
    for (f = &filp[0]; f < &filp[NR_FILPS]; f++) {
        if (f->filp_count == 0 && mutex_trylock(&f->filp_lock) == 0) {
            f->filp_mode = bits;
            f->filp_pos = 0;
            /* ... */
            *fpt = f;
            return(OK);
        }
    }
    return(ENFILE);
}
```

`get_fd()` 同时完成两个分配：

1. **分配 fd slot**：从 `start` 开始线性扫描 `fp_filp[]`，找到第一个 `NULL` 条目。这保证了 POSIX 要求的"分配最小可用 fd"语义。

2. **分配 filp slot**：扫描全局 `filp[]` 数组，找到 `filp_count == 0` 的空闲 slot。`mutex_trylock` 确保不会与其他线程竞争。

注意：`get_fd()` 只"预留"资源，不真正占用。调用者（如 `open()`）在操作成功后才设置 `fp_filp[fd]` 和 `filp_count`。

### 2.2 get_filp2()

```c
struct filp *get_filp2(struct fproc *rfp, int fild, tll_access_t locktype)
{
    filp = NULL;
    if (fild < 0 || fild >= OPEN_MAX)
        err_code = EBADF;
    else if (locktype != VNODE_OPCL && rfp->fp_filp[fild] != NULL &&
             rfp->fp_filp[fild]->filp_mode == FILP_CLOSED)
        err_code = EIO;  /* disallow all use except close(2) */
    else if ((filp = rfp->fp_filp[fild]) == NULL)
        err_code = EBADF;
    /* ... lock vnode if needed ... */
    return(filp);
}
```

`get_filp2()` 根据 fd 号查找对应的 `filp`，执行两层验证：

1. **范围检查**：`fild` 必须在 `[0, OPEN_MAX)` 范围内
2. **有效性检查**：`fp_filp[fild]` 不能为 NULL（fd 未打开）
3. **特殊状态检查**：如果 filp 处于 `FILP_CLOSED` 状态，只允许 close 操作

`locktype` 参数控制是否对 vnode 加锁，用于防止并发访问。

---

## 3. 文件描述符关闭 (close 路径)

### 3.1 close_fd()

```c
/* open.c */
int close_fd(struct fproc *rfp, int fd_nr, int may_suspend)
{
    if ((rfilp = get_filp2(rfp, fd_nr, VNODE_OPCL)) == NULL) return(err_code);
    vp = rfilp->filp_vno;

    /* Make all future get_filp2()'s fail */
    rfp->fp_filp[fd_nr] = NULL;

    r = close_filp(rfilp, may_suspend);

    FD_CLR(fd_nr, &rfp->fp_cloexec_set);

    /* Release file locks */
    /* ... */
    return(r);
}
```

`close_fd()` 的核心操作：

1. **解除 fd 绑定**：`rfp->fp_filp[fd_nr] = NULL`，使后续 `get_filp2()` 对此 fd 返回 `EBADF`
2. **委托 close_filp()**：处理 `filp_count` 递减和 vnode 释放
3. **清除 cloexec 标记**：`FD_CLR()` 清除 close-on-exec 位图
4. **释放文件锁**：遍历 `file_lock[]`，释放该进程持有的所有锁

`filp_count` 降到 0 时的清理由 `close_filp()` 完成（见 3.2 节）。

### 3.2 close_filp()

```c
/* filedes.c */
int close_filp(struct filp *f, int may_suspend)
{
    /* ... 设备特殊处理 ... */

    if (--f->filp_count == 0) {
        /* 最后一个引用消失 */
        unlock_vnode(f->filp_vno);
        put_vnode(f->filp_vno);    /* v_ref_count-- */
        f->filp_vno = NULL;
        f->filp_mode = FILP_CLOSED;
        f->filp_count = 0;
    } else if (f->filp_count < 0) {
        panic("VFS: invalid filp count");
    } else {
        /* 还有其他引用，只解锁 */
        unlock_vnode(f->filp_vno);
    }

    mutex_unlock(&f->filp_lock);
    return r;
}
```

`close_filp()` 是 `close_fd()` 的内部实现，处理引用计数递减和资源释放。与 `close_fd()` 的关系：`close_fd()` 负责解除 fd→filp 的绑定，`close_filp()` 负责 filp→vnode 的引用计数管理。

### 3.3 filp_count > 0 时的行为

fork 后父子进程共享同一个 `filp`，`filp_count == 2`。当其中一个进程 close 时：

- `filp_count` 从 2 降到 1——文件**不真正关闭**
- `close_fd()` 只解除当前进程的 `fp_filp[fd]` 引用（设为 NULL）
- `close_filp()` 走 `else` 分支，只解锁 vnode，不调用 `put_vnode()`
- 另一个进程的 `fp_filp[fd]` 引用仍然有效，可以继续读写

这就是 POSIX 要求的 fork 后文件共享语义：父子进程共享文件偏移量，一方的 close 不影响另一方。

---

## 4. dup/dup2 操作

### 4.1 do_dup()

`do_dup()` / `do_dup2()` 让同一进程内的两个 fd 指向同一个 `filp`：

```c
/* dup2: 让 fd2 指向 fd1 的 filp */
rfp->fp_filp[fd2] = rfp->fp_filp[fd1];  /* 共享 filp */
rfp->fp_filp[fd1]->filp_count++;         /* 递增引用计数 */
```

dup 后 `filp_count` 递增，与 fork 的效果相同。区别在于：
- **dup**：同一进程内的两个 fd 共享 filp
- **fork**：两个进程的同号 fd 共享 filp

两者都通过 `filp_count` 管理共享关系，close 时的行为也一致：只有 `filp_count` 降到 0 才真正关闭文件。

### 4.2 dup 与 fork 的比较

| 维度 | fork | dup |
|------|------|-----|
| filp_count 递增 | 是 | 是 |
| 跨进程 | 是 | 否（同一进程） |
| 共享偏移量 | 是（跨进程） | 是（同进程） |
| cloexec 继承 | 继承位图 | 新 fd 清除 cloexec |

---

## 5. 文件描述符与 vnode 的关系

### 5.1 close 时的 vnode 释放

当 `filp_count` 降到 0 时，`close_filp()` 调用 `put_vnode()`：

```c
if (--f->filp_count == 0) {
    put_vnode(f->filp_vno);    /* v_ref_count-- */
    f->filp_vno = NULL;
    f->filp_count = 0;
}
```

`put_vnode()` 递减 `v_ref_count`。如果 `v_ref_count` 也降到 0，则通知底层 FS 释放 inode（`req_putnode`），并回收 vnode slot。

fork 后的 close 链路：

```
父进程 close(fd)
  → filp_count: 2 → 1（不调用 put_vnode）

子进程 close(fd)
  → filp_count: 1 → 0（调用 put_vnode）
  → v_ref_count: 1 → 0（调用 req_putnode，释放 inode）
```

### 5.2 vnode 引用计数链

```
close_fd(fd)
  │
  ├── fp_filp[fd]->filp_count--
  │
  ├── if (filp_count == 0):
  │   ├── put_vnode(filp_vno)    // v_ref_count--
  │   │   └── if (v_ref_count == 0):
  │   │       └── req_putnode()  // 通知底层 FS
  │   └── filp slot 释放
  │
  └── fp_filp[fd] = NULL
```

---

## 6. pm_exit 中的批量关闭

### 6.1 关闭所有文件描述符

`pm_exit()` 通过 `free_proc()` 关闭进程的所有打开文件：

```c
/* misc.c: free_proc() */
for (i = 0; i < OPEN_MAX; i++) {
    (void) close_fd(fp, i, FALSE /*may_suspend*/);
}
```

遍历 `fp_filp[]` 数组，对每个非 NULL 条目调用 `close_fd()`。`close_fd()` 内部调用 `get_filp2()` 检查 fd 是否有效，无效的（NULL）直接跳过。

fork 后的场景：如果父进程 exit，`close_fd()` 将 `filp_count` 从 2 递减到 1，子进程的文件描述符不受影响。子进程后续 close 时才会将 `filp_count` 从 1 递减到 0，触发 vnode 释放。

---

## 7. C 源码

**文件**: `minix3/minix/servers/vfs/filedes.c`

```c
// close 时的引用计数递减
if (--f->filp_count == 0) {
    // 最后一个引用消失，释放 vnode
    put_vnode(f->filp_vno);
    f->filp_vno = NULL;
} else {
    // 还有其他引用，只解锁
    unlock_vnode(f->filp_vno);
}
```
