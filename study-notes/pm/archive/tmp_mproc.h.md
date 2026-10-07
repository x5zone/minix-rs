# servers/pm/mproc.h 逐行讲解

> **文件路径**: `minix3/minix/servers/pm/mproc.h`
> **核心功能**: PM（进程管理器）进程表结构定义
> **所属模块**: PM（Process Manager）

---

## 一、文件概述

### 1.1 功能说明（是什么）

`mproc.h` 定义了 PM（进程管理器）的**进程表结构**，存储每个进程的管理信息。这是 Minix3 微内核架构的核心数据结构之一。

**生活类比**：想象一个学校的学籍管理系统：
- 每个学生有一个档案袋（进程槽位）。
- 档案袋里记录学生的姓名、班级、成绩、家长信息等。
- 学校的教务处（PM）、财务处（内核）、图书馆（VFS）各自维护自己的档案系统。
- 但三个系统的档案通过学号（进程索引）关联，指向同一个学生。

### 1.2 设计原因（为什么）

**为什么需要进程表？**

1. **进程管理**：PM 需要跟踪所有进程的状态、权限、关系等信息。

2. **POSIX 兼容**：实现 POSIX 标准要求的进程管理功能（fork、exec、wait 等）。

3. **微内核架构**：内核只维护最小信息，PM 维护详细的进程管理信息。

**为什么与内核、VFS 的进程表分离？**

```
Minix3 三表分离架构:
┌─────────────────────────────────────────────────────────────┐
│ 内核进程表 (kernel/proc.h)                                   │
│ - 最小信息：端点、优先级、栈指针                              │
│ - 用于调度和 IPC                                             │
├─────────────────────────────────────────────────────────────┤
│ PM 进程表 (servers/pm/mproc.h)                               │
│ - 管理信息：pid、uid/gid、信号、父子关系                      │
│ - 用于 POSIX 进程管理                                        │
├─────────────────────────────────────────────────────────────┤
│ VFS 进程表 (servers/vfs/fproc.h)                             │
│ - 文件信息：文件描述符、当前目录                              │
│ - 用于文件系统操作                                           │
└─────────────────────────────────────────────────────────────┘
        ↓               ↓               ↓
    相同索引        相同索引        相同索引
        └───────────────┴───────────────┘
                    同一进程
```

### 1.3 应用场景（什么情景使用）

| 场景 | 说明 |
|------|------|
| fork() | 复制父进程的 mproc 结构 |
| exec() | 更新进程的内存映像信息 |
| wait() | 查找子进程状态 |
| 信号处理 | 查询和修改信号掩码 |
| 权限检查 | 检查 uid/gid 权限 |

---

## 二、逐行详细讲解

### 2.1 文件头注释

```c
/* This table has one slot per process.  It contains all the process management
 * information for each process.  Among other things, it defines the text, data
 * and stack segments, uids and gids, and various flags.  The kernel and file
 * systems have tables that are also indexed by process, with the contents
 * of corresponding slots referring to the same process in all three.
 */
```

**逐行解释**：

- **第1-5行**：注释说明进程表的作用。
  - "one slot per process"：每个进程一个槽位。
  - "process management information"：进程管理信息。
  - "text, data and stack segments"：文本、数据和栈段（已废弃，现在由 VM 管理）。
  - "uids and gids"：用户 ID 和组 ID。
  - "various flags"：各种标志。
  - "kernel and file systems have tables"：内核和文件系统也有表。
  - "indexed by process"：按进程索引。
  - "same process in all three"：三者指向同一个进程。

**翻译**：
```
这个表每个进程有一个槽位。它包含每个进程的所有进程管理信息。
其中包括文本、数据和栈段，uid 和 gid，以及各种标志。
内核和文件系统也有按进程索引的表，三者对应槽位的内容指向同一个进程。
```

---

### 2.2 头文件包含

```c
#include <limits.h>
#include <minix/timers.h>
#include <signal.h>

#include <sys/cdefs.h>

/* Needs to be included here, for 'ps' etc */
#include "const.h"
```

**逐行解释**：

- **第1行**：`#include <limits.h>` — 系统限制定义。
  - 提供 `NGROUPS_MAX`（最大补充组数）等常量。

- **第2行**：`#include <minix/timers.h>` — Minix 定时器支持。
  - 提供 `minix_timer_t` 类型，用于 `alarm()` 和 `setitimer()`。

- **第3行**：`#include <signal.h>` — 信号处理定义。
  - 提供 `sigset_t`、`struct sigaction` 等类型。

- **第5行**：`#include <sys/cdefs.h>` — 编译器扩展定义。
  - 提供 `__BEGIN_DECLS`、`__END_DECLS` 等宏。

- **第7-8行**：注释说明需要包含 `const.h`。
  - "for 'ps' etc"：为了 `ps` 命令等工具能访问进程状态。

- **第9行**：`#include "const.h"` — PM 常量定义。
  - 提供 `NR_PROCS`、`PROC_NAME_LEN` 等常量。

---

### 2.3 信号动作存储

```c
/*
 * The per-process sigaction structures are stored outside of the mproc table,
 * so that the MIB service can avoid pulling them in, as they account for
 * roughly 80% of the per-process state.
 */
typedef struct sigaction ixfer_sigaction;
EXTERN ixfer_sigaction mpsigact[NR_PROCS][_NSIG];
```

**逐行解释**：

- **第1-5行**：注释说明信号动作结构的存储位置。
  - "per-process sigaction structures"：每进程信号动作结构。
  - "stored outside of the mproc table"：存储在 mproc 表之外。
  - "MIB service can avoid pulling them in"：MIB 服务可以避免加载它们。
  - "roughly 80% of the per-process state"：约占进程状态的 80%。

**设计原因**：

```
信号动作分离存储的原因:
┌─────────────────────────────────────────────────────────────┐
│ 问题：struct sigaction 占用大量空间                          │
│ - 每个信号一个 sigaction（约 16-32 字节）                    │
│ - _NSIG 通常为 64 个信号                                     │
│ - 每进程需要 64 * 32 = 2048 字节                             │
│ - NR_PROCS 个进程需要大量内存                                │
├─────────────────────────────────────────────────────────────┤
│ 解决方案：分离存储                                           │
│ - mproc 结构体保持紧凑                                       │
│ - mpsigact 单独存储信号动作                                  │
│ - MIB 服务不需要信号处理，可以不加载 mpsigact                │
└─────────────────────────────────────────────────────────────┘
```

