# 19-pm-exit: pm_exit — fork 的逆操作

> 本文档分析 `minix3/minix/servers/vfs/misc.c` 中的 `pm_exit()` 函数，作为 fork 的逆操作对照。

---

## 1. 概述

### 1.1 pm_exit 的角色

`pm_exit()` 是 VFS 处理进程退出的核心函数，与 `pm_fork()` 构成对称关系：

- **pm_fork()**：建立引用（复制 fproc、递增 filp_count、dup_vnode）
- **pm_exit()**：释放引用（关闭 fd、递减 filp_count、put_vnode）

理解 exit 有助于验证 fork 的正确性：fork 建立的每个引用，exit 都必须正确释放。如果 exit 后子进程的文件描述符或目录 vnode 仍然可用，说明 fork 的引用计数递增是正确的。

### 1.2 函数签名

```c
void pm_exit(void)
```

`pm_exit()` 无参数，从全局变量 `fp`（当前工作线程关联的 `fproc` 指针）获取进程信息。这与 `pm_fork()` 不同——`pm_fork()` 的参数来自 PM 的请求消息（`pproc_e`、`proc_e`、`cpid`）。

原因：PM 发送 `VFS_PM_EXIT` 请求时，VFS 的工作线程已经绑定到退出的进程（`fp` 指向该进程的 `fproc`），无需额外参数。

---

## 2. 文件描述符关闭

### 2.1 遍历关闭

```c
for (i = 0; i < OPEN_MAX; i++) {
    (void) close_fd(fp, i, FALSE /*may_suspend*/);
}
```

`pm_exit()` 遍历 `fp_filp[]`，对每个 fd 调用 `close_fd()`。`close_fd()` 内部调用 `get_filp2()` 检查 fd 是否有效（NULL 的跳过），然后调用 `close_filp()` 递减 `filp_count`。

当 `filp_count` 降到 0 时，`close_filp()` 调用 `put_vnode()` 递减 `v_ref_count`，并可能触发 `req_putnode()` 通知底层 FS 释放 inode。

### 2.2 fork 后 close 的效果

fork 后父子进程共享 `filp`，`filp_count == 2`。父进程 exit 时：

- `close_fd()` 将 `fp_filp[i]` 设为 NULL，解除父进程与 filp 的绑定
- `close_filp()` 将 `filp_count` 从 2 递减到 1，走 `else` 分支（不调用 `put_vnode()`）
- 子进程的 `fp_filp[i]` 仍然指向同一个 `filp`，**不是悬空指针**

`filp` 是全局表 `filp[NR_FILPS]` 中的条目，`fproc` 只持有指向它的指针。父进程 close 后，`filp` 仍然存在（`filp_count > 0`），子进程的指针仍然有效。

---

## 3. 目录 vnode 释放

### 3.1 put_vnode(fp_rd)

```c
if (fp->fp_rd) { put_vnode(fp->fp_rd); fp->fp_rd = NULL; }
```

释放根目录 vnode 的引用。`put_vnode()` 递减 `v_ref_count`，如果降到 0 则通知底层 FS 释放 inode 并回收 vnode slot。释放后将 `fp_rd` 设为 NULL，防止后续误用。

### 3.2 put_vnode(fp_wd)

```c
if (fp->fp_wd) { put_vnode(fp->fp_wd); fp->fp_wd = NULL; }
```

释放工作目录 vnode 的引用，逻辑与 `fp_rd` 完全相同。NULL 检查的原因与 `pm_fork()` 中一致——reboot 期间 `fp_wd` 可能为 NULL。

### 3.3 fork 后 put_vnode 的效果

fork 后 `dup_vnode()` 将 `v_ref_count` 从 1 递增到 2。父进程 exit 时：

- `put_vnode()` 将 `v_ref_count` 从 2 递减到 1
- `v_ref_count > 0`，vnode slot 不回收，不通知底层 FS
- 子进程的 `fp_rd`/`fp_wd` 仍然指向有效的 vnode

只有子进程也 exit 时，`v_ref_count` 才从 1 降到 0，此时 vnode slot 被回收，并通知底层 FS 释放 inode。

---

## 4. 控制终端处理

### 4.1 会话领导者退出

