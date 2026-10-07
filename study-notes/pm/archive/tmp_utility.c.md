# servers/pm/utility.c 逐行讲解

> **文件路径**: `minix3/minix/servers/pm/utility.c`
> **核心功能**: PM 工具函数集

---

## 文件概述

这个文件实现了 PM（进程管理器）模块的通用工具函数，包括 PID 分配、进程查找、优先级转换、端点验证、VFS 通信和时间统计等功能。

**核心概念**:
- **PID 分配**: 唯一进程标识符的生成与管理
- **进程查找**: 通过 PID 定位进程控制块
- **优先级映射**: nice 值到调度队列的转换
- **异步通信**: 非阻塞 IPC 消息发送

---

## 头文件注释

```c
/* This file contains some utility routines for PM.
 *
 * The entry points are:
 *   get_free_pid:	get a free process or group id
 *   find_param:	look up a boot monitor parameter
 *   find_proc:		return process pointer from pid number
 *   nice_to_priority	convert nice level to priority queue
 *   pm_isokendpt:	check the validity of an endpoint
 *   tell_vfs:		send a request to VFS on behalf of a process
 *   set_rusage_times:	store user and system times in rusage structure
 */
```

**翻译**:
> 本文件包含 PM 的一些工具例程。
> 
> 入口点包括：
> - `get_free_pid`: 获取一个空闲的进程或组 ID
> - `find_param`: 查找引导监视器参数
> - `find_proc`: 根据 PID 号返回进程指针
> - `nice_to_priority`: 将 nice 级别转换为优先级队列
> - `pm_isokendpt`: 检查端点的有效性
> - `tell_vfs`: 代表进程向 VFS 发送请求
> - `set_rusage_times`: 将用户时间和系统时间存入 rusage 结构

**设计原因**: 这个注释块遵循 Minix 的文档风格，列出所有公开函数及其功能，方便开发者快速了解文件提供的功能接口。

---

## 头文件包含

```c
#include "pm.h"
#include <sys/resource.h>
#include <sys/stat.h>
#include <minix/callnr.h>
#include <minix/com.h>
#include <minix/endpoint.h>
#include <fcntl.h>
#include <signal.h>		/* needed only because mproc.h needs it */
#include "mproc.h"
```

**逐行讲解**:

| 行号 | 头文件 | 作用 | 关键内容 |
|------|--------|------|----------|
| 1 | `"pm.h"` | PM 模块主头文件 | 包含 PM 需要的所有公共定义 |
| 2 | `<sys/resource.h>` | 资源操作定义 | `PRIO_MIN`(-20), `PRIO_MAX`(20), `struct rusage` |
| 3 | `<sys/stat.h>` | 文件状态定义 | `mode_t` 类型，文件权限宏 |
| 4 | `<minix/callnr.h>` | 系统调用号定义 | `PM_BASE`(0x000), `NR_PM_CALLS`(48) |
| 5 | `<minix/com.h>` | 通信相关定义 | 任务号、进程号常量 |
| 6 | `<minix/endpoint.h>` | 端点类型定义 | `endpoint_t`, `_ENDPOINT_P()` 宏 |
| 7 | `<fcntl.h>` | 文件控制定义 | `O_RDONLY`, `O_WRONLY` 等打开标志 |
| 8 | `<signal.h>` | 信号定义 | `sigset_t`，信号常量（mproc.h 需要） |
| 9 | `"mproc.h"` | 进程控制块定义 | `struct mproc`, `NR_PROCS`, 进程标志位 |

**内存布局**: 这些头文件在编译时被展开，不会占用运行时内存。

---

```c
#include <minix/config.h>
#include <minix/timers.h>
#include <machine/archtypes.h>
#include "kernel/const.h"
#include "kernel/config.h"
#include "kernel/type.h"
#include "kernel/proc.h"
```

**逐行讲解**:

| 行号 | 头文件 | 作用 | 关键内容 |
|------|--------|------|----------|
| 1 | `<minix/config.h>` | Minix 配置定义 | `NR_SCHED_QUEUES`(16), `MAX_USER_Q`(0), `MIN_USER_Q`(15) |
| 2 | `<minix/timers.h>` | 定时器定义 | `clock_t` 类型，定时器操作 |
| 3 | `<machine/archtypes.h>` | 架构相关类型 | 机器相关的类型定义 |
| 4 | `"kernel/const.h"` | 内核常量 | 内核使用的常量定义 |
| 5 | `"kernel/config.h"` | 内核配置 | 内核配置选项 |
| 6 | `"kernel/type.h"` | 内核类型 | 内核使用的类型定义 |
| 7 | `"kernel/proc.h"` | 内核进程结构 | `struct proc`，内核进程表 |

**设计原因**: PM 需要访问内核的数据结构定义来与内核交互，这些头文件确保了类型和常量的一致性。

---

## get_free_pid 函数

```c
/*===========================================================================*
 *				get_free_pid				     *
 *===========================================================================*/
pid_t get_free_pid()
{
  static pid_t next_pid = INIT_PID + 1;		/* next pid to be assigned */
  register struct mproc *rmp;			/* check process table */
  int t;					/* zero if pid still free */
```

**逐行讲解**:

**第 33 行**: `pid_t get_free_pid()`
- **返回类型**: `pid_t`（有符号 32 位整数，定义在 `<sys/types.h>` 中）
- **函数功能**: 分配一个未被使用的 PID

**第 35 行**: `static pid_t next_pid = INIT_PID + 1;`
- **存储位置**: 静态数据段（.data 或 .bss），不在栈上
- **字节大小**: 4 字节（32 位系统）或 8 字节（64 位系统）
- **初始值**: `INIT_PID + 1 = 1 + 1 = 2`（INIT 进程的 PID 是 1，新进程从 2 开始）
- **生命周期**: 程序启动时初始化，整个运行期间保持值
- **设计原因**: 使用 `static` 保持 PID 分配的连续性，避免每次调用都从 2 开始搜索

**内存图示**:
```
静态数据段:
┌─────────────────────────────────────┐
│ next_pid: [2] [3] [4] ... [30000]   │  ← 随着进程创建递增
└─────────────────────────────────────┘
地址: 0x0804A000 (示例)
```

**第 36 行**: `register struct mproc *rmp;`
- **存储位置**: 建议编译器使用寄存器存储（现代编译器通常忽略）
- **字节大小**: 4 字节（32 位指针）或 8 字节（64 位指针）
- **用途**: 遍历进程表时的循环变量

**第 37 行**: `int t;`
- **存储位置**: 栈上
- **字节大小**: 4 字节
- **用途**: 标志变量，`t = 0` 表示 PID 空闲，`t = 1` 表示 PID 已被占用

---

```c
  /* Find a free pid for the child and put it in the table. */
  do {
	t = 0;
	next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1);
	for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
		if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid) {
			t = 1;
			break;
		}
  } while (t);					/* 't' = 0 means pid free */
  return(next_pid);
}
```

**逐行讲解**:

**第 40 行**: 注释 `/* Find a free pid for the child and put it in the table. */`
- **翻译**: 为子进程找到一个空闲的 PID 并放入表中

**第 41-48 行**: `do { ... } while (t);`
- **循环逻辑**: 持续尝试分配 PID，直到找到一个空闲的

**第 42 行**: `t = 0;`
- 每次循环开始时假设当前 PID 是空闲的

**第 43 行**: `next_pid = (next_pid < NR_PIDS ? next_pid + 1 : INIT_PID + 1);`
- **三元运算符**: 条件 ? 真值 : 假值
- **逻辑**: 
  - 如果 `next_pid < NR_PIDS`(30000)，则递增 `next_pid`
  - 否则，回绕到 `INIT_PID + 1 = 2`
- **设计原因**: PID 范围是 2 到 29999，循环使用

**第 44 行**: `for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)`
- **循环范围**: 遍历整个进程表（`NR_PROCS` 个条目）
- **内存图示**:
```
进程表 mproc[NR_PROCS]:
┌────────┬────────┬────────┬─────┬────────┐
│ mproc[0]│ mproc[1]│ mproc[2]│ ... │ mproc[NR_PROCS-1]│
└────────┴────────┴────────┴─────┴────────┘
    ↑
   rmp 从这里开始
```

**第 45 行**: `if (rmp->mp_pid == next_pid || rmp->mp_procgrp == next_pid)`
- **检查条件**: 
  - `rmp->mp_pid == next_pid`: PID 是否已被某个进程使用
  - `rmp->mp_procgrp == next_pid`: PID 是否已被某个进程组使用
