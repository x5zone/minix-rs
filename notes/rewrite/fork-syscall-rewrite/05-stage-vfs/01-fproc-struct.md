# 01-fproc-struct: FProc 结构体基本字段

> 本文档分析 `minix3/minix/servers/vfs/fproc.h` 中的 FProc 结构体基本字段。

---

## 1. 概述

### 1.1 FProc 的角色

FProc 是 VFS 的**每进程文件系统上下文**。在 Minix3 微内核架构中，内核和 VFS 各自维护独立的进程表：内核的 `struct proc` 管理调度与 IPC，VFS 的 `struct fproc` 管理文件系统语义。每个进程在 VFS 中都有一个对应的 FProc 实例，存储在全局数组 `fproc[NR_PROCS]` 中。

```c
/* fproc.h:10-13 */
/* This is the per-process information.  A slot is reserved for each potential
 * process. Thus NR_PROCS must be the same as in the kernel. It is not
 * possible or even necessary to tell when a slot is free here.
 */
EXTERN struct fproc {
  ...
} fproc[NR_PROCS];
```

关键设计要点：

- **槽位预留**：数组大小为 `NR_PROCS`，与内核进程表大小一致。每个"潜在进程"预留一个槽位，无需显式标记空闲——通过 `fp_pid == PID_FREE` 和 `fp_endpoint == NONE` 标识未使用的槽位
- **与内核进程表同下标映射**：`fproc[slot]` 与内核 `proc[slot]` 通过相同的数组下标一一对应，从 endpoint 中提取槽位号即可直接索引
- **松耦合关联**：内核和 VFS 通过 endpoint（端点号）这个轻量级标识符关联，而非直接引用对方的数据结构

与 Kernel 的 `struct proc` 对比——FProc 只关注文件系统相关状态

内核的 `struct proc` 和 VFS 的 `struct fproc` 是同一进程在两个不同层面的投影，关注域完全不同：

| 维度 | 内核 `struct proc` | VFS `struct fproc` |
|------|---------------------|---------------------|
| **核心职责** | 进程的**执行与通信** | 进程的**文件系统状态** |
| **调度** | `p_priority`, `p_rts_flags`, `p_nextready` 等 | 无调度字段 |
| **IPC** | `p_getfrom_e`, `p_sendto_e`, `p_sendmsg` 等 | `fp_msg`, `fp_pm_msg`（仅 VFS 层消息缓冲） |
| **内存管理** | `p_reg`（寄存器）, `p_seg`（段描述符） | 无 |
| **文件系统** | 无 | `fp_wd`, `fp_rd`, `fp_filp[]`, `fp_cloexec_set`, `fp_tty` |
| **数组范围** | `NR_TASKS + NR_PROCS`（含内核任务） | `NR_PROCS`（仅用户进程） |
| **并发控制** | 无（内核单线程） | `fp_lock`（互斥锁）, `fp_worker`（工作线程） |

两者通过 endpoint 建立联系（详见 [endpoint 概念](../../concepts/endpoint.md)）：

```c
/* glo.h:26-27 */
# define fproc_addr(e) (&fproc[_ENDPOINT_P(e)])   /* endpoint → fproc 指针 */
# define who_p       ((int) (fp - fproc))           /* fproc 指针 → slot 下标 */
```

`_ENDPOINT_P(e)` 从 endpoint 中提取 slot 编号，直接索引 `fproc[]` 数组，实现零开销映射。

### 1.2 文件位置

- `fproc.h` 完整路径为 `minix3/minix/servers/vfs/fproc.h`，`struct fproc` 定义在第 15~82 行（含 `fp_u` 联合体、凭证字段、工作线程字段等），`fproc[NR_PROCS]` 全局数组声明在第 82 行

FProc 与其他 VFS 数据结构的关系图

FProc 通过指针链与 `filp`、`vnode`、`vmnt` 构成 VFS 的核心数据关系网：

```
fproc.fp_filp[fd] ──→ filp ──filp_vno──→ vnode ──v_vmnt──→ vmnt
fproc.fp_wd ──────────────────────────→ vnode ──v_vmnt──→ vmnt
fproc.fp_rd ──────────────────────────→ vnode ──v_vmnt──→ vmnt
```

| 关系 | 指针字段 | 说明 |
|------|----------|------|
| fproc → filp | `fp_filp[fd]` | 文件描述符表，下标即 fd 号，NULL 表示未使用 |
| filp → vnode | `filp_vno` | 文件描述符指向的 vnode，多个 filp 可共享同一 vnode |
| vnode → vmnt | `v_vmnt` | vnode 所属的挂载点，确定文件在哪个文件系统上 |
| vmnt → vnode | `m_mounted_on` / `m_root_node` | 反向指针：挂载点目录 / 文件系统根目录 |