- **第6行**：`typedef struct sigaction ixfer_sigaction;`
  - 定义 `ixfer_sigaction` 类型别名。
  - `ixfer` 可能表示 "inter-process transfer"（进程间传输）。

- **第7行**：`EXTERN ixfer_sigaction mpsigact[NR_PROCS][_NSIG];`
  - `EXTERN`：宏定义，在头文件中声明，在 `.c` 文件中定义。
  - `mpsigact`：进程信号动作表。
  - `[NR_PROCS]`：每个进程一个条目。
  - `[_NSIG]`：每个信号一个 sigaction。

**内存布局**：

```
mpsigact 内存布局:
┌─────────────────────────────────────────────────────────────┐
│ mpsigact[0][0]  - 进程 0, 信号 1 (SIGHUP)                    │
│ mpsigact[0][1]  - 进程 0, 信号 2 (SIGINT)                    │
│ ...                                                          │
│ mpsigact[0][63] - 进程 0, 信号 64                            │
├─────────────────────────────────────────────────────────────┤
│ mpsigact[1][0]  - 进程 1, 信号 1                             │
│ ...                                                          │
├─────────────────────────────────────────────────────────────┤
│ mpsigact[NR_PROCS-1][_NSIG-1]                                │
└─────────────────────────────────────────────────────────────┘

总大小: NR_PROCS * _NSIG * sizeof(struct sigaction)
      ≈ 1024 * 64 * 32 = 2MB
```

---

### 2.4 进程表结构定义

```c
EXTERN struct mproc {
```

**逐行解释**：

- **第1行**：`EXTERN struct mproc {`
  - `EXTERN`：宏，声明外部变量。
  - `struct mproc`：定义进程结构体。
  - 注意：这个结构体定义了一个数组 `mproc[NR_PROCS]`。

---

### 2.5 退出状态字段

```c
  char mp_exitstatus;		/* storage for status when process exits */
  char mp_sigstatus;		/* storage for signal # for killed procs */
  char mp_eventsub;		/* process event subscriber, or NO_EVENTSUB */
```

**逐行解释**：

- **第1行**：`char mp_exitstatus;`
  - 存储进程退出状态。
  - 注释："storage for status when process exits" — 进程退出时的状态存储。
  - 大小：1 字节。
  - 用途：`exit(status)` 中的 `status & 0xFF`。

- **第2行**：`char mp_sigstatus;`
  - 存储杀死进程的信号编号。
  - 注释："storage for signal # for killed procs" — 被信号杀死的进程的信号编号存储。
  - 大小：1 字节。
  - 用途：`WTERMSIG(status)` 返回此值。

- **第3行**：`char mp_eventsub;`
  - 进程事件订阅者。
  - 注释："process event subscriber, or NO_EVENTSUB" — 进程事件订阅者，或 NO_EVENTSUB。
  - 大小：1 字节。
  - 用途：用于进程事件通知机制。

**内存布局**：

```
退出状态字段:
┌─────────────────────────────────────────────────────────────┐
│ mp_exitstatus (1 字节)                                       │
│ ┌───┬───┬───┬───┬───┬───┬───┬───┐                          │
│ │ 退出状态 (0-255)           │                              │
│ └───┴───┴───┴───┴───┴───┴───┴───┘                          │
├─────────────────────────────────────────────────────────────┤
│ mp_sigstatus (1 字节)                                        │
│ ┌───┬───┬───┬───┬───┬───┬───┬───┐                          │
│ │ 信号编号 (1-64)            │                              │
│ └───┴───┴───┴───┴───┴───┴───┴───┘                          │
├─────────────────────────────────────────────────────────────┤
│ mp_eventsub (1 字节)                                         │
│ ┌───┬───┬───┬───┬───┬───┬───┬───┐                          │
│ │ 订阅者索引或 NO_EVENTSUB   │                              │
│ └───┴───┴───┴───┴───┴───┴───┴───┘                          │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.6 进程标识字段

```c
  pid_t mp_pid;			/* process id */
  endpoint_t mp_endpoint;	/* kernel endpoint id */
  pid_t mp_procgrp;		/* pid of process group (used for signals) */
```

**逐行解释**：

- **第 1 行**：`pid_t mp_pid;`
  - 进程 ID。
  - 注释："process id" — 进程 ID。
  - 大小：通常 4 字节（`int` 类型）。
  - 用途：POSIX 进程标识，`getpid()` 返回此值。

- **第 2 行**：`endpoint_t mp_endpoint;`
  - 内核端点 ID。
  - 注释："kernel endpoint id" — 内核端点 ID。
  - 大小：4 字节。
  - 用途：与内核通信的标识符，用于 IPC。

**endpoint 的结构**：

```
endpoint = (generation << 15) + proc_slot
           ↑                    ↑
       代数 (17 位)          槽号 (15 位)