- **设计原因**: PID 和进程组 ID 必须唯一，不能冲突

**第 46-47 行**: `t = 1; break;`
- 如果 PID 已被占用，设置 `t = 1` 并跳出内层循环

**第 49 行**: `while (t);`
- **注释翻译**: `'t' = 0 means pid free`（t = 0 表示 PID 空闲）
- 如果 `t = 1`（PID 已占用），继续外层循环

**第 50 行**: `return(next_pid);`
- 返回找到的空闲 PID

**算法流程图**:
```
开始
  │
  ▼
next_pid++ (或回绕到 2)
  │
  ▼
遍历进程表检查冲突 ──────┐
  │                      │
  │ 发现冲突             │ 无冲突
  ▼                      │
t = 1, 继续循环 ◄────────┘
  │
  ▼
返回 next_pid
```

**设计原因**:
1. **线性搜索**: 简单可靠，适合进程数量有限的场景
2. **循环分配**: 避免短时间内重用 PID，防止旧信号误发
3. **双重检查**: 同时检查 PID 和进程组 ID，确保唯一性

---

## find_param 函数

```c
/*===========================================================================*
 *				find_param				     *
 *===========================================================================*/
char *
find_param(const char *name)
{
  register const char *namep;
  register char *envp;

  for (envp = (char *) monitor_params; *envp != 0;) {
	for (namep = name; *namep != 0 && *namep == *envp; namep++, envp++)
		;
	if (*namep == '\0' && *envp == '=')
		return(envp + 1);
	while (*envp++ != 0)
		;
  }
  return(NULL);
}
```

**逐行讲解**:

**第 56 行**: `char *find_param(const char *name)`
- **参数**: `name` - 要查找的参数名（如 "rootdev", "bootdev"）
- **返回值**: 找到的参数值字符串指针，或 `NULL`

**第 58 行**: `register const char *namep;`
- 用于遍历输入的参数名

**第 59 行**: `register char *envp;`
- 用于遍历 `monitor_params` 缓冲区

**第 61 行**: `for (envp = (char *) monitor_params; *envp != 0;)`
- **`monitor_params`**: 全局字符数组，存储引导监视器传递的参数
- **定义位置**: `minix3/minix/servers/pm/glo.h:10`
- **格式**: 类似环境变量，`name=value\0name2=value2\0\0`
- **内存图示**:
```
monitor_params 缓冲区:
┌─────────────────────────────────────────────────┐
│ rootdev=c0d0p0s0\0bootdev=c0d0p0\0\0            │
└─────────────────────────────────────────────────┘
 ↑                                               ^
 envp 开始                                       结束标记(连续两个\0)
```

**第 62 行**: `for (namep = name; *namep != 0 && *namep == *envp; namep++, envp++)`
- **内层循环**: 逐字符比较参数名
- **条件**: 
  - `*namep != 0`: 参数名还没结束
  - `*namep == *envp`: 当前字符匹配

**第 63 行**: `;`（空语句）
- 内层循环体为空，所有工作都在循环条件中完成

**第 64 行**: `if (*namep == '\0' && *envp == '=')`
- **匹配成功条件**: 
  - `*namep == '\0'`: 参数名完全匹配
  - `*envp == '='`: 后面跟着等号

**第 65 行**: `return(envp + 1);`
- 返回等号后面的值字符串

**第 66-67 行**: `while (*envp++ != 0) ;`
- 跳过当前参数，移动到下一个参数
- `envp++` 是后置自增，先检查再移动

**第 69 行**: `return(NULL);`
- 未找到参数，返回 `NULL`

**应用场景**: PM 启动时查找引导参数，如：
- `rootdev`: 根设备
- `bootdev`: 引导设备
- `hz`: 时钟频率

---

## find_proc 函数

```c
/*===========================================================================*
 *				find_proc  				     *
 *===========================================================================*/
struct mproc *find_proc(lpid)
pid_t lpid;
{
  register struct mproc *rmp;

  for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)
	if ((rmp->mp_flags & IN_USE) && rmp->mp_pid == lpid)
		return(rmp);

  return(NULL);
}
```

**逐行讲解**:

**第 75-76 行**: `struct mproc *find_proc(lpid) pid_t lpid;`
- **参数**: `lpid` - 要查找的进程 ID
- **返回值**: 指向 `mproc` 结构的指针，或 `NULL`
- **旧式语法**: 这是 K&R C 风格的函数声明，现代 C 推荐使用 `struct mproc *find_proc(pid_t lpid)`

**第 78 行**: `register struct mproc *rmp;`
- 循环变量，遍历进程表

**第 80 行**: `for (rmp = &mproc[0]; rmp < &mproc[NR_PROCS]; rmp++)`
- 遍历整个进程表

**第 81 行**: `if ((rmp->mp_flags & IN_USE) && rmp->mp_pid == lpid)`
- **条件 1**: `rmp->mp_flags & IN_USE` - 进程槽位正在使用
- **条件 2**: `rmp->mp_pid == lpid` - PID 匹配
- **`IN_USE` 标志**: 表示进程槽位已被分配

**第 82 行**: `return(rmp);`
- 找到匹配进程，返回指针

**第 84 行**: `return(NULL);`
- 未找到，返回 `NULL`

**设计原因**: 
- **线性搜索**: 进程表大小固定且不大（`NR_PROCS` 通常为 64-256），线性搜索效率可接受
- **IN_USE 检查**: 跳过空闲槽位，避免返回无效数据

---

## nice_to_priority 函数

```c
/*===========================================================================*
 *				nice_to_priority			     *
 *===========================================================================*/
int nice_to_priority(int nice, unsigned* new_q)
{
	if (nice < PRIO_MIN || nice > PRIO_MAX) return(EINVAL);

	*new_q = MAX_USER_Q + (nice-PRIO_MIN) * (MIN_USER_Q-MAX_USER_Q+1) /
	    (PRIO_MAX-PRIO_MIN+1);

	/* Neither of these should ever happen. */
	if ((signed) *new_q < MAX_USER_Q) *new_q = MAX_USER_Q;
	if (*new_q > MIN_USER_Q) *new_q = MIN_USER_Q;

	return (OK);
}
```

**逐行讲解**:

**第 90 行**: `int nice_to_priority(int nice, unsigned* new_q)`
- **参数**: 
  - `nice`: nice 值（-20 到 +20）
  - `new_q`: 输出参数，存储计算出的优先级队列号
- **返回值**: `OK`(0) 成功，`EINVAL`(22) 参数无效

**第 92 行**: `if (nice < PRIO_MIN || nice > PRIO_MAX) return(EINVAL);`
- **范围检查**: `PRIO_MIN = -20`, `PRIO_MAX = 20`
- **错误码**: `EINVAL` 表示无效参数

**第 94-95 行**: 优先级计算公式
```c
*new_q = MAX_USER_Q + (nice-PRIO_MIN) * (MIN_USER_Q-MAX_USER_Q+1) /
    (PRIO_MAX-PRIO_MIN+1);
```

**公式解析**:
- `MAX_USER_Q = 0`（最高优先级用户队列）
- `MIN_USER_Q = 15`（最低优先级用户队列，`NR_SCHED_QUEUES - 1`）
- `PRIO_MIN = -20`, `PRIO_MAX = 20`
- 公式展开: `new_q = 0 + (nice - (-20)) * (15 - 0 + 1) / (20 - (-20) + 1)`
- 简化: `new_q = (nice + 20) * 16 / 41`

**映射关系表**:
| nice 值 | 计算结果 | 优先级队列 |
|---------|----------|------------|
| -20 (最高) | 0 | MAX_USER_Q (最高) |
| 0 (默认) | 7-8 | 中等 |
| +20 (最低) | 15 | MIN_USER_Q (最低) |

**第 97 行**: 注释 `/* Neither of these should ever happen. */`
- **翻译**: 这两种情况都不应该发生

**第 98-99 行**: 边界保护
```c
if ((signed) *new_q < MAX_USER_Q) *new_q = MAX_USER_Q;
if (*new_q > MIN_USER_Q) *new_q = MIN_USER_Q;
```
- 防止整数溢出或计算误差导致越界

**设计原因**:
1. **线性映射**: nice 值与优先级队列线性对应
2. **反向关系**: nice 值越大（越"谦让"），优先级越低
3. **边界保护**: 确保结果在有效范围内