`filp` 是文件描述符与 vnode 之间的间接层——fork 后父子进程共享同一个 `filp`（`filp_count` 递增），共享读写偏移；而同一个 vnode 可被多个 `filp` 引用（`v_ref_count` 管理）。

---

## 2. 基本标识字段

### 2.1 fp_pid

`pid_t fp_pid`——进程 ID，由 PM 分配的 POSIX 进程标识符

```c
pid_t fp_pid;    /* process id */
```

`fp_pid` 在 VFS 中承担双重角色：

1. **进程标识**：POSIX 语义下的进程 ID，用于文件锁持有者标识（`lock_pid = fp->fp_pid`）、core dump 文件名（`core.<pid>`）等
2. **槽位空闲标记**：`fp_pid == PID_FREE`（值为 0）表示该槽位未被占用。PID 0 从不分配给用户进程，因此用 0 标记空闲是安全的

fork 时如何从 cpid 参数设置：

```c
/* misc.c: pm_fork() */
cp->fp_pid = cpid;    /* cpid 由 PM 通过 VFS_PM_FORK 消息传递 */
```

fork 前先检查子进程槽位是否空闲：`if (fproc[childno].fp_pid != PID_FREE) panic(...)`。整体复制父进程 fproc 后，用 PM 分配的 `cpid` 覆盖 `fp_pid`。

与 Kernel 的 `p_endpoint` 的区别——PID 是 PM 管理的逻辑 ID：

| 特性 | PID (`fp_pid`) | Endpoint (`fp_endpoint`) |
|------|----------------|--------------------------|
| **管理者** | PM（进程管理器） | 内核（Kernel） |
| **用途** | POSIX 语义：`getpid()`、文件锁标识、core dump | IPC 通信：消息路由、内核调度标识 |
| **空闲标记** | `PID_FREE`（值为 0） | `NONE`（值为 -1） |
| **稳定性** | 进程生命周期内不变 | 进程重启后可能改变（代数递增） |

### 2.2 fp_endpoint

`endpoint_t fp_endpoint`——内核端点号，VFS 与进程 IPC 通信的目标地址

```c
endpoint_t fp_endpoint;    /* kernel endpoint number of this process */
```

`fp_endpoint` 是 VFS 定位进程的核心标识。endpoint 编码了"代数（generation）+ 槽位号"，VFS 通过 `_ENDPOINT_P(fp_endpoint)` 提取槽位号，直接索引 `fproc[]` 数组。空闲槽位的 `fp_endpoint` 为 `NONE`（值为 -1）。

fork 时如何从 cproc 参数设置：

```c
/* misc.c: pm_fork() */
childno = _ENDPOINT_P(cproc);       /* 从子进程 endpoint 提取槽位号 */
cp->fp_endpoint = cproc;            /* 设置子进程的 endpoint */
```

PM 通过 `VFS_PM_FORK` 消息传递 `cproc`（子进程 endpoint），VFS 从中提取槽位号定位子进程的 fproc 条目。

用于 `fproc_addr(e)` 宏从 endpoint 定位 fproc：

```c
/* glo.h */
# define fproc_addr(e) (&fproc[_ENDPOINT_P(e)])
```

`fproc_addr()` 是 VFS 中最频繁使用的宏之一，将 endpoint 转换为 `struct fproc *` 指针。VFS 收到消息时，从 `m_source`（发送者 endpoint）获取 fproc：

```c
/* main.c: get_work() */
proc_p = _ENDPOINT_P(m_in.m_source);
fp = &fproc[proc_p];
```

VFS 还会校验 `fp->fp_endpoint` 与消息中的 endpoint 是否一致，防止进程替换导致的陈旧引用（详见 [endpoint 概念](../../concepts/endpoint.md)）。

### 2.3 fp_flags

`unsigned fp_flags`——进程标志位，使用位图编码多种进程状态

```c
unsigned fp_flags;
```

各标志定义：

| 标志 | 值 | 说明 |
|------|----|------|
| `FP_NOFLAGS` | 0x0000 | 无标志（初始/清除状态） |
| `FP_SRV_PROC` | 0x0001 | 服务进程 |
| `FP_REVIVED` | 0x0002 | 正在被恢复（从阻塞中唤醒） |
| `FP_SESLDR` | 0x0004 | 会话领导者 |
| `FP_PENDING` | 0x0010 | 有待处理的工作 |
| `FP_EXITING` | 0x0020 | 正在退出 |
| `FP_PM_WORK` | 0x0040 | 有延迟的 PM 请求 |

fork 时 `cp->fp_flags = FP_NOFLAGS`，清除所有标志。各标志的详细语义见 [02-fproc-flags](02-fproc-flags.md)。

---

## 3. 文件描述符表