```

- **generation（代数）**：高 17 位，槽位重用次数。每次槽位重用时加 1。
- **proc_slot（槽号）**：低 15 位，进程表索引。

```c
// 来自 minix/include/minix/endpoint.h
#define _ENDPOINT_GENERATION_SHIFT  15
#define _ENDPOINT(g, p) (((g) << 15) + (p))
#define _ENDPOINT_G(e) (((e)+MAX_NR_TASKS) >> 15)  // 提取代数
#define _ENDPOINT_P(e) ((((e)+MAX_NR_TASKS) & 0x7FFF) - MAX_NR_TASKS)  // 提取槽号
```

**为什么需要代数？**

```
槽位重用问题:
┌─────────────────────────────────────────────────────────────┐
│ 场景：没有代数                                               │
│ 1. 进程 A 使用槽位 5，endpoint = 5                          │
│ 2. 进程 A 退出                                               │
│ 3. 进程 B 创建，重用槽位 5，endpoint = 5                    │
│ 4. 旧消息发送到 endpoint=5，B 错误接收                       │
├─────────────────────────────────────────────────────────────┤
│ 解决方案：加入代数                                           │
│ 1. 进程 A: slot=5, generation=0 → endpoint = (0<<15)+5 = 5  │
│ 2. 进程 A 退出                                               │
│ 3. 进程 B: slot=5, generation=1 → endpoint = (1<<15)+5 = 32773 │
│ 4. 旧消息发送到 endpoint=5，B 不会接收 (B 的 endpoint 是 32773) │
└─────────────────────────────────────────────────────────────┘
```

**pid vs endpoint**：

```
pid vs endpoint:
┌─────────────────────────────────────────────────────────────┐
│ pid (进程 ID)                                                │
│ - POSIX 标准标识符                                           │
│ - 用户可见（ps 命令）                                        │
│ - 进程退出后可重用                                           │
│ - 范围：1 到 PID_MAX (通常 30000)                            │
│ - 示例：1, 2, 100, 5000                                     │
├─────────────────────────────────────────────────────────────┤
│ endpoint (端点 ID)                                           │
│ - Minix 特有标识符                                           │
│ - 内核 IPC 使用                                              │
│ - 包含槽位索引 + 代数                                        │
│ - 范围：负数 (tasks) 和正数 (processes)                      │
│   * Tasks: -NR_TASKS 到 -1 (如 -16 到 -1)                    │
│   * Processes: 0 到 NR_PROCS-1 + generation                  │
│   * 示例：0, 5, 32773 (generation=1, slot=5)                │
├─────────────────────────────────────────────────────────────┤
│ 关系                                                         │
│ - pid 是给用户看的                                           │
│ - endpoint 是给内核用的                                      │
│ - PM 维护 pid 到 endpoint 的映射                             │
│ - fork() 时子进程继承父进程的 generation+1                   │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.7 等待相关字段

```c
  pid_t mp_wpid;		/* pid this process is waiting for */
  vir_bytes mp_waddr;		/* struct rusage address while waiting */
  int mp_parent;		/* index of parent process */
  int mp_tracer;		/* index of tracer process, or NO_TRACER */
```

**逐行解释**：

- **第 1 行**：`pid_t mp_wpid;`
  - 等待的进程 ID。
  - 注释："pid this process is waiting for" — 此进程正在等待的 pid。
  - 大小：4 字节。
  - 用途：`waitpid(pid, ...)` 指定等待的进程。

- **第 2 行**：`pid_t mp_procgrp;`
  - 进程组 ID。
  - 注释："pid of process group (used for signals)" — 进程组的 pid（用于信号）。
  - 大小：4 字节。
  - 用途：信号发送到进程组，作业控制。

---

### 2.7 等待相关字段

```c
  pid_t mp_wpid;		/* pid this process is waiting for */
  vir_bytes mp_waddr;		/* struct rusage address while waiting */
  int mp_parent;		/* index of parent process */
  int mp_tracer;		/* index of tracer process, or NO_TRACER */
```

**逐行解释**：

- **第1行**：`pid_t mp_wpid;`
  - 等待的进程 ID。
  - 注释："pid this process is waiting for" — 此进程正在等待的 pid。
  - 大小：4 字节。
  - 用途：`waitpid(pid, ...)` 指定等待的进程。

- **第2行**：`vir_bytes mp_waddr;`
  - 等待地址。
  - 注释："struct rusage address while waiting" — 等待时的 struct rusage 地址。
  - 大小：4 或 8 字节（取决于架构）。
  - 用途：`wait4()` 需要写入 `rusage` 结构。

- **第3行**：`int mp_parent;`
  - 父进程索引。
  - 注释："index of parent process" — 父进程的索引。
  - 大小：4 字节。
  - 用途：`getppid()` 返回父进程的 pid。

- **第4行**：`int mp_tracer;`
  - 跟踪进程索引。
  - 注释："index of tracer process, or NO_TRACER" — 跟踪进程的索引，或 NO_TRACER。
  - 大小：4 字节。
  - 用途：`ptrace()` 调试支持。

**进程关系图**：

```
进程关系:
┌─────────────────────────────────────────────────────────────┐
│                    父进程 (mp_parent)                        │
│                         ↓                                   │
│                    子进程 1                                  │
│                    ↙    ↘                                   │
│              孙进程 1   孙进程 2                             │
│                                                              │
│ ptrace 关系:                                                 │
│ 跟踪进程 (mp_tracer) ──跟踪──→ 被跟踪进程                    │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.8 子进程时间统计

```c
  /* Child user and system times. Accounting done on child exit. */
  clock_t mp_child_utime;	/* cumulative user time of children */
  clock_t mp_child_stime;	/* cumulative sys time of children */
```

**逐行解释**：

- **第1行**：注释说明子进程时间统计。
  - "Child user and system times" — 子进程用户和系统时间。
  - "Accounting done on child exit" — 子进程退出时进行统计。

- **第2行**：`clock_t mp_child_utime;`
  - 子进程累计用户时间。
  - 注释："cumulative user time of children" — 子进程的累计用户时间。
  - 大小：4 或 8 字节。
  - 用途：`wait4()` 返回的 `rusage.ru_utime`。

- **第3行**：`clock_t mp_child_stime;`
  - 子进程累计系统时间。
  - 注释："cumulative sys time of children" — 子进程的累计系统时间。
  - 大小：4 或 8 字节。
  - 用途：`wait4()` 返回的 `rusage.ru_stime`。

---

### 2.9 用户和组 ID

```c
  /* Real, effective, and saved user and group IDs. */
  uid_t mp_realuid;		/* process' real uid */
  uid_t mp_effuid;		/* process' effective uid */
  uid_t mp_svuid;		/* process' saved uid */
  gid_t mp_realgid;		/* process' real gid */
  gid_t mp_effgid;		/* process' effective gid */
  gid_t mp_svgid;		/* process' saved gid */