---

## pm_isokendpt 函数

```c
/*===========================================================================*
 *				pm_isokendpt			 	     *
 *===========================================================================*/
int pm_isokendpt(int endpoint, int *proc)
{
	*proc = _ENDPOINT_P(endpoint);
	if (*proc < 0 || *proc >= NR_PROCS)
		return EINVAL;
	if (endpoint != mproc[*proc].mp_endpoint)
		return EDEADEPT;
	if (!(mproc[*proc].mp_flags & IN_USE))
		return EDEADEPT;
	return OK;
}
```

**逐行讲解**:

**第 105 行**: `int pm_isokendpt(int endpoint, int *proc)`
- **参数**: 
  - `endpoint`: 要验证的端点号
  - `proc`: 输出参数，存储进程表索引
- **返回值**: `OK` 有效，`EINVAL` 无效索引，`EDEADEPT` 端点已失效

**第 107 行**: `*proc = _ENDPOINT_P(endpoint);`
- **`_ENDPOINT_P` 宏**: 从端点号提取进程表索引
- **端点号结构**: `[生成号:16位][进程索引:16位]`
- **示例**: 端点号 `0x00010002` → 进程索引 `2`

**第 108-109 行**: `if (*proc < 0 || *proc >= NR_PROCS) return EINVAL;`
- 检查索引是否在有效范围内

**第 110-111 行**: `if (endpoint != mproc[*proc].mp_endpoint) return EDEADEPT;`
- **端点号匹配检查**: 进程可能已退出并被新进程重用槽位
- **`EDEADEPT`**: "dead endpoint"，端点已失效

**第 112-113 行**: `if (!(mproc[*proc].mp_flags & IN_USE)) return EDEADEPT;`
- 检查进程槽位是否正在使用

**第 114 行**: `return OK;`
- 所有检查通过，端点有效

**设计原因**:
1. **端点号验证**: Minix 使用端点号而非 PID 进行 IPC，需要验证其有效性
2. **生成号机制**: 端点号包含生成号，防止向已退出进程发送消息
3. **EDEADEPT**: 专门错误码表示端点已失效，调用者可以区分"无效参数"和"进程已退出"

---

## tell_vfs 函数

```c
/*===========================================================================*
 *				tell_vfs			 	     *
 *===========================================================================*/
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

**逐行讲解**:

**第 120-122 行**: 函数声明（K&R 风格）
- **参数**: 
  - `rmp`: 指向进程控制块的指针
  - `m_ptr`: 要发送的消息指针

**第 124-125 行**: 注释 `/* Send a request to VFS, without blocking. */`
- **翻译**: 向 VFS 发送请求，不阻塞

**第 126 行**: `int r;`
- 存储函数返回值

**第 128-129 行**: `if (rmp->mp_flags & (VFS_CALL | EVENT_CALL)) panic(...)`
- **检查**: 进程是否已经在等待 VFS 或事件响应
- **`VFS_CALL`**: 标志位 `0x00400`，表示进程正在等待 VFS 响应
- **`EVENT_CALL`**: 标志位 `0x80000`，表示进程正在等待事件订阅者响应
- **panic**: 如果进程不空闲，触发系统崩溃（设计上不应该发生）

**第 131 行**: `r = asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY);`
- **`asynsend3`**: 异步发送消息函数
- **`VFS_PROC_NR`**: VFS 进程的端点号
- **`AMF_NOREPLY`**: 标志 `010`（八进制），表示不需要回复

**第 132-133 行**: `if (r != OK) panic("unable to send to VFS: %d", r);`
- 发送失败时触发 panic

**第 135 行**: `rmp->mp_flags |= VFS_CALL;`
- 设置 `VFS_CALL` 标志，表示进程正在等待 VFS 响应

**设计原因**:
1. **异步通信**: PM 不能阻塞等待 VFS，否则会阻塞整个系统
2. **状态检查**: 防止重复发送导致状态混乱
3. **panic 机制**: 系统服务器内部错误应立即崩溃，避免数据损坏

**内存图示**:
```
进程控制块 rmp:
┌─────────────────────────────────────┐
│ mp_flags: [VFS_CALL 位]             │ ← 设置此位
│ ...                                 │
└─────────────────────────────────────┘