### 3.1 fp_filp[OPEN_MAX]

`struct filp *fp_filp[OPEN_MAX]`——文件描述符表，数组下标即 fd 号

```c
struct filp *fp_filp[OPEN_MAX];    /* the file descriptor table (free if NULL) */
```

`OPEN_MAX` 定义为 255（`sys/sys/syslimits.h`），表示每个进程最多可同时打开 255 个文件。数组的每个元素是一个指向 `struct filp` 的指针：

- **非 NULL**：该 fd 已打开，指向一个 `filp` 结构体，包含文件偏移量、打开模式、指向 vnode 的指针等
- **NULL**：该 fd 未使用

VFS 在打开文件时通过线性扫描 `fp_filp[]` 寻找第一个 NULL 槽位作为新 fd。

fork 时父子进程共享同一组 filp 指针——整体复制 `fp_filp[]` 后，对每个非 NULL 的 `filp` 递增 `filp_count`：

```c
/* misc.c: pm_fork() */
for (i = 0; i < OPEN_MAX; i++)
    if (cp->fp_filp[i] != NULL) cp->fp_filp[i]->filp_count++;
```

这使得 fork 后父子进程共享文件偏移量（POSIX 要求），详见 [13-pm-fork-filp](13-pm-fork-filp.md)。

### 3.2 fp_cloexec_set

`fd_set fp_cloexec_set`——FD_CLOEXEC 位图，标记哪些 fd 在 exec 时应关闭

```c
fd_set fp_cloexec_set;    /* bit map for POSIX Table 6-2 FD_CLOEXEC */
```

`fd_set` 是 POSIX 定义的位图类型，每一位对应一个 fd。若第 i 位被设置，表示 fd i 具有 close-on-exec 属性。

POSIX Table 6-2 中 FD_CLOEXEC 的语义：POSIX 规定 `open()` 和 `fcntl()` 可以设置 `FD_CLOEXEC` 标志，使得该文件描述符在 `exec()` 时自动关闭。这是为了防止 exec 后子程序意外继承父程序的文件描述符（安全性考虑）。

fork 时整个位图被复制（继承父进程的 cloexec 设置）。由于 `pm_fork()` 先整体复制 fproc（包括 `fp_cloexec_set`），子进程自然继承了父进程的所有 cloexec 标记。

exec 时根据此位图关闭对应描述符：

```c
/* exec.c */
for (i = 0; i < OPEN_MAX; i++)
    if (FD_ISSET(i, &rfp->fp_cloexec_set))
        (void) close_fd(rfp, i, FALSE);
```

`FD_CLOEXEC` 标志可通过 `fcntl(fd, F_SETFD, FD_CLOEXEC)` 设置，或通过 `open(..., O_CLOEXEC)` / `dup2(..., O_CLOEXEC)` 在创建时直接设置。

---

## 4. 目录字段

### 4.1 fp_wd

`struct vnode *fp_wd`——工作目录（当前目录），路径解析的起点

```c
struct vnode *fp_wd;    /* working directory; NULL during reboot */
```

相对路径（如 `foo/bar`）从 `fp_wd` 开始解析，绝对路径（如 `/usr/bin`）从 `fp_rd` 开始解析。

"NULL during reboot" 的含义：系统启动过程中，VFS 尚未挂载根文件系统时，所有进程的 `fp_wd` 和 `fp_rd` 均为 NULL。根文件系统挂载后，VFS 在 `mount.c` 中遍历所有活跃进程，将 `fp_wd` 和 `fp_rd` 设为根目录 vnode。

fork 时通过 `dup_vnode()` 增加引用计数——整体复制 fproc 后，`fp_wd` 指向与父进程相同的 vnode，需递增 `v_ref_count`：

```c
/* misc.c: pm_fork() */
if (cp->fp_wd) dup_vnode(cp->fp_wd);
```

详见 [14-pm-fork-vnode](14-pm-fork-vnode.md)。

### 4.2 fp_rd

`struct vnode *fp_rd`——根目录，路径解析的上界

```c
struct vnode *fp_rd;    /* root directory; NULL during reboot */
```

绝对路径解析从 `fp_rd` 开始。普通进程的 `fp_rd` 通常为文件系统根目录 `/`，但 `chroot()` 可将其改为任意目录，限制该进程只能访问 `fp_rd` 以下的路径。

chroot 的影响：`chroot()` 系统调用改变调用进程的 `fp_rd`，使其指向新的根目录 vnode。此后该进程及其子进程的绝对路径解析被限制在新根以下。`chroot()` 同时将 `fp_wd` 设为新根目录。

fork 时通过 `dup_vnode()` 增加引用计数：

```c
/* misc.c: pm_fork() */
if (cp->fp_rd) dup_vnode(cp->fp_rd);
```

