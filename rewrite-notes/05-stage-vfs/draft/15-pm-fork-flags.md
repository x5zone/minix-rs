# 15-pm-fork-flags: pm_fork — 标志重置与 PID/endpoint 设置

> 本文档分析 `minix3/minix/servers/vfs/misc.c` 中 `pm_fork()` 函数的标志清除和标识字段设置。

---

## 1. 概述

### 1.1 复制后修正的必要性

`pm_fork()` 通过整体复制 `fproc[parentno]` 到 `fproc[childno]` 来初始化子进程。这种"先复制，后修正"的策略高效但需要修正两类字段：

1. **标识字段**：子进程必须有自己的 PID 和 endpoint，不能与父进程共享。整体复制后，子进程的 `fp_pid` 和 `fp_endpoint` 仍然是父进程的值，必须覆盖。

2. **状态标志**：父进程的某些运行时状态（如 `FP_EXITING`、`FP_PENDING`、`FP_PM_WORK`）属于父进程的当前状态，不应被继承。子进程应以"干净"状态开始。

这与 VM 的 `vmproc` 处理方式一致：Minix3 中 `*vmc = *vmp` 整体复制后，也需要修正 `vm_slot`、`vm_endpoint` 等字段。

---

## 2. PID 设置

### 2.1 cp->fp_pid = cpid

```c
cp->fp_pid = cpid;
```

`cpid` 是 PM 为子进程分配的唯一 PID。整体复制后，子进程的 `fp_pid` 仍然是父进程的 PID，必须覆盖为子进程自己的 PID。

**PID 与 endpoint 的区别**：

| 属性 | PID | endpoint |
|------|-----|----------|
| 管理者 | PM | Kernel |
| 作用域 | 用户空间可见的进程标识 | 系统内部 IPC 寻址标识 |
| 唯一性 | 系统级唯一 | slot + generation 保证唯一 |
| 用途 | `kill()`、`waitpid()` 等 POSIX API | IPC 消息路由、进程间通信 |

PID 是 PM 管理的逻辑标识，用户空间通过 PID 识别进程。endpoint 是 Kernel 管理的通信标识，系统内部通过 endpoint 路由 IPC 消息。两者独立分配，没有数值上的对应关系。

---

## 3. endpoint 设置

### 3.1 cp->fp_endpoint = cproc

```c
cp->fp_endpoint = cproc;
```

`cproc` 是子进程的 endpoint，由 PM 通过 `sys_fork()` 系统调用从 Kernel 获取。整体复制后，子进程的 `fp_endpoint` 仍然是父进程的 endpoint，必须覆盖。

**endpoint 在 VFS 中的作用**：VFS 通过 `fproc_addr(e)` 宏从 endpoint 定位 `fproc`：

```c
/* glo.h */
#define fproc_addr(e) (&fproc[_ENDPOINT_P(e)])
```

`_ENDPOINT_P(e)` 从 endpoint 中提取 slot 号，直接作为 `fproc[]` 数组索引。这意味着 endpoint 的 slot 部分与 `fproc[]` 数组索引是一一对应的。

### 3.2 endpoint 与 slot 的关系

```c
childno = _ENDPOINT_P(cproc);
if (childno < 0 || childno >= NR_PROCS)
    panic("VFS: bogus child for forking: %d", cproc);
```

`_ENDPOINT_P(cproc)` 从子进程 endpoint 中提取 slot 号，这个 slot 号就是 `fproc[]` 数组的索引。因此 `pm_fork()` 可以直接用 `fproc[childno]` 访问子进程的 `fproc` 结构体。

这是 Minix3 的核心设计约定：**进程的 slot 号在 Kernel、PM、VM、VFS 之间保持一致**。Kernel 分配 slot，PM 通过 `sys_fork()` 获得 endpoint（包含 slot 信息），然后将 endpoint 传递给 VM 和 VFS。各服务器通过 `_ENDPOINT_P()` 提取 slot 号，直接索引各自的进程表（`proc[]`、`mproc[]`、`vmproc[]`、`fproc[]`）。

这也是为什么 `pm_fork()` 不调用 `isokendpt()` 验证子进程 endpoint——子进程的 `fproc[childno].fp_pid` 此时还是 `PID_FREE`，`isokendpt()` 会失败。代码改用 `fp_pid != PID_FREE` 断言来验证 slot 空闲。

---

## 4. 标志清除

### 4.1 cp->fp_flags = FP_NOFLAGS

```c
cp->fp_flags = FP_NOFLAGS;
```

整体复制后，子进程继承了父进程的所有标志。`FP_NOFLAGS`（值为 0）清除所有标志，子进程以干净状态开始。各标志清除的原因：

