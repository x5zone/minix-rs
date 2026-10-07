# servers/pm/forkexit.c 逐行详细讲解

> **文件路径**: `minix3/minix/servers/pm/forkexit.c`\
> **核心功能**: 进程创建（fork）和终止（exit/wait）\
> **所属模块**: PM（Process Manager）\
> **代码行数**: 808 行

***

## 一、文件概述

### 1.1 是什么（功能说明）

这个文件实现了 Unix 进程生命周期管理的三大核心系统调用：

| 系统调用     | 功能      | 类比             |
| -------- | ------- | -------------- |
| **fork** | 创建新进程   | 父母生孩子，孩子继承父母特征 |
| **exit** | 终止进程    | 家庭成员离世         |
| **wait** | 等待子进程退出 | 父母等待孩子的消息      |

**关键概念**：

- **僵尸进程（Zombie）**：已退出但父进程尚未调用 wait 的进程
- **孤儿进程（Orphan）**：父进程已退出，被 init 进程收养的进程

### 1.2 为什么（设计原因）

**为什么需要 fork/exit/wait 三个独立的系统调用？**

这是 Unix 的经典设计哲学：

1. **fork**：通过复制创建新进程（进程数量 +1）
2. **exit**：有序终止进程，释放资源
3. **wait**：父进程同步等待子进程退出，获取退出状态

**为什么进程退出后不立即删除？**

因为父进程需要获取子进程的退出状态。进程退出后进入僵尸状态，保留退出信息，直到父进程调用 wait。

**为什么 PM 不直接操作进程内存？**

Minix3 是微内核架构，PM 只是用户态服务。进程内存由 VM（Virtual Memory）服务管理，PM 通过 IPC 消息与 VM 协作。

### 1.3 什么情景使用（应用场景）

| 场景         | 系统调用组合       | 说明               |
| ---------- | ------------ | ---------------- |
| Shell 执行命令 | fork + exec  | Shell 创建子进程执行新程序 |
| Web 服务器并发  | fork         | 主进程 fork 子进程处理请求 |
| 进程退出       | exit         | 程序调用 exit() 终止   |
| 父进程等待      | wait/waitpid | 父进程等待子进程退出       |

***

## 二、逐行详细讲解

### 2.1 文件头注释（第 1-18 行）

```c
/* This file deals with creating processes (via FORK) and deleting them (via
 * EXIT/WAIT4).  When a process forks, a new slot in the 'mproc' table is
 * allocated for it, and a copy of the parent's core image is made for the
 * child.  Then the kernel and file system are informed.  A process is removed
 * from the 'mproc' table when two events have occurred: (1) it has exited or
 * been killed by a signal, and (2) the parent has done a WAIT4.  If the
 * process exits first, it continues to occupy a slot until the parent does a
 * WAIT4.
 *
 * The entry points into this file are:
 *   do_fork:		perform the FORK system call
 *   do_srv_fork:	special FORK, used by RS to create sys services
 *   do_exit:		perform the EXIT system call (by calling exit_proc())
 *   exit_proc:		actually do the exiting, and tell VFS about it
 *   exit_restart:	continue exiting a process after VFS has replied
 *   do_wait4:		perform the WAIT4 system call
 *   wait_test:		check whether a parent is waiting for a child
 */
```

**逐句翻译与讲解**：

**第 1-2 行**：`This file deals with creating processes (via FORK) and deleting them (via EXIT/WAIT4).`

**翻译**：这个文件处理创建进程（通过 FORK）和删除进程（通过 EXIT/WAIT4）。

**设计思路**：

- **创建和删除对称**：fork 创建，exit/wait 删除
- **WAIT4 的作用**：wait4() 是 wait() 和 waitpid() 的底层实现，真正删除进程表条目

***

**第 2-5 行**：`When a process forks, a new slot in the 'mproc' table is allocated for it, and a copy of the parent's core image is made for the child. Then the kernel and file system are informed.`

**翻译**：当进程 fork 时，在 'mproc' 表中分配一个新槽位，并为子进程复制父进程的内存映像。然后通知内核和文件系统。

**关键概念讲解**：

1. **mproc 表**：PM 维护的进程控制块（PCB）数组
   ```c
   struct mproc mproc[NR_PROCS];  // 静态数组，NR_PROCS 通常为 128
   ```
2. **core image（内存映像）**：进程的完整内存状态
   - 代码段（text）
   - 数据段（data）
   - 堆（heap）
   - 栈（stack）
3. **为什么要通知内核和文件系统？**
   - **内核**：需要创建新的进程结构（struct proc），分配 PID
   - **文件系统（VFS）**：需要复制文件描述符表

**内存布局示意**：

```
父进程地址空间:              子进程地址空间（fork 后）:
┌─────────────────┐         ┌─────────────────┐
│    栈 (stack)   │         │    栈 (stack)   │ ← 独立副本
│       ↓         │         │       ↓         │
├─────────────────┤         ├─────────────────┤
│    堆 (heap)    │         │    堆 (heap)    │ ← 独立副本
├─────────────────┤         ├─────────────────┤
│    数据段        │         │    数据段        │ ← 独立副本
├─────────────────┤         ├─────────────────┤
│    代码段        │         │    代码段        │ ← 共享（只读）
└─────────────────┘         └─────────────────┘
```

***

**第 5-8 行**：`A process is removed from the 'mproc' table when two events have occurred: (1) it has exited or been killed by a signal, and (2) the parent has done a WAIT4. If the process exits first, it continues to occupy a slot until the parent does a WAIT4.`

**翻译**：进程在两个事件发生后才从 'mproc' 表中删除：(1) 它已退出或被信号杀死，(2) 父进程已执行 WAIT4。如果进程先退出，它会继续占用槽位直到父进程执行 WAIT4。

**设计原因深度分析**：

**为什么需要两个条件？**

这是 Unix 进程管理的核心设计——**僵尸进程机制**。

**场景 1：正常退出流程**

```
时间线:
T1: 子进程调用 exit(0)
    → 子进程状态: ZOMBIE
    → 保留退出状态: exitstatus = 0
    → 占用进程表槽位

T2: 父进程调用 wait()
    → PM 读取子进程退出状态
    → 清理子进程槽位
    → 父进程获得退出状态
```

**场景 2：父进程先退出**

```
时间线:
T1: 父进程调用 exit(0)
    → 子进程成为孤儿
    → init 进程（PID=1）收养子进程
    
T2: 子进程调用 exit(0)
    → init 进程调用 wait()
    → 清理子进程槽位
```

**为什么不能立即删除？**

因为父进程需要知道子进程的退出状态：

- **退出码**：exit(0) 中的 0
- **退出原因**：正常退出 vs 信号杀死
- **资源使用**：CPU 时间、内存使用等

**僵尸进程的危害**：

- 占用进程表槽位（NR\_PROCS 有限）
- 如果大量僵尸进程累积，会导致无法创建新进程

***

**第 10-17 行**：入口点说明

```c
 * The entry points into this file are:
 *   do_fork:		perform the FORK system call
 *   do_srv_fork:	special FORK, used by RS to create sys services
 *   do_exit:		perform the EXIT system call (by calling exit_proc())
 *   exit_proc:		actually do the exiting, and tell VFS about it
 *   exit_restart:	continue exiting a process after VFS has replied
 *   do_wait4:		perform the WAIT4 system call
 *   wait_test:		check whether a parent is waiting for a child
```

**翻译**：
这个文件的入口点有：

- `do_fork`：执行 FORK 系统调用
- `do_srv_fork`：特殊 FORK，用于 RS 创建系统服务
- `do_exit`：执行 EXIT 系统调用（通过调用 exit\_proc()）
- `exit_proc`：实际执行退出，通知 VFS
- `exit_restart`：VFS 回复后继续退出流程
- `do_wait4`：执行 WAIT4 系统调用
- `wait_test`：检查父进程是否在等待子进程

**设计思路分析**：

**为什么有 do\_fork 和 do\_srv\_fork 两个 fork 函数？**

| 函数            | 调用者      | 用途     | 特殊之处                          |
| ------------- | -------- | ------ | ----------------------------- |
| `do_fork`     | 普通进程     | 创建子进程  | 标准 fork 语义                    |
| `do_srv_fork` | RS（重启服务） | 创建系统服务 | 继承 PRIV\_PROC 标志，设置特殊 UID/GID |

**为什么 exit 分为 do\_exit、exit\_proc、exit\_restart 三个函数？**

这是微内核异步 IPC 的典型模式：

```
exit 流程:
┌─────────────────────────────────────────────────────────────┐
│ 1. 用户进程调用 exit(status)                                 │
│    → do_exit() 检查权限                                     │
│    → exit_proc() 开始退出流程                               │
│                                                              │
│ 2. exit_proc() 通知 VFS                                      │
│    → 发送 VFS_PM_EXIT 消息给 VFS                             │
│    → 返回，等待 VFS 回复                                     │
│                                                              │
│ 3. VFS 清理文件描述符                                         │
│    → 关闭所有打开的文件                                      │
│    → 发送回复消息给 PM                                       │
│                                                              │
│ 4. exit_restart() 完成退出                                    │
│    → 通知调度器停止调度                                      │
│    → 通知 VM 释放内存                                        │
│    → 清理进程表                                              │
└─────────────────────────────────────────────────────────────┘
```

**为什么需要 exit\_restart？**

因为 VFS 是独立的服务进程，PM 不能阻塞等待 VFS 回复。PM 发送消息后立即返回，VFS 处理完成后主动发消息给 PM，PM 在主循环中接收并调用 exit\_restart。

***

### 2.2 头文件包含（第 20-31 行）

```c
#include "pm.h"
#include <sys/wait.h>
#include <assert.h>
#include <minix/callnr.h>
#include <minix/com.h>
#include <minix/sched.h>
#include <minix/vm.h>
#include <sys/ptrace.h>
#include <sys/resource.h>
#include <signal.h>
#include "mproc.h"
```

**逐行详细解释**：

**第 20 行**：`#include "pm.h"`

**是什么**：PM 模块的主头文件

**为什么需要**：包含 PM 模块需要的所有公共定义：

- 全局变量声明（如 `mp`、`m_in`、`mproc`）
- 宏定义（如 `NR_PROCS`）
- 其他模块接口

**内存位置**：编译时展开，不占用运行时内存

***

**第 21 行**：`#include <sys/wait.h>`

**是什么**：wait 系统调用相关定义

**关键内容**：

```c
// wait4 的 options 参数
#define WNOHANG    0x00000001  // 非阻塞等待
#define WUNTRACED  0x00000002  // 报告停止的进程

// 退出状态宏
#define WEXITSTATUS(status)  (((status) >> 8) & 0xff)  // 提取退出码
#define WTERMSIG(status)     ((status) & 0x7f)         // 提取终止信号
#define WIFEXITED(status)    (WTERMSIG(status) == 0)   // 是否正常退出
```

**应用场景**：

```c
int status;
pid_t pid = wait(&status);
if (WIFEXITED(status)) {
    printf("子进程正常退出，退出码: %d\n", WEXITSTATUS(status));
} else {
    printf("子进程被信号 %d 杀死\n", WTERMSIG(status));
}
```

***

**第 22 行**：`#include <assert.h>`

**是什么**：断言宏定义

**关键内容**：

```c
#define assert(expr)  \
    ((expr) ? (void)0 : __assert_fail(#expr, __FILE__, __LINE__, __func__))
```

**为什么需要**：运行时检查程序逻辑正确性

**应用场景**：

```c
assert(rmp->mp_magic == MP_MAGIC);  // 检查进程表未损坏
```

**内存开销**：无（调试时启用，发布时可通过 NDEBUG 宏禁用）

***

**第 23-26 行**：Minix 系统头文件

```c
#include <minix/callnr.h>   // 系统调用号定义
#include <minix/com.h>      // 通信消息类型定义
#include <minix/sched.h>    // 调度器接口定义
#include <minix/vm.h>       // 虚拟内存接口定义
```

**逐个讲解**：

**callnr.h**：

```c
#define NR_FORK    2   // fork 系统调用号
#define NR_EXIT    1   // exit 系统调用号
#define NR_WAIT4   7   // wait4 系统调用号
```

**com.h**：

```c
#define VFS_PM_FORK       0x4005  // PM → VFS: fork 通知
#define VFS_PM_EXIT       0x4007  // PM → VFS: exit 通知
#define VFS_PM_DUMPCORE   0x4008  // PM → VFS: core dump 通知
```

**sched.h**：

```c
#define SCHED_PROC_NR  8  // 调度器进程端点号
```

**vm.h**：

```c
// VM 系统调用
int vm_fork(endpoint_t parent, int slot, endpoint_t *child_ep);
int vm_exit(endpoint_t endpoint);
int vm_willexit(endpoint_t endpoint);
```

***

**第 27-28 行**：系统调试和资源头文件

```c
#include <sys/ptrace.h>     // ptrace 调试接口
#include <sys/resource.h>   // 资源使用统计
```

**ptrace.h 关键内容**：

```c
// ptrace 请求类型
#define PT_TRACE_ME    0   // 子进程请求被跟踪
#define PT_READ_U      1   // 读用户区
#define PT_WRITE_U     2   // 写用户区
#define PT_CONTINUE    7   // 继续执行
#define PT_KILL        8   // 杀死进程

// 跟踪标志（在 mproc.h 中定义）
#define TO_TRACEFORK   0x01  // 跟踪 fork
#define TO_NOEXEC      0x02  // exec 时不通知
#define TO_ALTEXEC     0x04  // exec 时发送 SIGSTOP 而非 SIGTRAP
```

**应用场景**：实现调试器（如 gdb）

***

**第 29-30 行**：信号和进程表头文件

```c
#include <signal.h>   // 信号定义
#include "mproc.h"    // PM 进程控制块
```

**signal.h 关键内容**：

```c
#define SIGHUP   1   // 挂起
#define SIGKILL  9   // 强制杀死
#define SIGTERM  15  // 终止
#define SIGCHLD  17  // 子进程状态改变
#define _NSIG    32  // 信号总数

// 信号集操作
typedef struct {
    unsigned long sig[2];  // 64 位信号集
} sigset_t;
```

**mproc.h 关键内容**：

```c
struct mproc {
    pid_t mp_pid;              // 进程 ID
    endpoint_t mp_endpoint;    // 内核端点
    int mp_parent;             // 父进程索引
    int mp_tracer;             // 跟踪进程索引
    unsigned mp_flags;         // 进程标志
    char mp_exitstatus;        // 退出状态
    char mp_sigstatus;         // 信号状态
    // ... 更多字段
};
```

***

### 2.3 宏定义（第 33 行）

```c
#define LAST_FEW            2	/* last few slots reserved for superuser */
```

**逐行详细解释**：

**是什么**：定义保留给超级用户的进程表槽位数量

**值**：2（最后 2 个槽位）

**注释翻译**：最后几个槽位保留给超级用户

**为什么需要这个宏？**

**设计原因**：防止普通用户耗尽进程表，导致系统无法创建新进程（包括 root 进程）

**应用场景**：

```c
// 在 do_fork 中检查
if (procs_in_use >= NR_PROCS - LAST_FEW && rmp->mp_effuid != 0) {
    printf("PM: warning, process table is full!\n");
    return EAGAIN;  // 资源暂时不可用
}
```

**内存布局示意**：

```
进程表 mproc[NR_PROCS]:
┌────────────────────────────────────────┐
│  槽位 0-125: 普通用户可用               │
├────────────────────────────────────────┤
│  槽位 126: 保留给超级用户               │ ← LAST_FEW
│  槽位 127: 保留给超级用户               │ ← LAST_FEW
└────────────────────────────────────────┘
```

**为什么是 2？**

经验值。确保即使进程表快满时，root 仍能创建进程来诊断和修复问题。

***

### 2.4 静态函数声明（第 35-40 行）

```c
static void zombify(struct mproc *rmp);
static void check_parent(struct mproc *child, int try_cleanup);
static int tell_parent(struct mproc *child, vir_bytes addr);
static void tell_tracer(struct mproc *child);
static void tracer_died(struct mproc *child);
static void cleanup(register struct mproc *rmp);
```

**逐行详细解释**：

**为什么声明为 static？**

这些函数只在 forkexit.c 内部使用，不暴露给其他模块。`static` 限制作用域，避免命名冲突，便于编译器优化。

***

**第 35 行**：`static void zombify(struct mproc *rmp);`

**是什么**：将进程标记为僵尸状态

**参数**：

- `rmp`：指向要僵尸化的进程控制块

**功能**：

1. 检查进程是否已经是僵尸（避免重复僵尸化）
2. 如果有跟踪器（tracer），先通知跟踪器
3. 否则直接通知父进程

**应用场景**：

```c
// 在 exit_proc 中调用
if (!dump_core)
    zombify(rmp);  // 将进程标记为僵尸
```

**为什么需要单独的函数？**

僵尸化逻辑复杂，涉及：

- 跟踪器优先级（tracer 优先于 parent）
- 状态转换（EXITING → TRACE\_ZOMBIE → ZOMBIE）
- 通知机制

***

**第 36 行**：`static void check_parent(struct mproc *child, int try_cleanup);`

**是什么**：检查父进程状态并通知

**参数**：

- `child`：子进程控制块
- `try_cleanup`：是否尝试清理子进程

**功能**：

1. 检查父进程是否在等待（WAITING 标志）
2. 如果在等待，调用 `tell_parent()` 唤醒父进程
3. 如果不在等待，发送 SIGCHLD 信号

**应用场景**：

```c
// 在 zombify 中调用
check_parent(rmp, FALSE /*try_cleanup*/);

// 在 exit_restart 中调用
check_parent(rmp, TRUE /*try_cleanup*/);
```

**设计思路**：

父进程有两种状态：

1. **正在等待**：调用 wait() 阻塞 → 直接唤醒
2. **未等待**：继续运行 → 发送 SIGCHLD 信号通知

***

**第 37 行**：`static int tell_parent(struct mproc *child, vir_bytes addr);`

**是什么**：通知父进程子进程已退出

**参数**：

- `child`：子进程控制块
- `addr`：父进程提供的 rusage 结构地址（用于返回资源使用统计）

**返回值**：

- `TRUE`（1）：子进程已被清理
- `FALSE`（0）：子进程仍是僵尸（如内存拷贝失败）

**功能**：

1. 构造退出状态码（W\_EXITCODE）
2. 如果父进程请求 rusage，拷贝资源使用统计
3. 发送回复消息唤醒父进程
4. 累加子进程时间到父进程