详见 [14-pm-fork-vnode](14-pm-fork-vnode.md)。

---

## 5. 终端字段

### 5.1 fp_tty

`dev_t fp_tty`——控制终端设备号，标识进程所属的终端会话

```c
dev_t fp_tty;    /* major/minor of controlling tty */
```

`dev_t` 编码了设备的主设备号和次设备号。若进程没有控制终端，`fp_tty` 为 0（`NO_DEV`）。

控制终端与会话管理的关系：POSIX 会话（session）由 `setsid()` 创建，会话领导者进程打开的第一个终端设备成为该会话的控制终端。同一会话中的所有进程共享同一个 `fp_tty`。控制终端用于：
- 信号传递：终端产生的信号（SIGINT、SIGQUIT、SIGTSTP）发送给该终端的前台进程组
- 作业控制：后台进程尝试从控制终端读取时收到 `SIGTTIN` 信号

fork 时子进程继承父进程的控制终端——`pm_fork()` 整体复制 fproc，`fp_tty` 自然被复制。子进程与父进程属于同一会话，共享同一控制终端。

---

## 6. 锁字段

### 6.1 fp_lock

`mutex_t fp_lock`——fproc 对象的互斥锁，保护 fproc 的并发访问

```c
mutex_t fp_lock;    /* mutex to lock fproc object */
```

`fp_lock` 是与 fproc 槽位绑定的同步原语，不是进程的属性。fork 时，子进程的 fproc 槽位已有自己的 `fp_lock`（在初始化时创建），不能被父进程的锁覆盖。否则会导致：
- 父子进程共享同一个 mutex，一方加锁后另一方永远阻塞
- 子进程槽位原有的等待队列丢失

```c
/* misc.c: pm_fork() */
c_fp_lock = fproc[childno].fp_lock;    /* 保存子进程自己的 mutex */
fproc[childno] = fproc[parentno];       /* 整体复制（覆盖了 fp_lock） */
fproc[childno].fp_lock = c_fp_lock;     /* 恢复子进程自己的 mutex */
```

VFS 多线程环境下 fp_lock 的保护范围：VFS 使用 worker thread 模型处理并发请求。`fp_lock` 保护 fproc 结构体不被多个工作线程同时修改，确保对 `fp_filp[]`、`fp_flags`、`fp_blocked_on` 等字段的访问是原子的。

---

## 7. 进程名字段

### 7.1 fp_name

`char fp_name[PROC_NAME_LEN]`——进程名，来自最后一次 `exec()` 的可执行文件名

```c
char fp_name[PROC_NAME_LEN];    /* Last exec() */
```

`PROC_NAME_LEN` 定义为 16（`minix/include/minix/type.h`），进程名最长 15 个字符 + '\0' 终止符。

进程名在 exec 时设置：`do_exec()` 从可执行文件路径中提取文件名，通过 `sys_datacopy()` 从用户空间复制到 `fp_name`，并强制在末尾添加 '\0'。

fork 时子进程继承父进程名（直到 exec）——`pm_fork()` 整体复制 fproc，`fp_name` 自然被复制。子进程在 `exec()` 之前，进程名与父进程相同。

---

## 8. fork 时的处理总结

| 字段 | fork 处理 | 说明 |
|------|----------|------|
| `fp_pid` | 设为 cpid | 使用子进程 PID |
| `fp_endpoint` | 设为 cproc | 使用子进程 endpoint |
| `fp_flags` | 设为 FP_NOFLAGS | 清除所有标志 |
| `fp_filp[]` | 指针复制 + filp_count++ | 共享 filp |
| `fp_cloexec_set` | 整体复制 | 继承 cloexec 位图 |
| `fp_wd` | 指针复制 + dup_vnode() | 共享 vnode |
| `fp_rd` | 指针复制 + dup_vnode() | 共享 vnode |
| `fp_tty` | 整体复制 | 继承控制终端 |
| `fp_lock` | 保留子进程自己的 mutex | 不从父进程复制 |
| `fp_name` | 整体复制 | 继承进程名 |

---

## 9. C 源码

**文件**: `minix3/minix/servers/vfs/fproc.h`

```c
EXTERN struct fproc {
  unsigned fp_flags;
  pid_t fp_pid;			/* process id */
  endpoint_t fp_endpoint;	/* kernel endpoint number of this process */
  struct vnode *fp_wd;		/* working directory; NULL during reboot */
  struct vnode *fp_rd;		/* root directory; NULL during reboot */
  struct filp *fp_filp[OPEN_MAX];/* the file descriptor table (free if NULL) */
  fd_set fp_cloexec_set;	/* bit map for POSIX Table 6-2 FD_CLOEXEC */
  dev_t fp_tty;			/* major/minor of controlling tty */
  // ... 后续字段见 02, 03
};
```