```c
if (fp->fp_flags & FP_SESLDR) {
    /* Session leader is exiting. Hang up its controlling tty. */
    if (fp->fp_tty != 0) {
        (void) dev_io(VFS_DEV_IOCTL, fp->fp_tty, 0, 0, 0,
                      TIOCHANGUP, NULL, fp);
    }
}
```

会话领导者（session leader）退出时，VFS 向其控制终端发送 `TIOCHANGUP`（hangup）信号。这会触发内核向该终端前台进程组的所有进程发送 `SIGHUP`，通知它们控制终端已断开。

### 4.2 fork 后的控制终端

子进程继承父进程的控制终端（`fp_tty` 通过整体复制被继承）。父进程 exit 时：

- 如果父进程是会话领导者，控制终端被 hangup，子进程失去控制终端
- 如果父进程不是会话领导者，控制终端不受影响，子进程继续使用

这是 POSIX 规定的行为：会话领导者退出时，其控制终端被释放，同一会话中的其他进程无法继续使用该终端。

---

## 5. 设备映射清理

### 5.1 dmap 清理

```c
dmap_unmap_by_endpt(fp->fp_endpoint);
```

如果退出进程是设备驱动，`dmap_unmap_by_endpt()` 清理其在 `dmap[]`（设备映射表）中的条目。普通用户进程的 endpoint 不会出现在 `dmap[]` 中，此调用无副作用。

### 5.2 vmnt 清理

```c
vmnt_unmap_by_endpt(fp->fp_endpoint);
```

如果退出进程是文件系统服务器，`vmnt_unmap_by_endpt()` 清理其在 `vmnt[]`（挂载点表）中的条目。普通用户进程的 endpoint 不会出现在 `vmnt[]` 中，此调用无副作用。

这两项清理与 fork 无直接关系——fork 创建的是普通用户进程，不会成为设备驱动或文件系统服务器。

---

## 6. 进程状态标记

### 6.1 FP_EXITING 标志

```c
fp->fp_flags |= FP_EXITING;
```

设置 `FP_EXITING` 标志，标记进程正在退出。此标志在退出过程中起保护作用：后续对退出进程的 VFS 请求会被拒绝，防止在清理过程中发生新的文件操作。

### 6.2 fp_pid 标记

```c
fp->fp_pid = PID_FREE;
```

将 `fp_pid` 设为 `PID_FREE`，标记 `fproc` slot 空闲。这与 `pm_fork()` 中的断言对应：

```c
assert(fproc[childno].fp_pid == PID_FREE);
```

`pm_fork()` 断言子进程的 slot 必须空闲（`fp_pid == PID_FREE`），`pm_exit()` 将 `fp_pid` 设为 `PID_FREE` 释放 slot。两者配合保证 slot 不会被重复分配。

---

## 7. fork 与 exit 的对称性

| 操作 | pm_fork (建立) | pm_exit (释放) |
|------|---------------|---------------|
| fproc 复制 | `fproc[child] = fproc[parent]` | `fp_pid = PID_FREE` |
| filp 引用 | `filp_count++` | `filp_count--` (close_fd) |
| vnode 引用 | `dup_vnode()` (v_ref_count++) | `put_vnode()` (v_ref_count--) |
| 进程标志 | `fp_flags = FP_NOFLAGS` | `fp_flags = FP_EXITING` |
| PID | `fp_pid = cpid` | `fp_pid = PID_FREE` |
| endpoint | `fp_endpoint = cproc` | (由 Kernel 清理) |

---

## 8. C 源码

**文件**: `minix3/minix/servers/vfs/misc.c` (pm_exit 关键部分)

```c
void pm_exit(void)
{
  // ... 关闭所有文件描述符 ...
  for (i = 0; i < OPEN_MAX; i++) {
      if (fp->fp_filp[i] != NULL) {
          close_fd(fp, i);
      }
  }
  // ... 释放目录 vnode ...
  if (fp->fp_rd) put_vnode(fp->fp_rd);
  if (fp->fp_wd) put_vnode(fp->fp_wd);
  // ... 控制终端处理 ...
  // ... 设备映射清理 ...
  // ... 标记进程退出 ...
  fp->fp_pid = PID_FREE;
}
```