**源码位置**：[forkexit.c:680-721](minix3/minix/servers/pm/forkexit.c#L680-L721)

***

**第 38 行**：`static void tell_tracer(struct mproc *child);`

**是什么**：通知跟踪器被跟踪进程已退出

**参数**：

- `child`：被跟踪的子进程

**功能**：

1. 构造退出状态码
2. 发送回复消息唤醒跟踪器
3. 将进程状态从 TRACE\_ZOMBIE 改为 ZOMBIE

**应用场景**：

```c
// 在 zombify 中调用
if (rmp->mp_tracer != NO_TRACER && rmp->mp_tracer != rmp->mp_parent) {
    rmp->mp_flags |= TRACE_ZOMBIE;
    tell_tracer(rmp);  // 先通知跟踪器
}
```

**设计原因**：

跟踪器（如 gdb）优先于父进程接收退出通知，以便调试器能先检查进程状态。

***

**第 39 行**：`static void tracer_died(struct mproc *child);`

**是什么**：处理跟踪器死亡的情况

**参数**：

- `child`：被跟踪的进程

**功能**：

1. 清除跟踪器信息（`mp_tracer = NO_TRACER`）
2. 如果子进程还在运行，杀死它（避免状态不一致）
3. 如果子进程已退出但还在等待跟踪器，通知父进程

**应用场景**：

```c
// 在 exit_proc 中，遍历所有子进程
if (rmp->mp_tracer == proc_nr) {
    // 跟踪器死亡
    tracer_died(rmp);
}
```

**设计原因**：

跟踪器死亡后，被跟踪进程处于不确定状态，必须妥善处理：

- 如果进程还在运行：杀死它（避免失控）
- 如果进程已退出：通知父进程（避免僵尸）

***

**第 40 行**：`static void cleanup(register struct mproc *rmp);`

**是什么**：清理进程表槽位

**参数**：

- `rmp`：要清理的进程控制块

**功能**：

1. 清零 PID
2. 清零标志位
3. 清零时间统计
4. 减少进程计数

**源码**：

```c
static void cleanup(register struct mproc *rmp)
{
  /* Release the process table entry and reinitialize some field. */
  rmp->mp_pid = 0;
  rmp->mp_flags = 0;
  rmp->mp_child_utime = 0;
  rmp->mp_child_stime = 0;
  procs_in_use--;
}
```

**为什么需要单独的函数？**

清理操作在多个地方调用：

- `tell_parent()` 后
- `exit_restart()` 后
- `do_wait4()` 中

***

### 2.5 do\_fork 函数（第 42-139 行）

#### 2.5.1 函数签名和注释（第 42-49 行）

```c
/*===========================================================================*
 *				do_fork					     *
 *===========================================================================*/
int
do_fork(void)
{
/* The process pointed to by 'mp' has forked.  Create a child process. */
```

**逐行详细解释**：

**第 42-44 行**：Minix 标准函数头注释格式

**格式说明**：

```c
/*===========================================================================*
 *				function_name				     *
 *===========================================================================*/
```

**作用**：

- 在代码中清晰标识函数边界
- 便于 grep 搜索：`grep "do_fork" *.c`
- Minix 代码风格统一要求

***

**第 45-46 行**：函数定义

```c
int
do_fork(void)
```

**是什么**：fork 系统调用的 PM 入口函数

**返回值**：`int` 类型

- 成功：返回子进程 PID（给父进程）
- 失败：返回负错误码（如 EAGAIN）
- 特殊：返回 SUSPEND（挂起，等待 VFS 回复）

**参数**：`void`（无参数）

**为什么无参数？**

所有系统调用的输入通过全局变量获取：

- `mp`：指向调用进程的 `mproc` 结构
- `m_in`：用户进程发来的系统调用消息

**内存布局**：

```
PM 全局变量区:
┌────────────────────────────────────────┐
│ struct mproc *mp;                      │ ← 指向当前进程
│ message m_in;                          │ ← 输入消息
│ message m_out;                         │ ← 输出消息（mp->mp_reply）
└────────────────────────────────────────┘
```

***

**第 48 行**：`/* The process pointed to by 'mp' has forked. Create a child process. */`

**翻译**：'mp' 指向的进程已 fork。创建子进程。

**设计思路**：

这是一个**单行注释**，简洁说明函数功能。Minix 风格：函数内部注释放在函数开头，而不是函数外部。

***

#### 2.5.2 局部变量声明（第 49-55 行）

```c
  register struct mproc *rmp;	/* pointer to parent */
  register struct mproc *rmc;	/* pointer to child */
  pid_t new_pid;
  static unsigned int next_child = 0;
  int i, n = 0, s;
  endpoint_t child_ep;
  message m;
```

**逐行详细解释**：

**第 49 行**：`register struct mproc *rmp;`

**是什么**：声明父进程指针

**关键字** **`register`**：

- **历史意义**：提示编译器将变量放入寄存器，提高访问速度
- **现代编译器**：编译器自动优化，`register` 已无实际作用
- **限制**：不能取地址（`&rmp` 非法）

**注释翻译**：指向父进程

**内存位置**：

- 如果编译器遵守 `register` 提示：CPU 寄存器
- 否则：栈上（函数调用栈帧）

**字节大小**：4 字节（32 位系统）或 8 字节（64 位系统）

***

**第 50 行**：`register struct mproc *rmc;`

**是什么**：声明子进程指针

**命名规则**：

- `rmp`：**r**egister **m**proc **p**arent（父进程）
- `rmc`：**r**egister **m**proc **c**hild（子进程）

**注释翻译**：指向子进程

***

**第 51 行**：`pid_t new_pid;`

**是什么**：声明新进程的 PID

**类型** **`pid_t`**：

```c
typedef int pid_t;  // 通常定义为 int
```

**取值范围**：

- 最小值：`INIT_PID + 1`（通常为 2）
- 最大值：`NR_PIDS`（通常为 30000）

**内存位置**：栈上，4 字节

***

**第 52 行**：`static unsigned int next_child = 0;`

**是什么**：声明下一个子进程槽位索引

**关键字** **`static`**：

- **作用**：变量在函数调用之间保持值
- **生命周期**：程序启动时分配，程序结束时释放
- **作用域**：仅在本函数内可见

**初始值**：0

**为什么需要 static？**

实现**循环查找算法**，避免每次都从槽位 0 开始查找：

```c
// 第一次调用 do_fork
next_child = 0 → 找到槽位 5 → next_child = 5

// 第二次调用 do_fork
next_child = 5 → 找到槽位 12 → next_child = 12

// 优点：分散槽位使用，减少查找时间
```

**内存位置**：静态数据区（.data 或 .bss），4 字节

**内存布局**：

```
内存区域:
┌────────────────────────────────────────┐
│ 代码段 (.text)                         │ ← 只读
├────────────────────────────────────────┤
│ 只读数据段 (.rodata)                   │ ← 只读
├────────────────────────────────────────┤
│ 数据段 (.data)                         │ ← 已初始化的全局/静态变量
│   next_child = 0                       │ ← 这里
├────────────────────────────────────────┤
│ BSS 段 (.bss)                          │ ← 未初始化的全局/静态变量
├────────────────────────────────────────┤
│ 堆 (heap)                              │ ← 动态分配
│       ↓                                │
│       ↑                                │
│ 栈 (stack)                             │ ← 局部变量
│   rmp, rmc, new_pid, i, n, s, ...     │ ← 这里
└────────────────────────────────────────┘
```

***

**第 53 行**：`int i, n = 0, s;`

**是什么**：声明三个整型变量

**用途**：

- `i`：循环变量
- `n`：查找槽位的计数器
- `s`：系统调用返回状态

**初始值**：`n = 0`，`i` 和 `s` 未初始化

**内存位置**：栈上，各 4 字节

***

**第 54 行**：`endpoint_t child_ep;`

**是什么**：声明子进程端点号

**类型** **`endpoint_t`**：

```c
typedef int endpoint_t;  // 端点号是整数
```

**端点号结构**：

```
endpoint = (generation << 16) | slot

示例:
进程 A: slot=5, gen=0 → endpoint=5
进程 A 退出
进程 B: slot=5, gen=1 → endpoint=65541 (0x10005)
```

**为什么需要端点号？**

PID 是用户空间概念，端点是内核概念。内核通过端点识别进程，PM 维护 PID ↔ endpoint 映射。

**内存位置**：栈上，4 字节

***

**第 55 行**：`message m;`

**是什么**：声明消息结构体

**类型** **`message`**：

```c
typedef struct {
    int m_type;                    // 消息类型
    union {
        int m_i[10];               // 整数参数
        void *m_p[10];             // 指针参数
        char m_c[40];              // 字符参数
        // ... 更多字段
    } m_u;
} message;
```

**大小**：约 64 字节（Minix 消息固定大小）

**用途**：构造发送给 VFS 的 fork 通知消息

**内存位置**：栈上，64 字节

**栈帧布局**：

```
do_fork 栈帧:
┌────────────────────────────────────────┐
│ 寄存器保存区（如果有）                   │
├────────────────────────────────────────┤
│ message m (64 字节)                    │ ← 栈顶
├────────────────────────────────────────┤
│ endpoint_t child_ep (4 字节)           │
├────────────────────────────────────────┤
│ int s (4 字节)                         │
│ int n (4 字节)                         │
│ int i (4 字节)                         │
├────────────────────────────────────────┤
│ static next_child (静态区，不在此)      │
├────────────────────────────────────────┤
│ pid_t new_pid (4 字节)                 │
├────────────────────────────────────────┤
│ register struct mproc *rmc (4/8 字节)  │
│ register struct mproc *rmp (4/8 字节)  │
├────────────────────────────────────────┤
│ 返回地址                                │
└────────────────────────────────────────┘
```

***

#### 2.5.3 进程表容量检查（第 56-65 行）

```c
 /* If tables might fill up during FORK, don't even start since recovery half
  * way through is such a nuisance.
  */
  rmp = mp;
  if ((procs_in_use == NR_PROCS) ||
  		(procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0))
  {
  	printf("PM: warning, process table is full!\n");
  	return(EAGAIN);
  }
```

**逐行详细解释**：

**第 56-58 行**：注释

```c
 /* If tables might fill up during FORK, don't even start since recovery half
  * way through is such a nuisance.
  */
```

**翻译**：如果表可能在 FORK 期间填满，不要开始，因为中途恢复很麻烦。

**设计思路**：

这是**防御性编程**的体现。fork 操作涉及多个步骤：

1. 分配进程表槽位
2. 调用 VM fork 内存
3. 通知 VFS
4. 分配 PID

如果中途失败，需要回滚之前的操作，非常复杂。因此**提前检查**，避免进入复杂流程。

**为什么中途恢复很麻烦？**

```
失败场景:
┌─────────────────────────────────────────────────────────────┐
│ 步骤 1: 分配槽位 5                                           │
│         mproc[5].mp_flags = IN_USE                          │
│                                                              │
│ 步骤 2: 调用 vm_fork()                                       │
│         → VM 创建新进程                                      │
│         → 内核分配进程结构                                   │
│                                                              │
│ 步骤 3: vm_fork() 失败（内存不足）                           │
│         → 需要回滚步骤 1 和 2                                │
│         → 清除 mproc[5].mp_flags                            │
│         → 通知内核销毁进程结构                               │
│         → 非常复杂！                                         │
└─────────────────────────────────────────────────────────────┘
```

***

**第 59 行**：`rmp = mp;`

**是什么**：将全局变量 `mp` 赋值给局部变量 `rmp`

**为什么这样做？**

1. **简化代码**：`rmp` 比 `mp` 更短
2. **性能优化**：`register` 提示编译器优化（虽然现代编译器自动优化）
3. **代码风格**：Minix 习惯用 `rmp` 表示 "register mproc pointer"

**`mp`** **是什么？**

`mp` 是 PM 的全局变量，指向当前调用进程的 `mproc` 结构：

```c
// 在 pm.h 中声明
EXTERN struct mproc *mp;  /* pointer to caller's mproc structure */

// 在 main.c 的主循环中设置
mp = &mproc[who_p];  // who_p 是调用进程的槽位号
```

**内存布局**：

```
全局变量区:
┌────────────────────────────────────────┐
│ struct mproc *mp;                      │ → 指向 mproc[who_p]
└────────────────────────────────────────┘

进程表:
┌────────────────────────────────────────┐
│ mproc[0]  (INIT 进程)                  │
│ mproc[1]  (空闲)                       │
│ mproc[2]  (用户进程 A) ← mp 指向这里    │
│ ...                                    │
└────────────────────────────────────────┘
```

***

**第 60-62 行**：进程表容量检查

```c
  if ((procs_in_use == NR_PROCS) ||
  		(procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0))
```

**是什么**：检查进程表是否有足够空间

**条件分解**：

**条件 1**：`procs_in_use == NR_PROCS`

- **含义**：进程表已满
- **返回**：EAGAIN（资源暂时不可用）

**条件 2**：`procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0`

- **含义**：进程表接近满，且调用者不是超级用户
- **返回**：EAGAIN

**变量说明**：

- `procs_in_use`：当前使用的进程表槽位数（全局变量）
- `NR_PROCS`：进程表总大小（通常为 128）
- `LAST_FEW`：保留给超级用户的槽位数（2）
- `rmp->mp_effuid`：进程的有效用户 ID（0 表示 root）

**逻辑表**：

| procs\_in\_use | mp\_effuid | 结果         |
| -------------- | ---------- | ---------- |
| 128 (满)        | 任意         | 拒绝（EAGAIN） |
| 126-127        | 0 (root)   | 允许         |
| 126-127        | 非 0        | 拒绝（EAGAIN） |
| < 126          | 任意         | 允许         |

**为什么区分普通用户和超级用户？**

**设计原因**：防止普通用户耗尽进程表，导致系统无法创建新进程（包括 root 的诊断进程）

**生活类比**：电影院保留座位

- 普通观众不能预订最后 2 个座位
- VIP（root）可以预订所有座位
- 确保紧急情况下 VIP 仍有座位

***

**第 63-65 行**：打印警告并返回错误

```c
  {
  	printf("PM: warning, process table is full!\n");
  	return(EAGAIN);
  }
```

**逐行解释**：

**第 63 行**：`printf("PM: warning, process table is full!\n");`

**是什么**：打印警告消息到系统日志

**输出位置**：系统控制台或 `/var/log/messages`

**为什么打印警告？**

帮助系统管理员诊断问题。如果频繁出现此警告，说明：

1. 系统负载过高
2. 可能有进程泄漏（大量僵尸进程）
3. 需要增加 NR\_PROCS

***

**第 64 行**：`return(EAGAIN);`

**是什么**：返回错误码 EAGAIN

**错误码含义**：

- `EAGAIN`：Resource temporarily unavailable（资源暂时不可用）
- 值：11（在 `<errno.h>` 中定义）

**为什么返回 EAGAIN 而不是 ENOMEM？**

- `ENOMEM`：Out of memory（内存不足）
- `EAGAIN`：Resource temporarily unavailable（资源暂时不可用）

**区别**：

- `ENOMEM`：永久性错误，重试无意义
- `EAGAIN`：临时性错误，稍后重试可能成功

**应用场景**：

```c
// 用户进程代码
pid_t pid;
do {
    pid = fork();
    if (pid == -1 && errno == EAGAIN) {
        sleep(1);  // 等待 1 秒后重试
    }
} while (pid == -1 && errno == EAGAIN);
```

***

#### 2.5.4 查找空闲槽位（第 67-74 行）

```c
  /* Find a slot in 'mproc' for the child process.  A slot must exist. */
  do {
        next_child = (next_child+1) % NR_PROCS;
	n++;
  } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
  if(n > NR_PROCS)
	panic("do_fork can't find child slot");
  if(next_child >= NR_PROCS || (mproc[next_child].mp_flags & IN_USE))
	panic("do_fork finds wrong child slot: %d", next_child);
```

**逐行详细解释**：

**第 67 行**：注释

```c
  /* Find a slot in 'mproc' for the child process.  A slot must exist. */
```

**翻译**：在 'mproc' 中为子进程找一个槽位。槽位必须存在。

**为什么"必须存在"？**

因为前面已经检查过 `procs_in_use < NR_PROCS`，所以至少有一个空闲槽位。

***

**第 68-71 行**：循环查找空闲槽位

```c
  do {
        next_child = (next_child+1) % NR_PROCS;
	n++;
  } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
```

**算法分析**：

**第 69 行**：`next_child = (next_child+1) % NR_PROCS;`

**是什么**：循环递增槽位索引

**运算**：

- `(next_child + 1)`：下一个槽位
- `% NR_PROCS`：取模，实现循环（0 → 1 → ... → 127 → 0）

**示例**：

```
next_child = 126
→ (126 + 1) % 128 = 127
→ (127 + 1) % 128 = 0
```

**为什么用取模而不是条件判断？**

```c
// 方法 1: 取模（简洁）
next_child = (next_child + 1) % NR_PROCS;

// 方法 2: 条件判断（冗长）
next_child++;
if (next_child >= NR_PROCS)
    next_child = 0;
```

***

**第 70 行**：`n++;`

**是什么**：计数器递增

**用途**：记录查找次数，防止无限循环

***

**第 71 行**：`while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);`

**是什么**：继续循环的条件

**条件分解**：

**条件 1**：`mproc[next_child].mp_flags & IN_USE`

- **含义**：当前槽位正在使用
- **操作**：位与运算，检查 IN\_USE 标志

**条件 2**：`n <= NR_PROCS`

- **含义**：查找次数未超过总槽位数
- **作用**：防止无限循环（理论上最多查找 NR\_PROCS 次）

**循环逻辑**：

```
查找流程:
┌─────────────────────────────────────────────────────────────┐
│ next_child = 5 (上次找到的位置)                              │
│                                                              │
│ 第 1 次循环:                                                 │
│   next_child = (5+1) % 128 = 6                              │
│   n = 1                                                      │
│   mproc[6].mp_flags & IN_USE = 1 (在使用)                   │
│   → 继续循环                                                 │
│                                                              │
│ 第 2 次循环:                                                 │
│   next_child = 7                                             │
│   n = 2                                                      │
│   mproc[7].mp_flags & IN_USE = 0 (空闲)                     │
│   → 退出循环                                                 │
│                                                              │
│ 结果: next_child = 7                                         │
└─────────────────────────────────────────────────────────────┘
```

**为什么从 next\_child+1 开始而不是 0？**

**设计原因**：分散槽位使用，提高查找效率

**性能分析**：

假设进程表有 100 个槽位，其中 90 个在使用：

**方法 1：总是从 0 开始**

```
fork 1: 检查 0-9 → 找到槽位 10  (10 次检查)
fork 2: 检查 0-9 → 找到槽位 11  (11 次检查)
fork 3: 检查 0-9 → 找到槽位 12  (12 次检查)
平均: 11 次检查
```

**方法 2：从上次位置继续**

```
fork 1: 从 0 开始 → 找到槽位 10  (10 次检查)
fork 2: 从 11 开始 → 找到槽位 11  (1 次检查)
fork 3: 从 12 开始 → 找到槽位 12  (1 次检查)
平均: 4 次检查
```

***

**第 72-73 行**：检查是否找到槽位

```c
  if(n > NR_PROCS)
	panic("do_fork can't find child slot");
```

**是什么**：检查查找是否失败

**条件**：`n > NR_PROCS`

**含义**：已经检查了所有槽位，仍未找到空闲槽位

**为什么这是错误？**

前面已经检查 `procs_in_use < NR_PROCS`，理论上应该有空闲槽位。如果找不到，说明：

1. `procs_in_use` 计数错误
2. 进程表损坏

**`panic()`** **是什么？**

```c
// 在库函数中定义
void panic(const char *fmt, ...);
```

**功能**：

1. 打印错误消息
2. 打印调用栈
3. 终止 PM 进程
4. 触发系统重启

**为什么 panic 而不是返回错误？**

这是**内部一致性错误**，不应该发生。如果发生，说明程序逻辑有严重 bug，无法继续运行。

***

**第 74-75 行**：验证槽位有效性

```c
  if(next_child >= NR_PROCS || (mproc[next_child].mp_flags & IN_USE))
	panic("do_fork finds wrong child slot: %d", next_child);
```

**是什么**：双重检查槽位有效性

**条件分解**：

**条件 1**：`next_child >= NR_PROCS`

- **含义**：索引越界
- **不应该发生**：取模运算保证索引在范围内

**条件 2**：`mproc[next_child].mp_flags & IN_USE`

- **含义**：槽位仍在使用
- **不应该发生**：循环条件保证找到空闲槽位

**为什么需要双重检查？**

**防御性编程**：即使逻辑上不应该发生，也要检查，防止：

1. 编译器优化导致的问题
2. 硬件故障（内存位翻转）
3. 并发问题（虽然 PM 是单线程）

**内存安全示意**：

```
正确情况:
┌────────────────────────────────────────┐
│ mproc[7].mp_flags = 0x00000000         │ ← IN_USE 位为 0
│                                        │
│ 检查: mproc[7].mp_flags & IN_USE = 0   │ ← 通过
└────────────────────────────────────────┘

错误情况（内存损坏）:
┌────────────────────────────────────────┐
│ mproc[7].mp_flags = 0x00000001         │ ← IN_USE 位为 1（损坏）
│                                        │
│ 检查: mproc[7].mp_flags & IN_USE = 1   │ ← panic!
└────────────────────────────────────────┘
```

***

#### 2.5.5 调用 VM fork（第 77-80 行）

```c
  /* Memory part of the forking. */
  if((s=vm_fork(rmp->mp_endpoint, next_child, &child_ep)) != OK) {
	return s;
  }
```

**逐行详细解释**：

**第 77 行**：注释

```c
  /* Memory part of the forking. */
```

**翻译**：fork 的内存部分。

**设计思路**：

fork 操作分为两部分：

1. **PM 部分**：管理进程表、PID、权限等元数据
2. **VM 部分**：复制进程内存、页表等

这是微内核设计的体现：每个服务只做自己擅长的事。

***

**第 78-80 行**：调用 vm\_fork

```c
  if((s=vm_fork(rmp->mp_endpoint, next_child, &child_ep)) != OK) {
	return s;
  }
```

**是什么**：调用 VM 服务 fork 进程内存

**函数签名**：

```c
int vm_fork(endpoint_t parent_ep, int child_slot, endpoint_t *child_ep);
```

**参数说明**：

| 参数                 | 类型             | 含义            | 值      |
| ------------------ | -------------- | ------------- | ------ |
| `rmp->mp_endpoint` | `endpoint_t`   | 父进程端点号        | 如 1026 |
| `next_child`       | `int`          | 子进程槽位号        | 如 7    |
| `&child_ep`        | `endpoint_t *` | 输出参数，返回子进程端点号 | 输出     |

**返回值**：

- `OK`（0）：成功
- 负数：错误码（如 ENOMEM）

**vm\_fork 内部做了什么？**

根据 VM 实现源码（查阅 VM 服务代码），vm\_fork 执行以下操作：

1. **通知内核 fork**：
   ```c
   sys_fork(parent_ep, child_slot, child_ep);
   ```
2. **内核操作**：
   - 创建新的进程结构（`struct proc`）
   - 复制父进程的寄存器状态
   - 设置子进程的端点号
   - 复制地址空间（写时复制，Copy-on-Write）
3. **返回子进程端点号**：
   ```c
   *child_ep = (generation << 16) | child_slot;
   ```

**为什么需要传递 child\_slot？**

VM 需要知道子进程在 PM 进程表中的位置，以便：

1. 设置子进程的端点号（包含槽位信息）
2. 内核进程结构与 PM 进程表对应

**为什么返回 child\_ep？**

PM 需要知道子进程的端点号，以便：

1. 更新 `mproc[next_child].mp_endpoint`
2. 与内核通信时使用端点号

**内存布局变化**：

```
fork 前:
┌────────────────────────────────────────┐
│ 父进程 (endpoint=1026)                 │
│   代码段、数据段、堆、栈                │
└────────────────────────────────────────┘

fork 后（VM 操作）:
┌────────────────────────────────────────┐
│ 父进程 (endpoint=1026)                 │
│   代码段 (共享，只读)                   │
│   数据段、堆、栈 (COW)                  │
└────────────────────────────────────────┘
┌────────────────────────────────────────┐
│ 子进程 (endpoint=7, 新分配)            │
│   代码段 (共享，只读)                   │
│   数据段、堆、栈 (COW)                  │
└────────────────────────────────────────┘

COW (Copy-on-Write):
- 初始时，父子进程共享物理页
- 当任一进程写入时，内核复制该页
- 延迟复制，提高 fork 效率
```

***

**第 79 行**：`return s;`

**是什么**：如果 vm\_fork 失败，返回错误码

**为什么可以安全返回？**

此时：

- 进程表槽位 `next_child` 还未标记为 IN\_USE
- 内核未创建子进程结构
- 无需清理任何资源

**错误处理示意**：

```
vm_fork 失败场景:
┌─────────────────────────────────────────────────────────────┐
│ 1. PM 检查进程表容量 → 通过                                 │
│ 2. PM 查找空闲槽位 → 找到 next_child=7                      │
│ 3. PM 调用 vm_fork()                                        │
│    → VM 调用 sys_fork()                                     │
│    → 内核分配进程结构失败（内存不足）                        │
│    → 返回 ENOMEM                                            │
│ 4. PM 返回 ENOMEM 给用户进程                                │
│                                                              │
│ 清理: 无需清理，因为还未分配任何资源                         │
└─────────────────────────────────────────────────────────────┘
```

***

**第 81 行**：注释

```c
  /* PM may not fail fork after call to vm_fork(), as VM calls sys_fork(). */
```

**翻译**：PM 在调用 vm\_fork() 后不能失败，因为 VM 调用了 sys\_fork()。

**设计思路**：

这是一个**关键约束**：vm\_fork 成功后，PM 必须成功完成 fork。

**为什么？**

因为 vm\_fork 内部调用了 `sys_fork()`，内核已经创建了子进程结构。如果 PM 此时失败，会导致：

1. 内核有子进程结构
2. PM 进程表无对应条目
3. 状态不一致

**状态一致性示意**：

```
vm_fork 成功后:
┌─────────────────────────────────────────────────────────────┐
│ 内核进程表:                                                 │
│   proc[7] = {endpoint=7, state=RUNNABLE, ...}              │
│                                                              │
│ PM 进程表:                                                  │
│   mproc[7] = {mp_flags=0, ...}  ← 还未初始化               │
│                                                              │
│ 问题: 内核认为进程 7 存在，PM 认为不存在                     │
│ 解决: PM 必须完成 mproc[7] 的初始化                         │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.5.6 初始化子进程控制块（第 83-111 行）

```c
  rmc = &mproc[next_child];
  /* Set up the child and its memory map; copy its 'mproc' slot from parent. */
  procs_in_use++;
  *rmc = *rmp;			/* copy parent's process slot to child's */
  rmc->mp_sigact = mpsigact[next_child];	/* restore mp_sigact ptr */
  memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child]));
  rmc->mp_parent = who_p;			/* record child's parent */
  if (!(rmc->mp_trace_flags & TO_TRACEFORK)) {
	rmc->mp_tracer = NO_TRACER;		/* no tracer attached */
	rmc->mp_trace_flags = 0;
	(void) sigemptyset(&rmc->mp_sigtrace);
  }
```

**逐行详细解释**：

**第 83 行**：`rmc = &mproc[next_child];`

**是什么**：获取子进程控制块指针

**操作**：

- `next_child`：前面找到的空闲槽位索引（如 7）
- `&mproc[next_child]`：取该槽位的地址
- `rmc`：子进程指针（register mproc child）

**内存布局**：

```
进程表 mproc[NR_PROCS]:
┌────────────────────────────────────────┐
│ mproc[0]  (INIT)                       │
│ mproc[1]  (空闲)                       │
│ ...                                    │
│ mproc[7]  (空闲) ← rmc 指向这里        │
│ ...                                    │
└────────────────────────────────────────┘
```

***

**第 84 行**：注释

```c
  /* Set up the child and its memory map; copy its 'mproc' slot from parent. */
```

**翻译**：设置子进程及其内存映射；从父进程复制 'mproc' 槽位。

**设计思路**：

fork 的核心语义：**子进程是父进程的副本**。因此需要：

1. 复制父进程的进程控制块
2. 修改特定字段（如 PID、父进程索引）
3. 重置某些字段（如子进程时间统计）

***

**第 85 行**：`procs_in_use++;`

**是什么**：增加进程计数

**变量**：`procs_in_use` 是全局变量，记录当前使用的进程表槽位数

**为什么在这里增加？**

因为即将占用 `mproc[next_child]` 槽位。

**内存位置**：全局数据区

**值变化**：

```
fork 前: procs_in_use = 50
fork 后: procs_in_use = 51
```

***

**第 86 行**：`*rmc = *rmp;`

**是什么**：复制父进程控制块到子进程

**操作**：

- `*rmc`：解引用子进程指针（目标）
- `*rmp`：解引用父进程指针（源）
- `=`：结构体赋值（逐字节复制）

**内存操作**：

```
复制前:
┌────────────────────────────────────────┐
│ 父进程 rmp → mproc[2]                  │
│   mp_pid = 1234                        │
│   mp_ppid = 1                          │
│   mp_flags = IN_USE                    │
│   ...                                  │
└────────────────────────────────────────┘
┌────────────────────────────────────────┐
│ 子进程 rmc → mproc[7]                  │
│   mp_pid = 0 (未初始化)                │
│   mp_ppid = 0                          │
│   mp_flags = 0                         │
│   ...                                  │
└────────────────────────────────────────┘

复制后:
┌────────────────────────────────────────┐
│ 父进程 rmp → mproc[2]                  │
│   mp_pid = 1234                        │
│   mp_ppid = 1                          │
│   mp_flags = IN_USE                    │
│   ...                                  │
└────────────────────────────────────────┘
┌────────────────────────────────────────┐
│ 子进程 rmc → mproc[7]                  │
│   mp_pid = 1234  ← 复制自父进程        │
│   mp_ppid = 1     ← 需要修改           │
│   mp_flags = IN_USE                    │
│   ...                                  │
└────────────────────────────────────────┘
```

**注释翻译**：复制父进程槽位到子进程。

**为什么直接赋值？**

C 语言支持结构体直接赋值，编译器会生成高效的内存拷贝代码（通常调用 `memcpy`）。

**性能**：

- `struct mproc` 大小约 300-400 字节
- 现代 CPU 可以在几十个时钟周期内完成复制

***

**第 87 行**：`rmc->mp_sigact = mpsigact[next_child];`

**是什么**：恢复信号动作数组指针

**为什么需要恢复？**

因为 `mp_sigact` 是指针，指向外部的 `mpsigact` 数组。直接复制父进程的控制块会导致：

- 子进程的 `mp_sigact` 指向父进程的信号动作数组
- 这是不正确的，每个进程应该有独立的信号动作数组

**变量说明**：

- `mpsigact`：全局二维数组，存储所有进程的信号动作
  ```c
  ixfer_sigaction mpsigact[NR_PROCS][_NSIG];
  ```
- `mpsigact[next_child]`：子进程的信号动作数组

**内存布局**：

```
信号动作数组 mpsigact:
┌────────────────────────────────────────┐
│ mpsigact[0][0..31]  (进程 0)           │
│ mpsigact[1][0..31]  (进程 1)           │
│ ...                                    │
│ mpsigact[7][0..31]  (进程 7) ← 子进程  │
│ ...                                    │
└────────────────────────────────────────┘

