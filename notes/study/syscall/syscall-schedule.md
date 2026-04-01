# 调度控制系统调用

> **模块定位**: 进程调度策略管理与运行状态控制
> 
> **核心文件**: 
> - `minix3/minix/kernel/system/do_schedctl.c` (46行)
> - `minix3/minix/kernel/system/do_schedule.c` (30行)
> - `minix3/minix/kernel/system/do_runctl.c` (76行)
> - `minix3/minix/kernel/system/do_statectl.c` (53行)

---

## 模块架构总览

### 四个系统调用的协作关系

```
┌─────────────────────────────────────────────────────────────────────┐
│                     调度控制模块全景图                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  调度器管理 (谁可以调度)                                      │  │
│  │  ┌────────────────┐        ┌────────────────┐               │  │
│  │  │ do_schedctl    │───────►│ 设置调度器     │               │  │
│  │  │ (SYS_SCHEDCTL) │        │ - 内核调度     │               │  │
│  │  └────────────────┘        │ - 用户态调度   │               │  │
│  │                            └────────────────┘               │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                              │                                      │
│                              │ p_scheduler 指针                     │
│                              ▼                                      │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  调度参数设置 (如何调度)                                      │  │
│  │  ┌────────────────┐        ┌────────────────┐               │  │
│  │  │ do_schedule    │───────►│ 设置调度参数   │               │  │
│  │  │ (SYS_SCHEDULE) │        │ - priority     │               │  │
│  │  └────────────────┘        │ - quantum      │               │  │
│  │        │                   │ - cpu affinity │               │  │
│  │        │                   │ - niced        │               │  │
│  │        ▼                   └────────────────┘               │  │
│  │  权限检查: caller == p_scheduler                             │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  运行状态控制 (是否运行)                                      │  │
│  │  ┌────────────────┐        ┌────────────────┐               │  │
│  │  │ do_runctl      │───────►│ RTS_PROC_STOP  │               │  │
│  │  │ (SYS_RUNCTL)   │        │ - RC_STOP      │               │  │
│  │  └────────────────┘        │ - RC_RESUME    │               │  │
│  │        │                   │ - RC_DELAY     │               │  │
│  │        ▼                   └────────────────┘               │  │
│  │  延迟停止机制: MF_SIG_DELAY                                  │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  状态表与IPC过滤 (通信控制)                                   │  │
│  │  ┌────────────────┐        ┌────────────────┐               │  │
│  │  │ do_statectl    │───────►│ 状态管理       │               │  │
│  │  │ (SYS_STATECTL) │        │ - IPC引用清理  │               │  │
│  │  └────────────────┘        │ - 状态表设置   │               │  │
│  │                            │ - IPC过滤器    │               │  │
│  │                            └────────────────┘               │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 核心设计理念

**1. 调度器委托机制**

Minix3 支持用户态调度器，这是微内核架构的独特设计：

```
传统宏内核 (Linux/Windows):
┌─────────────────────────────────────┐
│        内核调度器 (全局)             │
│   所有进程由内核统一调度              │
└─────────────────────────────────────┘

Minix3 微内核:
┌─────────────────────────────────────┐
│        内核调度器 (最小化)           │
│   只管理调度器进程本身                │
└─────────────────────────────────────┘
        │ p_scheduler
        ▼
┌───────────────────┐
│   SCHED 服务进程   │  ← 用户态调度器
│  (实现调度策略)    │
└───────────────────┘
        │
        ▼
   普通用户进程
```

**2. 状态标志位管理**

进程运行状态通过 RTS (Run-Time Status) 标志位控制：

| 标志位 | 值 | 含义 |
|--------|---|------|
| `RTS_PROC_STOP` | 0x02 | 进程被停止（不参与调度） |

**3. IPC 安全过滤**

通过黑名单/白名单机制控制进程间通信：

| 过滤器类型 | 默认行为 | 作用 |
|-----------|---------|------|
| 黑名单 (IPCF_BLACKLIST) | 允许所有 | 禁止特定端点 |
| 白名单 (IPCF_WHITELIST) | 禁止所有 | 只允许特定端点 |

---

## 调度器管理机制

### do_schedctl - 调度器设置

**源代码位置**: [do_schedctl.c](file://../minix3/minix/kernel/system/do_schedctl.c)

**核心功能**: 设置进程的调度器（内核或用户态进程）

#### 逐行代码分析

**源码位置**: [do_schedctl.c:10-46](file://../minix3/minix/kernel/system/do_schedctl.c#L10-L46)

```c
#include "kernel/system.h"
#include <minix/endpoint.h>

/*===========================================================================*
 *			          do_schedctl			     *
 *===========================================================================*/
int do_schedctl(struct proc * caller, message * m_ptr)
{
	struct proc *p;
	uint32_t flags;
	int priority, quantum, cpu;
	int proc_nr;
	int r;

	/* check parameter validity */
	flags = m_ptr->m_lsys_krn_schedctl.flags;
	if (flags & ~SCHEDCTL_FLAG_KERNEL) {
		printf("do_schedctl: flags 0x%x invalid, caller=%d\n", 
			flags, caller - proc);
		return EINVAL;
	}

	if (!isokendpt(m_ptr->m_lsys_krn_schedctl.endpoint, &proc_nr))
		return EINVAL;

	p = proc_addr(proc_nr);

	if ((flags & SCHEDCTL_FLAG_KERNEL) == SCHEDCTL_FLAG_KERNEL) {
		/* the kernel becomes the scheduler and starts 
		 * scheduling the process.
		 */
		priority = m_ptr->m_lsys_krn_schedctl.priority;
		quantum = m_ptr->m_lsys_krn_schedctl.quantum;
		cpu = m_ptr->m_lsys_krn_schedctl.cpu;

		/* Try to schedule the process. */
		if((r = sched_proc(p, priority, quantum, cpu, FALSE)) != OK)
			return r;
		p->p_scheduler = NULL;
	} else {
		/* the caller becomes the scheduler */
		p->p_scheduler = caller;
	}

	return(OK);
}
```

**逐行解析**:

| 行号 | 代码 | 内存位置 | 作用 |
|------|------|---------|------|
| 1-2 | `#include` | - | 引入系统头文件和端点定义 |
| 10 | `struct proc *p;` | 栈，8字节(64位) | 目标进程指针 |
| 11 | `uint32_t flags;` | 栈，4字节 | 调度控制标志位 |
| 12-13 | `int priority, quantum, cpu;` | 栈，各4字节 | 调度参数：优先级、时间片、CPU亲和性 |
| 14 | `int proc_nr;` | 栈，4字节 | 进程槽号 |
| 15 | `int r;` | 栈，4字节 | 返回值临时变量 |

**关键代码段详解**:

**1. 标志位验证 (第18-23行)**

```c
flags = m_ptr->m_lsys_krn_schedctl.flags;
if (flags & ~SCHEDCTL_FLAG_KERNEL) {
    printf("do_schedctl: flags 0x%x invalid, caller=%d\n", 
        flags, caller - proc);
    return EINVAL;
}
```

**位运算分析**:

```
SCHEDCTL_FLAG_KERNEL = 1 (二进制: 0000...0001)
~SCHEDCTL_FLAG_KERNEL = 0xFFFFFFFE (二进制: 1111...1110)

flags = 0x00000001 (合法)
  flags & ~SCHEDCTL_FLAG_KERNEL
= 0x00000001 & 0xFFFFFFFE
= 0x00000000
→ 条件为假，通过验证

flags = 0x00000002 (非法)
  flags & ~SCHEDCTL_FLAG_KERNEL
= 0x00000002 & 0xFFFFFFFE
= 0x00000002
→ 条件为真，返回 EINVAL
```