```

**逐行解释**：

- **第1行**：注释说明用户和组 ID。
  - "Real, effective, and saved" — 真实、有效和保存的。

- **第2行**：`uid_t mp_realuid;`
  - 真实用户 ID。
  - 注释："process' real uid" — 进程的真实 uid。
  - 大小：4 字节。
  - 用途：`getuid()` 返回此值，标识进程的真实所有者。

- **第3行**：`uid_t mp_effuid;`
  - 有效用户 ID。
  - 注释："process' effective uid" — 进程的有效 uid。
  - 大小：4 字节。
  - 用途：`geteuid()` 返回此值，用于权限检查。

- **第4行**：`uid_t mp_svuid;`
  - 保存的用户 ID。
  - 注释："process' saved uid" — 进程的保存 uid。
  - 大小：4 字节。
  - 用途：`setuid()` 可以在 real 和 saved 之间切换 effective。

- **第5-7行**：组 ID 类似。

**三种 ID 的用途**：

```
用户 ID 三元组:
┌─────────────────────────────────────────────────────────────┐
│ real uid (真实用户 ID)                                       │
│ - 登录时的用户 ID                                            │
│ - fork() 时从父进程继承                                      │
│ - 标识进程的真实所有者                                       │
├─────────────────────────────────────────────────────────────┤
│ effective uid (有效用户 ID)                                  │
│ - 用于权限检查                                               │
│ - 访问文件时检查此 ID                                        │
│ - setuid 程序执行时临时改变                                  │
├─────────────────────────────────────────────────────────────┤
│ saved uid (保存的用户 ID)                                    │
│ - 保存 exec 前的 effective uid                               │
│ - 允许在 real 和 saved 之间切换 effective                    │
│ - 用于实现 setuid 程序的安全切换                             │
└─────────────────────────────────────────────────────────────┘

示例：passwd 程序
1. 用户执行 passwd，real=1000, effective=1000, saved=1000
2. passwd 是 setuid root 程序
3. exec 后：real=1000, effective=0, saved=0
4. passwd 可以访问 /etc/shadow (需要 root)
5. 完成后可以切换回 effective=1000
```

---

### 2.10 补充组

```c
  /* Supplemental groups. */
  int mp_ngroups;		/* number of supplemental groups */
  gid_t mp_sgroups[NGROUPS_MAX];/* process' supplemental groups */
```

**逐行解释**：

- **第1行**：注释说明补充组。
  - "Supplemental groups" — 补充组。

- **第2行**：`int mp_ngroups;`
  - 补充组数量。
  - 注释："number of supplemental groups" — 补充组的数量。
  - 大小：4 字节。
  - 用途：`getgroups()` 返回此值。

- **第3行**：`gid_t mp_sgroups[NGROUPS_MAX];`
  - 补充组数组。
  - 注释："process' supplemental groups" — 进程的补充组。
  - 大小：`NGROUPS_MAX * 4` 字节（通常 64 * 4 = 256 字节）。
  - 用途：`getgroups()` 和 `setgroups()` 操作。

**补充组的作用**：

```
补充组的作用:
┌─────────────────────────────────────────────────────────────┐
│ 权限检查顺序                                                 │
│ 1. 检查 effective uid 是否匹配文件所有者                     │
│ 2. 检查 effective gid 是否匹配文件所属组                     │
│ 3. 检查补充组是否匹配文件所属组                              │
│ 4. 检查其他用户权限                                          │
├─────────────────────────────────────────────────────────────┤
│ 示例                                                         │
│ 用户 alice (uid=1000)                                        │
│ 主组 users (gid=100)                                         │
│ 补充组: docker(999), sudo(27)                                │
│                                                              │
│ alice 可以访问:                                              │
│ - docker 组的文件                                            │
│ - sudo 组的文件                                              │
│ - users 组的文件                                             │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.11 信号处理字段

```c
  /* Signal handling information. */
  sigset_t mp_ignore;		/* 1 means ignore the signal, 0 means don't */
  sigset_t mp_catch;		/* 1 means catch the signal, 0 means don't */
  sigset_t mp_sigmask;		/* signals to be blocked */
  sigset_t mp_sigmask2;		/* saved copy of mp_sigmask */
  sigset_t mp_sigpending;	/* pending signals to be handled */
  sigset_t mp_ksigpending;	/* bitmap for pending signals from the kernel */
  sigset_t mp_sigtrace;		/* signals to hand to tracer first */
  ixfer_sigaction *mp_sigact;	/* as in sigaction(2), pointer into mpsigact */
  vir_bytes mp_sigreturn; 	/* address of C library __sigreturn function */
```

**逐行解释**：

- **第1行**：注释说明信号处理信息。
  - "Signal handling information" — 信号处理信息。

- **第2行**：`sigset_t mp_ignore;`
  - 忽略信号集。
  - 注释："1 means ignore the signal, 0 means don't" — 1 表示忽略信号，0 表示不忽略。
  - 大小：通常 8 字节（64 位）。
  - 用途：`SIG_IGN` 设置的信号。

- **第3行**：`sigset_t mp_catch;`
  - 捕获信号集。
  - 注释："1 means catch the signal, 0 means don't" — 1 表示捕获信号，0 表示不捕获。
  - 大小：8 字节。
  - 用途：有自定义处理函数的信号。

- **第4行**：`sigset_t mp_sigmask;`
  - 信号掩码。
  - 注释："signals to be blocked" — 要阻塞的信号。
  - 大小：8 字节。
  - 用途：`sigprocmask()` 设置的阻塞信号。

- **第5行**：`sigset_t mp_sigmask2;`
  - 保存的信号掩码。
  - 注释："saved copy of mp_sigmask" — mp_sigmask 的保存副本。
  - 大小：8 字节。
  - 用途：信号处理函数返回时恢复。

- **第6行**：`sigset_t mp_sigpending;`
  - 待处理信号。
  - 注释："pending signals to be handled" — 待处理的信号。
  - 大小：8 字节。
  - 用途：`sigpending()` 返回此值。

- **第7行**：`sigset_t mp_ksigpending;`
  - 内核待处理信号。
  - 注释："bitmap for pending signals from the kernel" — 来自内核的待处理信号位图。
  - 大小：8 字节。
  - 用途：内核发送但 PM 尚未处理的信号。

- **第8行**：`sigset_t mp_sigtrace;`
  - 跟踪信号。
  - 注释："signals to hand to tracer first" — 先交给跟踪者的信号。
  - 大小：8 字节。
  - 用途：`ptrace()` 调试时拦截的信号。

- **第9行**：`ixfer_sigaction *mp_sigact;`
  - 信号动作指针。
  - 注释："as in sigaction(2), pointer into mpsigact" — 如 sigaction(2)，指向 mpsigact。
  - 大小：4 或 8 字节（指针）。
  - 用途：指向 `mpsigact[进程索引]`。