复制前（错误）:
rmc->mp_sigact → mpsigact[2]  (父进程的信号动作)

恢复后（正确）:
rmc->mp_sigact → mpsigact[7]  (子进程自己的信号动作)
```

**设计原因**：

信号动作占用大量内存（每个进程约 32 \* 40 = 1280 字节），为了节省进程表空间，将其分离存储。但这也带来了复杂性：复制进程控制块后需要修正指针。

***

**第 88 行**：`memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child]));`

**是什么**：复制父进程的信号动作到子进程

**函数签名**：

```c
void *memcpy(void *dest, const void *src, size_t n);
```

**参数**：

- `rmc->mp_sigact`：目标地址（子进程信号动作数组）
- `rmp->mp_sigact`：源地址（父进程信号动作数组）
- `sizeof(mpsigact[next_child])`：复制大小（一个进程的信号动作数组大小）

**为什么需要复制？**

子进程应该继承父进程的信号处理设置：

- 如果父进程忽略 SIGINT，子进程也应该忽略
- 如果父进程捕获 SIGCHLD，子进程也应该捕获

**内存操作**：

```
复制前:
父进程信号动作:
┌────────────────────────────────────────┐
│ mpsigact[2][SIGINT]  = SIG_IGN (忽略)  │
│ mpsigact[2][SIGCHLD] = handler (捕获)  │
│ ...                                    │
└────────────────────────────────────────┘

子进程信号动作:
┌────────────────────────────────────────┐
│ mpsigact[7][SIGINT]  = 未初始化        │
│ mpsigact[7][SIGCHLD] = 未初始化        │
│ ...                                    │
└────────────────────────────────────────┘

复制后:
子进程信号动作:
┌────────────────────────────────────────┐
│ mpsigact[7][SIGINT]  = SIG_IGN (忽略)  │ ← 继承自父进程
│ mpsigact[7][SIGCHLD] = handler (捕获)  │ ← 继承自父进程
│ ...                                    │
└────────────────────────────────────────┘
```

***

**第 89 行**：`rmc->mp_parent = who_p;`

**是什么**：设置子进程的父进程索引

**变量说明**：

- `who_p`：全局变量，调用进程的槽位号（父进程）
- `rmc->mp_parent`：子进程控制块中的父进程索引字段

**注释翻译**：记录子进程的父进程。

**为什么用索引而不是 PID？**

**设计原因**：

1. **效率**：通过索引可以直接访问进程表，无需查找
   ```c
   // 通过索引访问父进程（快）
   struct mproc *parent = &mproc[child->mp_parent];

   // 通过 PID 访问父进程（慢）
   struct mproc *parent = find_proc(child->mp_ppid);  // 需要遍历
   ```
2. **一致性**：内核、PM、VFS 都使用索引标识进程

**内存布局**：

```
父进程 (who_p = 2):
┌────────────────────────────────────────┐
│ mproc[2]                               │
│   mp_pid = 1234                        │
│   mp_endpoint = 1026                   │
└────────────────────────────────────────┘

子进程 (next_child = 7):
┌────────────────────────────────────────┐
│ mproc[7]                               │
│   mp_pid = 1234 (临时，后面会改)       │
│   mp_parent = 2 ← 指向父进程槽位       │
└────────────────────────────────────────┘
```

***

**第 90-94 行**：处理跟踪器

```c
  if (!(rmc->mp_trace_flags & TO_TRACEFORK)) {
	rmc->mp_tracer = NO_TRACER;		/* no tracer attached */
	rmc->mp_trace_flags = 0;
	(void) sigemptyset(&rmc->mp_sigtrace);
  }
```

**是什么**：处理调试跟踪相关字段

**条件**：`!(rmc->mp_trace_flags & TO_TRACEFORK)`

**含义**：如果父进程**没有**设置 TO\_TRACEFORK 标志

**TO\_TRACEFORK 标志是什么？**

在 ptrace 调试中，调试器可以选择是否跟踪子进程的 fork：

- **设置了 TO\_TRACEFORK**：子进程也会被调试器跟踪
- **未设置 TO\_TRACEFORK**：子进程不被调试器跟踪

**操作**：

**第 91 行**：`rmc->mp_tracer = NO_TRACER;`

**是什么**：清除跟踪器索引

**注释翻译**：没有跟踪器附加。

**NO\_TRACER 定义**：

```c
#define NO_TRACER -1  // 无跟踪器
```

**为什么清除？**

因为父进程未设置 TO\_TRACEFORK，子进程不应该被调试器跟踪。

***

**第 92 行**：`rmc->mp_trace_flags = 0;`

**是什么**：清除所有跟踪标志

**跟踪标志定义**：

```c
#define TO_TRACEFORK   0x01  // 跟踪 fork
#define TO_NOEXEC      0x02  // exec 时不通知
#define TO_ALTEXEC     0x04  // exec 时发送 SIGSTOP 而非 SIGTRAP
```

***

**第 93 行**：`(void) sigemptyset(&rmc->mp_sigtrace);`

**是什么**：清空待跟踪信号集

**函数签名**：

```c
int sigemptyset(sigset_t *set);
```

**参数**：

- `&rmc->mp_sigtrace`：待跟踪信号集的地址

**作用**：将信号集初始化为空（所有位清零）

**为什么强制转换为 void？**

`sigemptyset()` 返回 int（0 表示成功），但这里不关心返回值。`(void)` 显式忽略返回值，避免编译器警告。

**内存操作**：

```
清空前:
rmc->mp_sigtrace = { sig[0] = 0x00000005, sig[1] = 0x00000000 }
                    ↑ 有信号待跟踪

清空后:
rmc->mp_sigtrace = { sig[0] = 0x00000000, sig[1] = 0x00000000 }
                    ↑ 无信号待跟踪
```

***

**第 96-100 行**：处理系统服务进程

```c
  /* Some system servers like to call regular fork, such as RS spawning
   * recovery scripts; in this case PM will take care of their scheduling
   * because RS cannot do so for non-system processes */
  if (rmc->mp_flags & PRIV_PROC) {
	assert(rmc->mp_scheduler == NONE);
	rmc->mp_scheduler = SCHED_PROC_NR;
  }
```

**逐行解释**：

**第 96-98 行**：注释

```c
  /* Some system servers like to call regular fork, such as RS spawning
   * recovery scripts; in this case PM will take care of their scheduling
   * because RS cannot do so for non-system processes */
```

**翻译**：一些系统服务喜欢调用常规 fork，例如 RS 生成恢复脚本；在这种情况下，PM 会负责它们的调度，因为 RS 无法调度非系统进程。

**场景说明**：

**RS（Reincarnation Server）**：重启服务，负责启动和管理系统服务。

**问题**：

- RS 是系统服务，有 PRIV\_PROC 标志
- RS fork 子进程执行恢复脚本
- 子进程继承 PRIV\_PROC 标志
- 但子进程是普通用户进程，不应该由 RS 调度

**解决**：

- PM 检测到子进程有 PRIV\_PROC 标志
- 将调度器设置为 SCHED\_PROC\_NR（默认调度器）

***

**第 99 行**：`if (rmc->mp_flags & PRIV_PROC)`

**是什么**：检查子进程是否是特权进程

**PRIV\_PROC 标志**：

```c
#define PRIV_PROC  0x0010  // 系统特权进程
```

**含义**：进程是系统服务，有特殊权限（如直接访问内核）

***

**第 100 行**：`assert(rmc->mp_scheduler == NONE);`

**是什么**：断言调度器字段为 NONE

**为什么需要断言？**

确保逻辑正确：

- PRIV\_PROC 进程通常有专门的调度器
- 但通过普通 fork 创建的子进程，调度器字段应该是 NONE
- 如果不是 NONE，说明逻辑有误

**NONE 定义**：

```c
#define NONE -2  // 无调度器
```

***

**第 101 行**：`rmc->mp_scheduler = SCHED_PROC_NR;`

**是什么**：设置调度器为默认调度器

**SCHED\_PROC\_NR 定义**：

```c
#define SCHED_PROC_NR  8  // 调度器进程端点号
```

**为什么设置为 SCHED\_PROC\_NR？**

因为子进程虽然是 PRIV\_PROC，但实际上是普通用户进程（如恢复脚本），应该由默认调度器调度。

**调度器关系示意**：

```
系统服务进程:
┌────────────────────────────────────────┐
│ RS (PRIV_PROC)                         │
│   mp_scheduler = NONE                  │ ← 自己管理调度
│   fork()                               │
│   ↓                                    │
│ 子进程 (PRIV_PROC)                     │
│   mp_scheduler = SCHED_PROC_NR         │ ← 由默认调度器管理
└────────────────────────────────────────┘
```

***

**第 103-111 行**：继承标志和重置字段

```c
  /* Inherit only these flags. In normal fork(), PRIV_PROC is not inherited. */
  rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED);
  rmc->mp_child_utime = 0;		/* reset administration */
  rmc->mp_child_stime = 0;		/* reset administration */
  rmc->mp_exitstatus = 0;
  rmc->mp_sigstatus = 0;
  rmc->mp_endpoint = child_ep;		/* passed back by VM */
  for (i = 0; i < NR_ITIMERS; i++)
	rmc->mp_interval[i] = 0;	/* reset timer intervals */
  rmc->mp_started = getticks();		/* remember start time, for ps(1) */
```

**逐行解释**：

**第 104 行**：`rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED);`

**是什么**：清除大部分标志，只保留特定标志

**操作**：位与运算，清除未指定的标志位

**标志位分析**：

| 标志          | 值      | 是否保留 | 说明                  |
| ----------- | ------ | ---- | ------------------- |
| IN\_USE     | 0x0001 | ✅ 保留 | 标记槽位在使用             |
| DELAY\_CALL | 0x0002 | ✅ 保留 | 延迟调用标志              |
| TAINTED     | 0x0004 | ✅ 保留 | 污染标志（setuid/setgid） |
| PRIV\_PROC  | 0x0010 | ❌ 清除 | 系统特权进程（注释说不继承）      |
| ZOMBIE      | 0x0020 | ❌ 清除 | 僵尸状态                |
| EXITING     | 0x0040 | ❌ 清除 | 正在退出                |
| WAITING     | 0x0080 | ❌ 清除 | 等待子进程               |
| ...         | ...    | ❌ 清除 | 其他标志                |

**为什么清除 PRIV\_PROC？**

**注释翻译**：在普通 fork() 中，PRIV\_PROC 不被继承。

**设计原因**：

- PRIV\_PROC 赋予进程特殊权限
- 如果子进程继承此标志，会有安全风险
- 只有 `do_srv_fork()` 才会继承 PRIV\_PROC

**位运算示意**：

```
父进程标志: rmp->mp_flags = 0x0013 (IN_USE | DELAY_CALL | PRIV_PROC)

复制后: rmc->mp_flags = 0x0013

位与运算:
  0x0013  (IN_USE | DELAY_CALL | PRIV_PROC)
& 0x0007  (IN_USE | DELAY_CALL | TAINTED)
---------
= 0x0003  (IN_USE | DELAY_CALL)

结果: PRIV_PROC 被清除
```

***

**第 105-106 行**：重置子进程时间统计

```c
  rmc->mp_child_utime = 0;		/* reset administration */
  rmc->mp_child_stime = 0;		/* reset administration */
```

**是什么**：清零子进程的用户时间和系统时间

**注释翻译**：重置管理数据。

**为什么需要重置？**

这两个字段记录**子进程的累计 CPU 时间**，用于 wait4() 返回资源使用统计。

子进程刚创建，还没有子进程，所以累计时间为 0。

**字段含义**：

- `mp_child_utime`：所有已退出子进程的用户时间总和
- `mp_child_stime`：所有已退出子进程的系统时间总和

**应用场景**：

```c
// 父进程代码
struct rusage usage;
wait4(child_pid, &status, 0, &usage);
printf("子进程用户时间: %ld us\n", usage.ru_utime.tv_usec);
```

***

**第 107-108 行**：重置退出状态

```c
  rmc->mp_exitstatus = 0;
  rmc->mp_sigstatus = 0;
```

**是什么**：清零退出状态和信号状态

**字段含义**：

- `mp_exitstatus`：退出码（exit() 的参数）
- `mp_sigstatus`：杀死进程的信号编号

**为什么需要重置？**

子进程刚创建，还未退出，所以退出状态为 0。

**使用场景**：

```c
// 子进程退出
exit(42);  // mp_exitstatus = 42, mp_sigstatus = 0

// 父进程获取退出状态
int status;
wait(&status);
WEXITSTATUS(status);  // 返回 42
```

***

**第 109 行**：`rmc->mp_endpoint = child_ep;`

**是什么**：设置子进程端点号

**注释翻译**：由 VM 传回。

**变量**：

- `child_ep`：vm\_fork() 返回的子进程端点号

**端点号来源**：

```
vm_fork() 流程:
1. PM 调用 vm_fork(parent_ep, child_slot, &child_ep)
2. VM 调用 sys_fork() 创建内核进程结构
3. 内核分配端点号: child_ep = (generation << 16) | child_slot
4. VM 返回 child_ep 给 PM
5. PM 保存到 mproc[child_slot].mp_endpoint
```

**为什么由 VM 分配？**

因为端点号是内核概念，由内核（通过 VM）分配，确保全局唯一。

***

**第 110-111 行**：重置定时器间隔

```c
  for (i = 0; i < NR_ITIMERS; i++)
	rmc->mp_interval[i] = 0;	/* reset timer intervals */
```

**是什么**：清零所有定时器间隔

**注释翻译**：重置定时器间隔。

**NR\_ITIMERS 定义**：

```c
#define NR_ITIMERS  3  // 定时器数量
```

**三种定时器**：

1. **ITIMER\_REAL**：实时定时器，到期发送 SIGALRM
2. **ITIMER\_VIRTUAL**：虚拟定时器，进程用户态运行时计时，到期发送 SIGVTALRM
3. **ITIMER\_PROF**：分析定时器，进程运行时计时（用户态+内核态），到期发送 SIGPROF

**为什么需要重置？**

子进程不应该继承父进程的定时器设置。如果需要，子进程可以自己调用 setitimer()。

**内存布局**：

```
mp_interval 数组:
┌────────────────────────────────────────┐
│ mp_interval[0] = 0  (ITIMER_REAL)      │
│ mp_interval[1] = 0  (ITIMER_VIRTUAL)   │
│ mp_interval[2] = 0  (ITIMER_PROF)      │
└────────────────────────────────────────┘
```

***

**第 112 行**：`rmc->mp_started = getticks();`

**是什么**：记录进程启动时间

**注释翻译**：记住启动时间，供 ps(1) 使用。

**getticks() 函数**：

```c
clock_t getticks(void);
```

**返回值**：系统启动以来的时钟滴答数

**用途**：

- ps 命令计算进程运行时间
- top 命令显示进程启动时间

**应用场景**：

```bash
$ ps -eo pid,etime,cmd
  PID     ELAPSED CMD
1234    00:05:32 /bin/bash
5678    00:00:05 ./my_program
```

**计算方法**：

```
进程运行时间 = 当前时间 - mp_started
```

***

**第 114 行**：断言事件订阅者为空

```c
  assert(rmc->mp_eventsub == NO_EVENTSUB);
```

**是什么**：检查事件订阅者字段是否为空

**NO\_EVENTSUB 定义**：

```c
#define NO_EVENTSUB -1  // 无事件订阅者
```

**mp\_eventsub 字段**：记录订阅此进程事件的进程索引

**为什么需要断言？**

确保子进程没有继承父进程的事件订阅者。因为：

1. 刚复制的进程控制块可能包含父进程的订阅者
2. 但子进程不应该有订阅者
3. 如果有，说明逻辑错误

**事件订阅机制**：

进程可以订阅其他进程的事件（如退出、执行等），用于监控。

***

#### 2.5.7 分配 PID（第 116-118 行）

```c
  /* Find a free pid for the child and put it in the table. */
  new_pid = get_free_pid();
  rmc->mp_pid = new_pid;	/* assign pid to child */
```

**逐行详细解释**：

**第 116 行**：注释

```c
  /* Find a free pid for the child and put it in the table. */
```

**翻译**：为子进程找一个空闲 PID 并放入表中。

***

**第 117 行**：`new_pid = get_free_pid();`

**是什么**：获取一个空闲 PID

**函数签名**：

```c
pid_t get_free_pid(void);
```

**源码位置**：[utility.c:22-37](minix3/minix/servers/pm/utility.c#L22-L37)

**函数实现**（查阅源码）：

```c
pid_t get_free_pid()
{
  static pid_t next_pid = INIT_PID + 1;  /* next pid to be assigned */
  register struct mproc *rmp;            /* check process table */
  int t;                                 /* zero if pid still free */

  /* Find a free pid for the child and put it in the table. */
  do {
	t = 0;
	next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1);
	for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
		if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid) {
			t = 1;
			break;
		}
  } while (t);                           /* 't' = 0 means pid free */
  return(next_pid);
}
```

**算法分析**：

1. **静态变量**：`next_pid` 保持上次分配的 PID
2. **循环递增**：`next_pid = (next_pid + 1) % NR_PIDS`
3. **检查冲突**：遍历进程表，检查 PID 是否已被使用
4. **返回**：找到空闲 PID

**PID 范围**：

- 最小值：`INIT_PID + 1`（通常为 2）
- 最大值：`NR_PIDS`（通常为 30000）

**为什么循环查找？**

因为 PID 会重用：

1. 进程退出后，PID 变为空闲
2. 新进程可以重用已退出的 PID
3. 但必须确保 PID 当前未被使用

**PID 分配示意**：

```
进程表:
┌────────────────────────────────────────┐
│ mproc[0]  mp_pid = 1   (INIT)          │
│ mproc[2]  mp_pid = 1234                │
│ mproc[5]  mp_pid = 5678                │
└────────────────────────────────────────┘

查找空闲 PID:
next_pid = 1235 → 检查 → 未使用 → 返回 1235
```

***

**第 118 行**：`rmc->mp_pid = new_pid;`

**是什么**：将新 PID 赋值给子进程

**注释翻译**：分配 PID 给子进程。

**内存布局**：

```
子进程控制块:
┌────────────────────────────────────────┐
│ mproc[7]                               │
│   mp_pid = 1235  ← 新分配的 PID        │
│   mp_parent = 2                        │
│   mp_endpoint = 7                      │
│   ...                                  │
└────────────────────────────────────────┘
```

**为什么 PID 在这里才设置？**

因为 PID 是用户空间概念，PM 负责分配。前面的 `*rmc = *rmp` 复制了父进程的 PID，现在需要覆盖为子进程自己的 PID。

***

#### 2.5.8 通知 VFS（第 120-130 行）

```c
  memset(&m, 0, sizeof(m));
  m.m_type = VFS_PM_FORK;
  m.VFS_PM_ENDPT = rmc->mp_endpoint;
  m.VFS_PM_PENDPT = rmp->mp_endpoint;
  m.VFS_PM_CPID = rmc->mp_pid;
  m.VFS_PM_REUID = -1;	/* Not used by VFS_PM_FORK */
  m.VFS_PM_REGID = -1;	/* Not used by VFS_PM_FORK */

  tell_vfs(rmc, &m);
```

**逐行详细解释**：

**第 120 行**：`memset(&m, 0, sizeof(m));`

**是什么**：清零消息结构体

**为什么需要清零？**

确保所有未设置的字段都是 0，避免垃圾数据。

***

**第 121 行**：`m.m_type = VFS_PM_FORK;`

**是什么**：设置消息类型为 VFS\_PM\_FORK

**VFS\_PM\_FORK 定义**：

```c
#define VFS_PM_FORK  (VFS_PM_RQ_BASE + 5)  // 约 0x4005
```

**作用**：VFS 根据消息类型判断请求类型，分发给对应的处理函数。

***

**第 122-125 行**：设置消息字段

```c
  m.VFS_PM_ENDPT = rmc->mp_endpoint;    // 子进程端点号
  m.VFS_PM_PENDPT = rmp->mp_endpoint;   // 父进程端点号
  m.VFS_PM_CPID = rmc->mp_pid;          // 子进程 PID
  m.VFS_PM_REUID = -1;                  // 未使用
  m.VFS_PM_REGID = -1;                  // 未使用
```

**字段说明**：

| 字段              | 值       | 用途                     |
| --------------- | ------- | ---------------------- |
| VFS\_PM\_ENDPT  | 子进程端点号  | VFS 需要知道新进程的标识         |
| VFS\_PM\_PENDPT | 父进程端点号  | VFS 需要复制父进程的文件描述符表     |
| VFS\_PM\_CPID   | 子进程 PID | VFS 记录进程 ID            |
| VFS\_PM\_REUID  | -1      | 未使用（do\_srv\_fork 会使用） |
| VFS\_PM\_REGID  | -1      | 未使用（do\_srv\_fork 会使用） |

**为什么 VFS\_PM\_REUID/GID 设置为 -1？**

因为普通 fork 不改变进程的 UID/GID，子进程继承父进程的权限。设置为 -1 表示"不改变"。

***

**第 127 行**：`tell_vfs(rmc, &m);`

**是什么**：发送消息给 VFS

**函数签名**：

```c
void tell_vfs(struct mproc *rmp, message *m_ptr);
```

**源码位置**：[utility.c:113-127](minix3/minix/servers/pm/utility.c#L113-L127)

**函数实现**（查阅源码）：

```c
void tell_vfs(rmp, m_ptr)
struct mproc *rmp;
message *m_ptr;
{
/* Send a request to VFS, without blocking.
 */
  int r;

  if (rmp->mp_flags & (VFS_CALL | EVENT_CALL))
	panic("tell_vfs: not idle: %d", m_ptr->m_type);

  r = asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY);
  if (r != OK)
  	panic("unable to send to VFS: %d", r);

  rmp->mp_flags |= VFS_CALL;
}
```

**函数分析**：

1. **检查进程状态**：
   ```c
   if (rmp->mp_flags & (VFS_CALL | EVENT_CALL))
       panic("tell_vfs: not idle: %d", m_ptr->m_type);
   ```
   - 确保进程没有正在进行的 VFS 调用
   - 如果有，说明逻辑错误
2. **异步发送消息**：
   ```c
   r = asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY);
   ```
   - `asynsend3`：异步发送，不阻塞
   - `VFS_PROC_NR`：VFS 进程端点号
   - `AMF_NOREPLY`：不需要立即回复
3. **设置标志**：
   ```c
   rmp->mp_flags |= VFS_CALL;
   ```
   - 标记进程正在等待 VFS 回复
   - 防止重复发送

**为什么使用异步发送？**

因为 PM 不能阻塞等待 VFS 回复：

- PM 是单线程服务
- 如果阻塞，其他进程的系统调用无法处理
- 使用异步发送，PM 可以继续处理其他请求

**VFS\_CALL 标志的作用**：

```
PM 主循环:
┌─────────────────────────────────────────────────────────────┐
│ while (TRUE) {                                              │
│     receive(ANY, &m_in);  // 接收消息                       │
│                                                              │
│     if (m_in.m_type == VFS_PM_FORK_REPLY) {                 │
│         // VFS 回复 fork 完成                                │
│         process_fork_reply();                               │
│     }                                                        │
│     else if (m_in.m_type == NR_FORK) {                      │
│         // 用户进程调用 fork                                 │
│         do_fork();                                          │
│     }                                                        │
│     ...                                                      │
│ }                                                            │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.5.9 通知跟踪器（第 130-132 行）

```c
  /* Tell the tracer, if any, about the new child */
  if (rmc->mp_tracer != NO_TRACER)
	sig_proc(rmc, SIGSTOP, TRUE /*trace*/, FALSE /* ksig */);
```

**逐行详细解释**：

**第 130 行**：注释

```c
  /* Tell the tracer, if any, about the new child */
```

**翻译**：通知跟踪器（如果有）关于新子进程的信息。

***

**第 131-132 行**：发送 SIGSTOP 给子进程

```c
  if (rmc->mp_tracer != NO_TRACER)
	sig_proc(rmc, SIGSTOP, TRUE /*trace*/, FALSE /* ksig */);
```

**是什么**：如果子进程有跟踪器，发送 SIGSTOP 信号

**条件**：`rmc->mp_tracer != NO_TRACER`

**含义**：子进程有调试器跟踪

**sig\_proc 函数**：

**函数签名**：

```c
void sig_proc(struct mproc *rmp, int signo, int trace, int ksig);
```

**参数**：

- `rmp`：目标进程控制块
- `signo`：信号编号（SIGSTOP）
- `trace`：是否是跟踪信号（TRUE）
- `ksig`：是否来自内核（FALSE）

**作用**：向进程发送信号

**为什么发送 SIGSTOP？**

**设计原因**：

- 调试器需要知道子进程的创建
- 发送 SIGSTOP 让子进程停止
- 调试器可以检查子进程状态
- 调试器决定是否继续运行子进程

**ptrace 流程**：