消息 m_ptr:
┌─────────────────────────────────────┐
│ m_type: 消息类型                     │
│ m_source: 发送者端点                 │
│ ... 其他字段 ...                     │
└─────────────────────────────────────┘
         │
         │ asynsend3()
         ▼
┌─────────────────────────────────────┐
│ VFS 进程消息队列                     │
└─────────────────────────────────────┘
```

---

## set_rusage_times 函数

```c
/*===========================================================================*
 *				set_rusage_times		 	     *
 *===========================================================================*/
void
set_rusage_times(struct rusage * r_usage, clock_t user_time, clock_t sys_time)
{
	u64_t usec;

	usec = user_time * 1000000 / sys_hz();
	r_usage->ru_utime.tv_sec = usec / 1000000;
	r_usage->ru_utime.tv_usec = usec % 1000000;

	usec = sys_time * 1000000 / sys_hz();
	r_usage->ru_stime.tv_sec = usec / 1000000;
	r_usage->ru_stime.tv_usec = usec % 1000000;
}
```

**逐行讲解**:

**第 142 行**: `void set_rusage_times(struct rusage * r_usage, clock_t user_time, clock_t sys_time)`
- **参数**: 
  - `r_usage`: 资源使用统计结构指针
  - `user_time`: 用户态 CPU 时间（时钟滴答）
  - `sys_time`: 内核态 CPU 时间（时钟滴答）

**第 144 行**: `u64_t usec;`
- 64 位无符号整数，存储微秒值

**第 146 行**: `usec = user_time * 1000000 / sys_hz();`
- **转换**: 时钟滴答 → 微秒
- **`sys_hz()`**: 返回系统时钟频率（通常为 60 或 100 Hz）
- **公式**: `微秒 = 滴答数 * 1000000 / 频率`

**第 147 行**: `r_usage->ru_utime.tv_sec = usec / 1000000;`
- 计算秒数部分

**第 148 行**: `r_usage->ru_utime.tv_usec = usec % 1000000;`
- 计算微秒部分（0-999999）

**第 150-152 行**: 同样处理系统时间
```c
usec = sys_time * 1000000 / sys_hz();
r_usage->ru_stime.tv_sec = usec / 1000000;
r_usage->ru_stime.tv_usec = usec % 1000000;
```

**`struct rusage` 结构**:
```c
struct rusage {
    struct timeval ru_utime;  // 用户态时间
    struct timeval ru_stime;  // 系统态时间
    // ... 其他字段 ...
};

struct timeval {
    time_t tv_sec;   // 秒
    suseconds_t tv_usec;  // 微秒
};
```

**时间转换示例**:
```
假设: user_time = 600 滴答, sys_hz() = 100 Hz

usec = 600 * 1000000 / 100 = 6000000 微秒
tv_sec = 6000000 / 1000000 = 6 秒
tv_usec = 6000000 % 1000000 = 0 微秒

结果: ru_utime = {6 秒, 0 微秒}
```

**设计原因**:
1. **精度转换**: 内核使用时钟滴答计数，用户空间需要秒和微秒
2. **64 位计算**: 避免乘法溢出，`user_time * 1000000` 可能超过 32 位范围
3. **分离存储**: `timeval` 结构便于用户程序读取和处理

---

## 要点总结

1. **PID 分配**: 使用静态变量跟踪下一个候选 PID，循环搜索确保唯一性
2. **异步通信**: `tell_vfs` 使用 `asynsend3` 实现非阻塞 IPC，避免 PM 阻塞
3. **优先级映射**: nice 值通过线性公式映射到调度队列号

---

## 灾难预演

**场景 1**: 如果删除 `get_free_pid` 中的 `static` 关键字
```
后果: 每次调用都从 PID 2 开始搜索
      第一个进程获得 PID 2
      第二个进程也获得 PID 2（冲突！）
      系统崩溃或行为异常
```

**场景 2**: 如果 `tell_vfs` 不检查 `VFS_CALL` 标志
```
后果: 进程可能同时等待多个 VFS 响应
      响应消息匹配混乱
      进程状态不一致
```

**场景 3**: 如果 `set_rusage_times` 使用 32 位计算
```
后果: user_time * 1000000 可能溢出
      长时间运行的进程时间统计错误
      getrusage() 返回错误数据