**宏定义源码**: [minix/com.h:449](file://../minix3/minix/include/minix/com.h#L449)

```c
#  define SCHEDCTL_FLAG_KERNEL	1	/* mark kernel scheduler and remove 
					 * RTS_NO_QUANTUM; otherwise caller is 
					 * marked scheduler 
					 */
```

**2. 端点验证 (第25-28行)**

```c
if (!isokendpt(m_ptr->m_lsys_krn_schedctl.endpoint, &proc_nr))
    return EINVAL;

p = proc_addr(proc_nr);
```

**调用链**:
```
isokendpt(endpoint, &proc_nr)
  ├─ 检查 endpoint 是否有效
  ├─ 从 endpoint 提取进程槽号
  └─ 验证进程槽号是否在有效范围内

proc_addr(proc_nr)
  └─ 返回进程表中的进程结构体指针
     #define proc_addr(n) (&proc[n])
```

**3. 调度模式选择 (第30-43行)**

```c
if ((flags & SCHEDCTL_FLAG_KERNEL) == SCHEDCTL_FLAG_KERNEL) {
    /* 内核调度模式 */
    priority = m_ptr->m_lsys_krn_schedctl.priority;
    quantum = m_ptr->m_lsys_krn_schedctl.quantum;
    cpu = m_ptr->m_lsys_krn_schedctl.cpu;

    if((r = sched_proc(p, priority, quantum, cpu, FALSE)) != OK)
        return r;
    p->p_scheduler = NULL;
} else {
    /* 用户态调度器模式 */
    p->p_scheduler = caller;
}
```

**两种模式对比**:

| 模式 | 条件 | p_scheduler | 调度参数来源 |
|------|------|-------------|-------------|
| 内核调度 | `flags & SCHEDCTL_FLAG_KERNEL != 0` | `NULL` | 从消息中提取 |
| 用户调度 | `flags & SCHEDCTL_FLAG_KERNEL == 0` | `caller` | 由调度器进程决定 |

**sched_proc 函数调用**:

源码位置: [system.c:642-700](file://../minix3/minix/kernel/system.c#L642-L700)

```c
int sched_proc(struct proc *p, int priority, int quantum, int cpu, int niced)
{
	/* Make sure the values given are within the allowed range.*/
	if ((priority < TASK_Q && priority != -1) || priority > NR_SCHED_QUEUES)
		return(EINVAL);

	if (quantum < 1 && quantum != -1)
		return(EINVAL);

#ifdef CONFIG_SMP
	if ((cpu < 0 && cpu != -1) || (cpu > 0 && (unsigned) cpu >= ncpus))
		return(EINVAL);
	if (cpu != -1 && !(cpu_is_ready(cpu)))
		return EBADCPU;
#endif
    ...
}
```

**参数验证规则**:
- `priority`: 必须在 `[TASK_Q, NR_SCHED_QUEUES]` 范围内，或为 -1（表示不修改）
- `quantum`: 必须 >= 1，或为 -1（表示不修改）
- `cpu`: 必须是有效的 CPU 编号，或为 -1（表示不修改）
- `niced`: 在 `do_schedctl` 中固定为 `FALSE`，表示不是通过 nice 系统调用设置的

#### 两种调度模式对比

| 维度 | 内核调度 | 用户态调度 |
|------|---------|-----------|
| **标志位** | `SCHEDCTL_FLAG_KERNEL` | 无此标志 |
| **p_scheduler** | `NULL` | `caller` |
| **调度参数** | 从消息中提取 | 由调度器进程决定 |
| **适用场景** | 普通进程 | 需要自定义调度策略的进程 |

#### 调度器指针的语义

```c
struct proc {
    ...
    struct proc *p_scheduler;  // 调度器指针
    ...
};
```

**内存布局**:

```
进程结构体 (struct proc)
┌────────────────────────────────────┐
│ p_name[16]                         │ 16 字节
├────────────────────────────────────┤
│ p_endpoint                         │ 4 字节
├────────────────────────────────────┤
│ p_scheduler ──────┐                │ 8 字节 (64位指针)
├───────────────────┼────────────────┤
│ ...               │                │
└───────────────────┼────────────────┘
                    │
                    ▼
          ┌─────────────────┐
          │ 调度器进程结构体  │
          │ (或 NULL)        │
          └─────────────────┘
```

**指针值的含义**:
- `p_scheduler = NULL` → 内核是调度器
- `p_scheduler = &proc[X]` → proc[X] 是调度器

#### 关键宏定义

```c
// minix3/minix/include/minix/com.h:449
#define SCHEDCTL_FLAG_KERNEL  1  // 标记内核调度器
```

---

## 调度参数设置机制

### do_schedule - 调度参数配置

**源代码位置**: [do_schedule.c](file://../minix3/minix/kernel/system/do_schedule.c)

**核心功能**: 由调度器设置进程的调度参数

#### 逐行代码分析

**源码位置**: [do_schedule.c:1-30](file://../minix3/minix/kernel/system/do_schedule.c#L1-L30)

```c
#include "kernel/system.h"
#include <minix/endpoint.h>
#include "kernel/clock.h"

/*===========================================================================*
 *				do_schedule				     *
 *===========================================================================*/
int do_schedule(struct proc * caller, message * m_ptr)
{
	struct proc *p;
	int proc_nr;
	int priority, quantum, cpu, niced;

	if (!isokendpt(m_ptr->m_lsys_krn_schedule.endpoint, &proc_nr))
		return EINVAL;

	p = proc_addr(proc_nr);

	/* Only this process' scheduler can schedule it */
	if (caller != p->p_scheduler)
		return(EPERM);

	/* Try to schedule the process. */
	priority = m_ptr->m_lsys_krn_schedule.priority;
	quantum = m_ptr->m_lsys_krn_schedule.quantum;
	cpu = m_ptr->m_lsys_krn_schedule.cpu;
	niced = !!(m_ptr->m_lsys_krn_schedule.niced);

	return sched_proc(p, priority, quantum, cpu, niced);
}
```

**逐行解析**:

| 行号 | 代码 | 内存位置 | 作用 |
|------|------|---------|------|
| 1-3 | `#include` | - | 引入系统头文件、端点定义、时钟定义 |
| 10 | `struct proc *p;` | 栈，8字节(64位) | 目标进程指针 |
| 11 | `int proc_nr;` | 栈，4字节 | 进程槽号 |
| 12 | `int priority, quantum, cpu, niced;` | 栈，各4字节 | 调度参数 |

**关键代码段详解**:

**1. 端点验证 (第14-17行)**

```c
if (!isokendpt(m_ptr->m_lsys_krn_schedule.endpoint, &proc_nr))
    return EINVAL;

p = proc_addr(proc_nr);
```

与 `do_schedctl` 相同的验证流程，确保目标进程存在且有效。

**2. 权限检查 (第19-21行)**

```c
/* Only this process' scheduler can schedule it */
if (caller != p->p_scheduler)
    return(EPERM);
```

**这是最关键的安全检查**：

**内存布局**:
```
caller 进程结构体 (struct proc)
┌────────────────────────────────────┐
│ p_name[16]                         │
├────────────────────────────────────┤
│ p_endpoint                         │
├────────────────────────────────────┤
│ ...                                │
└────────────────────────────────────┘
        │
        │ 指针比较
        ▼
目标进程 p 结构体
┌────────────────────────────────────┐
│ p_name[16]                         │
├────────────────────────────────────┤
│ p_scheduler ────┐                  │  ← 与 caller 比较
├─────────────────┼──────────────────┤
│ ...             │                  │
└─────────────────┼──────────────────┘
                  │
                  ▼
        调度器进程结构体
```

**权限检查的三种情况**:

| 情况 | caller | p->p_scheduler | 结果 |
|------|--------|----------------|------|
| 合法调度 | SCHED 进程 | SCHED 进程 | OK，继续执行 |
| 非法调度 | 其他进程 | SCHED 进程 | EPERM，权限错误 |
| 内核调度 | 任意进程 | NULL | EPERM，权限错误 |

**为什么需要这个检查？**

```
攻击场景: 恶意进程 M 试图降低其他进程的优先级

恶意进程 M:
  sys_schedule(受害者进程, 最低优先级, 0, ...)

如果没有权限检查:
  ├─ 受害者进程优先级被设为最低
  ├─ 受害者进程时间片被设为 0
  ├─ 受害者进程永远得不到 CPU
  └─ 拒绝服务攻击 (DoS) 成功

有了权限检查:
  ├─ caller (M) != p->p_scheduler (SCHED)
  ├─ 返回 EPERM
  └─ 攻击被阻止
```

**3. 提取调度参数 (第23-26行)**

```c
priority = m_ptr->m_lsys_krn_schedule.priority;
quantum = m_ptr->m_lsys_krn_schedule.quantum;
cpu = m_ptr->m_lsys_krn_schedule.cpu;
niced = !!(m_ptr->m_lsys_krn_schedule.niced);
```

**`!!` 运算符的作用**:

```c
niced = !!(m_ptr->m_lsys_krn_schedule.niced);
```

将任意整数值转换为严格的布尔值 0 或 1：

| 输入值 | `!input` | `!!input` |
|--------|----------|-----------|
| 0 | 1 | 0 |
| 1 | 0 | 1 |
| 42 | 0 | 1 |
| -1 | 0 | 1 |

**为什么需要 `!!`？**

在 C 语言中，布尔值实际上是整数：
- `niced` 字段在 `sched_proc` 中会被用于位运算
- 确保值严格为 0 或 1，避免未定义行为
- 提高代码可读性和可维护性

**4. 调用 sched_proc (第28行)**

```c
return sched_proc(p, priority, quantum, cpu, niced);
```

**参数传递**:

| 参数 | 来源 | 含义 |
|------|------|------|
| `p` | `proc_addr(proc_nr)` | 目标进程指针 |
| `priority` | 消息字段 | 新优先级 |
| `quantum` | 消息字段 | 新时间片 |
| `cpu` | 消息字段 | CPU 亲和性 |
| `niced` | 消息字段（经 `!!` 转换） | 是否被 nice |

**与 do_schedctl 的区别**:

| 维度 | do_schedctl | do_schedule |
|------|-------------|-------------|
| **调用者** | 任意特权进程 | 必须是调度器进程 |
| **权限检查** | 无 | `caller == p->p_scheduler` |
| **niced 参数** | 固定为 `FALSE` | 从消息中提取 |
| **调度器指针** | 设置 `p_scheduler` | 不修改 `p_scheduler` |
| **用途** | 注册调度器 | 设置调度参数 |

#### 与 do_schedctl 的配合

```
┌─────────────────────────────────────────────────────────────────────┐
│                    调度器设置与参数配置流程                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  阶段 1: 调度器注册 (系统初始化时)                                   │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │  SCHED 服务启动                                             │    │
│  │      │                                                      │    │
│  │      ├─► sys_schedctl(endpoint, SCHEDCTL_FLAG_KERNEL, ...) │    │
│  │      │   → p->p_scheduler = NULL (内核调度)                 │    │
│  │      │                                                      │    │
│  │      └─► 或 sys_schedctl(endpoint, 0, ...)                 │    │
│  │          → p->p_scheduler = SCHED (用户态调度)              │    │
│  └────────────────────────────────────────────────────────────┘    │
│                                                                     │
│  阶段 2: 调度参数设置 (运行时)                                       │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │  SCHED 服务调用                                             │    │
│  │      │                                                      │    │
│  │      └─► sys_schedule(endpoint, priority, quantum, ...)    │    │
│  │          │                                                  │    │
│  │          ├─► 检查: caller == p->p_scheduler ?              │    │
│  │          │   ├─ 是 → 允许设置                               │    │
│  │          │   └─ 否 → EPERM (权限错误)                       │    │
│  │          │                                                  │    │
│  │          └─► sched_proc() 设置参数                          │    │
│  └────────────────────────────────────────────────────────────┘    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

#### 调度参数详解

| 参数 | 类型 | 含义 | 取值范围 |
|------|------|------|---------|
| `priority` | int | 优先级（数值越小越高） | 0-MAX_PRIORITY |
| `quantum` | int | 时间片（毫秒） | > 0 |
| `cpu` | int | CPU 亲和性（绑定 CPU） | 0-NR_CPUS-1 |
| `niced` | int | 是否被 nice（降低优先级） | 0 或 1 |

**`!!` 转换的作用**:

```c
niced = !!(m_ptr->m_lsys_krn_schedule.niced);
```

将任意整数值转换为严格的 0 或 1：
- `!!0` → `0`
- `!!1` → `1`
- `!!42` → `1`
- `!!-1` → `1`

#### 权限检查的重要性

**如果没有权限检查**:

```
恶意进程攻击场景:
┌─────────────────────────────────────────────────────────────────────┐
│  恶意进程 M 调用 sys_schedule(其他进程, 最低优先级, 0, ...)        │
│                                                                     │
│  后果:                                                              │
│  1. 其他进程优先级被设为最低                                         │
│  2. 其他进程时间片被设为 0（永远得不到 CPU）                         │
│  3. 拒绝服务攻击 (DoS)                                              │
│  4. 系统完全瘫痪                                                    │
└─────────────────────────────────────────────────────────────────────┘

有了权限检查:
  - 只有 p->p_scheduler 指向的进程才能修改
  - 用户 A 无法修改用户 B 的调度参数
  - 安全隔离得到保障
```

---

## 运行状态控制机制

### do_runctl - 进程停止与恢复

**源代码位置**: [do_runctl.c](file://../minix3/minix/kernel/system/do_runctl.c)

**核心功能**: 控制进程的运行/停止状态

#### 逐行代码分析

**源码位置**: [do_runctl.c:1-76](file://../minix3/minix/kernel/system/do_runctl.c#L1-L76)

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_RUNCTL
 *
 * The parameters for this kernel call are:
 *    m1_i1:	RC_ENDPT	process number to control
 *    m1_i2:	RC_ACTION	stop or resume the process
 *    m1_i3:	RC_FLAGS	request flags
 */

#include "kernel/system.h"
#include <assert.h>

#if USE_RUNCTL

/*===========================================================================*
 *				  do_runctl				     *
 *===========================================================================*/
int do_runctl(struct proc * caller, message * m_ptr)
{
/* Control a process's RTS_PROC_STOP flag. Used for process management.
 * If the process is queued sending a message or stopped for system call
 * tracing, and the RC_DELAY request flag is given, set MF_SIG_DELAY instead
 * of RTS_PROC_STOP, and send a SIGSNDELAY signal later when the process is done
 * sending (ending the delay). Used by PM for safe signal delivery.
 */
  int proc_nr, action, flags;
  register struct proc *rp;

  /* Extract the message parameters and do sanity checking. */
  if (!isokendpt(m_ptr->RC_ENDPT, &proc_nr)) return(EINVAL);
  if (iskerneln(proc_nr)) return(EPERM);
  rp = proc_addr(proc_nr);

  action = m_ptr->RC_ACTION;
  flags = m_ptr->RC_FLAGS;

  /* Is the target sending or syscall-traced? Then set MF_SIG_DELAY instead.
   * Do this only when the RC_DELAY flag is set in the request flags field.
   * The process will not become runnable before PM has called SYS_ENDKSIG.
   * Note that asynchronous messages are not covered: a process using SENDA
   * should not also install signal handlers *and* expect POSIX compliance.
   */

  if (action == RC_STOP && (flags & RC_DELAY)) {
	if (RTS_ISSET(rp, RTS_SENDING) || (rp->p_misc_flags & MF_SC_DEFER))
		rp->p_misc_flags |= MF_SIG_DELAY;

	if (rp->p_misc_flags & MF_SIG_DELAY)
		return (EBUSY);
  }

  /* Either set or clear the stop flag. */
  switch (action) {
  case RC_STOP:
#if CONFIG_SMP
	  /* check if we must stop a process on a different CPU */
	  if (rp->p_cpu != cpuid) {
		  smp_schedule_stop_proc(rp);
		  break;
	  }
#endif
	  RTS_SET(rp, RTS_PROC_STOP);
	break;
  case RC_RESUME:
	assert(RTS_ISSET(rp, RTS_PROC_STOP));
	RTS_UNSET(rp, RTS_PROC_STOP);
	break;
  default:
	return(EINVAL);
  }

  return(OK);
}

#endif /* USE_RUNCTL */
```

**逐行解析**:

| 行号 | 代码 | 内存位置 | 作用 |
|------|------|---------|------|
| 1-9 | 注释 | - | 文档说明：系统调用类型、参数定义 |
| 11-12 | `#include` | - | 引入系统头文件和断言定义 |
| 14 | `#if USE_RUNCTL` | - | 条件编译：是否启用 RUNCTL |
| 28 | `int proc_nr, action, flags;` | 栈，各4字节 | 进程槽号、操作类型、标志位 |
| 29 | `register struct proc *rp;` | 寄存器或栈，8字节 | 目标进程指针（频繁访问） |

**关键代码段详解**:

**1. 参数验证 (第32-35行)**

```c
/* Extract the message parameters and do sanity checking. */
if (!isokendpt(m_ptr->RC_ENDPT, &proc_nr)) return(EINVAL);
if (iskerneln(proc_nr)) return(EPERM);
rp = proc_addr(proc_nr);
```

**宏定义源码**: [minix/com.h:429-432](file://../minix3/minix/include/minix/com.h#L429-L432)

```c
#  define RC_STOP           0	/* stop the process */
#  define RC_RESUME         1	/* clear the stop flag */
#  define RC_DELAY          1	/* delay stop if process is sending */
```

**消息字段映射**:

| 字段 | 宏定义 | 含义 |
|------|--------|------|
| `m1_i1` | `RC_ENDPT` | 目标进程端点 |
| `m1_i2` | `RC_ACTION` | 操作：停止或恢复 |
| `m1_i3` | `RC_FLAGS` | 标志位（如 RC_DELAY） |

**为什么不能停止内核进程？**

```c
if (iskerneln(proc_nr)) return(EPERM);
```

**内核进程的特殊性**:
- 内核进程（如 IDLE, CLOCK, SYSTEM）是系统核心组件
- 停止它们会导致系统崩溃
- 例如：停止 CLOCK 进程 → 系统时钟停止 → 调度失效

**2. 延迟停止机制 (第44-53行)**

```c
if (action == RC_STOP && (flags & RC_DELAY)) {
    if (RTS_ISSET(rp, RTS_SENDING) || (rp->p_misc_flags & MF_SC_DEFER))
        rp->p_misc_flags |= MF_SIG_DELAY;

    if (rp->p_misc_flags & MF_SIG_DELAY)
        return (EBUSY);
}
```

**条件判断逻辑**:

```
条件 1: action == RC_STOP
  └─ 只有停止操作才需要延迟

条件 2: flags & RC_DELAY
  └─ 调用者请求延迟停止

条件 3: RTS_ISSET(rp, RTS_SENDING)
  └─ 进程正在发送消息（同步 IPC）

条件 4: rp->p_misc_flags & MF_SC_DEFER
  └─ 进程有延迟的系统调用

如果 (条件 1 && 条件 2 && (条件 3 || 条件 4)):
  ├─ 设置 MF_SIG_DELAY 标志
  └─ 返回 EBUSY（告诉调用者稍后再试）
```

**标志位详解**:

| 标志位 | 定义位置 | 含义 |
|--------|---------|------|
| `RTS_SENDING` | `proc.h` | 进程正在发送同步消息 |
| `MF_SC_DEFER` | `proc.h` | 进程有延迟的系统调用 |
| `MF_SIG_DELAY` | `proc.h` | 进程被标记为延迟停止 |

**为什么需要延迟停止？**

```
场景: PM 想要向进程 P 发送信号（需要停止 P）

┌─────────────────────────────────────────────────────────────────────┐
│  进程 P 正在执行 SEND 操作                                          │
│      │                                                              │
│      ├─► P 调用 send(RECEIVER, msg)                                 │
│      │   ├─ P 设置 RTS_SENDING 标志                                │
│      │   ├─ P 进入等待状态                                          │
│      │   └─ 等待 RECEIVER 接收消息                                  │
│      │                                                              │
│      ▼                                                              │
│  PM 调用 do_runctl(P, RC_STOP, RC_DELAY)                            │
│      │                                                              │
│      ├─► 检测到 RTS_SENDING 标志                                    │
│      ├─► 设置 MF_SIG_DELAY 标志                                     │
│      └─► 返回 EBUSY                                                 │
│                                                                     │
│  PM 收到 EBUSY:                                                     │
│      └─► 稍后重试                                                   │
│                                                                     │
│  RECEIVER 接收消息:                                                  │
│      ├─► P 的 RTS_SENDING 标志被清除                                │
│      ├─► P 的 MF_SIG_DELAY 标志触发信号                             │
│      └─► PM 再次尝试停止 P                                          │
└─────────────────────────────────────────────────────────────────────┘

如果立即停止（不使用 RC_DELAY）:
  ├─► P 被标记为 RTS_PROC_STOP
  ├─► RECEIVER 回复消息
  ├─► P 无法处理回复（因为已停止）
  └─► IPC 通信中断，可能导致死锁
```

**3. 停止/恢复操作 (第56-70行)**

```c
switch (action) {
case RC_STOP:
#if CONFIG_SMP
    /* check if we must stop a process on a different CPU */
    if (rp->p_cpu != cpuid) {
        smp_schedule_stop_proc(rp);
        break;
    }
#endif
    RTS_SET(rp, RTS_PROC_STOP);
    break;
case RC_RESUME:
    assert(RTS_ISSET(rp, RTS_PROC_STOP));
    RTS_UNSET(rp, RTS_PROC_STOP);
    break;
default:
    return(EINVAL);
}
```

**SMP 支持**:

```c
#if CONFIG_SMP
if (rp->p_cpu != cpuid) {
    smp_schedule_stop_proc(rp);
    break;
}
#endif
```

**为什么需要跨 CPU 操作？**

```
多核系统场景:

CPU 0 (当前 CPU):
  └─► 执行 do_runctl(P, RC_STOP, ...)

CPU 1:
  └─► 进程 P 正在运行

如果直接设置 RTS_PROC_STOP:
  ├─► CPU 1 上的 P 可能还在运行
  ├─► 竞态条件
  └─► 数据不一致

正确做法:
  ├─► 检测到 p->p_cpu != cpuid
  ├─► 调用 smp_schedule_stop_proc(rp)
  │   └─► 发送 IPI (Inter-Processor Interrupt)
  │       └─► CPU 1 停止 P
  └─► 安全停止
```

**RTS 标志位操作**:

```c
RTS_SET(rp, RTS_PROC_STOP);    // 设置停止标志
RTS_UNSET(rp, RTS_PROC_STOP);  // 清除停止标志
```

**宏定义** (在 `proc.h` 中):

```c
#define RTS_SET(rp, f)      ((rp)->p_rts_flags |= (f))
#define RTS_UNSET(rp, f)    ((rp)->p_rts_flags &= ~(f))
#define RTS_ISSET(rp, f)    ((rp)->p_rts_flags & (f))
```

**RTS_PROC_STOP 的作用**:

```
进程调度器检查:
  if (p->p_rts_flags == 0) {
      // 进程可运行
      enqueue(p);
  } else {
      // 进程不可运行
      // RTS_PROC_STOP 标志阻止进程被调度
  }
```

**恢复操作的断言**:

```c
case RC_RESUME:
    assert(RTS_ISSET(rp, RTS_PROC_STOP));
    RTS_UNSET(rp, RTS_PROC_STOP);
    break;
```

**为什么需要断言？**

- 确保恢复操作只针对已停止的进程
- 如果进程未被停止，说明逻辑错误
- 断言失败会触发 kernel panic

#### 延迟停止机制的设计原因

**问题场景**:

```
场景: PM 想要停止一个正在发送消息的进程 P

┌─────────────────────────────────────────────────────────────────────┐
│  进程 P 正在执行 SEND 操作                                          │
│      │                                                              │
│      ├─► P 已发送消息，等待接收方回复                                │
│      │   (RTS_SENDING 标志位已设置)                                 │
│      │                                                              │
│      ▼                                                              │
│  PM 调用 do_runctl(P, RC_STOP)                                      │
│      │                                                              │
│      ├─► 如果立即设置 RTS_PROC_STOP:                                │
│      │   - P 被标记为"已停止"                                       │
│      │   - 但 P 正在等待接收方回复                                   │
│      │   - 接收方回复后，P 无法处理（因为已停止）                    │
│      │   - IPC 通信中断，可能导致死锁                                │
│      │                                                              │
│      └─► 使用 RC_DELAY 延迟停止:                                    │
│          - 设置 MF_SIG_DELAY 标志                                   │
│          - 返回 EBUSY (告诉 PM 稍后再试)                            │
│          - P 完成发送后，会收到 SIGSNDELAY 信号                     │
│          - PM 可以在信号处理中再次尝试停止                           │
└─────────────────────────────────────────────────────────────────────┘
```

**延迟停止流程**:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    延迟停止机制流程图                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. PM 检测到需要停止进程 P (例如收到信号)                          │
│     │                                                               │
│     ▼                                                               │
│  2. PM 调用 sys_delay_stop(P)                                       │
│     │   (实际调用 sys_runctl(P, RC_STOP, RC_DELAY))                │
│     │                                                               │
│     ▼                                                               │
│  3. do_runctl 检查 P 是否正在发送消息                               │
│     │                                                               │
│     ├─► 是 (RTS_SENDING 已设置):                                   │
│     │   - 设置 MF_SIG_DELAY 标志                                    │
│     │   - 返回 EBUSY                                                │
│     │   - P 完成发送后收到 SIGSNDELAY 信号                          │
│     │   - PM 在信号处理中再次尝试停止                               │
│     │                                                               │
│     └─► 否 (P 未在发送):                                           │
│         - 立即设置 RTS_PROC_STOP                                    │
│         - P 被停止                                                  │
│         - 返回 OK                                                   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

#### 关键宏定义

```c
// minix3/minix/include/minix/com.h:429-432
#define RC_STOP    0  // 停止进程
#define RC_RESUME  1  // 恢复进程
#define RC_DELAY   1  // 延迟停止标志

// minix3/minix/include/minix/syslib.h:46-48
#define sys_stop(proc_ep)        sys_runctl(proc_ep, RC_STOP, 0)
#define sys_delay_stop(proc_ep)  sys_runctl(proc_ep, RC_STOP, RC_DELAY)
#define sys_resume(proc_ep)      sys_runctl(proc_ep, RC_RESUME, 0)
```

#### RTS 标志位操作

```c
// minix3/minix/kernel/proc.h:143
#define RTS_PROC_STOP  0x02  // 进程被停止

// 标志位操作宏
RTS_SET(rp, RTS_PROC_STOP);    // 设置停止标志
RTS_UNSET(rp, RTS_PROC_STOP);  // 清除停止标志
RTS_ISSET(rp, RTS_PROC_STOP);  // 检查是否已停止
```

**内存视图**:

```
进程结构体中的标志位字段:
┌────────────────────────────────────┐
│ p_rts_flags (32位整数)             │
├────────────────────────────────────┤
│ bit 0: RTS_NO_QUANTUM              │
│ bit 1: RTS_PROC_STOP  ← 停止标志   │
│ bit 2: RTS_SENDING                 │
│ bit 3: RTS_RECEIVING               │
│ ...                                │
└────────────────────────────────────┘
```

---

## 状态表与 IPC 过滤机制

### do_statectl - 状态控制接口

**源代码位置**: [do_statectl.c](file://../minix3/minix/kernel/system/do_statectl.c)

**核心功能**: 状态表设置与 IPC 过滤器管理

#### 逐行代码分析

**源码位置**: [do_statectl.c:1-53](file://../minix3/minix/kernel/system/do_statectl.c#L1-L53)

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_STATECTL
 *
 * The parameters for this kernel call are:
 *   m_lsys_krn_sys_statectl.request	(state control request)
 */

#include "kernel/system.h"

#if USE_STATECTL

/*===========================================================================*
 *			          do_statectl				     *
 *===========================================================================*/
int do_statectl(struct proc * caller, message * m_ptr)
{
/* Handle sys_statectl(). A process has issued a state control request. */

  switch(m_ptr->m_lsys_krn_sys_statectl.request)
  {
  case SYS_STATE_CLEAR_IPC_REFS:
	/* Clear IPC references for all the processes communicating
	 * with the caller.
	 */
	clear_ipc_refs(caller, EDEADSRCDST);
	return(OK);
  case SYS_STATE_SET_STATE_TABLE:
	/* Set state table for the caller. */
	priv(caller)->s_state_table = (vir_bytes) m_ptr->m_lsys_krn_sys_statectl.address;
	priv(caller)->s_state_entries = m_ptr->m_lsys_krn_sys_statectl.length;
	return(OK);
  case SYS_STATE_ADD_IPC_BL_FILTER:
	/* Add an IPC blacklist filter for the caller. */
	return add_ipc_filter(caller, IPCF_BLACKLIST,
	    (vir_bytes) m_ptr->m_lsys_krn_sys_statectl.address,
	    m_ptr->m_lsys_krn_sys_statectl.length);
  case SYS_STATE_ADD_IPC_WL_FILTER:
	/* Add an IPC whitelist filter for the caller. */
	return add_ipc_filter(caller, IPCF_WHITELIST,
	    (vir_bytes) m_ptr->m_lsys_krn_sys_statectl.address,
	    m_ptr->m_lsys_krn_sys_statectl.length);
  case SYS_STATE_CLEAR_IPC_FILTERS:
	/* Clear any IPC filter for the caller. */
	clear_ipc_filters(caller);
	return OK;
  default:
	printf("do_statectl: bad request %d\n",
		m_ptr->m_lsys_krn_sys_statectl.request);
	return EINVAL;
  }
}

#endif /* USE_STATECTL */
```

**逐行解析**:

| 行号 | 代码 | 内存位置 | 作用 |
|------|------|---------|------|
| 1-6 | 注释 | - | 文档说明：系统调用类型、参数定义 |
| 8 | `#include` | - | 引入系统头文件 |
| 10 | `#if USE_STATECTL` | - | 条件编译：是否启用 STATECTL |
| 19 | `switch` | - | 根据请求类型分发处理 |

**宏定义源码**: [minix/com.h:442-446](file://../minix3/minix/include/minix/com.h#L442-L446)

```c
#define SYS_STATE_CLEAR_IPC_REFS    1	/* clear IPC references */
#define SYS_STATE_SET_STATE_TABLE   2	/* set state map */
#define SYS_STATE_ADD_IPC_BL_FILTER 3	/* set IPC blacklist filter */
#define SYS_STATE_ADD_IPC_WL_FILTER 4	/* set IPC whitelist filter */
#define SYS_STATE_CLEAR_IPC_FILTERS 5	/* clear IPC filters */
```

**关键代码段详解**:

**1. 清除 IPC 引用 (第21-26行)**

```c
case SYS_STATE_CLEAR_IPC_REFS:
    /* Clear IPC references for all the processes communicating
     * with the caller.
     */
    clear_ipc_refs(caller, EDEADSRCDST);
    return(OK);
```

**clear_ipc_refs 函数**:

源码位置: [system.c:577-620](file://../minix3/minix/kernel/system.c#L577-L620)

```c
void clear_ipc_refs(
  register struct proc *rc,		/* slot of process to clean up */
  int caller_ret			/* code to return on callers */
)
{
/* Clear IPC references for a given process slot. */
  struct proc *rp;			/* iterate over process table */
  int src_id;

  /* Tell processes that sent asynchronous messages to 'rc' they are not
   * going to be delivered */
  while ((src_id = has_pending_asend(rc, ANY)) != NULL_PRIV_ID)
      cancel_async(proc_addr(id_to_nr(src_id)), rc);

  for (rp = BEG_PROC_ADDR; rp < END_PROC_ADDR; rp++) {
      if(isemptyp(rp))
	continue;

      /* Unset pending notification bits. */
      unset_sys_bit(priv(rp)->s_notify_pending, priv(rc)->s_id);

      /* Check if process is waiting for a message from 'rc'. */
      if (RTS_ISSET(rp, RTS_SENDING) && rp->p_sendto == rc->p_endpoint) {
          /* Found one. Make it runnable again. */
          RTS_UNSET(rp, RTS_SENDING);
          rp->p_sendto = NONE;
          if (!RTS_ISSET(rp, RTS_NO_ENDIAN)) {
              rp->p_reg.retreg = caller_ret;	/* report reason */
          }
      }
  }
}
```

**为什么需要清除 IPC 引用？**

```
场景: 进程 B 即将退出

┌─────────────────────────────────────────────────────────────────────┐
│  进程 A 正在等待进程 B 的消息                                        │
│      │                                                              │
│      ├─► A 调用 receive(B, &msg)                                    │
│      │   ├─ A 设置 RTS_RECEIVING 标志                              │
│      │   ├─ A 设置 p_sendto = B                                     │
│      │   └─ A 进入等待状态                                          │
│      │                                                              │
│      ▼                                                              │
│  进程 B 退出:                                                        │
│      │                                                              │
│      ├─► PM 调用 sys_statectl(SYS_STATE_CLEAR_IPC_REFS)            │
│      │   │                                                          │
│      │   ├─► 遍历进程表                                             │
│      │   │   └─► 发现 A 正在等待 B                                  │
│      │   │                                                          │
│      │   ├─► 清除 A 的 RTS_SENDING 标志                            │
│      │   ├─► 设置 A 的返回值为 EDEADSRCDST                         │
│      │   └─► A 变为可运行状态                                       │
│      │                                                              │
│      └─► A 从 receive 返回，收到 EDEADSRCDST 错误                  │
│                                                                     │
│  如果不清除 IPC 引用:                                                │
│      └─► A 永远等待已退出的 B → 死锁                                │
└─────────────────────────────────────────────────────────────────────┘
```

**2. 设置状态表 (第27-32行)**

```c
case SYS_STATE_SET_STATE_TABLE:
    /* Set state table for the caller. */
    priv(caller)->s_state_table = (vir_bytes) m_ptr->m_lsys_krn_sys_statectl.address;
    priv(caller)->s_state_entries = m_ptr->m_lsys_krn_sys_statectl.length;
    return(OK);
```

**状态表的作用**:

状态表用于 SEF (System Event Framework) 的 live update 机制：
- 记录进程的状态信息
- 在进程更新时保存/恢复状态
- 支持进程的平滑升级

**priv 结构体字段**:

```c
struct priv {
    ...
    vir_bytes s_state_table;    // 状态表地址
    int s_state_entries;        // 状态表条目数
    ...
};
```

**3. 添加 IPC 过滤器 (第33-42行)**

```c
case SYS_STATE_ADD_IPC_BL_FILTER:
    /* Add an IPC blacklist filter for the caller. */
    return add_ipc_filter(caller, IPCF_BLACKLIST,
        (vir_bytes) m_ptr->m_lsys_krn_sys_statectl.address,
        m_ptr->m_lsys_krn_sys_statectl.length);
case SYS_STATE_ADD_IPC_WL_FILTER:
    /* Add an IPC whitelist filter for the caller. */
    return add_ipc_filter(caller, IPCF_WHITELIST,
        (vir_bytes) m_ptr->m_lsys_krn_sys_statectl.address,
        m_ptr->m_lsys_krn_sys_statectl.length);
```

**add_ipc_filter 函数**:

源码位置: [system.c:705-745](file://../minix3/minix/kernel/system.c#L705-L745)

```c
int add_ipc_filter(struct proc *rp, int type, vir_bytes address,
	size_t length)
{
	int num_elements, r;
	ipc_filter_t *ipcf, **ipcfp;

	/* Validate arguments. */
	if (type != IPCF_BLACKLIST && type != IPCF_WHITELIST)
		return EINVAL;

	if (length % sizeof(ipc_filter_el_t) != 0)
		return EINVAL;

	num_elements = length / sizeof(ipc_filter_el_t);
	if (num_elements <= 0 || num_elements > IPCF_MAX_ELEMENTS)
		return E2BIG;

	/* Allocate a new IPC filter slot. */
	IPCF_POOL_ALLOCATE_SLOT(type, &ipcf);
	if (ipcf == NULL)
		return ENOMEM;

	/* Fill details. */
	ipcf->num_elements = num_elements;
	ipcf->next = NULL;
	r = data_copy(rp->p_endpoint, address,
		KERNEL, (vir_bytes)ipcf->elements, length);
	if (r == OK)
		r = check_ipc_filter(ipcf, TRUE /*fill_flags*/);
	if (r != OK) {
		IPCF_POOL_FREE_SLOT(ipcf);
		return r;
	}
    ...
}
```

**IPC 过滤器结构**:

源码位置: [ipc_filter.h:44-51](file://../minix3/minix/kernel/ipc_filter.h#L44-L51)

```c
struct ipc_filter_s {
  int type;                           // IPCF_BLACKLIST 或 IPCF_WHITELIST
  int num_elements;                   // 过滤器元素数量
  int flags;                          // 过滤器标志
  struct ipc_filter_s *next;          // 链表指针（支持多个过滤器）
  ipc_filter_el_t elements[IPCF_MAX_ELEMENTS];  // 过滤器元素数组
};
typedef struct ipc_filter_s ipc_filter_t;
```

**过滤器类型**:

源码位置: [ipc_filter.h:11-15](file://../minix3/minix/kernel/ipc_filter.h#L11-L15)

```c
#define IPCF_NONE        0	/* no ipc filter */
#define IPCF_BLACKLIST   1	/* blacklist filter type */
#define IPCF_WHITELIST   2	/* whitelist filter type */
```

**黑名单 vs 白名单**:

```
黑名单 (IPCF_BLACKLIST):
  默认行为: 允许所有 IPC
  过滤规则: 禁止匹配的消息
  使用场景: 阻止特定进程或消息类型

白名单 (IPCF_WHITELIST):
  默认行为: 禁止所有 IPC
  过滤规则: 只允许匹配的消息
  使用场景: 严格限制通信范围
```

**过滤器匹配逻辑**:

源码位置: [system.c:850-865](file://../minix3/minix/kernel/system.c#L850-L865)

```c
int allow_ipc_filtered_msg(struct proc *rp, endpoint_t src_e,
    vir_bytes m_src_v, message *m_src_p)
{
    ...
    allow = (ipcf->type == IPCF_BLACKLIST);  // 黑名单默认允许
    
    for (i = 0; i < ipcf->num_elements; i++) {
        e = &ipcf->elements[i];
        if (IPCF_EL_MATCH(e, m_src_p)) {
            if (allow != (ipcf->type == IPCF_WHITELIST)) {
                /* matched */
                allow = (ipcf->type == IPCF_WHITELIST);
            }
        }
    }
    ...
}
```

**匹配规则**:

| 过滤器类型 | 默认行为 | 匹配后行为 |
|-----------|---------|-----------|
| 黑名单 | 允许 | 禁止 |
| 白名单 | 禁止 | 允许 |

**4. 清除 IPC 过滤器 (第43-46行)**

```c
case SYS_STATE_CLEAR_IPC_FILTERS:
    /* Clear any IPC filter for the caller. */
    clear_ipc_filters(caller);
    return OK;
```

**clear_ipc_filters 函数**:

源码位置: [system.c:747-761](file://../minix3/minix/kernel/system.c#L747-L761)

```c
void clear_ipc_filters(struct proc *rp)
{
	ipc_filter_t *ipcf, **ipcfp;

	ipcfp = &rp->p_priv->s_ipc_filter;
	while (*ipcfp != NULL) {
		ipcf = *ipcfp;
		*ipcfp = ipcf->next;
		IPCF_POOL_FREE_SLOT(ipcf);
	}
}
```

**为什么需要清除过滤器？**

- 进程不再需要 IPC 限制时
- 进程退出时清理资源
- 切换安全策略时

#### 五个子功能对比

| 子功能 | 作用 | 使用场景 |
|--------|------|---------|
| `CLEAR_IPC_REFS` | 清除 IPC 引用 | 进程退出时清理 |
| `SET_STATE_TABLE` | 设置状态转换表 | 状态机管理 |
| `ADD_IPC_BL_FILTER` | 添加黑名单 | 禁止特定通信 |
| `ADD_IPC_WL_FILTER` | 添加白名单 | 只允许特定通信 |
| `CLEAR_IPC_FILTERS` | 清除过滤器 | 重置通信策略 |

#### IPC 引用清理机制

**为什么需要清理 IPC 引用？**

```
场景: 进程 A 正在等待进程 B 的回复

┌─────────────────────────────────────────────────────────────────────┐
│  进程 A (等待回复)           进程 B (已退出)                        │
│      │                           │                                 │
│      ├─► SEND(B, msg)            ├─► exit()                       │
│      │   - A 设置 RTS_RECEIVING  │   - B 进程结构体被清理           │
│      │   - A 等待 B 回复         │   - 但 A 仍在等待 B              │
│      │                           │                                 │
│      ▼                           ▼                                 │
│  问题: A 永远等不到 B 的回复 → 死锁                                │
│                                                                     │
│  解决: B 退出时调用 SYS_STATE_CLEAR_IPC_REFS                       │
│      │                                                              │
│      └─► clear_ipc_refs(B, EDEADSRCDST)                            │
│          - 查找所有等待 B 的进程                                    │
│          - 唤醒它们，返回 EDEADSRCDST 错误                          │
│          - A 收到错误，知道 B 已退出                                │
└─────────────────────────────────────────────────────────────────────┘
```

#### IPC 过滤器机制

**黑名单 vs 白名单**:

```
黑名单 (IPCF_BLACKLIST):
┌─────────────────────────────────────────────────────────────────────┐
│  默认: 允许所有 IPC                                                 │
│  规则: 禁止列表中的端点                                             │
│                                                                     │
│  示例:                                                              │
│  黑名单 = [ENDPOINT_MALWARE]                                        │
│  - 可以和任何进程通信，除了 MALWARE                                 │
│  - 适合: 大多数服务进程                                             │
└─────────────────────────────────────────────────────────────────────┘

白名单 (IPCF_WHITELIST):
┌─────────────────────────────────────────────────────────────────────┐
│  默认: 禁止所有 IPC                                                 │
│  规则: 只允许列表中的端点                                           │
│                                                                     │
│  示例:                                                              │
│  白名单 = [ENDPOINT_VFS, ENDPOINT_PM]                               │
│  - 只能和 VFS、PM 通信                                              │
│  - 其他任何 IPC 都被拒绝                                            │
│  - 适合: 高安全级别进程                                             │
└─────────────────────────────────────────────────────────────────────┘
```

**过滤器检查流程**:

```c
// minix3/minix/kernel/system.c:853-860
allow = (ipcf->type == IPCF_BLACKLIST);  // 黑名单默认允许

for (i = 0; i < ipcf->ipcf_count; i++) {
    if (allow != (ipcf->type == IPCF_WHITELIST)) {
        if (target == ipcf->ipcf_endpoints[i]) {
            allow = (ipcf->type == IPCF_WHITELIST);
            break;
        }
    }
}
```

#### 关键宏定义

```c
// minix3/minix/include/minix/com.h:442-444
#define SYS_STATE_CLEAR_IPC_REFS    1  // 清除 IPC 引用
#define SYS_STATE_SET_STATE_TABLE   2  // 设置状态表
#define SYS_STATE_ADD_IPC_BL_FILTER 3  // 添加黑名单

// minix3/minix/kernel/ipc_filter.h:14-15
#define IPCF_BLACKLIST   1  // 黑名单类型
#define IPCF_WHITELIST   2  // 白名单类型
```

---

## 模块级 Rust 重构建议

### 1. 类型安全的调度器委托

```rust
use core::ptr::NonNull;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheduler {
    Kernel,
    User(NonNull<Proc>),
}

impl Scheduler {
    pub fn set_for(&self, proc: &mut Proc) {
        proc.p_scheduler = match self {
            Scheduler::Kernel => None,
            Scheduler::User(sched) => Some(*sched),
        };
    }
    
    pub fn from_proc(proc: &Proc) -> Self {
        match proc.p_scheduler {
            None => Scheduler::Kernel,
            Some(sched) => Scheduler::User(sched),
        }
    }
}

impl Proc {
    pub fn is_scheduler(&self, caller: &Proc) -> bool {
        match self.p_scheduler {
            Some(sched) => sched == NonNull::from(caller),
            None => caller.is_kernel_process(),
        }
    }
}
```

### 2. 标志位的类型安全抽象

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SchedctlFlags: u32 {
        const KERNEL = SCHEDCTL_FLAG_KERNEL;
    }
    
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RtsFlags: u32 {
        const NO_QUANTUM = 0x01;
        const PROC_STOP = 0x02;
        const SENDING = 0x04;
        const RECEIVING = 0x08;
    }
    
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MiscFlags: u32 {
        const SC_DEFER = 0x01;
        const SIG_DELAY = 0x02;
    }
}

impl SchedctlFlags {
    pub fn from_bits_truncate(bits: u32) -> Self {
        Self::from_bits_truncate(bits & SCHEDCTL_FLAG_KERNEL)
    }
    
    pub fn is_kernel_scheduler(&self) -> bool {
        self.contains(Self::KERNEL)
    }
}
```

### 3. 调度参数的类型安全封装

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Priority(u8);

impl Priority {
    pub const MIN: u8 = 0;
    pub const MAX: u8 = 15;
    
    pub fn new(value: u8) -> Result<Self, ScheduleError> {
        if value <= Self::MAX {
            Ok(Priority(value))
        } else {
            Err(ScheduleError::InvalidPriority)
        }
    }
    
    pub fn value(&self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantum(u32);

impl Quantum {
    pub fn new(milliseconds: u32) -> Result<Self, ScheduleError> {
        if milliseconds > 0 {
            Ok(Quantum(milliseconds))
        } else {
            Err(ScheduleError::InvalidQuantum)
        }
    }
    
    pub fn milliseconds(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CpuAffinity(u8);

impl CpuAffinity {
    pub fn new(cpu: u8) -> Self {
        CpuAffinity(cpu % NR_CPUS)
    }
    
    pub fn cpu(&self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SchedParams {
    pub priority: Priority,
    pub quantum: Quantum,
    pub cpu: CpuAffinity,
    pub niced: bool,
}
```

### 4. 运行状态控制的类型安全实现

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunctlAction {
    Stop,
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunctlFlags(u32);

bitflags::bitflags! {
    impl RunctlFlags: u32 {
        const DELAY = RC_DELAY;
    }
}

pub fn do_runctl(
    caller: &Proc,
    endpoint: Endpoint,
    action: RunctlAction,
    flags: RunctlFlags,
) -> Result<(), RunctlError> {
    let target = validate_endpoint(endpoint)
        .map_err(|_| RunctlError::InvalidEndpoint)?;
    
    if target.is_kernel_process() {
        return Err(RunctlError::CannotStopKernel);
    }
    
    match action {
        RunctlAction::Stop => {
            if flags.contains(RunctlFlags::DELAY) {
                handle_delayed_stop(&target)?;
            } else {
                target.rts_flags.set(RtsFlags::PROC_STOP, true);
            }
        }
        RunctlAction::Resume => {
            if !target.rts_flags.contains(RtsFlags::PROC_STOP) {
                return Err(RunctlError::NotStopped);
            }
            target.rts_flags.set(RtsFlags::PROC_STOP, false);
        }
    }
    
    Ok(())
}

fn handle_delayed_stop(target: &Proc) -> Result<(), RunctlError> {
    if target.rts_flags.contains(RtsFlags::SENDING) 
        || target.misc_flags.contains(MiscFlags::SC_DEFER) {
        target.misc_flags.set(MiscFlags::SIG_DELAY, true);
        return Err(RunctlError::Busy);
    }
    
    if target.misc_flags.contains(MiscFlags::SIG_DELAY) {
        return Err(RunctlError::Busy);
    }
    
    target.rts_flags.set(RtsFlags::PROC_STOP, true);
    Ok(())
}
```

### 5. IPC 过滤器的类型安全设计

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcFilterType {
    Blacklist,
    Whitelist,
}

#[derive(Debug, Clone)]
pub struct IpcFilter {
    filter_type: IpcFilterType,
    endpoints: Vec<Endpoint, MAX_FILTER_ENTRIES>,
}

impl IpcFilter {
    pub fn new(filter_type: IpcFilterType) -> Self {
        Self {
            filter_type,
            endpoints: Vec::new(),
        }
    }
    
    pub fn add_endpoint(&mut self, endpoint: Endpoint) -> Result<(), IpcFilterError> {
        if self.endpoints.len() >= MAX_FILTER_ENTRIES {
            return Err(IpcFilterError::TooManyEntries);
        }
        
        if self.endpoints.contains(&endpoint) {
            return Err(IpcFilterError::DuplicateEndpoint);
        }
        
        self.endpoints.push(endpoint);
        Ok(())
    }
    
    pub fn is_allowed(&self, target: Endpoint) -> bool {
        let in_list = self.endpoints.contains(&target);
        
        match self.filter_type {
            IpcFilterType::Blacklist => !in_list,  // 黑名单：不在列表中则允许
            IpcFilterType::Whitelist => in_list,   // 白名单：在列表中才允许
        }
    }
}

pub fn do_statectl(
    caller: &mut Proc,
    request: StatectlRequest,
) -> Result<(), StatectlError> {
    match request {
        StatectlRequest::ClearIpcRefs => {
            clear_ipc_refs(caller, ErrorCode::DeadSrcDst);
            Ok(())
        }
        StatectlRequest::SetStateTable { address, length } => {
            caller.privilege.s_state_table = address;
            caller.privilege.s_state_entries = length;
            Ok(())
        }
        StatectlRequest::AddIpcFilter { filter_type, address, length } => {
            add_ipc_filter(caller, filter_type, address, length)
        }
        StatectlRequest::ClearIpcFilters => {
            clear_ipc_filters(caller);
            Ok(())
        }
    }
}
```

### 6. 错误处理的类型安全

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedctlError {
    InvalidFlags,
    InvalidEndpoint,
    SchedProcFailed(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleError {
    InvalidEndpoint,
    PermissionDenied,
    InvalidPriority,
    InvalidQuantum,
    SchedProcFailed(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunctlError {
    InvalidEndpoint,
    CannotStopKernel,
    NotStopped,
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatectlError {
    InvalidRequest,
    IpcFilterError(IpcFilterError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcFilterError {
    TooManyEntries,
    DuplicateEndpoint,
    InvalidAddress,
}
```

### 7. 完整的系统调用接口

```rust
pub trait SyscallHandler {
    type Error;
    
    fn handle(&self, caller: &mut Proc, msg: &Message) -> Result<(), Self::Error>;
}

pub struct SchedctlSyscall;

impl SyscallHandler for SchedctlSyscall {
    type Error = SchedctlError;
    
    fn handle(&self, caller: &mut Proc, msg: &Message) -> Result<(), Self::Error> {
        let request = SchedctlRequest::from_message(msg)?;
        do_schedctl(caller, &request)
    }
}

pub struct ScheduleSyscall;

impl SyscallHandler for ScheduleSyscall {
    type Error = ScheduleError;
    
    fn handle(&self, caller: &mut Proc, msg: &Message) -> Result<(), Self::Error> {
        let request = ScheduleRequest::from_message(msg)?;
        do_schedule(caller, &request)
    }
}

pub struct RunctlSyscall;

impl SyscallHandler for RunctlSyscall {
    type Error = RunctlError;
    
    fn handle(&self, caller: &mut Proc, msg: &Message) -> Result<(), Self::Error> {
        let request = RunctlRequest::from_message(msg)?;
        do_runctl(caller, request.endpoint, request.action, request.flags)
    }
}

pub struct StatectlSyscall;

impl SyscallHandler for StatectlSyscall {
    type Error = StatectlError;
    
    fn handle(&self, caller: &mut Proc, msg: &Message) -> Result<(), Self::Error> {
        let request = StatectlRequest::from_message(msg)?;
        do_statectl(caller, request)
    }
}
```

---

## 现代 64 位硬件演进

### 多核 CPU 亲和性

```
32 位系统:
  - cpu 参数: 0-3 (最多 4 个 CPU)
  - 简单的位掩码即可表示

64 位系统:
  - cpu 参数: 0-127 或更多
  - 需要更复杂的亲和性掩码
  - 考虑 NUMA 架构
  - 迁移成本更高 (缓存失效)
```

### SMP 支持

```c
// do_runctl.c 中的 SMP 支持
#if CONFIG_SMP
if (rp->p_cpu != cpuid) {
    smp_schedule_stop_proc(rp);  // 跨 CPU 停止进程
    break;
}
#endif
```

**Rust 中的 SMP 抽象**:

```rust
#[cfg(feature = "smp")]
pub fn do_runctl_smp(target: &mut Proc, action: RunctlAction) -> Result<(), RunctlError> {
    if target.cpu != current_cpu() {
        smp_schedule_stop_proc(target);
        return Ok(());
    }
    
    // 本地 CPU 操作
    match action {
        RunctlAction::Stop => target.rts_flags.set(RtsFlags::PROC_STOP, true),
        RunctlAction::Resume => target.rts_flags.set(RtsFlags::PROC_STOP, false),
    }
    
    Ok(())
}
```

---

## 要点总结

### 核心知识点

1. **调度器委托机制**: Minix3 支持用户态调度器，通过 `p_scheduler` 指针实现调度器委托
2. **延迟停止设计**: `RC_DELAY` 标志确保进程在发送消息时不会被立即停止，避免 IPC 死锁
3. **IPC 过滤安全**: 黑名单/白名单机制提供细粒度的进程间通信控制

### 灾难预演

**场景 1: 删除权限检查**

```c
// 如果删除 do_schedule 中的权限检查
if (caller != p->p_scheduler)
    return(EPERM);
```

后果:
- 任何进程都能修改其他进程的调度参数
- 恶意进程可以将其他进程的 quantum 设为 0
- 系统完全瘫痪 (拒绝服务攻击)

**场景 2: 忽略延迟停止**

```c
// 如果不使用 RC_DELAY，直接停止进程
RTS_SET(rp, RTS_PROC_STOP);  // 立即停止
```

后果:
- 正在发送消息的进程被停止
- 接收方回复后，发送方无法处理
- IPC 通信中断，可能导致死锁

**场景 3: 不清理 IPC 引用**

```c
// 如果进程退出时不调用 CLEAR_IPC_REFS
```

后果:
- 等待该进程回复的其他进程永远阻塞
- 系统资源泄漏
- 死锁蔓延

### 互动自测

1. **问题**: `p_scheduler = NULL` 和 `p_scheduler = caller` 的区别？
   **答案**: NULL 表示内核调度，caller 表示用户态调度器

2. **问题**: 为什么 `do_schedule` 要检查 `caller == p->p_scheduler`？
   **答案**: 防止恶意进程修改其他进程的调度参数，确保只有调度器能设置参数

3. **问题**: `RC_DELAY` 标志的作用是什么？
   **答案**: 延迟停止正在发送消息的进程，避免 IPC 死锁

4. **问题**: 黑名单和白名单的默认行为有什么不同？
   **答案**: 黑名单默认允许所有 IPC，白名单默认禁止所有 IPC

5. **问题**: 为什么进程退出时需要清理 IPC 引用？
   **答案**: 避免其他进程因为不知道对方已退出而永久等待

---

**文档版本**: 2026-03-31