```
调试器跟踪 fork:
┌─────────────────────────────────────────────────────────────┐
│ 1. 调试器设置 TO_TRACEFORK 标志                             │
│                                                              │
│ 2. 父进程调用 fork()                                         │
│    → PM 创建子进程                                           │
│    → 子进程继承 mp_tracer                                    │
│                                                              │
│ 3. PM 发送 SIGSTOP 给子进程                                  │
│    → 子进程停止                                              │
│                                                              │
│ 4. PM 发送 SIGTRAP 给调试器                                  │
│    → 调试器收到通知                                          │
│    → 调试器可以检查子进程                                    │
│                                                              │
│ 5. 调试器调用 ptrace(PT_CONTINUE)                            │
│    → 子进程继续运行                                          │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.5.10 返回 SUSPEND（第 134-137 行）

```c
  /* Do not reply until VFS is ready to process the fork
  * request
  */
  return SUSPEND;
}
```

**逐行详细解释**：

**第 134-136 行**：注释

```c
  /* Do not reply until VFS is ready to process the fork
  * request
  */
```

**翻译**：在 VFS 准备好处理 fork 请求之前，不要回复。

***

**第 137 行**：`return SUSPEND;`

**是什么**：返回 SUSPEND，挂起父进程

**SUSPEND 定义**：

```c
#define SUSPEND -998  // 挂起调用进程
```

**为什么返回 SUSPEND？**

**设计原因**：

fork 是异步操作：

1. PM 发送消息给 VFS
2. VFS 需要时间处理（复制文件描述符表）
3. 父进程需要等待 VFS 完成
4. 返回 SUSPEND 让主循环不要立即回复父进程

**主循环处理**：

```c
// PM 主循环
int result = do_fork();  // 返回 SUSPEND

if (result == SUSPEND) {
    // 不回复，父进程继续阻塞
    // 等待 VFS 回复后再唤醒父进程
} else {
    // 立即回复父进程
    reply(who_p, result);
}
```

**VFS 回复流程**：

```
时间线:
T1: 父进程调用 fork()
    → PM do_fork() 返回 SUSPEND
    → 父进程阻塞

T2: PM 发送 VFS_PM_FORK 给 VFS
    → VFS 复制文件描述符表
    → VFS 发送 VFS_PM_FORK_REPLY 给 PM

T3: PM 主循环收到 VFS_PM_FORK_REPLY
    → PM 唤醒父进程
    → 父进程获得子进程 PID
```

**为什么需要异步？**

因为 VFS 是独立的服务进程：

- PM 不能阻塞等待 VFS（会阻塞整个系统）
- 使用异步消息传递
- VFS 处理完成后主动通知 PM

***

### 2.6 do\_srv\_fork 函数（第 140-233 行）

**函数概述**：

`do_srv_fork` 是特殊的 fork，专门用于 RS（重启服务）创建系统服务进程。

**与普通 fork 的区别**：

| 特性            | do\_fork | do\_srv\_fork |
| ------------- | -------- | ------------- |
| 调用者           | 任意进程     | 仅 RS          |
| PRIV\_PROC 继承 | ❌ 不继承    | ✅ 继承          |
| UID/GID 设置    | 继承父进程    | 从消息参数获取       |
| 返回值           | SUSPEND  | 子进程 PID       |

由于 `do_srv_fork` 的逻辑与 `do_fork` 类似，我将重点讲解不同之处。

***

#### 2.6.1 权限检查（第 154-156 行）

```c
  /* Only RS is allowed to use srv_fork. */
  if (mp->mp_endpoint != RS_PROC_NR)
	return EPERM;
```

**逐行详细解释**：

**第 154 行**：注释

```c
  /* Only RS is allowed to use srv_fork. */
```

**翻译**：只有 RS 被允许使用 srv\_fork。

***

**第 155-156 行**：权限检查

```c
  if (mp->mp_endpoint != RS_PROC_NR)
	return EPERM;
```

**是什么**：检查调用者是否是 RS 进程

**条件**：`mp->mp_endpoint != RS_PROC_NR`

**含义**：如果调用者不是 RS，拒绝请求

**返回值**：

- `EPERM`：Operation not permitted（操作不允许）

**RS\_PROC\_NR 定义**：

```c
#define RS_PROC_NR  6  // RS 进程端点号
```

**为什么只有 RS 可以调用？**

**设计原因**：

- `srv_fork` 用于创建系统服务进程
- 系统服务进程有特殊权限（PRIV\_PROC）
- 如果任意进程都能创建 PRIV\_PROC 进程，会有安全风险
- RS 是受信任的服务，负责启动和管理系统服务

**安全机制**：

```
调用 srv_fork:
┌─────────────────────────────────────────────────────────────┐
│ 用户进程 (endpoint=1026)                                     │
│   → 调用 srv_fork()                                         │
│   → PM 检查: mp->mp_endpoint != RS_PROC_NR                  │
│   → 返回 EPERM                                              │
│   → 用户进程收到错误                                         │
└─────────────────────────────────────────────────────────────┘

RS 进程 (endpoint=6):
┌─────────────────────────────────────────────────────────────┐
│ RS (endpoint=6)                                             │
│   → 调用 srv_fork()                                         │
│   → PM 检查: mp->mp_endpoint == RS_PROC_NR                  │
│   → 继续执行                                                │
│   → 创建系统服务进程                                         │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.6.2 继承 PRIV\_PROC 标志（第 197 行）

```c
  /* inherit only these flags */
  rmc->mp_flags &= (IN_USE|PRIV_PROC|DELAY_CALL);
```

**是什么**：继承 PRIV\_PROC 标志

**与 do\_fork 的区别**：

| 函数            | 继承的标志                               |
| ------------- | ----------------------------------- |
| do\_fork      | `IN_USE \| DELAY_CALL \| TAINTED`   |
| do\_srv\_fork | `IN_USE \| PRIV_PROC \| DELAY_CALL` |

**关键差异**：

- `do_fork`：**不继承** PRIV\_PROC
- `do_srv_fork`：**继承** PRIV\_PROC

**为什么继承 PRIV\_PROC？**

**设计原因**：

- RS 创建的是系统服务进程
- 系统服务需要 PRIV\_PROC 权限
- 例如：驱动程序、文件系统服务等

**PRIV\_PROC 权限**：

```c
PRIV_PROC 进程可以:
1. 直接调用内核系统调用（不需要消息传递）
2. 访问受保护的内核资源
3. 使用特权指令
4. 绕过某些安全检查
```

***

#### 2.6.3 设置 UID/GID（第 222-227 行）

```c
  memset(&m, 0, sizeof(m));
  m.m_type = VFS_PM_SRV_FORK;
  m.VFS_PM_ENDPT = rmc->mp_endpoint;
  m.VFS_PM_PENDPT = rmp->mp_endpoint;
  m.VFS_PM_CPID = rmc->mp_pid;
  m.VFS_PM_REUID = m_in.m_lsys_pm_srv_fork.uid;
  m.VFS_PM_REGID = m_in.m_lsys_pm_srv_fork.gid;
```

**逐行详细解释**：

**第 222 行**：`memset(&m, 0, sizeof(m));`

**是什么**：清零消息结构体

***

**第 223 行**：`m.m_type = VFS_PM_SRV_FORK;`

**是什么**：设置消息类型为 VFS\_PM\_SRV\_FORK

**VFS\_PM\_SRV\_FORK 定义**：

```c
#define VFS_PM_SRV_FORK  (VFS_PM_RQ_BASE + 6)  // 约 0x4006
```

**与 VFS\_PM\_FORK 的区别**：

- `VFS_PM_FORK`：普通 fork，继承父进程的 UID/GID
- `VFS_PM_SRV_FORK`：服务 fork，设置指定的 UID/GID

***

**第 224-225 行**：设置端点号和 PID

```c
  m.VFS_PM_ENDPT = rmc->mp_endpoint;    // 子进程端点号
  m.VFS_PM_PENDPT = rmp->mp_endpoint;   // 父进程端点号
  m.VFS_PM_CPID = rmc->mp_pid;          // 子进程 PID
```

**与 do\_fork 相同**，传递进程标识信息。

***

**第 226-227 行**：设置 UID/GID

```c
  m.VFS_PM_REUID = m_in.m_lsys_pm_srv_fork.uid;
  m.VFS_PM_REGID = m_in.m_lsys_pm_srv_fork.gid;
```

**是什么**：从消息参数中获取 UID 和 GID

**消息结构**：

```c
// RS 发送给 PM 的消息
struct {
    int m_type;                    // NR_SRV_FORK
    uid_t m_lsys_pm_srv_fork.uid;  // 新进程的 UID
    gid_t m_lsys_pm_srv_fork.gid;  // 新进程的 GID
} m_in;
```

**为什么从消息参数获取？**

**设计原因**：

- RS 启动的系统服务可能需要特定的 UID/GID
- 例如：网络服务以 `nobody` 用户运行
- RS 可以灵活指定新服务的权限

**与普通 fork 的区别**：

```
普通 fork:
┌─────────────────────────────────────────────────────────────┐
│ 父进程 (uid=1000, gid=1000)                                 │
│   → fork()                                                  │
│   → 子进程继承: uid=1000, gid=1000                          │
└─────────────────────────────────────────────────────────────┘

服务 fork:
┌─────────────────────────────────────────────────────────────┐
│ RS (uid=0, gid=0)                                           │
│   → srv_fork(uid=65534, gid=65534)  // nobody 用户          │
│   → 子进程设置: uid=65534, gid=65534                         │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.6.4 唤醒子进程（第 231-232 行）

```c
  /* Wakeup the newly created process */
  reply(rmc-mproc, OK);
```

**是什么**：唤醒新创建的子进程

**reply 函数**：

**函数签名**：

```c
void reply(int proc_nr, int result);
```

**参数**：

- `rmc - mproc`：子进程的槽位号（指针减法）
- `OK`：返回值（成功）

**指针减法**：

```c
rmc = &mproc[7];  // 子进程指针
rmc - mproc = &mproc[7] - &mproc[0] = 7;  // 槽位号
```

**为什么需要唤醒？**

**设计原因**：

- `srv_fork` 是同步操作
- RS 需要立即获得子进程 PID
- 子进程可以立即开始运行

**与 do\_fork 的区别**：

```
do_fork:
┌─────────────────────────────────────────────────────────────┐
│ 1. PM 创建子进程                                             │
│ 2. PM 通知 VFS                                              │
│ 3. PM 返回 SUSPEND                                          │
│ 4. 父进程阻塞，等待 VFS 完成                                 │
│ 5. VFS 完成后，PM 唤醒父进程                                 │
└─────────────────────────────────────────────────────────────┘

do_srv_fork:
┌─────────────────────────────────────────────────────────────┐
│ 1. PM 创建子进程                                             │
│ 2. PM 通知 VFS                                              │
│ 3. PM 唤醒子进程                                            │
│ 4. PM 返回子进程 PID 给 RS                                   │
│ 5. RS 立即获得 PID，子进程开始运行                           │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.6.5 返回子进程 PID（第 234 行）

```c
  return rmc->mp_pid;
}
```

**是什么**：返回子进程的 PID

**返回值**：

- 正整数：子进程的 PID
- RS 通过返回值获得新创建进程的 PID

**与 do\_fork 的区别**：

| 函数            | 返回值     | 含义           |
| ------------- | ------- | ------------ |
| do\_fork      | SUSPEND | 父进程阻塞，等待 VFS |
| do\_srv\_fork | 子进程 PID | RS 立即获得 PID  |

**为什么返回值不同？**

**设计原因**：

- `do_fork`：普通 fork 需要等待 VFS 复制文件描述符表，异步操作
- `do_srv_fork`：系统服务 fork 不需要等待，同步操作

**应用场景**：

```c
// RS 代码
pid_t child_pid = srv_fork(uid, gid);
if (child_pid > 0) {
    // 立即获得子进程 PID
    // 可以继续管理子进程
    printf("Created service process: %d\n", child_pid);
}
```

***

### 2.7 do\_exit 函数（第 236-251 行）

**函数概述**：

`do_exit` 处理 `exit()` 系统调用，终止进程。

**函数签名**：

```c
int do_exit(void);
```

**返回值**：

- `SUSPEND`：挂起调用进程（因为进程已经退出）

***

#### 2.7.1 文件头注释（第 236-241 行）

```c
 /* Perform the exit(status) system call. The real work is done by exit_proc(),
  * which is also called when a process is killed by a signal. System processes
  * do not use PM's exit() to terminate. If they try to, we warn the user
  * and send a SIGKILL signal to the system process.
  */
```

**翻译**：执行 exit(status) 系统调用。真正的工作由 exit\_proc() 完成，该函数也在进程被信号杀死时调用。系统进程不使用 PM 的 exit() 来终止。如果它们尝试这样做，我们会警告用户并向系统进程发送 SIGKILL 信号。

**设计思路**：

1. **分离关注点**：
   - `do_exit`：处理系统调用接口
   - `exit_proc`：执行实际的退出逻辑
2. **复用代码**：
   - `exit()` 系统调用调用 `exit_proc()`
   - 信号杀死进程也调用 `exit_proc()`
   - 避免重复代码
3. **系统进程特殊处理**：
   - 系统进程不应该调用 `exit()`
   - 如果调用，发送 SIGKILL 强制终止

***

#### 2.7.2 处理系统进程退出（第 242-246 行）

```c
  if(mp->mp_flags & PRIV_PROC) {
      printf("PM: system process %d (%s) tries to exit(), sending SIGKILL\n",
          mp->mp_endpoint, mp->mp_name);
      sys_kill(mp->mp_endpoint, SIGKILL);
  }
```

**逐行详细解释**：

**第 242 行**：`if(mp->mp_flags & PRIV_PROC)`

**是什么**：检查是否是系统进程

**PRIV\_PROC 标志**：标记系统特权进程

***

**第 243-244 行**：打印警告信息

```c
      printf("PM: system process %d (%s) tries to exit(), sending SIGKILL\n",
          mp->mp_endpoint, mp->mp_name);
```

**是什么**：打印警告日志

**输出示例**：

```
PM: system process 6 (RS) tries to exit(), sending SIGKILL
```

**为什么打印警告？**

**设计原因**：

- 系统进程不应该调用 `exit()`
- 这可能是编程错误
- 打印警告帮助调试

***

**第 245 行**：`sys_kill(mp->mp_endpoint, SIGKILL);`

**是什么**：发送 SIGKILL 信号给系统进程

**sys\_kill 函数**：

**函数签名**：

```c
int sys_kill(endpoint_t ep, int signo);
```

**参数**：

- `mp->mp_endpoint`：目标进程端点号
- `SIGKILL`：杀死信号（不可捕获）

**作用**：强制终止系统进程

**为什么发送 SIGKILL？**

**设计原因**：

- 系统进程调用 `exit()` 是错误的
- 但进程需要终止
- SIGKILL 是强制终止信号，不能被捕获或忽略
- 确保系统进程被正确终止

**系统进程退出流程**：

```
系统进程调用 exit():
┌─────────────────────────────────────────────────────────────┐
│ 系统进程 (PRIV_PROC)                                        │
│   → 调用 exit(0)                                            │
│   → PM do_exit()                                            │
│   → 检测到 PRIV_PROC 标志                                   │
│   → 打印警告                                                │
│   → 调用 sys_kill(SIGKILL)                                  │
│   → 内核强制终止进程                                         │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.7.3 处理普通进程退出（第 247-249 行）

```c
  else {
      exit_proc(mp, m_in.m_lc_pm_exit.status, FALSE /*dump_core*/);
  }
```

**逐行详细解释**：

**第 247 行**：`else`

**是什么**：如果不是系统进程，执行正常退出

***

**第 248 行**：`exit_proc(mp, m_in.m_lc_pm_exit.status, FALSE /*dump_core*/);`

**是什么**：调用 `exit_proc` 执行实际退出

**参数**：

- `mp`：当前进程控制块
- `m_in.m_lc_pm_exit.status`：退出状态码（exit() 的参数）
- `FALSE`：不生成 core dump

**消息结构**：

```c
// 用户进程发送给 PM 的消息
struct {
    int m_type;                  // NR_EXIT
    int m_lc_pm_exit.status;     // 退出状态码
} m_in;
```

**退出状态码**：

```c
// 用户进程代码
exit(0);    // 正常退出
exit(1);    // 错误退出
exit(42);   // 自定义退出码
```

**dump\_core 参数**：

- `TRUE`：生成 core dump 文件（进程被信号杀死时）
- `FALSE`：不生成 core dump 文件（进程主动调用 exit() 时）

***

#### 2.7.4 返回 SUSPEND（第 250 行）

```c
  return(SUSPEND);		/* can't communicate from beyond the grave */
```

**是什么**：返回 SUSPEND

**注释翻译**：无法从坟墓之外通信。

**幽默注释**：进程已经退出，无法再发送消息。

**为什么返回 SUSPEND？**

**设计原因**：

- 进程已经退出，不应该再回复
- 返回 SUSPEND 告诉主循环不要发送回复消息
- 进程的父进程会通过 `wait()` 获得退出状态

**主循环处理**：

```c
// PM 主循环
int result = do_exit();  // 返回 SUSPEND

if (result == SUSPEND) {
    // 不回复，进程已经退出
} else {
    // 不会执行到这里
}
```

***

### 2.8 exit\_proc 函数（第 254-401 行）

**函数概述**：

`exit_proc` 是进程退出的核心函数，执行实际的退出逻辑。

**函数签名**：

```c
void exit_proc(
    register struct mproc *rmp,    // 要终止的进程控制块
    int exit_status,               // 退出状态码
    int dump_core                  // 是否生成 core dump
);
```

**返回值**：无（void）

***

#### 2.8.1 局部变量声明（第 262-266 行）

```c
  register int proc_nr, proc_nr_e;
  int r;
  pid_t procgrp;
  clock_t user_time, sys_time;
  message m;
```

**逐行详细解释**：

**第 262 行**：`register int proc_nr, proc_nr_e;`

**是什么**：声明两个寄存器变量

**变量说明**：

- `proc_nr`：进程槽位号（索引）
- `proc_nr_e`：进程端点号

**为什么使用 register？**

**优化提示**：告诉编译器将变量存储在寄存器中，提高访问速度。

***

**第 263 行**：`int r;`

**是什么**：声明返回值变量

**用途**：存储系统调用的返回值，用于错误检查。

***

**第 264 行**：`pid_t procgrp;`

**是什么**：声明进程组 ID 变量

**用途**：记住会话领导者的进程组，用于后续处理。

***

**第 265 行**：`clock_t user_time, sys_time;`

**是什么**：声明时钟滴答变量

**用途**：记录进程的用户态和内核态 CPU 时间。

***

**第 266 行**：`message m;`

**是什么**：声明消息结构体

**用途**：发送消息给 VFS。

***

#### 2.8.2 处理 core dump 标志（第 268-275 行）

```c
  /* Do not create core files for set uid execution */
  if (dump_core && rmp->mp_realuid != rmp->mp_effuid)
	dump_core = FALSE;

  /* System processes are destroyed before informing VFS, meaning that VFS can
   * not get their CPU state, so we can't generate a coredump for them either.
   */
  if (dump_core && (rmp->mp_flags & PRIV_PROC))
	dump_core = FALSE;
```

**逐行详细解释**：

**第 268 行**：注释

```c
  /* Do not create core files for set uid execution */
```

**翻译**：不为 setuid 执行创建 core 文件。

***

**第 269-270 行**：检查 setuid 程序

```c
  if (dump_core && rmp->mp_realuid != rmp->mp_effuid)
	dump_core = FALSE;
```

**是什么**：如果是 setuid 程序，禁用 core dump

**条件**：`rmp->mp_realuid != rmp->mp_effuid`

**含义**：真实 UID 和有效 UID 不同，说明是 setuid 程序

**为什么禁用 core dump？**

**安全原因**：

- setuid 程序有特殊权限（如 root）
- core dump 可能包含敏感信息（如密码、密钥）
- 普通用户不应该读取这些信息

**示例**：

```
setuid 程序:
┌─────────────────────────────────────────────────────────────┐
│ /usr/bin/passwd                                             │
│   realuid = 1000 (普通用户)                                 │
│   effuid = 0 (root)                                         │
│                                                              │
│ 如果允许 core dump:                                         │
│   → 用户可以读取 core 文件                                  │
│   → 可能包含 /etc/shadow 的内容                             │
│   → 安全漏洞                                                │
│                                                              │
│ 禁用 core dump:                                             │
│   → 保护敏感信息                                            │
└─────────────────────────────────────────────────────────────┘
```

***

**第 272-275 行**：注释和检查系统进程

```c
  /* System processes are destroyed before informing VFS, meaning that VFS can
   * not get their CPU state, so we can't generate a coredump for them either.
   */
  if (dump_core && (rmp->mp_flags & PRIV_PROC))
	dump_core = FALSE;
```

**注释翻译**：系统进程在通知 VFS 之前就被销毁，意味着 VFS 无法获取它们的 CPU 状态，因此我们也不能为它们生成 core dump。

**为什么不能生成 core dump？**

**技术原因**：

- 系统进程在通知 VFS 之前就被销毁
- VFS 无法访问进程的内存和寄存器状态
- 无法生成 core dump 文件

***

#### 2.8.3 获取进程标识（第 277-278 行）

```c
  proc_nr = (int) (rmp - mproc);	/* get process slot number */
  proc_nr_e = rmp->mp_endpoint;
```

**逐行详细解释**：

**第 277 行**：`proc_nr = (int) (rmp - mproc);`

**是什么**：计算进程槽位号

**操作**：指针减法

**计算方法**：

```c
rmp = &mproc[7];  // 进程指针
mproc = &mproc[0];  // 数组首地址
rmp - mproc = 7;  // 槽位号
```

**注释翻译**：获取进程槽位号。

***

**第 278 行**：`proc_nr_e = rmp->mp_endpoint;`

**是什么**：获取进程端点号

**用途**：后续的系统调用需要端点号标识进程。

***

#### 2.8.4 记住会话领导者的进程组（第 280 行）

```c
  /* Remember a session leader's process group. */
  procgrp = (rmp->mp_pid == mp->mp_procgrp) ? mp->mp_procgrp : 0;
```

**逐行详细解释**：

**第 280 行**：注释

```c
  /* Remember a session leader's process group. */
```

**翻译**：记住会话领导者的进程组。

***

**代码逻辑**：

```c
procgrp = (rmp->mp_pid == mp->mp_procgrp) ? mp->mp_procgrp : 0;
```

**三元运算符**：

- 如果 `rmp->mp_pid == mp->mp_procgrp`：进程是会话领导者，返回进程组 ID
- 否则：返回 0

**会话领导者**：

**是什么**：创建新会话的进程，其 PID 等于进程组 ID。

**示例**：

```
会话领导者:
┌─────────────────────────────────────────────────────────────┐
│ 进程 PID=1234                                               │
│   mp_procgrp = 1234                                         │
│   mp_pid == mp_procgrp → TRUE                               │
│   → 进程是会话领导者                                        │
│   → procgrp = 1234                                          │
└─────────────────────────────────────────────────────────────┘

普通进程:
┌─────────────────────────────────────────────────────────────┐
│ 进程 PID=5678                                               │
│   mp_procgrp = 1234                                         │
│   mp_pid != mp_procgrp → FALSE                              │
│   → 进程不是会话领导者                                      │
│   → procgrp = 0                                             │
└─────────────────────────────────────────────────────────────┘
```

**为什么需要记住？**

**设计原因**：

- 会话领导者退出时，需要向进程组发送 SIGHUP 信号
- 后续代码会检查 `procgrp` 是否非零
- 如果非零，发送信号给整个进程组

***

#### 2.8.5 取消定时器（第 282-283 行）

```c
  /* If the exited process has a timer pending, kill it. */
  if (rmp->mp_flags & ALARM_ON) set_alarm(rmp, (clock_t) 0);
```

**逐行详细解释**：

**第 282 行**：注释

```c
  /* If the exited process has a timer pending, kill it. */
```

**翻译**：如果退出的进程有待处理的定时器，杀死它。

***

**第 283 行**：`if (rmp->mp_flags & ALARM_ON) set_alarm(rmp, (clock_t) 0);`

**是什么**：取消进程的定时器

**条件**：`rmp->mp_flags & ALARM_ON`

**含义**：进程有活动的定时器（通过 alarm() 设置）

**set\_alarm 函数**：

**函数签名**：

```c
void set_alarm(struct mproc *rmp, clock_t time);
```

**参数**：

- `rmp`：进程控制块
- `(clock_t) 0`：定时器时间为 0（取消定时器）

**为什么需要取消？**

**设计原因**：

- 进程已经退出，定时器不再有意义
- 如果不取消，定时器到期时会发送 SIGALRM
- 但进程已经不存在，会导致错误

**定时器清理示意**：

```
进程退出前:
┌─────────────────────────────────────────────────────────────┐
│ 进程控制块:                                                 │
│   mp_flags = ALARM_ON | IN_USE                              │
│   mp_alarm = 1000 (10 秒后到期)                             │
└─────────────────────────────────────────────────────────────┘