- **第10行**：`vir_bytes mp_sigreturn;`
  - sigreturn 地址。
  - 注释："address of C library __sigreturn function" — C 库 __sigreturn 函数的地址。
  - 大小：4 或 8 字节。
  - 用途：信号处理完成后恢复上下文。

**信号处理流程**：

```
信号处理流程:
┌─────────────────────────────────────────────────────────────┐
│ 1. 内核发送信号                                              │
│    → 设置 mp_ksigpending 位                                  │
│                                                              │
│ 2. PM 处理信号                                               │
│    → 检查 mp_ignore (是否忽略)                               │
│    → 检查 mp_catch (是否捕获)                                │
│    → 检查 mp_sigmask (是否阻塞)                              │
│                                                              │
│ 3. 如果捕获                                                   │
│    → 设置 mp_sigmask2 (保存当前掩码)                         │
│    → 设置新的 mp_sigmask                                     │
│    → 构建信号帧，跳转到处理函数                              │
│                                                              │
│ 4. 信号处理完成                                              │
│    → 调用 __sigreturn                                        │
│    → 恢复 mp_sigmask2 到 mp_sigmask                          │
│    → 继续执行                                                │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.12 定时器字段

```c
  minix_timer_t mp_timer;	/* watchdog timer for alarm(2), setitimer(2) */
  clock_t mp_interval[NR_ITIMERS];	/* setitimer(2) repetition intervals */
  clock_t mp_started;		/* when the process was started, for ps(1) */
```

**逐行解释**：

- **第1行**：`minix_timer_t mp_timer;`
  - 定时器。
  - 注释："watchdog timer for alarm(2), setitimer(2)" — alarm(2) 和 setitimer(2) 的看门狗定时器。
  - 大小：取决于 `minix_timer_t` 定义。
  - 用途：`alarm()` 和 `setitimer()` 实现。

- **第2行**：`clock_t mp_interval[NR_ITIMERS];`
  - 定时器间隔。
  - 注释："setitimer(2) repetition intervals" — setitimer(2) 重复间隔。
  - 大小：`NR_ITIMERS * 4` 或 `NR_ITIMERS * 8` 字节。
  - 用途：`ITIMER_REAL`、`ITIMER_VIRTUAL`、`ITIMER_PROF`。

- **第3行**：`clock_t mp_started;`
  - 启动时间。
  - 注释："when the process was started, for ps(1)" — 进程启动时间，用于 ps(1)。
  - 大小：4 或 8 字节。
  - 用途：`ps` 命令显示进程运行时间。

---

### 2.13 标志和消息字段

```c
  unsigned mp_flags;		/* flag bits */
  unsigned mp_trace_flags;	/* trace options */
  message mp_reply;		/* reply message to be sent to one */
```

**逐行解释**：

- **第1行**：`unsigned mp_flags;`
  - 进程标志位。
  - 注释："flag bits" — 标志位。
  - 大小：4 字节。
  - 用途：存储进程状态（见下文标志定义）。

- **第2行**：`unsigned mp_trace_flags;`
  - 跟踪标志位。
  - 注释："trace options" — 跟踪选项。
  - 大小：4 字节。
  - 用途：`ptrace()` 的 `PTRACE_SETOPTIONS`。

- **第3行**：`message mp_reply;`
  - 回复消息。
  - 注释："reply message to be sent to one" — 要发送给进程的回复消息。
  - 大小：约 64 字节。
  - 用途：存储系统调用的返回值。

---

### 2.14 执行帧字段

```c
  /* Process execution frame. Both fields are used by procfs. */
  vir_bytes mp_frame_addr;	/* ptr to proc's initial stack arguments */
  size_t mp_frame_len;		/* size of proc's initial stack arguments */