| 标志 | 值 | 清除原因 |
|------|----|----------|
| `FP_EXITING` | 0x0020 | 子进程刚创建，未在退出中 |
| `FP_SRV_PROC` | 0x0001 | 子进程不是服务进程（除非通过 `VFS_PM_SRV_FORK` 创建） |
| `FP_REVIVED` | 0x0002 | 子进程未被恢复，没有挂起的恢复状态 |
| `FP_SESLDR` | 0x0004 | 子进程不是会话领导者（POSIX 规定 fork 不继承会话领导权） |
| `FP_PENDING` | 0x0010 | 子进程无待处理操作 |
| `FP_PM_WORK` | 0x0040 | 子进程无 PM 延迟请求 |

注意：`FP_SRV_PROC` 的清除意味着即使父进程是系统服务（如 RS 启动的进程），fork 创建的子进程也不是服务进程。服务进程的创建使用单独的 `VFS_PM_SRV_FORK` 路径。

### 4.2 标志清除的 POSIX 语义

POSIX 规定 fork 后子进程应以"干净"状态开始。具体来说：

- 子进程只有一个执行线程（即使父进程是多线程的）
- 子进程不继承父进程的文件锁（`flock`/`fcntl` 锁）
- 子进程不继承父进程的定时器（`alarm`、`timer_create` 等）
- 子进程的信号处置被继承，但挂起的信号被清除

`FP_NOFLAGS` 的清除策略符合 POSIX 精神：父进程的运行时状态（退出中、待处理、PM 工作等）是父进程的"瞬时状态"，不应传递给子进程。子进程应该像一个全新进程一样，只是恰好继承了父进程的文件描述符和目录等"持久资源"。

---

## 5. 不修改的字段

### 5.1 fp_blocked_on

`fp_blocked_on` 通过整体复制被继承，但不需要修正。`pm_fork()` 中有断言保证：

```c
#if !defined(NDEBUG)
assert(pp->fp_blocked_on == FP_BLOCKED_ON_NONE);
#endif
```

父进程在调用 fork 时不可能处于阻塞状态（VFS 的 `pm_fork()` 由 PM 的同步请求触发，发起 fork 的进程必然在运行中）。因此整体复制后，子进程的 `fp_blocked_on` 也是 `FP_BLOCKED_ON_NONE`，无需修正。

### 5.2 阻塞状态联合体

`fp_u`（阻塞状态联合体）也通过整体复制被继承。由于父进程未被阻塞（`fp_blocked_on == FP_BLOCKED_ON_NONE`），联合体中的内容是无效的残留数据，不会被访问。子进程也不会访问这个联合体，因为 `fp_blocked_on` 已经是 `FP_BLOCKED_ON_NONE`。

不需要显式清零，因为联合体的有效性由 `fp_blocked_on` 控制，而非内容本身。

### 5.3 凭证字段

`fp_realuid`/`fp_effuid`/`fp_realgid`/`fp_effgid`/`fp_ngroups`/`fp_sgroups[]` 通过整体复制被继承，不需要修正。

POSIX 规定 fork 后子进程完全继承父进程的凭证：实际用户 ID、有效用户 ID、实际组 ID、有效组 ID、补充组 ID 均与父进程相同。这是合理的——子进程是父进程的副本，应拥有相同的权限。

---

## 6. fp_worker 字段

### 6.1 工作线程关联

整体复制后，子进程的 `fp_worker` 指向父进程的工作线程。但这不需要修正。

`fp_worker` 是 VFS 工作线程模型的字段，记录当前正在为该进程服务的工作线程。`pm_fork()` 在 `service_pm()` 上下文中执行，此时工作线程正在处理 PM 请求。子进程刚被创建，还没有发起任何 VFS 请求，因此 `fp_worker` 不会被用于子进程。

当子进程后续发起 VFS 请求时，VFS 会分配一个新的工作线程，并将 `fp_worker` 设置为该线程。此时旧的（从父进程继承的）`fp_worker` 值已被覆盖，不会造成问题。

---

## 7. pm_fork 完整流程回顾

```
pm_fork(pproc, cproc, cpid):
  1. okendpt(pproc, &parentno)          — 验证父进程
  2. childno = _ENDPOINT_P(cproc)       — 提取子进程 slot
  3. assert(fproc[childno].fp_pid == PID_FREE) — 验证 slot 空闲
  4. c_fp_lock = fproc[childno].fp_lock — 保存子进程 mutex
  5. fproc[childno] = fproc[parentno]   — 整体复制
  6. fproc[childno].fp_lock = c_fp_lock — 恢复子进程 mutex
  7. for i in 0..OPEN_MAX: filp_count++ — 递增 filp 引用计数 (13)
  8. cp->fp_pid = cpid                  — 设置 PID (本文档)
  9. cp->fp_endpoint = cproc            — 设置 endpoint (本文档)
  10. cp->fp_flags = FP_NOFLAGS         — 清除标志 (本文档)
  11. dup_vnode(cp->fp_rd)              — 递增根目录引用 (14)
  12. dup_vnode(cp->fp_wd)              — 递增工作目录引用 (14)
```

---

## 8. C 源码

**文件**: `minix3/minix/servers/vfs/misc.c` (标志与标识设置部分)

```c
cp->fp_pid = cpid;
cp->fp_endpoint = cproc;
cp->fp_flags = FP_NOFLAGS;
```