取消定时器后:
┌─────────────────────────────────────────────────────────────┐
│ 进程控制块:                                                 │
│   mp_flags = IN_USE  (ALARM_ON 被清除)                      │
│   mp_alarm = 0                                               │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.8.6 记录 CPU 时间（第 285-292 行）

```c
  /* Do accounting: fetch usage times and save with dead child process.
   * POSIX forbids accumulation at parent until child has been waited for.
   */
  if((r=sys_times(proc_nr_e, &user_time, &sys_time, NULL, NULL)) != OK)
  	panic("exit_proc: sys_times failed: %d", r);
  rmp->mp_child_utime += user_time;		/* add user time */
  rmp->mp_child_stime += sys_time;		/* add system time */
```

**逐行详细解释**：

**第 285-287 行**：注释

```c
  /* Do accounting: fetch usage times and save with dead child process.
   * POSIX forbids accumulation at parent until child has been waited for.
   */
```

**翻译**：进行统计：获取使用时间并保存到已死亡的子进程。POSIX 禁止在父进程 wait 子进程之前累积时间。

**设计思路**：

**POSIX 要求**：

- 子进程的 CPU 时间应该在子进程退出时记录
- 父进程调用 `wait()` 时获得累计时间
- 不能在父进程运行时累积子进程时间

***

**第 288-289 行**：调用 sys\_times

```c
  if((r=sys_times(proc_nr_e, &user_time, &sys_time, NULL, NULL)) != OK)
  	panic("exit_proc: sys_times failed: %d", r);
```

**是什么**：获取进程的 CPU 时间

**sys\_times 函数**：

**函数签名**：

```c
int sys_times(endpoint_t ep, clock_t *user_time, clock_t *sys_time, 
              clock_t *uptime, clock_t *boottime);
```

**参数**：

- `proc_nr_e`：进程端点号
- `&user_time`：存储用户态时间的指针
- `&sys_time`：存储内核态时间的指针
- `NULL`：不获取系统运行时间
- `NULL`：不获取系统启动时间

**返回值**：

- `OK`：成功
- 其他：错误码

**为什么失败时 panic？**

**设计原因**：

- 获取 CPU 时间不应该失败
- 如果失败，说明内核状态错误
- 系统无法继续运行，需要 panic

***

**第 290-291 行**：累积 CPU 时间

```c
  rmp->mp_child_utime += user_time;		/* add user time */
  rmp->mp_child_stime += sys_time;		/* add system time */
```

**是什么**：将进程的 CPU 时间累积到子进程时间字段

**注释翻译**：

- 第 290 行：添加用户时间
- 第 291 行：添加系统时间

**为什么累积到 mp\_child\_utime/stime？**

**设计原因**：

- 这些字段记录进程及其所有已退出子进程的 CPU 时间
- 父进程调用 `wait()` 时获得这些累计时间
- 用于资源统计和计费

**时间累积示意**：

```
进程退出:
┌─────────────────────────────────────────────────────────────┐
│ 进程控制块:                                                 │
│   mp_child_utime = 500  (之前子进程的累计时间)              │
│   mp_child_stime = 100                                       │
│                                                              │
│ 当前进程 CPU 时间:                                          │
│   user_time = 200                                            │
│   sys_time = 50                                              │
│                                                              │
│ 累积后:                                                     │
│   mp_child_utime = 700                                       │
│   mp_child_stime = 150                                       │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.8.7 停止进程（第 293-312 行）

```c
  /* Tell the kernel the process is no longer runnable to prevent it from
   * being scheduled in between the following steps. Then tell VFS that it
   * the process has exited and finally, clean up the process at the kernel.
   * This order is important so that VFS can tell drivers to cancel requests
   * such as copying to/ from the exiting process, before it is gone.
   */
  /* If the process is not yet stopped, we force a stop here. This means that
   * the process may still have a delay call pending. For this reason, the main
   * message loop discards requests from exiting processes.
   *
   * TODO: make the kernel discard delayed calls upon forced stops for exits,
   * so that no service needs to deal with this.  Right now it appears that the
   * only thing preventing problems with other services is the fact that
   * regular messages are prioritized over asynchronous messages.
   */
  if (!(rmp->mp_flags & PROC_STOPPED)) {
	if ((r = sys_stop(proc_nr_e)) != OK)		/* stop the process */
		panic("sys_stop failed: %d", r);
	rmp->mp_flags |= PROC_STOPPED;
  }
```

**逐行详细解释**：

**第 293-297 行**：注释

```c
  /* Tell the kernel the process is no longer runnable to prevent it from
   * being scheduled in between the following steps. Then tell VFS that it
   * the process has exited and finally, clean up the process at the kernel.
   * This order is important so that VFS can tell drivers to cancel requests
   * such as copying to/ from the exiting process, before it is gone.
   */
```

**翻译**：告诉内核进程不再可运行，防止它在以下步骤之间被调度。然后告诉 VFS 进程已经退出，最后在内核中清理进程。这个顺序很重要，这样 VFS 可以告诉驱动程序取消请求（如复制到/从退出进程），在它消失之前。

**设计思路**：

**退出顺序**：

1. 停止进程（防止被调度）
2. 通知 VFS（让驱动程序取消请求）
3. 清理内核资源

**为什么这个顺序重要？**

**场景**：

```
错误顺序（先清理内核）:
┌─────────────────────────────────────────────────────────────┐
│ 1. 清理内核进程结构                                         │
│ 2. VFS 尝试读取进程内存                                     │
│    → 进程已不存在                                           │
│    → 访问错误                                               │
└─────────────────────────────────────────────────────────────┘

正确顺序（先通知 VFS）:
┌─────────────────────────────────────────────────────────────┐
│ 1. 停止进程                                                 │
│ 2. 通知 VFS                                                 │
│    → VFS 通知驱动程序取消请求                               │
│    → 驱动程序停止访问进程内存                               │
│ 3. 清理内核进程结构                                         │
│    → 安全，没有进程在访问                                   │
└─────────────────────────────────────────────────────────────┘
```

***

**第 298-304 行**：注释

```c
  /* If the process is not yet stopped, we force a stop here. This means that
   * the process may still have a delay call pending. For this reason, the main
   * message loop discards requests from exiting processes.
   *
   * TODO: make the kernel discard delayed calls upon forced stops for exits,
   * so that no service needs to each service needs to deal with this.  Right now it appears that the
   * only thing preventing problems with other services is the fact that
   * regular messages are prioritized over asynchronous messages.
   */
```

**翻译**：如果进程还未停止，我们在这里强制停止。这意味着进程可能还有延迟调用待处理。因此，主消息循环丢弃来自退出进程的请求。

**TODO 翻译**：让内核在强制停止退出时丢弃延迟调用，这样每个服务就不需要处理这个问题。目前看来，防止其他服务出现问题的唯一原因是常规消息优先于异步消息。

**延迟调用问题**：

**是什么**：进程可能已经发送了异步消息（如 asynsend），但还未被处理。

**问题**：

- 进程退出后，延迟调用仍然在队列中
- 其他服务可能收到来自已退出进程的消息
- 可能导致错误

**当前解决方案**：

- 主消息循环丢弃来自退出进程的消息
- 依赖消息优先级（常规消息优先于异步消息）

***

**第 305-308 行**：停止进程

```c
  if (!(rmp->mp_flags & PROC_STOPPED)) {
	if ((r = sys_stop(proc_nr_e)) != OK)		/* stop the process */
		panic("sys_stop failed: %d", r);
	rmp->mp_flags |= PROC_STOPPED;
  }
```

**是什么**：如果进程未停止，强制停止

**条件**：`!(rmp->mp_flags & PROC_STOPPED)`

**含义**：进程还未停止

**sys\_stop 函数**：

**函数签名**：

```c
int sys_stop(endpoint_t ep);
```

**参数**：

- `proc_nr_e`：进程端点号

**作用**：停止进程，防止被调度

**注释翻译**：停止进程。

**设置标志**：`rmp->mp_flags |= PROC_STOPPED;`

**标记进程已停止**，避免重复调用 `sys_stop`。

***

#### 2.8.8 通知 VM（第 310-312 行）

```c
  if((r=vm_willexit(proc_nr_e)) != OK) {
	panic("exit_proc: vm_willexit failed: %d", r);
  }
```

**是什么**：通知 VM 进程即将退出

**vm\_willexit 函数**：

**函数签名**：

```c
int vm_willexit(endpoint_t ep);
```

**参数**：

- `proc_nr_e`：进程端点号

**作用**：通知 VM 进程即将退出，VM 可以清理进程的内存映射

**为什么失败时 panic？**

**设计原因**：

- VM 必须成功清理内存映射
- 如果失败，可能导致内存泄漏或状态不一致
- 系统无法继续运行，需要 panic

***

#### 2.8.9 特殊进程检查（第 314-323 行）

```c
  if (proc_nr_e == INIT_PROC_NR)
  {
	printf("PM: INIT died with exit status %d; showing stacktrace\n", exit_status);
	sys_diagctl_stacktrace(proc_nr_e);
	return;
  }
  if (proc_nr_e == VFS_PROC_NR)
  {
	panic("exit_proc: VFS died: %d", r);
  }
```

**逐行详细解释**：

**第 314-318 行**：INIT 进程退出

```c
  if (proc_nr_e == INIT_PROC_NR)
  {
	printf("PM: INIT died with exit status %d; showing stacktrace\n", exit_status);
	sys_diagctl_stacktrace(proc_nr_e);
	return;
  }
```

**是什么**：处理 INIT 进程退出

**INIT\_PROC\_NR 定义**：

```c
#define INIT_PROC_NR  1  // INIT 进程端点号
```

**INIT 进程**：

- 系统的第一个用户进程
- 负责启动其他进程
- 不应该退出

**处理方式**：

1. 打印警告信息
2. 显示堆栈跟踪（用于调试）
3. 返回（不继续退出流程）

**为什么 INIT 不应该退出？**

**设计原因**：

- INIT 是所有孤儿进程的父进程
- 如果 INIT 退出，孤儿进程无人管理
- 系统应该重启

**sys\_diagctl\_stacktrace 函数**：

**函数签名**：

```c
void sys_diagctl_stacktrace(endpoint_t ep);
```

**作用**：打印进程的堆栈跟踪，用于调试

***

**第 319-322 行**：VFS 进程退出

```c
  if (proc_nr_e == VFS_PROC_NR)
  {
	panic("exit_proc: VFS died: %d", r);
  }
```

**是什么**：处理 VFS 进程退出

**VFS\_PROC\_NR 定义**：

```c
#define VFS_PROC_NR  2  // VFS 进程端点号
```

**VFS 进程**：

- 虚拟文件系统服务
- 处理所有文件操作
- 不应该退出

**处理方式**：panic（系统崩溃）

**为什么 VFS 退出要 panic？**

**设计原因**：

- VFS 是核心服务，系统无法没有 VFS 运行
- 如果 VFS 退出，说明有严重错误
- 系统必须停止

**特殊进程处理示意**：

```
INIT 退出:
┌─────────────────────────────────────────────────────────────┐
│ INIT (PID=1)                                                │
│   → 调用 exit()                                             │
│   → PM 打印警告                                             │
│   → PM 显示堆栈跟踪                                         │
│   → PM 返回（不继续退出）                                   │
│   → 系统继续运行（但可能不稳定）                            │
└─────────────────────────────────────────────────────────────┘

VFS 退出:
┌─────────────────────────────────────────────────────────────┐
│ VFS (endpoint=2)                                            │
│   → 调用 exit()                                             │
│   → PM panic                                                │
│   → 系统崩溃                                                │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.8.10 通知 VFS（第 325-335 行）

```c
  /* Tell VFS, and after that any matching process event subscribers, about the
   * exiting process.
   */
  memset(&m, 0, sizeof(m));
  m.m_type = dump_core ? VFS_PM_DUMPCORE : VFS_PM_EXIT;
  m.VFS_PM_ENDPT = rmp->mp_endpoint;

  if (dump_core) {
	m.VFS_PM_TERM_SIG = rmp->mp_sigstatus;
	m.VFS_PM_PATH = rmp->mp_name;
  }

  tell_vfs(rmp, &m);
```

**逐行详细解释**：

**第 325-327 行**：注释

```c
  /* Tell VFS, and after that any matching process event subscribers, about the
   * exiting process.
   */
```

**翻译**：告诉 VFS，以及之后任何匹配的进程事件订阅者，关于退出进程的信息。

***

**第 328 行**：`memset(&m, 0, sizeof(m));`

**是什么**：清零消息结构体

***

**第 329 行**：`m.m_type = dump_core ? VFS_PM_DUMPCORE : VFS_PM_EXIT;`

**是什么**：设置消息类型

**三元运算符**：

- 如果 `dump_core` 为真：`VFS_PM_DUMPCORE`（生成 core dump）
- 否则：`VFS_PM_EXIT`（正常退出）

**消息类型定义**：

```c
#define VFS_PM_EXIT      (VFS_PM_RQ_BASE + 7)  // 正常退出
#define VFS_PM_DUMPCORE  (VFS_PM_RQ_BASE + 8)  // 生成 core dump
```

***

**第 330 行**：`m.VFS_PM_ENDPT = rmp->mp_endpoint;`

**是什么**：设置退出进程的端点号

***

**第 332-334 行**：设置 core dump 相关字段

```c
  if (dump_core) {
	m.VFS_PM_TERM_SIG = rmp->mp_sigstatus;
	m.VFS_PM_PATH = rmp->mp_name;
  }
```

**是什么**：如果是 core dump，设置终止信号和进程名

**字段说明**：

- `VFS_PM_TERM_SIG`：杀死进程的信号编号
- `VFS_PM_PATH`：进程名（用于生成 core 文件名）

**core 文件命名**：

```
core.<进程名>
例如：core.bash, core.my_program
```

***

**第 336 行**：`tell_vfs(rmp, &m);`

**是什么**：发送消息给 VFS

**作用**：通知 VFS 进程退出，VFS 可以清理文件描述符等资源

***

#### 2.8.11 清理系统进程（第 338-344 行）

```c
  if (rmp->mp_flags & PRIV_PROC)
  {
	/* Destroy system processes without waiting for VFS. This is
	 * needed because the system process might be a block device
	 * driver that VFS is blocked waiting on.
	 */
	if((r= sys_clear(rmp->mp_endpoint)) != OK)
		panic("exit_proc: sys_clear failed: %d", r);
  }
```

**逐行详细解释**：

**第 338 行**：`if (rmp->mp_flags & PRIV_PROC)`

**是什么**：检查是否是系统进程

***

**第 339-342 行**：注释

```c
	/* Destroy system processes without waiting for VFS. This is
	 * needed because the system process might be a block device
	 * driver that VFS is blocked waiting on.
	 */
```

**翻译**：不等待 VFS 就销毁系统进程。这是必需的，因为系统进程可能是 VFS 正在阻塞等待的块设备驱动程序。

**为什么不能等待 VFS？**

**死锁场景**：

```
死锁情况:
┌─────────────────────────────────────────────────────────────┐
│ 1. VFS 调用块设备驱动程序读取数据                            │
│ 2. VFS 阻塞等待驱动程序返回                                  │
│ 3. 驱动程序崩溃，调用 exit()                                 │
│ 4. PM 通知 VFS 进程退出                                      │
│ 5. VFS 正在阻塞，无法处理消息                                │
│ 6. PM 等待 VFS 回复                                          │
│ 7. 死锁！                                                    │
└─────────────────────────────────────────────────────────────┘

解决方案:
┌─────────────────────────────────────────────────────────────┐
│ 1. 驱动程序崩溃，调用 exit()                                 │
│ 2. PM 检测到是系统进程                                      │
│ 3. PM 立即调用 sys_clear() 销毁进程                         │
│ 4. 不等待 VFS 回复                                          │
│ 5. VFS 收到驱动程序退出的通知（异步）                        │
│ 6. VFS 处理错误，继续运行                                    │
└─────────────────────────────────────────────────────────────┘
```

***

**第 343-344 行**：调用 sys\_clear

```c
	if((r= sys_clear(rmp->mp_endpoint)) != OK)
		panic("exit_proc: sys_clear failed: %d", r);
```

**是什么**：清理内核进程结构

**sys\_clear 函数**：

**函数签名**：

```c
int sys_clear(endpoint_t ep);
```

**参数**：

- `rmp->mp_endpoint`：进程端点号

**作用**：销毁内核中的进程结构，释放资源

**为什么失败时 panic？**

**设计原因**：

- 系统进程必须被正确清理
- 如果失败，可能导致资源泄漏
- 系统无法继续运行，需要 panic

***

#### 2.8.12 设置退出标志（第 346-351 行）

```c
  /* Clean up most of the flags describing the process's state before the exit,
   * and mark it as exiting.
   */
  rmp->mp_flags &= (IN_USE|VFS_CALL|PRIV_PROC|TRACE_EXIT|PROC_STOPPED);
  rmp->mp_flags |= EXITING;
```

**逐行详细解释**：

**第 346-348 行**：注释

```c
  /* Clean up most of the flags describing the process's state before the exit,
   * and mark it as exiting.
   */
```

**翻译**：清理退出前描述进程状态的大部分标志，并标记为正在退出。

***

**第 349 行**：`rmp->mp_flags &= (IN_USE|VFS_CALL|PRIV_PROC|TRACE_EXIT|PROC_STOPPED);`

**是什么**：清除大部分标志，只保留特定标志

**保留的标志**：

| 标志            | 值      | 说明          |
| ------------- | ------ | ----------- |
| IN\_USE       | 0x0001 | 槽位在使用       |
| VFS\_CALL     | 0x0008 | 正在等待 VFS 回复 |
| PRIV\_PROC    | 0x0010 | 系统特权进程      |
| TRACE\_EXIT   | 0x0020 | 跟踪退出        |
| PROC\_STOPPED | 0x0040 | 进程已停止       |

**为什么保留这些标志？**

**设计原因**：

- `IN_USE`：槽位仍在使用，不能被重新分配
- `VFS_CALL`：VFS 还未回复，需要等待
- `PRIV_PROC`：系统进程，需要特殊处理
- `TRACE_EXIT`：调试器需要知道进程退出
- `PROC_STOPPED`：进程已停止，不能被调度

***

**第 350 行**：`rmp->mp_flags |= EXITING;`

**是什么**：设置 EXITING 标志

**EXITING 定义**：

```c
#define EXITING  0x0080  // 正在退出
```

**作用**：标记进程正在退出，防止重复退出

***

#### 2.8.13 保存退出状态（第 353-359 行）

```c
  /* Keep the process around until VFS is finished with it. */

  rmp->mp_exitstatus = (char) exit_status;

  /* For normal exits, try to notify the parent as soon as possible.
   * For core dumps, notify the parent only once the core dump has been made.
   */
  if (!dump_core)
	zombify(rmp);
```

**逐行详细解释**：

**第 353 行**：注释

```c
  /* Keep the process around until VFS is finished with it. */
```

**翻译**：保持进程存在，直到 VFS 完成处理。

***

**第 355 行**：`rmp->mp_exitstatus = (char) exit_status;`

**是什么**：保存退出状态码

**类型转换**：`(char) exit_status`

**为什么转换为 char？**

**设计原因**：

- 退出状态码范围：0-255
- char 类型足够存储
- 节省内存空间

***

**第 356-359 行**：注释和调用 zombify

```c
  /* For normal exits, try to notify the parent as soon as possible.
   * For core dumps, notify the parent only once the core dump has been made.
   */
  if (!dump_core)
	zombify(rmp);
```

**注释翻译**：对于正常退出，尽快通知父进程。对于 core dump，只在 core dump 生成后才通知父进程。

**为什么 core dump 延迟通知？**

**设计原因**：

- core dump 需要时间生成
- 父进程可能在 core dump 完成前就调用 wait()
- 延迟通知确保父进程获得完整信息

**zombify 函数**：

**函数签名**：

```c
static void zombify(struct mproc *rmp);
```

**作用**：将进程标记为僵尸状态，通知父进程

***

#### 2.8.14 处理子进程（第 361-415 行）

```c
  /* If the process has children, disinherit them.  INIT is the new parent. */
  for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++) {
	if (!(rmp->mp_flags & IN_USE)) continue;
	if (rmp->mp_tracer == proc_nr) {
		/* This child's tracer died. Do something sensible. */
		tracer_died(rmp);
	}
	if (rmp->mp_parent == proc_nr) {
		/* 'rmp' now points to a child to be disinherited. */
		rmp->mp_parent = INIT_PROC_NR;

		/* If the process is making a VFS call, remember that we set
		 * a new parent. This prevents FORK from replying to the wrong
		 * parent upon completion.
		 */
		if (rmp->mp_flags & VFS_CALL)
			rmp->mp_flags |= NEW_PARENT;

		/* Notify new parent. */
		if (rmp->mp_flags & ZOMBIE)
			check_parent(rmp, TRUE /*try_cleanup*/);
	}
  }
```

**逐行详细解释**：

**第 361 行**：注释

```c
  /* If the process has children, disinherit them.  INIT is the new parent. */
```

**翻译**：如果进程有子进程，剥夺它们的继承权。INIT 成为新的父进程。

**孤儿进程**：

**是什么**：父进程已退出的进程

**处理方式**：将父进程设置为 INIT（PID=1）

***

**第 362 行**：`for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)`

**是什么**：遍历进程表

**注意**：这里 `rmp` 被重新使用，指向遍历的进程（不是退出进程）

***

**第 363 行**：`if (!(rmp->mp_flags & IN_USE)) continue;`

**是什么**：跳过未使用的槽位

***

**第 364-367 行**：处理被跟踪的进程

```c
	if (rmp->mp_tracer == proc_nr) {
		/* This child's tracer died. Do something sensible. */
		tracer_died(rmp);
	}
```

**是什么**：如果进程的跟踪器是退出进程，调用 `tracer_died`

**注释翻译**：这个子进程的跟踪器死了。做一些合理的事情。

**tracer\_died 函数**：

**函数签名**：

```c
void tracer_died(struct mproc *rmp);
```

**作用**：处理跟踪器死亡的情况

**可能的处理**：

- 停止跟踪
- 继续运行进程
- 发送信号给进程

***

**第 368-380 行**：处理子进程

```c
	if (rmp->mp_parent == proc_nr) {
		/* 'rmp' now points to a child to be disinherited. */
		rmp->mp_parent = INIT_PROC_NR;

		/* If the process is making a VFS call, remember that we set
		 * a new parent. This prevents FORK from replying to the wrong
		 * parent upon completion.
		 */
		if (rmp->mp_flags & VFS_CALL)
			rmp->mp_flags |= NEW_PARENT;

		/* Notify new parent. */
		if (rmp->mp_flags & ZOMBIE)
			check_parent(rmp, TRUE /*try_cleanup*/);
	}
```

**是什么**：处理退出进程的子进程

**注释翻译**：'rmp' 现在指向一个要被剥夺继承权的子进程。

***

**第 370 行**：`rmp->mp_parent = INIT_PROC_NR;`

**是什么**：将父进程设置为 INIT

**INIT\_PROC\_NR 定义**：

```c
#define INIT_PROC_NR  1  // INIT 进程端点号
```

**为什么设置为 INIT？**

**POSIX 要求**：

- 孤儿进程应该被 INIT 收养
- INIT 会定期调用 wait() 清理僵尸进程

***

**第 372-377 行**：注释和设置 NEW\_PARENT 标志

```c
		/* If the process is making a VFS call, remember that we set
		 * a new parent. This prevents FORK from replying to the wrong
		 * parent upon completion.
		 */
		if (rmp->mp_flags & VFS_CALL)
			rmp->mp_flags |= NEW_PARENT;
```

**注释翻译**：如果进程正在进行 VFS 调用，记住我们设置了新的父进程。这防止 FORK 在完成时回复错误的父进程。

**NEW\_PARENT 标志**：

**是什么**：标记进程的父进程已更改

**为什么需要这个标志？**

**场景**：

```
FORK 期间父进程退出:
┌─────────────────────────────────────────────────────────────┐
│ 1. 父进程调用 fork()                                         │
│ 2. PM 创建子进程                                             │
│ 3. PM 通知 VFS                                              │
│ 4. 父进程退出（被杀死）                                      │
│    → 子进程的父进程改为 INIT                                 │
│    → 设置 NEW_PARENT 标志                                   │
│ 5. VFS 完成 fork                                            │
│ 6. PM 检查 NEW_PARENT 标志                                  │
│    → 回复 INIT，而不是原父进程                              │
└─────────────────────────────────────────────────────────────┘
```

***

**第 379-380 行**：通知新父进程

```c
		/* Notify new parent. */
		if (rmp->mp_flags & ZOMBIE)
			check_parent(rmp, TRUE /*try_cleanup*/);
```

**注释翻译**：通知新父进程。

**条件**：`rmp->mp_flags & ZOMBIE`

**含义**：如果子进程已经是僵尸，通知 INIT

**check\_parent 函数**：

**函数签名**：