```

**逐行解释**：

- **第1行**：注释说明执行帧字段。
  - "Process execution frame" — 进程执行帧。
  - "Both fields are used by procfs" — 两个字段都被 procfs 使用。

- **第2行**：`vir_bytes mp_frame_addr;`
  - 栈帧地址。
  - 注释："ptr to proc's initial stack arguments" — 指向进程初始栈参数的指针。
  - 大小：4 或 8 字节。
  - 用途：`exec()` 时保存初始栈位置。

- **第3行**：`size_t mp_frame_len;`
  - 栈帧长度。
  - 注释："size of proc's initial stack arguments" — 进程初始栈参数的大小。
  - 大小：4 或 8 字节。
  - 用途：`/proc/[pid]/cmdline` 读取命令行参数。

---

### 2.15 调度和名称字段

```c
  /* Scheduling priority. */
  signed int mp_nice;		/* nice is PRIO_MIN..PRIO_MAX, standard 0. */

  /* User space scheduling */
  endpoint_t mp_scheduler;	/* scheduler endpoint id */

  char mp_name[PROC_NAME_LEN];	/* process name */

  int mp_magic;			/* sanity check, MP_MAGIC */
} mproc[NR_PROCS];
```

**逐行解释**：

- **第1行**：注释说明调度优先级。
  - "Scheduling priority" — 调度优先级。

- **第2行**：`signed int mp_nice;`
  - nice 值。
  - 注释："nice is PRIO_MIN..PRIO_MAX, standard 0" — nice 值范围 PRIO_MIN 到 PRIO_MAX，标准值为 0。
  - 大小：4 字节。
  - 用途：`nice()` 和 `setpriority()` 设置。

- **第3-4行**：注释说明用户空间调度。
  - "User space scheduling" — 用户空间调度。

- **第5行**：`endpoint_t mp_scheduler;`
  - 调度器端点。
  - 注释："scheduler endpoint id" — 调度器端点 ID。
  - 大小：4 字节。
  - 用途：Minix 支持用户态调度器。

- **第6行**：`char mp_name[PROC_NAME_LEN];`
  - 进程名。
  - 注释："process name" — 进程名。
  - 大小：`PROC_NAME_LEN` 字节（通常 16 字节）。
  - 用途：`ps` 命令显示进程名。

- **第7行**：`int mp_magic;`
  - 魔数。
  - 注释："sanity check, MP_MAGIC" — 完整性检查，MP_MAGIC。
  - 大小：4 字节。
  - 用途：检测内存损坏。

- **第8行**：`} mproc[NR_PROCS];`
  - 定义 `mproc` 数组，大小为 `NR_PROCS`。

---

### 2.16 标志位定义

```c
/* Flag values */
#define IN_USE		0x00001	/* set when 'mproc' slot in use */
#define WAITING		0x00002	/* set by WAIT4 system call */
#define ZOMBIE		0x00004	/* waiting for parent to issue WAIT4 call */
#define PROC_STOPPED	0x00008	/* process is stopped in the kernel */
#define ALARM_ON	0x00010	/* set when SIGALRM timer started */
#define EXITING		0x00020	/* set by EXIT, process is now exiting */
#define TOLD_PARENT	0x00040	/* parent wait() completed, ZOMBIE off */
#define TRACE_STOPPED	0x00080	/* set if process stopped for tracing */
#define SIGSUSPENDED	0x00100	/* set by SIGSUSPEND system call */
#define VFS_CALL       	0x00400	/* set if waiting for VFS (normal calls) */
#define NEW_PARENT	0x00800	/* process's parent changed during VFS call */
#define UNPAUSED	0x01000	/* VFS has replied to unpause request */
#define PRIV_PROC	0x02000	/* system process, special privileges */
#define PARTIAL_EXEC	0x04000	/* process got a new map but no content */
#define TRACE_EXIT	0x08000	/* tracer is forcing this process to exit */
#define TRACE_ZOMBIE	0x10000	/* waiting for tracer to issue WAIT4 call */
#define DELAY_CALL	0x20000	/* waiting for call before sending signal */
#define TAINTED		0x40000 /* process is 'tainted' */
#define EVENT_CALL	0x80000	/* waiting for process event subscriber */
```

**逐行解释**：

| 标志 | 值 | 含义 | 使用场景 |
|------|-----|------|----------|
| `IN_USE` | 0x00001 | 槽位在使用 | 进程已分配 |
| `WAITING` | 0x00002 | 等待子进程 | `wait4()` 调用 |
| `ZOMBIE` | 0x00004 | 僵尸进程 | 进程已退出，等待父进程 wait |
| `PROC_STOPPED` | 0x00008 | 进程停止 | `SIGSTOP` 或调试断点 |
| `ALARM_ON` | 0x00010 | 闹钟开启 | `alarm()` 设置 |
| `EXITING` | 0x00020 | 正在退出 | `exit()` 调用 |
| `TOLD_PARENT` | 0x00040 | 已通知父进程 | 父进程已 wait |
| `TRACE_STOPPED` | 0x00080 | 跟踪停止 | `ptrace()` 调试 |
| `SIGSUSPENDED` | 0x00100 | 信号挂起 | `sigsuspend()` 调用 |
| `VFS_CALL` | 0x00400 | VFS 调用中 | 等待 VFS 响应 |
| `NEW_PARENT` | 0x00800 | 新父进程 | VFS 调用期间父进程改变 |
| `UNPAUSED` | 0x01000 | 取消暂停 | VFS 已响应取消暂停 |
| `PRIV_PROC` | 0x02000 | 特权进程 | 系统进程 |
| `PARTIAL_EXEC` | 0x04000 | 部分执行 | `exec()` 部分完成 |
| `TRACE_EXIT` | 0x08000 | 跟踪退出 | 跟踪者强制退出 |
| `TRACE_ZOMBIE` | 0x10000 | 跟踪僵尸 | 等待跟踪者 wait |
| `DELAY_CALL` | 0x20000 | 延迟调用 | 等待调用完成再发信号 |
| `TAINTED` | 0x40000 | 污染进程 | 进程被标记为"污染" |
| `EVENT_CALL` | 0x80000 | 事件调用 | 等待事件订阅者 |

**进程状态转换图**：

```
进程状态转换:
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   fork() ──→ IN_USE                                         │
│                  │                                          │
│                  ↓                                          │
│             运行中                                           │
│              /   \                                          │
│    SIGSTOP /     \ exit()                                   │
│            ↓       ↓                                        │
│      PROC_STOPPED  EXITING ──→ ZOMBIE                       │
│            │                    │                           │
│    SIGCONT │                    │ wait()                    │
│            ↓                    ↓                           │
│          运行中           TOLD_PARENT ──→ 释放槽位          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

### 2.17 魔数定义

```c
#define MP_MAGIC	0xC0FFEE0
```

**逐行解释**：

- **第1行**：`#define MP_MAGIC 0xC0FFEE0`
  - 定义魔数 `0xC0FFEE0`（十六进制）。
  - 看起来像 "COFFEE"（咖啡）。
  - 用于检测内存损坏。

**魔数检查**：

```
魔数检查流程:
┌─────────────────────────────────────────────────────────────┐
│ 1. 分配进程槽位时                                            │
│    mproc[i].mp_magic = MP_MAGIC;                            │
│                                                              │
│ 2. 访问进程槽位时                                            │
│    if (mproc[i].mp_magic != MP_MAGIC) {                     │
│        panic("mproc corrupted!");                           │
│    }                                                         │
│                                                              │
│ 3. 释放进程槽位时                                            │
│    mproc[i].mp_magic = 0;                                   │
└─────────────────────────────────────────────────────────────┘
```

---

## 三、理论关联

### 3.1 进程控制块（PCB）

在操作系统理论中，进程控制块（PCB）是存储进程信息的数据结构。`struct mproc` 就是 Minix3 的 PCB 实现。

```
PCB 包含的信息:
┌─────────────────────────────────────────────────────────────┐
│ 进程标识                                                     │
│ - pid, endpoint, name                                       │
├─────────────────────────────────────────────────────────────┤
│ 进程状态                                                     │
│ - mp_flags (运行、就绪、阻塞、僵尸等)                        │
├─────────────────────────────────────────────────────────────┤
│ 进程关系                                                     │
│ - parent, tracer, procgrp                                   │
├─────────────────────────────────────────────────────────────┤
│ 权限信息                                                     │
│ - uid, gid, groups                                          │
├─────────────────────────────────────────────────────────────┤
│ 信号处理                                                     │
│ - sigmask, sigpending, sigact                               │
├─────────────────────────────────────────────────────────────┤
│ 资源使用                                                     │
│ - child_utime, child_stime, started                         │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 微内核架构

Minix3 采用微内核架构，进程管理由用户态的 PM 服务实现：

```
微内核 vs 宏内核:
┌─────────────────────────────────────────────────────────────┐
│ 宏内核 (如 Linux)                                            │
│ - 进程表在内核中                                             │
│ - fork/exec/wait 在内核实现                                  │
│ - 系统调用直接操作进程表                                     │
├─────────────────────────────────────────────────────────────┤
│ 微内核 (Minix3)                                              │
│ - 进程表在 PM 服务中                                         │
│ - fork/exec/wait 在用户态实现                                │
│ - 通过 IPC 与内核和 VFS 协作                                 │
└─────────────────────────────────────────────────────────────┘
```

---

## 四、Rust 实现与对比

### 4.1 结构体定义对比

**C 语言版本**：
```c
EXTERN struct mproc {
  char mp_exitstatus;
  pid_t mp_pid;
  endpoint_t mp_endpoint;
  unsigned mp_flags;
  char mp_name[PROC_NAME_LEN];
  int mp_magic;
} mproc[NR_PROCS];
```

**Rust 版本**：
```rust
#![no_std]
#![no_main]