```

---

## 互动自测

1. **问题**: 为什么 PID 需要循环使用而不是一直递增？
   **答案**: PID 是有限资源（最大 30000），长时间运行的系统会耗尽。循环使用可以重用已退出进程的 PID。

2. **问题**: `pm_isokendpt` 为什么要检查 `endpoint != mproc[*proc].mp_endpoint`？
   **答案**: 进程槽位可能被新进程重用。端点号包含生成号，如果生成号不匹配，说明原进程已退出，新进程使用了相同槽位。

3. **问题**: `tell_vfs` 为什么使用 `AMF_NOREPLY` 标志？
   **答案**: PM 发送请求后不能阻塞等待回复。VFS 处理完成后会主动发送回复消息，PM 在主循环中处理。

---

## Rust 实现对比

```rust
#![no_std]

use core::sync::atomic::{AtomicI32, Ordering};

const INIT_PID: i32 = 1;
const NR_PIDS: i32 = 30000;
const NR_PROCS: usize = 64;
const PRIO_MIN: i32 = -20;
const PRIO_MAX: i32 = 20;
const MAX_USER_Q: u32 = 0;
const MIN_USER_Q: u32 = 15;

pub struct MProc {
    pub mp_flags: u32,
    pub mp_pid: i32,
    pub mp_procgrp: i32,
    pub mp_endpoint: i32,
}

pub struct ProcessTable {
    procs: [Option<MProc>; NR_PROCS],
}

impl ProcessTable {
    pub const fn new() -> Self {
        const NONE: Option<MProc> = None;
        Self { procs: [NONE; NR_PROCS] }
    }
}

static NEXT_PID: AtomicI32 = AtomicI32::new(INIT_PID + 1);

pub fn get_free_pid(table: &ProcessTable) -> i32 {
    loop {
        let mut pid = NEXT_PID.load(Ordering::SeqCst);
        pid = if pid < NR_PIDS { pid + 1 } else { INIT_PID + 1 };
        NEXT_PID.store(pid, Ordering::SeqCst);
        
        let mut conflict = false;
        for proc in table.procs.iter().flatten() {
            if proc.mp_pid == pid || proc.mp_procgrp == pid {
                conflict = true;
                break;
            }
        }
        
        if !conflict {
            return pid;
        }
    }
}

pub fn find_proc(table: &ProcessTable, pid: i32) -> Option<&MProc> {
    table.procs.iter().flatten().find(|p| {
        (p.mp_flags & 0x01) != 0 && p.mp_pid == pid
    })
}

pub fn nice_to_priority(nice: i32) -> Result<u32, i32> {
    if nice < PRIO_MIN || nice > PRIO_MAX {
        return Err(22);
    }
    
    let new_q = MAX_USER_Q 
        + ((nice - PRIO_MIN) as u32 * (MIN_USER_Q - MAX_USER_Q + 1)) 
        / ((PRIO_MAX - PRIO_MIN + 1) as u32);
    
    Ok(new_q.clamp(MAX_USER_Q, MIN_USER_Q))
}

#[derive(Debug)]
pub enum PmError {
    InvalidEndpoint,
    DeadEndpoint,
}

pub fn pm_isokendpt(table: &ProcessTable, endpoint: i32) -> Result<usize, PmError> {
    let proc_idx = (endpoint & 0xFFFF) as usize;
    
    if proc_idx >= NR_PROCS {
        return Err(PmError::InvalidEndpoint);
    }
    
    match &table.procs[proc_idx] {
        Some(proc) => {
            if proc.mp_endpoint != endpoint {
                return Err(PmError::DeadEndpoint);
            }
            if (proc.mp_flags & 0x01) == 0 {
                return Err(PmError::DeadEndpoint);
            }
            Ok(proc_idx)
        }
        None => Err(PmError::DeadEndpoint),
    }
}
```

**Rust 优势**:
1. **原子操作**: `AtomicI32` 保证多核环境下的 PID 分配安全
2. **Option 类型**: 明确表示进程槽位可能为空，避免空指针
3. **Result 类型**: 强制处理错误，不会忘记检查返回值
4. **clamp 方法**: 简洁的范围限制，替代手动边界检查