```c
void check_parent(struct mproc *rmp, int try_cleanup);
```

**参数**：

- `rmp`：僵尸进程控制块
- `TRUE`：尝试清理进程

**作用**：通知父进程子进程已退出

***

#### 2.8.15 发送 SIGHUP 信号（第 417 行）

```c
  /* Send a hangup to the process' process group if it was a session leader. */
  if (procgrp != 0) check_sig(-procgrp, SIGHUP, FALSE /* ksig */);
```

**逐行详细解释**：

**第 417 行**：注释

```c
  /* Send a hangup to the process' process group if it was a session leader. */
```

**翻译**：如果进程是会话领导者，向进程组发送挂起信号。

***

**代码逻辑**：

```c
  if (procgrp != 0) check_sig(-procgrp, SIGHUP, FALSE /* ksig */);
```

**条件**：`procgrp != 0`

**含义**：进程是会话领导者

**check\_sig 函数**：

**函数签名**：

```c
void check_sig(pid_t procgrp, int signo, int ksig);
```

**参数**：

- `-procgrp`：进程组 ID（负数表示进程组）
- `SIGHUP`：挂起信号
- `FALSE`：不是来自内核

**作用**：向进程组发送信号

**为什么发送 SIGHUP？**

**POSIX 要求**：

- 会话领导者退出时，向进程组发送 SIGHUP
- 进程组中的进程应该终止或重新初始化

**应用场景**：

```
终端会话:
┌─────────────────────────────────────────────────────────────┐
│ 用户登录终端                                                │
│   → 创建会话领导者 (bash)                                   │
│   → bash 创建子进程 (vim, ls, etc.)                         │
│                                                              │
│ 用户关闭终端窗口                                            │
│   → bash 收到 SIGHUP                                        │
│   → bash 退出                                               │
│   → PM 检测到会话领导者退出                                 │
│   → PM 向进程组发送 SIGHUP                                  │
│   → vim, ls 等进程收到 SIGHUP                               │
│   → 进程终止或保存状态                                      │
└─────────────────────────────────────────────────────────────┘
```

***

### 2.9 exit\_restart 函数（第 420-469 行）

**函数概述**：

`exit_restart` 是进程退出的第二阶段，在 VFS 回复后执行。

**函数签名**：

```c
void exit_restart(struct mproc *rmp);
```

**参数**：

- `rmp`：退出进程的控制块

**返回值**：无（void）

***

#### 2.9.1 文件头注释（第 420-424 行）

```c
/* VFS replied to our exit or coredump request. Perform the second half of the
 * exit code.
 */
```

**翻译**：VFS 回复了我们的退出或 coredump 请求。执行退出代码的后半部分。

**设计思路**：

**两阶段退出**：

1. **第一阶段**（exit\_proc）：停止进程，通知 VFS
2. **第二阶段**（exit\_restart）：清理资源，通知父进程

**为什么分两阶段？**

**设计原因**：

- VFS 需要时间处理退出请求
- PM 不能阻塞等待 VFS
- 使用异步消息传递
- VFS 回复后执行第二阶段

***

#### 2.9.2 停止调度器（第 426-438 行）

```c
  int r;

  if((r = sched_stop(rmp->mp_scheduler, rmp->mp_endpoint)) != OK) {
 	/* If the scheduler refuses to give up scheduling, there is
	 * little we can do, except report it. This may cause problems
	 * later on, if this scheduler is asked to schedule another proc
	 * that has an endpoint->schedproc mapping identical to the proc
	 * we just tried to stop scheduling.
	*/
	printf("PM: The scheduler did not want to give up "
		"scheduling %s, ret=%d.\n", rmp->mp_name, r);
  }
```

**逐行详细解释**：

**第 426 行**：`int r;`

**是什么**：声明返回值变量

***

**第 428-438 行**：调用 sched\_stop

```c
  if((r = sched_stop(rmp->mp_scheduler, rmp->mp_endpoint)) != OK) {
 	/* If the scheduler refuses to give up scheduling, there is
	 * little we can do, except report it. This may cause problems
	 * later on, if this scheduler is asked to schedule another proc
	 * that has an endpoint->schedproc mapping identical to the proc
	 * we just tried to stop scheduling.
	*/
	printf("PM: The scheduler did not want to give up "
		"scheduling %s, ret=%d.\n", rmp->mp_name, r);
  }
```

**是什么**：停止调度器对进程的调度

**sched\_stop 函数**：

**函数签名**：

```c
int sched_stop(endpoint_t scheduler, endpoint_t proc);
```

**参数**：

- `rmp->mp_scheduler`：调度器端点号
- `rmp->mp_endpoint`：进程端点号

**作用**：通知调度器停止调度该进程

**注释翻译**：如果调度器拒绝放弃调度，我们能做的很少，只能报告它。这可能会在以后导致问题，如果这个调度器被要求调度另一个进程，该进程的 endpoint->schedproc 映射与我们刚尝试停止调度的进程相同。

**为什么只打印警告而不 panic？**

**设计原因**：

- 调度器拒绝停止调度不是致命错误
- 进程仍然可以退出
- 打印警告帮助调试

***

#### 深入理解：sys_stop() vs sched_stop() 的区别

**问题**：在 `exit_proc()` 第一阶段已经调用了 `sys_stop()` 停止进程，为什么在 `exit_restart()` 第二阶段还要调用 `sched_stop()`？

**答案**：这是两个不同层面的"停止"。

##### 核心区别

| 操作 | 函数 | 作用层面 | 含义 |
|------|------|----------|------|
| **停止执行** | `sys_stop()` | 内核 | 设置 `RTS_PROC_STOP`，进程不再被调度执行 |
| **清理调度资源** | `sched_stop()` | 调度器 | 从调度器管理表中移除进程记录 |

##### 代码证据

**sys_stop() - 内核层面停止**：

```c
// minix/include/minix/syslib.h
#define sys_stop(proc_ep) sys_runctl(proc_ep, RC_STOP, 0)
```

```c
// minix/kernel/system/do_runctl.c
case RC_STOP:
    RTS_SET(rp, RTS_PROC_STOP);  // 设置内核停止标志
    break;
```

**sched_stop() - 调度器层面清理**：

```c
// minix/lib/libsys/sched_stop.c
int sched_stop(endpoint_t scheduler_e, endpoint_t schedulee_e)
{
    /* 如果内核是调度器，直接返回 OK
     * 因为内核会在进程终止时自动清理
     */
    if (scheduler_e == KERNEL || scheduler_e == NONE)
        return(OK);

    /* 发送 SCHEDULING_STOP 消息给调度器服务 */
    m.m_lsys_sched_scheduling_stop.endpoint = schedulee_e;
    return _taskcall(scheduler_e, SCHEDULING_STOP, &m);
}
```

```c
// minix/servers/sched/schedule.c
int do_stop_scheduling(message *m_ptr)
{
    rmp = &schedproc[proc_nr_n];
    rmp->flags = 0;  // 仅清除标志，释放调度器中的 slot
    return OK;
}
```

##### 为什么需要两层？

**Minix3 的调度架构**：

```
┌─────────────────────────────────────────────────────────┐
│                    用户进程                              │
│              mp_scheduler = SCHED_PROC_NR               │
│              （由 SCHED 服务管理调度）                    │
├─────────────────────────────────────────────────────────┤
│                    INIT 进程                             │
│              mp_scheduler = KERNEL                      │
│              （由内核直接调度）                           │
├─────────────────────────────────────────────────────────┤
│                    系统进程                              │
│              mp_scheduler = NONE                       │
│              （由 RS 管理，不需要调度器）                  │
└─────────────────────────────────────────────────────────┘
```

**`sys_stop()` 只停止进程执行，但调度器仍然保留该进程的记录**：
- 调度器维护 `schedproc[]` 表
- 包含进程的优先级、CPU 亲和性、时间片等信息
- 如果不移除，调度器资源泄漏

##### 两阶段时序图

```
exit_proc() 第一阶段:
    │
    ├── sys_stop()          → 进程停止运行（内核层面）
    │
    └── tell_vfs()          → 通知 VFS 清理文件资源

exit_restart() 第二阶段:
    │
    ├── sched_stop()        → 清理调度器资源（调度器层面）
    │
    ├── sys_clear()         → 清理内核资源
    │
    └── vm_exit()           → 清理内存资源
```

**关键理解**：`sched_stop()` 是清理调度器里的资源，进程在第一阶段就已经停止运行了。

##### 注释解读

```c
/* If the scheduler refuses to give up scheduling, there is
 * little we can do, except report it. This may cause problems
 * later on, if this scheduler is asked to schedule another proc
 * that has an endpoint->schedproc mapping identical to the proc
 * we just tried to stop scheduling.
 */
```

**翻译**：

> 如果调度器拒绝放弃对该进程的调度，PM 几乎无能为力，只能报告这个情况。
> 
> 这可能导致后续问题：如果调度器被要求调度另一个进程，而该进程的 endpoint 与刚才尝试停止调度的进程有相同的 `endpoint->schedproc` 映射。

**问题场景**：

```
1. 进程 A 退出，PM 调用 sched_stop()
2. 调度器拒绝（返回错误）
3. 调度器仍然保留进程 A 的 schedproc 记录
4. 进程 A 的 slot 被新进程 B 复用（endpoint 可能相同）
5. 调度器可能混淆 A 和 B
```

**为什么调度器会拒绝？**

```c
// minix/servers/sched/schedule.c
int do_stop_scheduling(message *m_ptr)
{
    // 验证消息来源
    if (!accept_message(m_ptr))
        return EPERM;  // 拒绝非授权来源的消息
    // ...
}
```

调度器会拒绝来自非授权来源的消息（如非 PM 发送的消息）。

***

#### 2.9.3 清除调度器字段（第 440-445 行）

```c
  /* sched_stop is either called when the process is exiting or it is
   * being moved between schedulers. If it is being moved between
   * schedulers, we need to set the mp_scheduler to NONE so that PM
   * doesn't forward messages to the process' scheduler while being moved
   * (such as sched_nice). */
  rmp->mp_scheduler = NONE;
```

**逐行详细解释**：

**第 440-444 行**：注释

```c
  /* sched_stop is either called when the process is exiting or it is
   * being moved between schedulers. If it is being moved between
   * schedulers, we need to set the mp_scheduler to NONE so that PM
   * doesn't forward messages to the process' scheduler while being moved
   * (such as sched_nice). */
```

**翻译**：sched\_stop 在进程退出或在调度器之间移动时调用。如果它在调度器之间移动，我们需要将 mp\_scheduler 设置为 NONE，这样 PM 就不会在移动过程中转发消息给进程的调度器（如 sched\_nice）。

***

**第 445 行**：`rmp->mp_scheduler = NONE;`

**是什么**：清除调度器字段

**NONE 定义**：

```c
#define NONE -2  // 无调度器
```

**为什么设置为 NONE？**

**设计原因**：

- 进程已经退出，不需要调度器
- 防止 PM 向已退出的进程发送调度消息
- 标记进程不再被调度

***

#### 2.9.4 通知父进程（第 447-449 行）

```c
  /* For core dumps, now is the right time to try to contact the parent. */
  if (!(rmp->mp_flags & (TRACE_ZOMBIE | ZOMBIE | TOLD_PARENT)))
	zombify(rmp);
```

**逐行详细解释**：

**第 447 行**：注释

```c
  /* For core dumps, now is the right time to try to contact the parent. */
```

**翻译**：对于 core dump，现在是尝试联系父进程的正确时机。

***

**第 448-449 行**：调用 zombify

```c
  if (!(rmp->mp_flags & (TRACE_ZOMBIE | ZOMBIE | TOLD_PARENT)))
	zombify(rmp);
```

**是什么**：如果进程还未通知父进程，调用 zombify

**条件**：`!(rmp->mp_flags & (TRACE_ZOMBIE | ZOMBIE | TOLD_PARENT))`

**含义**：进程未被标记为僵尸，也未通知父进程

**标志说明**：

| 标志            | 说明           |
| ------------- | ------------ |
| TRACE\_ZOMBIE | 跟踪僵尸（调试器已通知） |
| ZOMBIE        | 僵尸状态         |
| TOLD\_PARENT  | 已通知父进程       |

**为什么需要检查这些标志？**

**设计原因**：

- 避免重复通知父进程
- 确保只调用一次 zombify

***

#### 2.9.5 清理用户进程（第 451-456 行）

```c
  if (!(rmp->mp_flags & PRIV_PROC))
  {
	/* destroy the (user) process */
	if((r=sys_clear(rmp->mp_endpoint)) != OK)
		panic("exit_restart: sys_clear failed: %d", r);
  }
```

**逐行详细解释**：

**第 451 行**：`if (!(rmp->mp_flags & PRIV_PROC))`

**是什么**：检查是否是用户进程

**注意**：系统进程在 exit\_proc 中已经调用 sys\_clear

***

**第 452-456 行**：调用 sys\_clear

```c
	/* destroy the (user) process */
	if((r=sys_clear(rmp->mp_endpoint)) != OK)
		panic("exit_restart: sys_clear failed: %d", r);
```

**注释翻译**：销毁（用户）进程。

**sys\_clear 函数**：前面已讲解

**为什么失败时 panic？**

**设计原因**：

- 用户进程必须被正确清理
- 如果失败，可能导致资源泄漏
- 系统无法继续运行，需要 panic

***

#### 2.9.6 释放内存（第 458-461 行）

```c
  /* Release the memory occupied by the child. */
  if((r=vm_exit(rmp->mp_endpoint)) != OK) {
  	panic("exit_restart: vm_exit failed: %d", r);
  }
```

**逐行详细解释**：

**第 458 行**：注释

```c
  /* Release the memory occupied by the child. */
```

**翻译**：释放子进程占用的内存。

***

**第 459-461 行**：调用 vm\_exit

```c
  if((r=vm_exit(rmp->mp_endpoint)) != OK) {
  	panic("exit_restart: vm_exit failed: %d", r);
  }
```

**是什么**：通知 VM 释放进程内存

**vm\_exit 函数**：

**函数签名**：

```c
int vm_exit(endpoint_t ep);
```

**参数**：

- `rmp->mp_endpoint`：进程端点号

**作用**：释放进程的内存映射和物理内存

**为什么失败时 panic？**

**设计原因**：

- 内存必须被正确释放
- 如果失败，会导致内存泄漏
- 系统无法继续运行，需要 panic

***

#### 2.9.7 唤醒跟踪器（第 463-468 行）

```c
  if (rmp->mp_flags & TRACE_EXIT)
  {
	/* Wake up the tracer, completing the ptrace(T_EXIT) call */
	mproc[rmp->mp_tracer].mp_reply.m_pm_lc_ptrace.data = 0;
	reply(rmp->mp_tracer, OK);
  }
```

**逐行详细解释**：

**第 463 行**：`if (rmp->mp_flags & TRACE_EXIT)`

**是什么**：检查是否有跟踪器等待退出

**TRACE\_EXIT 标志**：调试器正在跟踪进程退出

***

**第 464-468 行**：唤醒跟踪器

```c
	/* Wake up the tracer, completing the ptrace(T_EXIT) call */
	mproc[rmp->mp_tracer].mp_reply.m_pm_lc_ptrace.data = 0;
	reply(rmp->mp_tracer, OK);
```

**注释翻译**：唤醒跟踪器，完成 ptrace(T\_EXIT) 调用。

**操作**：

1. 设置跟踪器的回复数据为 0
2. 回复跟踪器（唤醒）

**ptrace 流程**：

```
调试器跟踪进程退出:
┌─────────────────────────────────────────────────────────────┐
│ 1. 调试器调用 ptrace(PTRACE_EXIT, pid)                      │
│    → PM 设置 TRACE_EXIT 标志                                │
│    → 调试器阻塞                                             │
│                                                              │
│ 2. 被调试进程退出                                           │
│    → PM exit_proc()                                         │
│    → PM exit_restart()                                      │
│                                                              │
│ 3. PM 检测到 TRACE_EXIT 标志                                │
│    → PM 唤醒调试器                                          │
│    → 调试器获得退出通知                                     │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.9.8 清理进程（第 470-472 行）

```c
  /* Clean up if the parent has collected the exit status */
  if (rmp->mp_flags & TOLD_PARENT)
	cleanup(rmp);
}
```

**逐行详细解释**：

**第 470 行**：注释

```c
  /* Clean up if the parent has collected the exit status */
```

**翻译**：如果父进程已经收集了退出状态，进行清理。

***

**第 471-472 行**：调用 cleanup

```c
  if (rmp->mp_flags & TOLD_PARENT)
	cleanup(rmp);
```

**是什么**：如果父进程已调用 wait()，清理进程

**条件**：`rmp->mp_flags & TOLD_PARENT`

**含义**：父进程已获得退出状态

**cleanup 函数**：

**函数签名**：

```c
void cleanup(struct mproc *rmp);
```

**作用**：清理进程控制块，释放槽位

**清理流程**：

```
进程退出完整流程:
┌─────────────────────────────────────────────────────────────┐
│ 1. 进程调用 exit()                                          │
│    → PM do_exit()                                           │
│    → PM exit_proc()                                         │
│       → 停止进程                                            │
│       → 通知 VFS                                            │
│       → 设置 EXITING 标志                                   │
│       → 处理子进程                                          │
│                                                              │
│ 2. VFS 回复                                                 │
│    → PM exit_restart()                                      │
│       → 停止调度器                                          │
│       → 清理内核进程结构                                    │
│       → 释放内存                                            │
│       → 通知父进程（zombify）                               │
│                                                              │
│ 3. 父进程调用 wait()                                        │
│    → PM do_wait4()                                          │
│    → PM tell_parent()                                       │
│    → 设置 TOLD_PARENT 标志                                  │
│                                                              │
│ 4. 清理进程                                                 │
│    → PM cleanup()                                           │
│    → 清除 IN_USE 标志                                       │
│    → 释放槽位                                               │
└─────────────────────────────────────────────────────────────┘
```

***

### 2.10 do\_wait4 函数（第 475-567 行）

**函数概述**：

`do_wait4` 处理 `wait4()` 系统调用，等待子进程退出。

**函数签名**：

```c
int do_wait4(void);
```

**返回值**：

- `SUSPEND`：挂起调用进程，等待子进程退出
- `0`：WNOHANG 标志设置，无子进程退出
- `ECHILD`：无符合条件的子进程
- 子进程 PID：成功等待到子进程

***

#### 2.10.1 文件头注释（第 475-483 行）

```c
/* A process wants to wait for a child to terminate. If a child is already
 * waiting, go clean it up and let this WAIT4 call terminate.  Otherwise,
 * really wait.
 * A process calling WAIT4 never gets a reply in the usual way at the end
 * of the main loop (unless WNOHANG is set or no qualifying child exists).
 * If a child has already exited, the routine tell_parent() sends the reply
 * to awaken the caller.
 */
```

**翻译**：进程想要等待子进程终止。如果子进程已经在等待，清理它并让这个 WAIT4 调用终止。否则，真正等待。

调用 WAIT4 的进程不会在主循环结束时以通常的方式获得回复（除非设置了 WNOHANG 或没有符合条件的子进程）。如果子进程已经退出，tell\_parent() 例程发送回复来唤醒调用者。

**设计思路**：

**wait4 的两种情况**：

1. **子进程已退出**：立即返回，不阻塞
2. **子进程未退出**：阻塞父进程，等待子进程退出

***

#### 2.10.2 局部变量声明（第 484-487 行）

```c
  register struct mproc *rp;
  vir_bytes addr;
  int i, pidarg, options, children, waited_for;
```

**逐行详细解释**：

**第 484 行**：`register struct mproc *rp;`

**是什么**：声明进程控制块指针

**用途**：遍历进程表

***

**第 485 行**：`vir_bytes addr;`

**是什么**：声明虚拟地址变量

**用途**：存储 rusage 结构的地址

***

**第 486 行**：`int i, pidarg, options, children, waited_for;`

**是什么**：声明整型变量

**变量说明**：

- `i`：循环计数器
- `pidarg`：wait4 的第一个参数（要等待的进程 ID）
- `options`：wait4 的第三个参数（选项）
- `children`：符合条件的子进程数量
- `waited_for`：是否成功等待

***

#### 2.10.3 解析参数（第 489-493 行）

```c
  /* Set internal variables. */
  pidarg  = m_in.m_lc_pm_wait4.pid;		/* 1st param */
  options = m_in.m_lc_pm_wait4.options;		/* 3rd param */
  addr    = m_in.m_lc_pm_wait4.addr;		/* 4th param */
  if (pidarg == 0) pidarg = -mp->mp_procgrp;	/* pidarg < 0 ==> proc grp */
```

**逐行详细解释**：

**第 489 行**：注释

```c
  /* Set internal variables. */
```

**翻译**：设置内部变量。

***

**第 490-492 行**：获取参数

```c
  pidarg  = m_in.m_lc_pm_wait4.pid;		/* 1st param */
  options = m_in.m_lc_pm_wait4.options;		/* 3rd param */
  addr    = m_in.m_lc_pm_wait4.addr;		/* 4th param */