extern crate alloc;
use alloc::string::String;

pub type Pid = i32;
pub type Endpoint = i32;
pub type Uid = u32;
pub type Gid = u32;
pub type Clock = u64;

pub const NR_PROCS: usize = 1024;
pub const PROC_NAME_LEN: usize = 16;
pub const MP_MAGIC: u32 = 0xC0FFEE0;
pub const NGROUPS_MAX: usize = 64;
pub const NR_ITIMERS: usize = 3;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ProcessFlags: u32 {
        const IN_USE       = 0x00001;
        const WAITING      = 0x00002;
        const ZOMBIE       = 0x00004;
        const PROC_STOPPED = 0x00008;
        const ALARM_ON     = 0x00010;
        const EXITING      = 0x00020;
        const TOLD_PARENT  = 0x00040;
        const TRACE_STOPPED = 0x00080;
        const SIGSUSPENDED = 0x00100;
        const VFS_CALL     = 0x00400;
        const NEW_PARENT   = 0x00800;
        const UNPAUSED     = 0x01000;
        const PRIV_PROC    = 0x02000;
        const PARTIAL_EXEC = 0x04000;
        const TRACE_EXIT   = 0x08000;
        const TRACE_ZOMBIE = 0x10000;
        const DELAY_CALL   = 0x20000;
        const TAINTED      = 0x40000;
        const EVENT_CALL   = 0x80000;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SigSet(pub u64);

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SigAction {
    pub sa_handler: usize,
    pub sa_mask: SigSet,
    pub sa_flags: i32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Message {
    pub m_source: i32,
    pub m_type: i32,
    pub data: [u8; 56],
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MinixTimer {
    pub exp_time: Clock,
    pub func: Option<fn(*mut core::ffi::c_void)>,
    pub next: *mut MinixTimer,
}

#[derive(Debug)]
pub struct MProc {
    pub exit_status: i8,
    pub sig_status: i8,
    pub event_sub: i8,
    pub pid: Pid,
    pub endpoint: Endpoint,
    pub proc_grp: Pid,
    pub wpid: Pid,
    pub waddr: usize,
    pub parent: usize,
    pub tracer: usize,
    pub child_utime: Clock,
    pub child_stime: Clock,
    pub real_uid: Uid,
    pub eff_uid: Uid,
    pub sv_uid: Uid,
    pub real_gid: Gid,
    pub eff_gid: Gid,
    pub sv_gid: Gid,
    pub ngroups: i32,
    pub sgroups: [Gid; NGROUPS_MAX],
    pub sig_ignore: SigSet,
    pub sig_catch: SigSet,
    pub sig_mask: SigSet,
    pub sig_mask2: SigSet,
    pub sig_pending: SigSet,
    pub ksig_pending: SigSet,
    pub sig_trace: SigSet,
    pub sig_act: *mut SigAction,
    pub sig_return: usize,
    pub timer: MinixTimer,
    pub interval: [Clock; NR_ITIMERS],
    pub started: Clock,
    pub flags: ProcessFlags,
    pub trace_flags: u32,
    pub reply: Message,
    pub frame_addr: usize,
    pub frame_len: usize,
    pub nice: i32,
    pub scheduler: Endpoint,
    pub name: [u8; PROC_NAME_LEN],
    pub magic: u32,
}

impl MProc {
    pub const fn new() -> Self {
        Self {
            exit_status: 0,
            sig_status: 0,
            event_sub: 0,
            pid: 0,
            endpoint: 0,
            proc_grp: 0,
            wpid: 0,
            waddr: 0,
            parent: 0,
            tracer: 0,
            child_utime: 0,
            child_stime: 0,
            real_uid: 0,
            eff_uid: 0,
            sv_uid: 0,
            real_gid: 0,
            eff_gid: 0,
            sv_gid: 0,
            ngroups: 0,
            sgroups: [0; NGROUPS_MAX],
            sig_ignore: SigSet(0),
            sig_catch: SigSet(0),
            sig_mask: SigSet(0),
            sig_mask2: SigSet(0),
            sig_pending: SigSet(0),
            ksig_pending: SigSet(0),
            sig_trace: SigSet(0),
            sig_act: core::ptr::null_mut(),
            sig_return: 0,
            timer: MinixTimer {
                exp_time: 0,
                func: None,
                next: core::ptr::null_mut(),
            },
            interval: [0; NR_ITIMERS],
            started: 0,
            flags: ProcessFlags::empty(),
            trace_flags: 0,
            reply: Message {
                m_source: 0,
                m_type: 0,
                data: [0; 56],
            },
            frame_addr: 0,
            frame_len: 0,
            nice: 0,
            scheduler: 0,
            name: [0; PROC_NAME_LEN],
            magic: MP_MAGIC,
        }
    }

    pub fn is_in_use(&self) -> bool {
        self.flags.contains(ProcessFlags::IN_USE)
    }

    pub fn is_zombie(&self) -> bool {
        self.flags.contains(ProcessFlags::ZOMBIE)
    }

    pub fn is_stopped(&self) -> bool {
        self.flags.contains(ProcessFlags::PROC_STOPPED)
    }

    pub fn is_waiting(&self) -> bool {
        self.flags.contains(ProcessFlags::WAITING)
    }

    pub fn set_flag(&mut self, flag: ProcessFlags) {
        self.flags |= flag;
    }

    pub fn clear_flag(&mut self, flag: ProcessFlags) {
        self.flags -= flag;
    }

    pub fn check_magic(&self) -> bool {
        self.magic == MP_MAGIC
    }

    pub fn set_name(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let len = core::cmp::min(bytes.len(), PROC_NAME_LEN - 1);
        self.name[..len].copy_from_slice(&bytes[..len]);
        self.name[len] = 0;
    }

    pub fn get_name(&self) -> &str {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(PROC_NAME_LEN);
        core::str::from_utf8(&self.name[..end]).unwrap_or("")
    }
}

pub struct ProcessTable {
    procs: [MProc; NR_PROCS],
}

impl ProcessTable {
    pub const fn new() -> Self {
        Self {
            procs: [MProc::new(); NR_PROCS],
        }
    }

    pub fn get(&self, index: usize) -> Option<&MProc> {
        if index < NR_PROCS && self.procs[index].is_in_use() {
            Some(&self.procs[index])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut MProc> {
        if index < NR_PROCS && self.procs[index].is_in_use() {
            Some(&mut self.procs[index])
        } else {
            None
        }
    }

    pub fn find_by_pid(&self, pid: Pid) -> Option<(usize, &MProc)> {
        self.procs.iter()
            .enumerate()
            .filter(|(_, p)| p.is_in_use())
            .find(|(_, p)| p.pid == pid)
    }

    pub fn find_by_endpoint(&self, endpoint: Endpoint) -> Option<(usize, &MProc)> {
        self.procs.iter()
            .enumerate()
            .filter(|(_, p)| p.is_in_use())
            .find(|(_, p)| p.endpoint == endpoint)
    }

    pub fn alloc_slot(&mut self) -> Option<usize> {
        self.procs.iter_mut()
            .enumerate()
            .find(|(_, p)| !p.is_in_use())
            .map(|(i, p)| {
                p.flags = ProcessFlags::IN_USE;
                p.magic = MP_MAGIC;
                i
            })
    }

    pub fn free_slot(&mut self, index: usize) {
        if index < NR_PROCS {
            self.procs[index] = MProc::new();
        }
    }
}
```

### 4.2 Rust 优势分析

| 方面 | C 语言 | Rust |
|------|--------|------|
| 标志位 | 整数 + 宏 | `bitflags` 宏，类型安全 |
| 空指针 | 可能导致崩溃 | `Option<T>` 强制处理 |
| 数组越界 | 未定义行为 | 运行时检查或编译时检查 |
| 内存安全 | 手动管理 | 所有权系统自动管理 |
| 方法封装 | 直接访问字段 | 方法封装，隐藏实现细节 |

---

## 五、要点总结

### 5.1 核心知识点

1. **进程表结构**：`mproc` 是 PM 的核心数据结构，存储每个进程的管理信息。

2. **三表分离**：内核、PM、VFS 各自维护进程表，通过相同索引关联。

3. **信号处理**：信号动作单独存储，节省空间，支持 MIB 服务。

### 5.2 设计亮点

- **魔数检查**：`MP_MAGIC` 检测内存损坏。
- **标志位设计**：使用位标志节省空间，支持多种状态组合。
- **信号分离**：`mpsigact` 分离存储，减少进程结构大小。

---

## 六、灾难预演

### 6.1 如果 mp_magic 被覆盖

**后果**：无法检测内存损坏。

**现象**：
- 内存损坏可能被忽略。
- 进程表数据可能被破坏。
- 系统可能崩溃或行为异常。

### 6.2 如果 mp_flags 设置错误

**后果**：进程状态错误。

**现象**：
- `ZOMBIE` 标志错误：父进程无法 wait。
- `IN_USE` 标志错误：槽位可能被重复分配。
- `WAITING` 标志错误：父进程可能永久阻塞。

### 6.3 如果 mp_pid 重复

**后果**：进程标识冲突。

**现象**：
- `find_by_pid()` 返回错误的进程。
- 信号可能发送到错误的进程。
- `waitpid()` 可能等待错误的进程。

---

## 七、互动自测

### 问题 1：为什么 pid 和 endpoint 需要分开？

<details>
<summary>点击查看答案</summary>

pid 和 endpoint 分开的原因：

1. **标准不同**：
   - `pid` 是 POSIX 标准，用户可见。
   - `endpoint` 是 Minix 特有，内核使用。

2. **生命周期不同**：
   - `pid` 在进程退出后可重用。
   - `endpoint` 包含槽位索引，便于内核快速定位。

3. **用途不同**：
   - `pid` 用于用户态进程标识（`ps`、`kill` 等）。
   - `endpoint` 用于 IPC 通信标识。

4. **格式不同**：
   - `pid` 是正整数（1 到 PID_MAX）。
   - `endpoint` 是负数（如 -100, -102）。
</details>

### 问题 2：为什么信号动作要单独存储？

<details>
<summary>点击查看答案</summary>

信号动作单独存储的原因：

1. **节省空间**：
   - `struct sigaction` 约占 16-32 字节。
   - 每进程 64 个信号，需要 1024-2048 字节。
   - 分离后 `mproc` 结构体更紧凑。

2. **MIB 服务优化**：
   - MIB（管理信息库）服务不需要信号处理。
   - 分离后 MIB 可以不加载 `mpsigact`，减少内存占用。

3. **缓存友好**：
   - `mproc` 结构体更小，缓存命中率更高。
   - 信号处理代码路径不需要频繁访问信号动作。
</details>

### 问题 3：mp_flags 中的 ZOMBIE 和 TOLD_PARENT 有什么区别？

<details>
<summary>点击查看答案</summary>

`ZOMBIE` 和 `TOLD_PARENT` 的区别：

1. **ZOMBIE 状态**：
   - 进程已退出，等待父进程 `wait()`。
   - 进程槽位仍被占用。
   - 退出状态保存在 `mp_exitstatus`。

2. **TOLD_PARENT 状态**：
   - 父进程已调用 `wait()` 获取退出状态。
   - 进程即将被释放。
   - `ZOMBIE` 标志被清除。

3. **状态转换**：
   ```
   进程退出 → ZOMBIE
   父进程 wait → TOLD_PARENT → 释放槽位
   ```

4. **设计原因**：
   - 分离两个状态，支持 `ptrace` 跟踪。
   - 跟踪者可能需要先处理退出事件。
</details>