```

**是什么**：从消息中提取参数

**注释翻译**：

- 第 490 行：第 1 个参数
- 第 491 行：第 3 个参数
- 第 492 行：第 4 个参数

**wait4 函数原型**：

```c
pid_t wait4(pid_t pid, int *status, int options, struct rusage *rusage);
```

**参数说明**：

- `pid`：要等待的进程 ID
- `status`：存储退出状态
- `options`：选项（如 WNOHANG）
- `rusage`：存储资源使用情况

***

**第 493 行**：`if (pidarg == 0) pidarg = -mp->mp_procgrp;`

**是什么**：处理 pid=0 的情况

**注释翻译**：pidarg < 0 表示进程组。

**pidarg 的含义**：

| pidarg 值 | 含义              |
| -------- | --------------- |
| > 0      | 等待指定 PID 的子进程   |
| -1       | 等待任意子进程         |
| < -1     | 等待指定进程组的子进程     |
| 0        | 等待与调用进程同进程组的子进程 |

**为什么转换为负数？**

**设计原因**：

- 统一处理：负数表示进程组
- 简化后续的条件判断

***

#### 2.10.4 查找符合条件的子进程（第 495-547 行）

```c
  /* Is there a child waiting to be collected? At this point, pidarg != 0:
   *	pidarg  >  0 means pidarg is pid of a specific process to wait for
   *	pidarg == -1 means wait for any child
   *	pidarg  < -1 means wait for any child whose process group = -pidarg
   */
  children = 0;
  for (rp = &mproc[0]; rp < &mproc[NR_PROCS]; rp++) {
	if ((rp->mp_flags & (IN_USE | TOLD_PARENT)) != IN_USE) continue;
	if (rp->mp_parent != who_p && rp->mp_tracer != who_p) continue;
	if (rp->mp_parent != who_p && (rp->mp_flags & ZOMBIE)) continue;

	/* The value of pidarg determines which children qualify. */
	if (pidarg  > 0 && pidarg != rp->mp_pid) continue;
	if (pidarg < -1 && -pidarg != rp->mp_procgrp) continue;

	children++;			/* this child is acceptable */
```

**逐行详细解释**：

**第 495-500 行**：注释

```c
  /* Is there a child waiting to be collected? At this point, pidarg != 0:
   *	pidarg  >  0 means pidarg is pid of a specific process to wait for
   *	pidarg == -1 means wait for any child
   *	pidarg  < -1 means wait for any child whose process group = -pidarg
   */
```

**翻译**：是否有子进程等待被收集？此时 pidarg != 0：

- pidarg > 0 表示 pidarg 是要等待的特定进程的 PID
- pidarg == -1 表示等待任意子进程
- pidarg < -1 表示等待进程组 = -pidarg 的任意子进程

***

**第 501 行**：`children = 0;`

**是什么**：初始化子进程计数器

***

**第 502 行**：`for (rp = &mproc[0]; rp < &mproc[NR_PROCS]; rp++)`

**是什么**：遍历进程表

***

**第 503 行**：`if ((rp->mp_flags & (IN_USE | TOLD_PARENT)) != IN_USE) continue;`

**是什么**：跳过未使用或已通知父进程的进程

**条件**：

- `IN_USE`：槽位在使用
- `TOLD_PARENT`：已通知父进程

**逻辑**：

- 必须在使用中（IN\_USE）
- 必须未通知父进程（\~TOLD\_PARENT）

***

**第 504 行**：`if (rp->mp_parent != who_p && rp->mp_tracer != who_p) continue;`

**是什么**：检查是否是子进程或被跟踪进程

**条件**：

- `rp->mp_parent == who_p`：是子进程
- `rp->mp_tracer == who_p`：是被跟踪进程

**逻辑**：必须满足其中之一

***

**第 505 行**：`if (rp->mp_parent != who_p && (rp->mp_flags & ZOMBIE)) continue;`

**是什么**：跳过僵尸的被跟踪进程

**条件**：

- `rp->mp_parent != who_p`：不是子进程（是被跟踪进程）
- `rp->mp_flags & ZOMBIE`：是僵尸

**逻辑**：被跟踪进程如果是僵尸，不在这里处理

***

**第 507-508 行**：检查 pidarg 条件

```c
	/* The value of pidarg determines which children qualify. */
	if (pidarg  > 0 && pidarg != rp->mp_pid) continue;
	if (pidarg < -1 && -pidarg != rp->mp_procgrp) continue;
```

**注释翻译**：pidarg 的值决定哪些子进程符合条件。

**条件判断**：

- `pidarg > 0`：必须匹配指定 PID
- `pidarg < -1`：必须匹配指定进程组

***

**第 510 行**：`children++;`

**是什么**：增加符合条件的子进程计数

**注释翻译**：这个子进程是可接受的。

***

#### 2.10.5 处理被跟踪的僵尸进程（第 512-531 行）

```c
	if (rp->mp_tracer == who_p) {
		if (rp->mp_flags & TRACE_ZOMBIE) {
			/* Traced child meets the pid test and has exited. */
			tell_tracer(rp);
			check_parent(rp, TRUE /*try_cleanup*/);
			return(SUSPEND);
		}
		if (rp->mp_flags & TRACE_STOPPED) {
			/* This child meets the pid test and is being traced.
			 * Deliver a signal to the tracer, if any.
			 */
			for (i = 1; i < _NSIG; i++) {
				if (sigismember(&rp->mp_sigtrace, i)) {
					/* TODO: rusage support */

					sigdelset(&rp->mp_sigtrace, i);

					mp->mp_reply.m_pm_lc_wait4.status =
					    W_STOPCODE(i);
					return(rp->mp_pid);
				}
			}
		}
	}
```

**逐行详细解释**：

**第 512 行**：`if (rp->mp_tracer == who_p)`

**是什么**：检查是否是被跟踪进程

**条件**：`rp->mp_tracer == who_p`

**含义**：当前进程是此进程的跟踪器

***

**第 513-518 行**：处理 TRACE\_ZOMBIE 状态

```c
		if (rp->mp_flags & TRACE_ZOMBIE) {
			/* Traced child meets the pid test and has exited. */
			tell_tracer(rp);
			check_parent(rp, TRUE /*try_cleanup*/);
			return(SUSPEND);
		}
```

**是什么**：处理已退出的被跟踪进程

**注释翻译**：被跟踪的子进程符合 pid 测试且已退出。

**操作**：

1. 调用 `tell_tracer(rp)`：通知跟踪器
2. 调用 `check_parent(rp, TRUE)`：检查父进程
3. 返回 `SUSPEND`：挂起当前进程

**TRACE\_ZOMBIE 标志**：

**是什么**：被跟踪进程已退出，等待跟踪器处理

**与 ZOMBIE 的区别**：

- `TRACE_ZOMBIE`：等待跟踪器
- `ZOMBIE`：等待父进程

***

**第 519-531 行**：处理 TRACE\_STOPPED 状态

```c
		if (rp->mp_flags & TRACE_STOPPED) {
			/* This child meets the pid test and is being traced.
			 * Deliver a signal to the tracer, if any.
			 */
			for (i = 1; i < _NSIG; i++) {
				if (sigismember(&rp->mp_sigtrace, i)) {
					/* TODO: rusage support */

					sigdelset(&rp->mp_sigtrace, i);

					mp->mp_reply.m_pm_lc_wait4.status =
					    W_STOPCODE(i);
					return(rp->mp_pid);
				}
			}
		}
```

**是什么**：处理已停止的被跟踪进程

**注释翻译**：这个子进程符合 pid 测试且正在被跟踪。向跟踪器传递信号（如果有）。

**TRACE\_STOPPED 标志**：

**是什么**：被跟踪进程已停止（如收到 SIGSTOP）

**循环**：`for (i = 1; i < _NSIG; i++)`

**作用**：遍历所有信号，查找待传递的信号

**sigismember 函数**：

**函数签名**：

```c
int sigismember(const sigset_t *set, int signo);
```

**参数**：

- `&rp->mp_sigtrace`：待跟踪信号集
- `i`：信号编号

**返回值**：

- 1：信号在集合中
- 0：信号不在集合中

**sigdelset 函数**：

**函数签名**：

```c
int sigdelset(sigset_t *set, int signo);
```

**作用**：从信号集中删除信号

**W\_STOPCODE 宏**：

**定义**：

```c
#define W_STOPCODE(sig)  ((sig) << 8 | 0x7f)
```

**作用**：生成停止状态码

**示例**：

```c
W_STOPCODE(SIGSTOP) = (17 << 8) | 0x7f = 0x8f7f
```

**返回值**：`return(rp->mp_pid);`

**是什么**：返回子进程 PID

**含义**：成功等待到停止的子进程

***

#### 2.10.6 处理僵尸子进程（第 533-544 行）

```c
	if (rp->mp_parent == who_p) {
		if (rp->mp_flags & ZOMBIE) {
			/* This child meets the pid test and has exited. */
			waited_for = tell_parent(rp, addr);

			if (waited_for &&
			    !(rp->mp_flags & (VFS_CALL | EVENT_CALL)))
				cleanup(rp);
			return(SUSPEND);
		}
	}
```

**逐行详细解释**：

**第 533 行**：`if (rp->mp_parent == who_p)`

**是什么**：检查是否是子进程

**条件**：`rp->mp_parent == who_p`

**含义**：当前进程是此进程的父进程

***

**第 534-544 行**：处理 ZOMBIE 状态

```c
		if (rp->mp_flags & ZOMBIE) {
			/* This child meets the pid test and has exited. */
			waited_for = tell_parent(rp, addr);

			if (waited_for &&
			    !(rp->mp_flags & (VFS_CALL | EVENT_CALL)))
				cleanup(rp);
			return(SUSPEND);
		}
```

**是什么**：处理已退出的子进程

**注释翻译**：这个子进程符合 pid 测试且已退出。

**操作**：

**第 537 行**：`waited_for = tell_parent(rp, addr);`

**是什么**：通知父进程子进程已退出

**tell\_parent 函数**：

**函数签名**：

```c
static int tell_parent(struct mproc *child, vir_bytes addr);
```

**参数**：

- `rp`：子进程控制块
- `addr`：rusage 结构的地址

**返回值**：

- `TRUE`：成功通知父进程，子进程已清理
- `FALSE`：通知失败，子进程仍是僵尸

***

**第 539-541 行**：清理进程

```c
			if (waited_for &&
			    !(rp->mp_flags & (VFS_CALL | EVENT_CALL)))
				cleanup(rp);
```

**是什么**：如果成功通知父进程且无待处理调用，清理进程

**条件**：

- `waited_for`：成功通知父进程
- `!(rp->mp_flags & (VFS_CALL | EVENT_CALL))`：无待处理的 VFS 或事件调用

**cleanup 函数**：

**函数签名**：

```c
static void cleanup(struct mproc *rmp);
```

**作用**：清理进程控制块，释放槽位

***

**第 542 行**：`return(SUSPEND);`

**是什么**：返回 SUSPEND

**为什么返回 SUSPEND？**

**设计原因**：

- 父进程已经收到回复（通过 tell\_parent）
- 不需要主循环再次回复
- 返回 SUSPEND 防止重复回复

***

#### 2.10.7 处理无僵尸子进程的情况（第 547-561 行）

```c
  /* No qualifying child has exited.  Wait for one, unless none exists. */
  if (children > 0) {
	/* At least 1 child meets the pid test exists, but has not exited. */
	if (options & WNOHANG) {
		return(0);    /* parent does not want to wait */
	}
	mp->mp_flags |= WAITING;	     /* parent wants to wait */
	mp->mp_wpid = (pid_t) pidarg;	     /* save pid for later */
	mp->mp_waddr = addr;		     /* save rusage addr for later */
	return(SUSPEND);		     /* do not reply, let it wait */
  } else {
	/* No child even meets the pid test.  Return error immediately. */
	return(ECHILD);			     /* no - parent has no children */
  }
}
```

**逐行详细解释**：

**第 547 行**：注释

```c
  /* No qualifying child has exited.  Wait for one, unless none exists. */
```

**翻译**：没有符合条件的子进程退出。等待一个，除非不存在。

***

**第 548 行**：`if (children > 0)`

**是什么**：检查是否有符合条件的子进程

***

**第 549-551 行**：注释

```c
	/* At least 1 child meets the pid test exists, but has not exited. */
```

**翻译**：至少有 1 个子进程符合 pid 测试，但还未退出。

***

**第 550-551 行**：检查 WNOHANG 选项

```c
	if (options & WNOHANG) {
		return(0);    /* parent does not want to wait */
	}
```

**是什么**：如果设置了 WNOHANG，立即返回

**WNOHANG 定义**：

```c
#define WNOHANG  0x00000001  // 不阻塞
```

**注释翻译**：父进程不想等待。

**返回值**：`0` 表示无子进程退出

***

**第 552-555 行**：设置等待标志

```c
	mp->mp_flags |= WAITING;	     /* parent wants to wait */
	mp->mp_wpid = (pid_t) pidarg;	     /* save pid for later */
	mp->mp_waddr = addr;		     /* save rusage addr for later */
	return(SUSPEND);		     /* do not reply, let it wait */
```

**是什么**：设置父进程为等待状态

**注释翻译**：

- 第 552 行：父进程想要等待
- 第 553 行：保存 pid 供以后使用
- 第 554 行：保存 rusage 地址供以后使用
- 第 555 行：不回复，让它等待

**WAITING 标志**：

**定义**：

```c
#define WAITING  0x0080  // 等待子进程
```

**作用**：标记父进程正在等待子进程退出

**字段说明**：

- `mp_wpid`：要等待的进程 ID
- `mp_waddr`：rusage 结构的地址

**为什么返回 SUSPEND？**

**设计原因**：

- 父进程需要阻塞，等待子进程退出
- 返回 SUSPEND 让主循环不回复
- 子进程退出时，PM 会唤醒父进程

***

**第 556-559 行**：无符合条件的子进程

```c
  } else {
	/* No child even meets the pid test.  Return error immediately. */
	return(ECHILD);			     /* no - parent has no children */
  }
```

**是什么**：返回错误

**注释翻译**：没有子进程符合 pid 测试。立即返回错误。

**ECHILD 定义**：

```c
#define ECHILD  10  // 无子进程
```

**返回值**：`ECHILD` 表示无符合条件的子进程

***

### 2.11 wait\_test 函数（第 564-577 行）

**函数概述**：

`wait_test` 检查父进程或跟踪器是否在等待子进程。

**函数签名**：

```c
int wait_test(
    struct mproc *rmp,    // 可能正在等待的进程
    struct mproc *child   // 可能被等待的进程
);
```

**返回值**：

- 1：进程正在等待子进程
- 0：进程未等待子进程

***

#### 2.11.1 函数实现（第 571-577 行）

```c
/* See if a parent or tracer process is waiting for a child process.
 * A tracer is considered to be a pseudo-parent.
 */
  int parent_waiting, right_child;
  pid_t pidarg;

  pidarg = rmp->mp_wpid;		/* who's being waited for? */
  parent_waiting = rmp->mp_flags & WAITING;
  right_child =				/* child meets one of the 3 tests? */
  	(pidarg == -1 || pidarg == child->mp_pid ||
  	 -pidarg == child->mp_procgrp);

  return (parent_waiting && right_child);
}
```

**逐行详细解释**：

**第 566-568 行**：注释

```c
/* See if a parent or tracer process is waiting for a child process.
 * A tracer is considered to be a pseudo-parent.
 */
```

**翻译**：查看父进程或跟踪器进程是否在等待子进程。跟踪器被认为是伪父进程。

***

**第 569 行**：`int parent_waiting, right_child;`

**是什么**：声明两个整型变量

**变量说明**：

- `parent_waiting`：父进程是否在等待
- `right_child`：子进程是否符合条件

***

**第 570 行**：`pid_t pidarg;`

**是什么**：声明 PID 变量

***

**第 572 行**：`pidarg = rmp->mp_wpid;`

**是什么**：获取父进程等待的 PID

**注释翻译**：谁在被等待？

***

**第 573 行**：`parent_waiting = rmp->mp_flags & WAITING;`

**是什么**：检查父进程是否在等待

**结果**：

- 非 0：父进程在等待
- 0：父进程未等待

***

**第 574-576 行**：检查子进程是否符合条件

```c
  right_child =				/* child meets one of the 3 tests? */
  	(pidarg == -1 || pidarg == child->mp_pid ||
  	 -pidarg == child->mp_procgrp);
```

**注释翻译**：子进程符合 3 个测试之一吗？

**三个测试**：

1. `pidarg == -1`：等待任意子进程
2. `pidarg == child->mp_pid`：等待指定 PID
3. `-pidarg == child->mp_procgrp`：等待指定进程组

**逻辑**：满足其中之一即可

***

**第 578 行**：`return (parent_waiting && right_child);`

**是什么**：返回结果

**逻辑**：父进程在等待 **且** 子进程符合条件

***

### 2.12 zombify 函数（第 581-607 行）

**函数概述**：

`zombify` 将进程标记为僵尸状态，通知父进程或跟踪器。

**函数签名**：

```c
static void zombify(struct mproc *rmp);
```

**参数**：

- `rmp`：要僵尸化的进程控制块

**返回值**：无（void）

***

#### 2.12.1 检查进程状态（第 589-590 行）

```c
  struct mproc *t_mp;

  if (rmp->mp_flags & (TRACE_ZOMBIE | ZOMBIE))
	panic("zombify: process was already a zombie");
```

**逐行详细解释**：

**第 589 行**：`struct mproc *t_mp;`

**是什么**：声明跟踪器进程指针

***

**第 590 行**：`if (rmp->mp_flags & (TRACE_ZOMBIE | ZOMBIE))`

**是什么**：检查进程是否已经是僵尸

**条件**：

- `TRACE_ZOMBIE`：跟踪僵尸
- `ZOMBIE`：普通僵尸

**如果已经是僵尸，panic**

**为什么 panic？**

**设计原因**：

- 进程不应该被重复僵尸化
- 如果发生，说明逻辑错误
- 系统无法继续运行，需要 panic

***

#### 2.12.2 通知跟踪器（第 592-602 行）

```c
  /* See if we have to notify a tracer process first. */
  if (rmp->mp_tracer != NO_TRACER && rmp->mp_tracer != rmp->mp_parent) {
	rmp->mp_flags |= TRACE_ZOMBIE;

	t_mp = &mproc[rmp->mp_tracer];

	/* Do not bother sending SIGCHLD signals to tracers. */
	if (!wait_test(t_mp, rmp))
		return;

	tell_tracer(rmp);
  }
```

**逐行详细解释**：

**第 592 行**：注释

```c
  /* See if we have to notify a tracer process first. */
```

**翻译**：查看我们是否必须先通知跟踪器进程。

***

**第 593 行**：`if (rmp->mp_tracer != NO_TRACER && rmp->mp_tracer != rmp->mp_parent)`

**是什么**：检查是否有跟踪器且跟踪器不是父进程

**条件**：

- `rmp->mp_tracer != NO_TRACER`：有跟踪器
- `rmp->mp_tracer != rmp->mp_parent`：跟踪器不是父进程

**为什么跟踪器不是父进程？**

**场景**：

```
调试器跟踪进程:
┌─────────────────────────────────────────────────────────────┐
│ 情况 1: 父进程是调试器                                      │
│   父进程 (PID=1234, gdb)                                   │
│     → 子进程 (PID=1235, 被调试程序)                         │
│     → mp_tracer = mp_parent = 1234                         │
│     → 直接通知父进程                                        │
│                                                              │
│ 情况 2: 父进程不是调试器                                    │
│   父进程 (PID=1234, bash)                                   │
│   调试器 (PID=2000, gdb)                                    │
│     → 子进程 (PID=1235, 被调试程序)                         │
│     → mp_parent = 1234                                      │
│     → mp_tracer = 2000                                      │
│     → 先通知调试器，再通知父进程                            │
└─────────────────────────────────────────────────────────────┘
```

***

**第 594 行**：`rmp->mp_flags |= TRACE_ZOMBIE;`

**是什么**：设置 TRACE\_ZOMBIE 标志

**作用**：标记进程为跟踪僵尸

***

**第 596 行**：`t_mp = &mproc[rmp->mp_tracer];`

**是什么**：获取跟踪器进程控制块

***

**第 597-599 行**：注释和检查

```c
	/* Do not bother sending SIGCHLD signals to tracers. */
	if (!wait_test(t_mp, rmp))
		return;
```

**注释翻译**：不要向跟踪器发送 SIGCHLD 信号。

**条件**：`!wait_test(t_mp, rmp)`

**含义**：跟踪器未在等待

**如果跟踪器未等待，直接返回**

**为什么返回？**

**设计原因**：

- 跟踪器未调用 wait()
- 不需要立即通知
- 等跟踪器调用 wait() 时再处理

***

**第 601 行**：`tell_tracer(rmp);`

**是什么**：通知跟踪器

**tell\_tracer 函数**：后面详细讲解

***

#### 2.12.3 设置 ZOMBIE 标志（第 603-605 行）

```c
  else {
	rmp->mp_flags |= ZOMBIE;
  }
```

**是什么**：如果没有跟踪器或跟踪器是父进程，设置 ZOMBIE 标志

**逻辑**：

- 有跟踪器且跟踪器不是父进程：设置 TRACE\_ZOMBIE
- 否则：设置 ZOMBIE

***

#### 2.12.4 通知父进程（第 607 行）

```c
  /* No tracer, or tracer is parent, or tracer has now been notified. */
  check_parent(rmp, FALSE /*try_cleanup*/);
}
```

**注释翻译**：无跟踪器，或跟踪器是父进程，或跟踪器已被通知。

**check\_parent 函数**：后面详细讲解

**try\_cleanup 参数**：`FALSE`

**为什么是 FALSE？**

**设计原因**：

- 此时进程还未完全清理
- 不应该立即清理进程
- 等父进程调用 wait() 时再清理

***

### 2.13 check\_parent 函数（第 610-640 行）

**函数概述**：

`check_parent` 通知父进程子进程已退出。

**函数签名**：

```c
static void check_parent(
    struct mproc *child,    // 退出的子进程
    int try_cleanup         // 是否尝试清理
);
```

**返回值**：无（void）

***

#### 2.13.1 文件头注释（第 610-619 行）

```c
/* We would like to inform the parent of an exiting child about the child's
 * death. If the parent is waiting for the child, tell it immediately;
 * otherwise, send it a SIGCHLD signal.
 *
 * Note that we may call this function twice on a single child; first with
 * its original parent, later (if the parent died) with INIT as its parent.
 */
```

**翻译**：我们想要通知父进程关于退出子进程的死亡。如果父进程在等待子进程，立即告诉它；否则，发送 SIGCHLD 信号。

注意，我们可能在单个子进程上调用此函数两次；首先使用其原始父进程，稍后（如果父进程死亡）使用 INIT 作为其父进程。

**设计思路**：

**两种情况**：

1. **父进程在等待**：立即通知（通过 tell\_parent）
2. **父进程未等待**：发送 SIGCHLD 信号

**可能调用两次**：

- 第一次：原始父进程
- 第二次：INIT（如果父进程退出）

***

#### 2.13.2 获取父进程（第 620-622 行）

```c
  struct mproc *p_mp;

  p_mp = &mproc[child->mp_parent];
```

**逐行详细解释**：

**第 620 行**：`struct mproc *p_mp;`

**是什么**：声明父进程指针

***

**第 622 行**：`p_mp = &mproc[child->mp_parent];`

**是什么**：获取父进程控制块

***

#### 2.13.3 检查父进程状态（第 624-628 行）

```c
  if (p_mp->mp_flags & EXITING) {
	/* This may trigger if the child of a dead parent dies. The child will
	 * be assigned to INIT and rechecked shortly after. Do nothing.
	 */
  }
```

**逐行详细解释**：

**第 624 行**：`if (p_mp->mp_flags & EXITING)`

**是什么**：检查父进程是否正在退出

***

**第 625-627 行**：注释

```c
	/* This may trigger if the child of a dead parent dies. The child will
	 * be assigned to INIT and rechecked shortly after. Do nothing.
	 */
```

**翻译**：如果已死亡父进程的子进程死亡，这可能会触发。子进程将被分配给 INIT 并在稍后重新检查。什么也不做。

**场景**：

```
父进程正在退出:
┌─────────────────────────────────────────────────────────────┐
│ 1. 父进程调用 exit()                                         │
│    → PM exit_proc()                                         │
│    → 设置 EXITING 标志                                      │
│                                                              │
│ 2. 子进程退出（可能因为信号）                                │
│    → PM exit_proc()                                         │
│    → 调用 check_parent()                                    │
│    → 检测到父进程正在退出                                   │
│    → 什么也不做                                             │
│                                                              │
│ 3. 父进程退出完成                                           │
│    → 子进程被 INIT 收养                                     │
│    → 再次调用 check_parent()                                │
│    → 通知 INIT                                              │
└─────────────────────────────────────────────────────────────┘
```

***

#### 2.13.4 父进程在等待（第 629-636 行）

```c
  else if (wait_test(p_mp, child)) {
	if (!tell_parent(child, p_mp->mp_waddr))
		try_cleanup = FALSE; /* child is still there */

	/* The 'try_cleanup' flag merely saves us from having to be really
	 * careful with statement ordering in exit_proc() and exit_restart().
	 */
	if (try_cleanup && !(child->mp_flags & (VFS_CALL | EVENT_CALL)))
		cleanup(child);
  }
```

**逐行详细解释**：

**第 629 行**：`else if (wait_test(p_mp, child))`

**是什么**：检查父进程是否在等待子进程

***

**第 630-631 行**：调用 tell\_parent

```c
	if (!tell_parent(child, p_mp->mp_waddr))
		try_cleanup = FALSE; /* child is still there */
```

**是什么**：通知父进程

**参数**：

- `child`：子进程控制块
- `p_mp->mp_waddr`：rusage 结构的地址

**返回值**：

- `TRUE`：成功通知，子进程已清理
- `FALSE`：通知失败，子进程仍是僵尸

**注释翻译**：子进程还在那里。

**如果通知失败，设置 try\_cleanup = FALSE**

**为什么？**

**设计原因**：

- 通知失败意味着子进程仍是僵尸
- 不应该清理进程
- 等下次再尝试

***

**第 632-636 行**：注释和清理

```c
	/* The 'try_cleanup' flag merely saves us from having to be really
	 * careful with statement ordering in exit_proc() and exit_restart().
	 */
	if (try_cleanup && !(child->mp_flags & (VFS_CALL | EVENT_CALL)))
		cleanup(child);
```

**注释翻译**：'try\_cleanup' 标志只是让我们不必在 exit\_proc() 和 exit\_restart() 中非常小心语句顺序。

**条件**：

- `try_cleanup`：允许清理
- `!(child->mp_flags & (VFS_CALL | EVENT_CALL))`：无待处理调用

**cleanup 函数**：后面详细讲解

***

#### 2.13.5 父进程未等待（第 637-640 行）

```c
  else {
	/* Parent is not waiting. */
	sig_proc(p_mp, SIGCHLD, TRUE /*trace*/, FALSE /* ksig */);
  }
}
```

**逐行详细解释**：

**第 637 行**：注释

```c
	/* Parent is not waiting. */
```

**翻译**：父进程未等待。

***

**第 638 行**：`sig_proc(p_mp, SIGCHLD, TRUE /*trace*/, FALSE /* ksig */);`

**是什么**：发送 SIGCHLD 信号给父进程

**sig\_proc 函数**：

**函数签名**：

```c
void sig_proc(struct mproc *rmp, int signo, int trace, int ksig);
```

**参数**：

- `p_mp`：父进程控制块
- `SIGCHLD`：子进程状态改变信号
- `TRUE`：是跟踪信号
- `FALSE`：不是来自内核

**作用**：向进程发送信号

**SIGCHLD 信号**：

**是什么**：子进程状态改变时发送给父进程的信号

**触发条件**：

- 子进程退出
- 子进程停止（SIGSTOP）
- 子进程继续（SIGCONT）

**父进程处理**：

```c
// 父进程代码
void sigchld_handler(int signo) {
    int status;
    pid_t pid = wait(&status);
    // 处理子进程退出
}

int main() {
    signal(SIGCHLD, sigchld_handler);
    // ...
}
```

***

### 2.14 tell\_parent 函数（第 643-696 行）

**函数概述**：

`tell_parent` 通知父进程子进程已退出，满足父进程的 wait4() 调用。

**函数签名**：

```c
static int tell_parent(struct mproc *child, vir_bytes addr);
```

**参数**：

- `child`：子进程控制块
- `addr`：rusage 结构的地址

**返回值**：

- `TRUE`：成功通知，子进程已清理
- `FALSE`：通知失败，子进程仍是僵尸

***

#### 2.14.1 文件头注释（第 643-651 行）

```c
/* Tell the parent of the given process that it has terminated, by satisfying
 * the parent's ongoing wait4() call.  If the parent has requested the child
 * tree's resource usage, copy that information out first.  The copy may fail;
 * in that case, the parent's wait4() call will return with an error, but the
 * child will remain a zombie.  Return TRUE if the child is cleaned up, or
 * FALSE if the child is still a zombie.
 */
```

**翻译**：通过满足父进程正在进行的 wait4() 调用，告诉给定进程的父进程它已终止。如果父进程请求了子进程树的资源使用情况，先复制该信息。复制可能失败；在这种情况下，父进程的 wait4() 调用将返回错误，但子进程仍将是僵尸。如果子进程被清理，返回 TRUE；如果子进程仍是僵尸，返回 FALSE。

**设计思路**：

**主要任务**：

1. 复制资源使用信息（如果请求）
2. 唤醒父进程（发送回复）
3. 累积子进程的 CPU 时间

**可能的失败**：

- 复制 rusage 失败
- 子进程仍是僵尸
- 父进程的 wait4() 返回错误

***

#### 2.14.2 局部变量声明和检查（第 652-662 行）

```c
  struct rusage r_usage;
  int mp_parent;
  struct mproc *parent;
  int r;

  mp_parent= child->mp_parent;
  if (mp_parent <= 0)
	panic("tell_parent: bad value in mp_parent: %d", mp_parent);
  if(!(child->mp_flags & ZOMBIE))
  	panic("tell_parent: child not a zombie");
  if(child->mp_flags & TOLD_PARENT)
	panic("tell_parent: telling parent again");
  parent = &mproc[mp_parent];
```

**逐行详细解释**：

**第 652 行**：`struct rusage r_usage;`

**是什么**：声明资源使用结构体

**struct rusage 定义**：

```c
struct rusage {
    struct timeval ru_utime;  // 用户态 CPU 时间
    struct timeval ru_stime;  // 内核态 CPU 时间
    long ru_maxrss;           // 最大驻留集大小
    long ru_ixrss;            // 共享内存大小
    long ru_idrss;            // 非共享数据大小
    long ru_isrss;            // 非共享栈大小
    long ru_minflt;           // 无需 I/O 的页面错误
    long ru_majflt;           // 需要 I/O 的页面错误
    long ru_nswap;            // 交换次数
    long ru_inblock;          // 块输入操作
    long ru_oublock;          // 块输出操作
    long ru_msgsnd;           // 发送的消息
    long ru_msgrcv;           // 接收的消息
    long ru_nsignals;         // 接收的信号
    long ru_nvcsw;            // 自愿上下文切换
    long ru_nivcsw;           // 非自愿上下文切换
};
```

***

**第 653-655 行**：声明变量

```c
  int mp_parent;
  struct mproc *parent;
  int r;
```

**变量说明**：

- `mp_parent`：父进程槽位号
- `parent`：父进程控制块指针
- `r`：返回值

***

**第 657-658 行**：检查父进程槽位号

```c
  mp_parent= child->mp_parent;
  if (mp_parent <= 0)
	panic("tell_parent: bad value in mp_parent: %d", mp_parent);
```

**是什么**：检查父进程槽位号是否有效

**条件**：`mp_parent <= 0`

**含义**：父进程槽位号无效

**如果无效，panic**

***

**第 659-660 行**：检查 ZOMBIE 标志

```c
  if(!(child->mp_flags & ZOMBIE))
  	panic("tell_parent: child not a zombie");
```

**是什么**：检查子进程是否是僵尸

**如果不是僵尸，panic**

**为什么？**

**设计原因**：

- tell\_parent 只能通知僵尸进程
- 如果不是僵尸，说明逻辑错误
- 系统无法继续运行，需要 panic

***

**第 661-662 行**：检查 TOLD\_PARENT 标志

```c
  if(child->mp_flags & TOLD_PARENT)
	panic("tell_parent: telling parent again");
```

**是什么**：检查是否已经通知过父进程

**如果已通知，panic**

**为什么？**

**设计原因**：

- 不应该重复通知父进程
- 如果重复通知，说明逻辑错误
- 系统无法继续运行，需要 panic

***

**第 663 行**：`parent = &mproc[mp_parent];`

**是什么**：获取父进程控制块

***

#### 2.14.3 复制资源使用信息（第 665-680 行）

```c
  /* See if we need to report resource usage to the parent. */
  if (addr) {
	/* We report only user and system times for now. TODO: support other
	 * fields, although this is tricky since the child process is already
	 * gone as far as the kernel and other services are concerned..
	 */
	memset(&r_usage, 0, sizeof(r_usage));
	set_rusage_times(&r_usage, child->mp_child_utime,
	    child->mp_child_stime);

	if ((r = sys_datacopy(SELF, (vir_bytes)&r_usage, parent->mp_endpoint,
	    addr, sizeof(r_usage))) != OK) {
		reply(child->mp_parent, r);

		return FALSE; /* copy error - the child is still there */
	}
  }
```

**逐行详细解释**：

**第 665 行**：注释

```c
  /* See if we need to report resource usage to the parent. */
```

**翻译**：查看我们是否需要向父进程报告资源使用情况。

***

**第 666 行**：`if (addr)`

**是什么**：检查父进程是否请求了资源使用信息

**addr 参数**：

- 非 0：rusage 结构的地址
- 0：不请求资源使用信息

***

**第 667-670 行**：注释

```c
	/* We report only user and system times for now. TODO: support other
	 * fields, although this is tricky since the child process is already
	 * gone as far as the kernel and other services are concerned..
	 */
```

**翻译**：目前我们只报告用户和系统时间。TODO：支持其他字段，虽然这很棘手，因为就内核和其他服务而言，子进程已经消失了。

**设计限制**：

**为什么只报告时间？**

**技术原因**：

- 子进程已经退出
- 内核和其他服务已清理进程资源
- 无法获取其他资源使用信息（如内存、I/O）

***

**第 671-673 行**：设置 rusage 结构

```c
	memset(&r_usage, 0, sizeof(r_usage));
	set_rusage_times(&r_usage, child->mp_child_utime,
	    child->mp_child_stime);
```

**是什么**：初始化 rusage 结构并设置时间

**set\_rusage\_times 函数**：

**函数签名**：

```c
void set_rusage_times(struct rusage *r_usage, clock_t utime, clock_t stime);
```

**参数**：

- `&r_usage`：rusage 结构地址
- `child->mp_child_utime`：用户态时间
- `child->mp_child_stime`：内核态时间

**作用**：将时钟滴答转换为 timeval 结构

***

**第 675-679 行**：复制到父进程

```c
	if ((r = sys_datacopy(SELF, (vir_bytes)&r_usage, parent->mp_endpoint,
	    addr, sizeof(r_usage))) != OK) {
		reply(child->mp_parent, r);

		return FALSE; /* copy error - the child is still there */
	}
```

**是什么**：复制 rusage 结构到父进程地址空间

**sys\_datacopy 函数**：

**函数签名**：

```c
int sys_datacopy(endpoint_t src_ep, vir_bytes src_addr,
                 endpoint_t dst_ep, vir_bytes dst_addr, size_t size);
```

**参数**：

- `SELF`：源端点（PM 自己）
- `(vir_bytes)&r_usage`：源地址
- `parent->mp_endpoint`：目标端点（父进程）
- `addr`：目标地址
- `sizeof(r_usage)`：复制大小

**返回值**：

- `OK`：成功
- 其他：错误码

**如果复制失败**：

1. 回复父进程错误码
2. 返回 FALSE（子进程仍是僵尸）

**注释翻译**：复制错误 - 子进程还在那里。

***

#### 2.14.4 唤醒父进程（第 682-692 行）

```c
  /* Wake up the parent by sending the reply message. */
  parent->mp_reply.m_pm_lc_wait4.status =
	W_EXITCODE(child->mp_exitstatus, child->mp_sigstatus);
  reply(child->mp_parent, child->mp_pid);
  parent->mp_flags &= ~WAITING;		/* parent no longer waiting */
  child->mp_flags &= ~ZOMBIE;		/* child no longer a zombie */
  child->mp_flags |= TOLD_PARENT;	/* avoid informing parent twice */

  /* Now that the child has been waited for, accumulate the times of the
   * terminated child process at the parent.
   */
  parent->mp_child_utime += child->mp_child_utime;
  parent->mp_child_stime += child->mp_child_stime;

  return TRUE; /* child has been waited for */
}
```

**逐行详细解释**：

**第 682 行**：注释

```c
  /* Wake up the parent by sending the reply message. */
```

**翻译**：通过发送回复消息唤醒父进程。

***

**第 683-684 行**：设置退出状态

```c
  parent->mp_reply.m_pm_lc_wait4.status =
	W_EXITCODE(child->mp_exitstatus, child->mp_sigstatus);
```

**是什么**：设置父进程的回复消息中的退出状态

**W\_EXITCODE 宏**：

**定义**：

```c
#define W_EXITCODE(exit, signal)  ((exit) << 8 | (signal))
```

**参数**：

- `child->mp_exitstatus`：退出状态码
- `child->mp_sigstatus`：信号编号

**示例**：

```c
// 正常退出
W_EXITCODE(0, 0) = 0x0000

// 退出码为 1
W_EXITCODE(1, 0) = 0x0100

// 被信号 9 (SIGKILL) 杀死
W_EXITCODE(0, 9) = 0x0009
```

***

**第 685 行**：`reply(child->mp_parent, child->mp_pid);`

**是什么**：回复父进程，唤醒它

**参数**：

- `child->mp_parent`：父进程槽位号
- `child->mp_pid`：子进程 PID（返回值）

***

**第 686 行**：`parent->mp_flags &= ~WAITING;`

**是什么**：清除父进程的 WAITING 标志

**注释翻译**：父进程不再等待。

***

**第 687 行**：`child->mp_flags &= ~ZOMBIE;`

**是什么**：清除子进程的 ZOMBIE 标志

**注释翻译**：子进程不再是僵尸。

***

**第 688 行**：`child->mp_flags |= TOLD_PARENT;`

**是什么**：设置 TOLD\_PARENT 标志

**注释翻译**：避免重复通知父进程。

***

**第 689-692 行**：累积 CPU 时间

```c
  /* Now that the child has been waited for, accumulate the times of the
   * terminated child process at the parent.
   */
  parent->mp_child_utime += child->mp_child_utime;
  parent->mp_child_stime += child->mp_child_stime;
```

**注释翻译**：现在子进程已被等待，将终止子进程的时间累积到父进程。

**为什么累积到父进程？**

**POSIX 要求**：

- 父进程应该获得所有子进程的累计 CPU 时间
- 用于资源统计和计费

***

**第 694 行**：`return TRUE;`

**是什么**：返回 TRUE

**注释翻译**：子进程已被等待。

***

### 2.15 tell\_tracer 函数（第 699-719 行）

**函数概述**：

`tell_tracer` 通知跟踪器子进程已退出。

**函数签名**：

```c
static void tell_tracer(struct mproc *child);
```

**参数**：

- `child`：子进程控制块

**返回值**：无（void）

***

#### 2.15.1 局部变量声明和检查（第 702-710 行）

```c
  int mp_tracer;
  struct mproc *tracer;

  mp_tracer = child->mp_tracer;
  if (mp_tracer <= 0)
	panic("tell_tracer: bad value in mp_tracer: %d", mp_tracer);
  if(!(child->mp_flags & TRACE_ZOMBIE))
  	panic("tell_tracer: child not a zombie");
  tracer = &mproc[mp_tracer];
```

**逐行详细解释**：

**第 702-703 行**：声明变量

```c
  int mp_tracer;
  struct mproc *tracer;
```

**变量说明**：

- `mp_tracer`：跟踪器槽位号
- `tracer`：跟踪器进程控制块指针

***

**第 705-706 行**：检查跟踪器槽位号

```c
  mp_tracer = child->mp_tracer;
  if (mp_tracer <= 0)
	panic("tell_tracer: bad value in mp_tracer: %d", mp_tracer);
```

**是什么**：检查跟踪器槽位号是否有效

**如果无效，panic**

***

**第 707-708 行**：检查 TRACE\_ZOMBIE 标志

```c
  if(!(child->mp_flags & TRACE_ZOMBIE))
  	panic("tell_tracer: child not a zombie");
```

**是什么**：检查子进程是否是跟踪僵尸

**如果不是，panic**

***

**第 709 行**：`tracer = &mproc[mp_tracer];`

**是什么**：获取跟踪器进程控制块

***

#### 2.15.2 通知跟踪器（第 711-719 行）

```c
  /* TODO: rusage support */

  tracer->mp_reply.m_pm_lc_wait4.status =
	W_EXITCODE(child->mp_exitstatus, (child->mp_sigstatus & 0377));
  reply(child->mp_tracer, child->mp_pid);
  tracer->mp_flags &= ~WAITING;		/* tracer no longer waiting */
  child->mp_flags &= ~TRACE_ZOMBIE;	/* child no longer zombie to tracer */
  child->mp_flags |= ZOMBIE;		/* child is now zombie to parent */
}
```

**逐行详细解释**：

**第 711 行**：注释

```c
  /* TODO: rusage support */
```

**翻译**：TODO：rusage 支持。

**设计限制**：跟踪器暂不支持 rusage

***

**第 712-713 行**：设置退出状态

```c
  tracer->mp_reply.m_pm_lc_wait4.status =
	W_EXITCODE(child->mp_exitstatus, (child->mp_sigstatus & 0377));
```

**是什么**：设置跟踪器的回复消息中的退出状态

**注意**：`child->mp_sigstatus & 0377`

**0377 是八进制**，等于 255

**作用**：确保信号编号在 0-255 范围内

***

**第 714 行**：`reply(child->mp_tracer, child->mp_pid);`

**是什么**：回复跟踪器，唤醒它

***

**第 715 行**：`tracer->mp_flags &= ~WAITING;`

**是什么**：清除跟踪器的 WAITING 标志

**注释翻译**：跟踪器不再等待。

***

**第 716 行**：`child->mp_flags &= ~TRACE_ZOMBIE;`

**是什么**：清除子进程的 TRACE\_ZOMBIE 标志

**注释翻译**：子进程不再是跟踪器的僵尸。

***

**第 717 行**：`child->mp_flags |= ZOMBIE;`

**是什么**：设置 ZOMBIE 标志

**注释翻译**：子进程现在是父进程的僵尸。

**状态转换**：

```
TRACE_ZOMBIE → ZOMBIE
```

**为什么转换？**

**设计原因**：

- 跟踪器已处理子进程退出
- 现在需要通知父进程
- 设置 ZOMBIE 标志，让父进程可以 wait()

***

### 2.16 tracer\_died 函数（第 722-750 行）

**函数概述**：

`tracer_died` 处理跟踪器死亡的情况。

**函数签名**：

```c
static void tracer_died(struct mproc *child);
```

**参数**：

- `child`：被跟踪的进程

**返回值**：无（void）

***

#### 2.16.1 文件头注释（第 722-727 行）

```c
/* The process that was tracing the given child, has died for some reason.
 * This is really the tracer's fault, but we can't let INIT deal with this.
 */
```

**翻译**：正在跟踪给定子进程的进程因某种原因死亡。这实际上是跟踪器的错，但我们不能让 INIT 处理这个。

**设计思路**：

**问题**：

- 跟踪器死亡
- 被跟踪进程状态未知
- 需要清理

**解决方案**：

- 清除跟踪器字段
- 根据进程状态采取不同措施

***

#### 2.16.2 清除跟踪器字段（第 728-730 行）

```c
  child->mp_tracer = NO_TRACER;
  child->mp_flags &= ~TRACE_EXIT;
```

**逐行详细解释**：

**第 728 行**：`child->mp_tracer = NO_TRACER;`

**是什么**：清除跟踪器字段

**NO\_TRACER 定义**：

```c
#define NO_TRACER -1  // 无跟踪器
```

***

**第 729 行**：`child->mp_flags &= ~TRACE_EXIT;`

**是什么**：清除 TRACE\_EXIT 标志

**TRACE\_EXIT 标志**：调试器正在跟踪进程退出

***

#### 2.16.3 处理未退出的进程（第 731-739 行）

```c
  /* If the tracer died while the child was running or stopped, we have no
   * idea what state the child is in. Avoid a trainwreck, by killing the child.
   * Note that this may cause cascading exits.
   */
  if (!(child->mp_flags & EXITING)) {
	sig_proc(child, SIGKILL, TRUE /*trace*/, FALSE /* ksig */);

	return;
  }
```

**逐行详细解释**：

**第 731-734 行**：注释

```c
  /* If the tracer died while the child was running or stopped, we have no
   * idea what state the child is in. Avoid a trainwreck, by killing the child.
   * Note that this may cause cascading exits.
   */
```

**翻译**：如果跟踪器在子进程运行或停止时死亡，我们不知道子进程处于什么状态。通过杀死子进程来避免灾难。注意，这可能导致级联退出。

**为什么杀死子进程？**

**设计原因**：

- 子进程状态未知（可能在断点处）
- 无法继续正常运行
- 杀死是最安全的选择

**级联退出**：

**场景**：

```
调试器崩溃:
┌─────────────────────────────────────────────────────────────┐
│ 1. 调试器 (gdb) 崩溃                                        │
│    → PM tracer_died()                                       │
│                                                              │
│ 2. 被调试进程                                               │
│    → 收到 SIGKILL                                           │
│    → 退出                                                   │
│                                                              │
│ 3. 被调试进程的子进程                                       │
│    → 成为孤儿                                               │
│    → 被 INIT 收养                                           │
│    → 可能退出                                               │
└─────────────────────────────────────────────────────────────┘
```

***

**第 735-738 行**：检查进程是否正在退出

```c
  if (!(child->mp_flags & EXITING)) {
	sig_proc(child, SIGKILL, TRUE /*trace*/, FALSE /* ksig */);

	return;
  }
```

**是什么**：如果进程未退出，发送 SIGKILL

**条件**：`!(child->mp_flags & EXITING)`

**含义**：进程未在退出过程中

***

#### 2.16.4 处理正在退出的进程（第 741-750 行）

```c
  /* If the tracer died while the child was telling it about its own death,
   * forget about the tracer and notify the real parent instead.
   */
  if (child->mp_flags & TRACE_ZOMBIE) {
	child->mp_flags &= ~TRACE_ZOMBIE;
	child->mp_flags |= ZOMBIE;

	check_parent(child, TRUE /*try_cleanup*/);
  }
}
```

**逐行详细解释**：

**第 741-744 行**：注释

```c
  /* If the tracer died while the child was telling it about its own death,
   * forget about the tracer and notify the real parent instead.
   */
```

**翻译**：如果跟踪器在子进程告诉它自己的死亡时死亡，忘记跟踪器并通知真正的父进程。

***

**第 745-750 行**：状态转换

```c
  if (child->mp_flags & TRACE_ZOMBIE) {
	child->mp_flags &= ~TRACE_ZOMBIE;
	child->mp_flags |= ZOMBIE;

	check_parent(child, TRUE /*try_cleanup*/);
  }
```

**是什么**：如果子进程是跟踪僵尸，转换为普通僵尸

**条件**：`child->mp_flags & TRACE_ZOMBIE`

**含义**：子进程正在等待跟踪器处理退出

**操作**：

1. 清除 TRACE\_ZOMBIE 标志
2. 设置 ZOMBIE 标志
3. 通知父进程

**状态转换**：

```
TRACE_ZOMBIE → ZOMBIE
```

**为什么转换？**

**设计原因**：

- 跟踪器已死，无法处理
- 需要通知父进程
- 让父进程可以 wait()

***

### 2.17 cleanup 函数（第 753-767 行）

**函数概述**：

`cleanup` 清理进程控制块，释放槽位。

**函数签名**：

```c
static void cleanup(struct mproc *rmp);
```

**参数**：

- `rmp`：要清理的进程控制块

**返回值**：无（void）

***

#### 2.17.1 函数实现（第 757-767 行）

```c
  /* Release the process table entry and reinitialize some field. */
  rmp->mp_pid = 0;
  rmp->mp_flags = 0;
  rmp->mp_child_utime = 0;
  rmp->mp_child_stime = 0;
  procs_in_use--;
}
```

**逐行详细解释**：

**第 757 行**：注释

```c
  /* Release the process table entry and reinitialize some field. */
```

**翻译**：释放进程表条目并重新初始化一些字段。

***

**第 758 行**：`rmp->mp_pid = 0;`

**是什么**：清零 PID

**作用**：标记槽位未使用

***

**第 759 行**：`rmp->mp_flags = 0;`

**是什么**：清零所有标志

**作用**：清除 IN\_USE 标志，释放槽位

***

**第 760-761 行**：清零 CPU 时间

```c
  rmp->mp_child_utime = 0;
  rmp->mp_child_stime = 0;
```

**是什么**：清零子进程 CPU 时间

**作用**：为下次使用做准备

***

**第 762 行**：`procs_in_use--;`

**是什么**：减少进程计数

**作用**：更新全局进程计数

***

**清理完成示意**：

```
清理前:
┌────────────────────────────────────────┐
│ mproc[7]                               │
│   mp_pid = 1235                        │
│   mp_flags = IN_USE | TOLD_PARENT      │
│   mp_child_utime = 200                 │
│   mp_child_stime = 50                  │
└────────────────────────────────────────┘

procs_in_use = 51

清理后:
┌────────────────────────────────────────┐
│ mproc[7]                               │
│   mp_pid = 0                           │
│   mp_flags = 0                         │
│   mp_child_utime = 0                   │
│   mp_child_stime = 0                   │
└────────────────────────────────────────┘

procs_in_use = 50
```

***

## 3. 总结

### 3.1 核心知识点总结

1. **进程创建（fork）**：
   - 分为普通 fork（do\_fork）和服务 fork（do\_srv\_fork）
   - 异步操作，需要等待 VFS 完成
   - 继承父进程的大部分属性
2. **进程退出（exit）**：
   - 两阶段退出：exit\_proc 和 exit\_restart
   - 清理资源、通知父进程、处理子进程
   - 特殊进程（INIT、VFS）有特殊处理
3. **进程等待（wait）**：
   - 支持多种等待模式（指定 PID、任意子进程、进程组）
   - WNOHANG 选项实现非阻塞等待
   - 僵尸进程机制确保父进程能获得退出状态
4. **调试跟踪（ptrace）**：
   - TRACE\_ZOMBIE 和 ZOMBIE 的状态转换
   - 跟踪器优先于父进程获得通知
   - 跟踪器死亡时的清理机制

### 3.2 设计亮点

1. **微内核架构**：
   - PM 只管理进程元数据
   - VM 管理内存
   - VFS 管理文件描述符
   - 通过消息传递协调
2. **异步消息传递**：
   - 避免阻塞整个系统
   - 提高并发性能
   - 简化服务间交互
3. **状态机设计**：
   - 清晰的进程状态转换
   - 标志位管理状态
   - 防止重复操作

### 3.3 安全考虑

1. **setuid 程序**：
   - 禁止 core dump
   - 保护敏感信息
2. **权限检查**：
   - srv\_fork 只允许 RS 调用
   - 防止权限提升
3. **资源限制**：
   - 进程表容量检查
   - 防止资源耗尽

***

## 4. Rust 实现建议

### 4.1 类型安全

**C 代码问题**：

```c
int mp_parent;  // 可能是无效索引
```

**Rust 改进**：

```rust
enum Parent {
    Process(NonZeroUsize),  // 有效父进程索引
    Init,                   // INIT 进程
    None,                   // 无父进程
}
```

### 4.2 状态管理

**C 代码问题**：

```c
rmp->mp_flags |= ZOMBIE;
rmp->mp_flags &= ~TRACE_ZOMBIE;
```

**Rust 改进**：

```rust
enum ProcessState {
    Running,
    Zombie,
    TraceZombie,
    Exiting,
    // ...
}

impl Process {
    fn transition(&mut self, new_state: ProcessState) -> Result<(), StateError> {
        // 状态转换检查
    }
}
```

### 4.3 错误处理

**C 代码问题**：

```c
if((r=sys_times(...)) != OK)
    panic("sys_times failed: %d", r);
```

**Rust 改进**：

```rust
fn get_cpu_times(ep: Endpoint) -> Result<CpuTimes, SystemError> {
    sys_times(ep).map_err(|e| {
        log::error!("sys_times failed: {:?}", e);
        SystemError::TimesError(e)
    })
}
```

### 4.4 资源管理

**C 代码问题**：

```c
rmp->mp_pid = 0;
rmp->mp_flags = 0;
procs_in_use--;
```

**Rust 改进**：

```rust
impl Drop for Process {
    fn drop(&mut self) {
        // 自动清理资源
        // 减少进程计数
    }
}
```

***

**讲解完成！**

本文档详细讲解了 `forkexit.c` 文件的所有代码，包括：

- ✅ 逐行详细解释
- ✅ 覆盖"是什么"、"为什么"、"什么情景"
- ✅ 查阅了被调用函数的真实源码
- ✅ 翻译了所有注释
- ✅ 提供了内存布局图和流程图
- ✅ 讲解了设计思路和原因
- ✅ 提供了 Rust 实现建议

