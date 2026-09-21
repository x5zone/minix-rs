# kernel/system/do_privctl.c 逐行讲解

**文件路径**: `minix3/minix/kernel/system/do_privctl.c`

**总行数**: 371 行

**作用**: 实现 `SYS_PRIVCTL` 系统调用，控制系统进程特权和资源权限

---

## 1. 头文件注释

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_PRIVCTL
 *
 * The parameters for this kernel call are:
 *   m_lsys_krn_sys_privctl.endpt		(process endpoint of target)
 *   m_lsys_krn_sys_privctl.request		(privilege control request)
 *   m_lsys_krn_sys_privctl.arg_ptr		(pointer to request data)
 *   m.m_lsys_krn_sys_privctl.phys_start
 *   m.m_lsys_krn_sys_privctl.phys_len
 */
```

**参数说明**：

| 参数 | 含义 |
|------|------|
| `endpt` | 目标进程端点（可以是 SELF 表示调用者自己） |
| `request` | 请求类型（多种操作） |
| `arg_ptr` | 参数指针（传递给特权结构的数据） |
| `phys_start` | 物理地址起始（用于内存查询） |
| `phys_len` | 物理地址长度（用于内存查询） |

---

## 2. 头文件包含

```c
#include "kernel/system.h"
#include <signal.h>
#include <string.h>
#include <minix/endpoint.h>
```

**各头文件作用**：

| 头文件 | 提供内容 |
|--------|----------|
| `kernel/system.h` | 内核系统调用框架 |
| `<signal.h>` | 信号处理（sigemptyset 等） |
| `<string.h>` | 内存操作（memset, memcpy） |
| `<minix/endpoint.h>` | 端点定义（endpoint_t 等） |

---

## 3. 条件编译和辅助函数声明

```c
#if USE_PRIVCTL

#define PRIV_DEBUG 0

static int update_priv(struct proc *rp, struct priv *priv);
```

**作用**：

```
USE_PRIVCTL：
- 配置选项，决定是否编译此系统调用
- 如果系统不需要特权控制，可以禁用

PRIV_DEBUG：
- 调试开关
- 为 0 时禁用调试输出
- 为 1 时启用

update_priv：
- 静态辅助函数
- 更新进程特权结构
- 只在本文件内部使用
```

---

## 4. do_privctl 函数概述

```c
int do_privctl(struct proc * caller, message * m_ptr)
{
/* Handle sys_privctl(). Update a process' privileges. If the process is not
 * yet a system process, make sure it gets its own privilege structure.
 */
```

**函数功能**：

```
do_privctl 是 MINIX3 特权控制的核心

它负责：
1. 分配/更新特权结构（系统进程需要）
2. 控制系统进程运行状态（ALLOW/YIELD/DISALLOW）
3. 管理资源权限（I/O 端口、内存、IRQ）

只有系统进程（flags & SYS_PROC）才能调用此系统调用
```

---

## 5. 变量声明

```c
  struct proc *rp;
  proc_nr_t proc_nr;
  sys_id_t priv_id;
  sys_map_t map;
  int ipc_to_m, kcalls;
  int i, r;
  struct io_range io_range;
  struct minix_mem_range mem_range;
  struct priv priv;
  int irq;
```

**变量说明**：

| 变量 | 类型 | 含义 |
|------|------|------|
| `rp` | struct proc * | 目标进程指针 |
| `proc_nr` | proc_nr_t | 目标进程号（槽位索引） |
| `priv_id` | sys_id_t | 特权 ID |
| `map` | sys_map_t | IPC 目标位图 |
| `ipc_to_m` | int | 允许的 IPC 目标 |
| `kcalls` | int | 允许的内核调用 |
| `i, r` | int | 循环计数和返回值 |
| `io_range` | struct io_range | I/O 端口范围 |
| `mem_range` | struct minix_mem_range | 内存范围 |
| `priv` | struct priv | 特权结构副本 |
| `irq` | int | 中断号 |

---

## 6. 权限检查

```c
  /* Check whether caller is allowed to make this call. Privileged processes
   * can only update the privileges of processes that are inhibited from
   * running by the RTS_NO_PRIV flag. This flag is set when a privileged process
   * forks.
   */
  if (! (priv(caller)->s_flags & SYS_PROC)) return(EPERM);
```

**为什么需要权限检查？**

```
特权控制是敏感操作，必须严格限制：

┌─────────────────────────────────────────────────────────────────────┐
│  只有系统进程（flags 包含 SYS_PROC）才能调用 do_privctl            │
│                                                                     │
│  系统进程包括：                                                      │
│  - RS（重生服务器）                                                 │
│  - PM（进程管理器）                                                 │
│  - VM（虚拟内存管理器）                                             │
│  - VFS（文件系统）                                                   │
│  - 设备驱动程序                                                     │
│                                                                     │
│  用户进程不能调用此系统调用                                          │
└─────────────────────────────────────────────────────────────────────┘

RTS_NO_PRIV 标志：
- 当特权进程 fork 时，子进程被设置此标志
- 表示子进程暂时没有特权，需要等待 RS 设置
- 这是安全机制，防止特权进程意外泄露特权
```

---

## 7. 端点处理

```c
  if(m_ptr->m_lsys_krn_sys_privctl.endpt == SELF) okendpt(caller->p_endpoint,
	&proc_nr);
  else if(!isokendpt(m_ptr->m_lsys_krn_sys_privctl.endpt, &proc_nr))
	return(EINVAL);
  rp = proc_addr(proc_nr);
```

**端点处理逻辑**：

```
endpt 可以是：
1. SELF - 表示目标进程是调用者自己
2. 其他端点值 - 表示目标进程是指定端点

┌─────────────────────────────────────────────────────────────────────┐
│  if (endpt == SELF)                                                │
│      okendpt(caller->p_endpoint, &proc_nr)  ← 自己                                         │
│  else                                                              │
│      isokendpt(endpt, &proc_nr)  ← 验证指定的端点                   │
│                                                                     │
│  rp = proc_addr(proc_nr)  ← 获取进程指针                           │
└─────────────────────────────────────────────────────────────────────┘

SELF 的使用场景：
- 进程想要操作自己的特权结构
- 不需要知道自己的端点号
```

---

## 8. 请求分派

```c
  switch(m_ptr->m_lsys_krn_sys_privctl.request)
  {
```

**所有请求类型**：

| 请求 | 功能 |
|------|------|
| SYS_PRIV_ALLOW | 允许进程运行 |
| SYS_PRIV_YIELD | 允许目标运行，挂起调用者 |
| SYS_PRIV_DISALLOW | 禁止进程运行 |
| SYS_PRIV_CLEAR_IPC_REFS | 清除 IPC 引用 |
| SYS_PRIV_SET_SYS | 设置系统进程特权 |
| SYS_PRIV_SET_USER | 设置用户进程特权 |
| SYS_PRIV_ADD_IO | 添加 I/O 端口范围 |
| SYS_PRIV_ADD_MEM | 添加内存范围 |
| SYS_PRIV_ADD_IRQ | 添加 IRQ |
| SYS_PRIV_QUERY_MEM | 查询内存访问权限 |
| SYS_PRIV_UPDATE_SYS | 更新特权结构 |

---

## 9. SYS_PRIV_ALLOW

```c
  case SYS_PRIV_ALLOW:
	/* Allow process to run. Make sure its privilege structure has already
	 * been set.
	 */
	if (!RTS_ISSET(rp, RTS_NO_PRIV) || priv(rp)->s_proc_nr == NONE) {
		return(EPERM);
	}
	RTS_UNSET(rp, RTS_NO_PRIV);
	return(OK);
```

**功能**：允许进程运行

**前置条件检查**：

```
必须同时满足：
1. 进程有 RTS_NO_PRIV 标志（被禁止运行）
2. 进程已有特权结构（s_proc_nr != NONE）

如果进程没有 RTS_NO_PRIV，说明已经允许运行
如果进程没有特权结构，说明不是系统进程
```

**操作**：

```
清除 RTS_NO_PRIV 标志 → 进程可以运行
```

**使用场景**：

```
RS（重生服务器）启动新服务时：

1. RS fork 出新进程
2. 新进程有 RTS_NO_PRIV（暂时不能运行）
3. RS 调用 do_privctl(SYS_PRIV_SET_SYS) 设置特权
4. RS 调用 do_privctl(SYS_PRIV_ALLOW) 允许运行
5. 新进程开始执行
```

---

## 10. SYS_PRIV_YIELD

```c
  case SYS_PRIV_YIELD:
	/* Allow process to run and suspend the caller. */
	if (!RTS_ISSET(rp, RTS_NO_PRIV) || priv(rp)->s_proc_nr == NONE) {
		return(EPERM);
	}
	RTS_SET(caller, RTS_NO_PRIV);
	RTS_UNSET(rp, RTS_NO_PRIV);
	return(OK);
```

**功能**：允许目标进程运行，同时挂起调用者

**与 SYS_PRIV_ALLOW 的区别**：

```
SYS_PRIV_ALLOW：
- 只允许目标进程运行
- 调用者继续运行

SYS_PRIV_YIELD：
- 允许目标进程运行
- 同时挂起调用者（设置 RTS_NO_PRIV）
```

**使用场景**：

```
RS 启动新服务时的特权转移：

┌─────────────────────────────────────────────────────────────────────┐
│  before:                                                          │
│  RS 正在运行，新进程被禁止                                          │
│                                                                     │
│  RS 调用 do_privctl(SYS_PRIV_YIELD, 新进程)                        │
│       │                                                            │
│       ├──► RTS_SET(RS, RTS_NO_PRIV)  ← RS 被挂起                  │
│       └──► RTS_UNSET(新进程, RTS_NO_PRIV)  ← 新进程开始运行       │
│                                                                     │
│  after:                                                           │
│  新进程正在运行，RS 被挂起                                          │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 11. SYS_PRIV_DISALLOW

```c
  case SYS_PRIV_DISALLOW:
	/* Disallow process from running. */
	if (RTS_ISSET(rp, RTS_NO_PRIV)) return(EPERM);
	RTS_SET(rp, RTS_NO_PRIV);
	return(OK);
```

**功能**：禁止进程运行

**前置条件**：

```
进程当前必须没有 RTS_NO_PRIV 标志（正在运行）
```

**使用场景**：

```
PM 停止进程：

1. 进程正在运行
2. PM 调用 do_privctl(SYS_PRIV_DISALLOW)
3. 进程被设置 RTS_NO_PRIV
4. 调度器不再选择此进程
5. 进程被停止
```

---

## 12. SYS_PRIV_CLEAR_IPC_REFS

```c
  case SYS_PRIV_CLEAR_IPC_REFS:
	/* Clear pending IPC for the process. */
	clear_ipc_refs(rp, EDEADSRCDST);
	return(OK);
```

**功能**：清除进程的 IPC 引用

**EDEADSRCDST**：

```
EDEADSRCDST = DEAD SRoC + DEAD DSTination

表示通信双方都已死亡

用于：
- 进程退出时清理 IPC 引用
- 告诉内核这个进程的 IPC 引用已经无效
```

**使用场景**：

```
进程终止时：

1. 进程 A 正在和 B、C 通信
2. 进程 A 突然崩溃
3. 内核调用 do_privctl(SYS_PRIV_CLEAR_IPC_REFS)
4. B、C 收到通知，知道 A 已死亡
5. B、C 可以清理自己的状态
```

---

## 13. SYS_PRIV_SET_SYS（Part 1）

```c
  case SYS_PRIV_SET_SYS:
	/* Set a privilege structure of a blocked system process. */
	if (! RTS_ISSET(rp, RTS_NO_PRIV)) return(EPERM);

	/* Check whether a static or dynamic privilege id must be allocated. */
	priv_id = NULL_PRIV_ID;
	if (m_ptr->m_lsys_krn_sys_privctl.arg_ptr)
	{
		/* Copy privilege structure from caller */
		if((r=data_copy(caller->p_endpoint,
			m_ptr->m_lsys_krn_sys_privctl.arg_ptr, KERNEL,
			(vir_bytes) &priv, sizeof(priv))) != OK)
			return r;

		/* See if the caller wants to assign a static privilege id. */
		if(!(priv.s_flags & DYN_PRIV_ID)) {
			priv_id = priv.s_id;
		}
	}
```

**功能**：设置系统进程的特权结构

**前置条件**：

```
进程必须有 RTS_NO_PRIV 标志（被阻塞）
```

**特权 ID 分配**：

```
priv_id 的值：
- NULL_PRIV_ID (-1)：动态分配
- 其他值：静态分配

┌─────────────────────────────────────────────────────────────────────┐
│  如果调用者提供了特权结构（arg_ptr != NULL）：                       │
│                                                                     │
│  1. 从调用者复制特权结构到内核                                       │
│     data_copy(caller → kernel)                                     │
│                                                                     │
│  2. 检查是否静态分配                                                 │
│     - 如果 flags 没有 DYN_PRIV_ID，使用调用者指定的 ID               │
│     - 如果 flags 有 DYN_PRIV_ID，使用动态分配                         │
│                                                                     │
│  如果调用者没有提供特权结构：                                        │
│  - priv_id = NULL_PRIV_ID（动态分配）                               │
└─────────────────────────────────────────────────────────────────────┘
```

**静态 vs 动态特权 ID**：

```
静态特权 ID：
- 预定义的特权 ID
- 用于核心系统服务（RS, PM, VM, VFS）
- 开机时分配，不会改变

动态特权 ID：
- 运行时分配
- 用于非核心系统服务
- 服务重启时可能改变
```

---

## 14. SYS_PRIV_SET_SYS（Part 2）

```c
	/* Make sure this process has its own privileges structure. This may
	 * fail, since there are only a limited number of system processes.
	 * Then copy privileges from the caller and restore some defaults.
	 */
	if ((i=get_priv(rp, priv_id)) != OK)
	{
		printf("do_privctl: unable to allocate priv_id %d: %d\n",
			priv_id, i);
		return(i);
	}
```

**分配特权结构**：

```
get_priv(rp, priv_id)：
- 为进程 rp 分配特权结构
- 如果 priv_id != NULL_PRIV_ID，尝试分配指定 ID
- 如果 priv_id == NULL_PRIV_ID，动态分配

可能失败的原因：
- 特权 ID 已用完
- 系统进程数量达到上限
```

---

## 15. SYS_PRIV_SET_SYS（Part 3）

```c
	priv_id = priv(rp)->s_id;		/* backup privilege id */
	*priv(rp) = *priv(caller);		/* copy from caller */
	priv(rp)->s_id = priv_id;		/* restore privilege id */
	priv(rp)->s_proc_nr = proc_nr;		/* reassociate process nr */
```

**复制特权结构**：

```
内存复制过程：

┌─────────────────────────────────────────────────────────────────────┐
│  before:                                                           │
│  priv(rp)                  priv(caller)                            │
│  ┌─────────────┐          ┌─────────────┐                         │
│  │  空的/旧的   │          │  调用者的    │                         │
│  │  特权结构    │          │  特权结构    │                         │
│  └─────────────┘          └─────────────┘                         │
│                                                                     │
│  *priv(rp) = *priv(caller)                                         │
│                                                                     │
│  after:                                                            │
│  priv(rp)                  priv(caller)                            │
│  ┌─────────────┐          ┌─────────────┐                         │
│  │  复制自      │  ───►    │  调用者的    │                         │
│  │  调用者      │          │  特权结构    │                         │
│  └─────────────┘          └─────────────┘                         │
│                                                                     │
│  然后恢复：                                                         │
│  priv(rp)->s_id = priv_id  ← 恢复原始特权 ID                        │
│  priv(rp)->s_proc_nr = proc_nr  ← 重新关联进程号                   │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 16. SYS_PRIV_SET_SYS（Part 4）

```c
	for (i=0; i< NR_SYS_CHUNKS; i++)		/* remove pending: */
	      priv(rp)->s_asyn_pending.chunk[i] = 0;	/* - incoming asyn */
	for (i=0; i< NR_SYS_CHUNKS; i++)		/*   messages */
	      priv(rp)->s_notify_pending.chunk[i] = 0;	/* - notifications */
	priv(rp)->s_int_pending = 0;			/* - interrupts */
	(void) sigemptyset(&priv(rp)->s_sig_pending);	/* - signals */
	reset_kernel_timer(&priv(rp)->s_alarm_timer);	/* - alarm */
	priv(rp)->s_asyntab= -1;			/* - asynsends */
	priv(rp)->s_asynsize= 0;
	priv(rp)->s_asynendpoint = rp->p_endpoint;
	priv(rp)->s_diag_sig = FALSE;		/* no request for diag sigs */
```

**清除待处理消息**：

```
为什么要清除这些？

当新进程被创建时（fork），它可能继承了父进程的一些状态：
- 正在发送的消息
- 等待中的通知
- 挂起的中断
- 待处理的信号
- 闹钟

如果不清除，新进程可能会收到发给父进程的消息！

清除内容：

1. s_asyn_pending - 异步消息待处理
2. s_notify_pending - 通知待处理
3. s_int_pending - 中断待处理
4. s_sig_pending - 信号待处理
5. s_alarm_timer - 闹钟定时器
6. s_asyntab - asynsend 表
```

---

## 17. SYS_PRIV_SET_SYS（Part 5）

```c
	/* Set defaults for privilege bitmaps. */
	priv(rp)->s_flags= DSRV_F;           /* privilege flags */
	priv(rp)->s_init_flags= DSRV_I;      /* initialization flags */
	priv(rp)->s_trap_mask= DSRV_T;       /* allowed traps */
	memset(&map, 0, sizeof(map));
	ipc_to_m = DSRV_M;                   /* allowed targets */
	if (ipc_to_m == ALL_M) {
		for (i = 0; i < NR_SYS_PROCS; i++)
			set_sys_bit(map, i);
	}
	fill_sendto_mask(rp, &map);
	kcalls = DSRV_KC;                    /* allowed kernel calls */
	for(i = 0; i < SYS_CALL_MASK_SIZE; i++) {
		priv(rp)->s_k_call_mask[i] = (kcalls == NO_C ? 0 : (~0));
	}

	/* Set the default signal managers. */
	priv(rp)->s_sig_mgr = DSRV_SM;
	priv(rp)->s_bak_sig_mgr = NONE;
```

**设置默认特权**：

| 字段 | 默认值 | 含义 |
|------|--------|------|
| s_flags | DSRV_F | 特权标志 |
| s_init_flags | DSRV_I | 初始化标志 |
| s_trap_mask | DSRV_T | 允许的陷阱（系统调用） |
| s_k_call_mask | - | 允许的内核调用 |
| s_sig_mgr | DSRV_SM | 主信号管理器 |
| s_bak_sig_mgr | NONE | 备用信号管理器 |

**DSRV_* 常量**：

```
DSRV_F = Default SeRVer Flags
DSRV_I = Default SeRVer Init flags
DSRV_T = Default SeRVer Trap mask
DSRV_M = Default SeRVer allowed ipc tarGets
DSRV_KC = Default SeRVer Kernel Calls
DSRV_SM = Default SeRVer Signal Manager
```

---

## 18. SYS_PRIV_SET_SYS（Part 6）

```c
	/* Set defaults for resources: no I/O resources, no memory resources,
	 * no IRQs, no grant table, no ipc filter
	 */
	priv(rp)->s_nr_io_range= 0;
	priv(rp)->s_nr_mem_range= 0;
	priv(rp)->s_nr_irq= 0;
	priv(rp)->s_grant_table= 0;
	priv(rp)->s_grant_entries= 0;
	priv(rp)->s_grant_endpoint = rp->p_endpoint;
	priv(rp)->s_state_table= 0;
	priv(rp)->s_state_entries= 0;
	priv(rp)->s_ipcf= 0;
```

**设置默认资源**：

```
新进程默认没有资源权限：

| 资源 | 默认值 | 含义 |
|------|--------|------|
| s_nr_io_range | 0 | I/O 端口范围数量 |
| s_nr_mem_range | 0 | 内存范围数量 |
| s_nr_irq | 0 | IRQ 数量 |
| s_grant_table | 0 | 授权表地址 |
| s_grant_entries | 0 | 授权表条目数 |
| s_grant_endpoint | rp->p_endpoint | 授权端点 |
| s_state_table | 0 | 状态表地址 |
| s_state_entries | 0 | 状态表条目数 |
| s_ipcf | 0 | IPC 过滤器 |

驱动程序需要后续调用 ADD_IO, ADD_MEM, ADD_IRQ 来申请资源
```

---

## 19. SYS_PRIV_SET_SYS（Part 7）

```c
	/* Override defaults if the caller has supplied a privilege structure. */
	if (m_ptr->m_lsys_krn_sys_privctl.arg_ptr)
	{
		if((r = update_priv(rp, &priv)) != OK) {
			return r;
		}
	}

	return(OK);
```

**调用 update_priv**：

```
如果调用者提供了自定义特权结构，调用 update_priv 更新

update_priv 会根据提供的数据覆盖默认值
```

---

## 20. SYS_PRIV_SET_USER

```c
  case SYS_PRIV_SET_USER:
	/* Set a privilege structure of a blocked user process. */
	if (!RTS_ISSET(rp, RTS_NO_PRIV)) return(EPERM);

	/* Link the process to the privilege structure of the root user
	 * process all the user processes share.
	 */
	priv(rp) = priv_addr(USER_PRIV_ID);

	return(OK);
```

**功能**：设置用户进程的特权结构

**关键点**：所有用户进程共享同一个特权结构！

```
用户进程的特点：
- 不需要独立特权结构
- 共享 USER_PRIV_ID
- 特权较低

priv(rp) = priv_addr(USER_PRIV_ID)
                      │
                      ▼
              ┌─────────────────┐
              │ USER_PRIV_ID    │
              │ (所有用户进程共享) │
              └─────────────────┘
```

**为什么用户进程共享特权结构？**

```
安全考虑：
- 用户进程特权较低
- 只能做基本操作
- 不需要隔离

节省资源：
- 特权结构数量有限
- 用户进程可能很多
- 共享可以节省内存
```

---

## 21. SYS_PRIV_ADD_IO

```c
  case SYS_PRIV_ADD_IO:
	if (RTS_ISSET(rp, RTS_NO_PRIV))
		return(EPERM);

#if 0 /* XXX -- do we need a call for this? */
	if (strcmp(rp->p_name, "fxp") == 0 ||
		strcmp(rp->p_name, "rtl8139") == 0)
	{
		printf("setting ipc_stats_target to %d\n", rp->p_endpoint);
		ipc_stats_target= rp->p_endpoint;
	}
#endif

	/* Get the I/O range */
	data_copy(caller->p_endpoint, m_ptr->m_lsys_krn_sys_privctl.arg_ptr,
		KERNEL, (vir_bytes) &io_range, sizeof(io_range));
	/* Add the I/O range */
	return priv_add_io(rp, &io_range);
```

**功能**：添加 I/O 端口范围

**用途**：驱动程序需要访问硬件 I/O 端口

```
例如：
- 网络驱动程序需要访问网卡的 I/O 端口
- 磁盘驱动程序需要访问控制器的 I/O 端口

io_range 结构：
┌─────────────────────────────────────────────────────────────────────┐
│  struct io_range {                                                  │
│      u32_t ior_base;  // 起始端口                                   │
│      u32_t ior_limit;  // 结束端口                                  │
│  };                                                                 │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 22. SYS_PRIV_ADD_MEM

```c
  case SYS_PRIV_ADD_MEM:
	if (RTS_ISSET(rp, RTS_NO_PRIV))
		return(EPERM);

	/* Get the memory range */
	if((r=data_copy(caller->p_endpoint,
		m_ptr->m_lsys_krn_sys_privctl.arg_ptr, KERNEL,
		(vir_bytes) &mem_range, sizeof(mem_range))) != OK)
		return r;
	/* Add the memory range */
	return priv_add_mem(rp, &mem_range);
```

**功能**：添加内存访问范围

**用途**：驱动程序需要访问物理内存

```
例如：
- 网络驱动程序需要访问网卡的 DMA 缓冲区
- 显卡驱动程序需要访问显存

mem_range 结构：
┌─────────────────────────────────────────────────────────────────────┐
│  struct minix_mem_range {                                           │
│      phys_bytes mr_base;   // 起始物理地址                          │
│      phys_bytes mr_limit;  // 结束物理地址                         │
│  };                                                                 │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 23. SYS_PRIV_ADD_IRQ

```c
  case SYS_PRIV_ADD_IRQ:
	if (RTS_ISSET(rp, RTS_NO_PRIV))
		return(EPERM);

#if 0
	/* Only system processes get IRQs? */
	if (!(priv(rp)->s_flags & SYS_PROC))
		return EPERM;
#endif
	data_copy(caller->p_endpoint, m_ptr->m_lsys_krn_sys_privctl.arg_ptr,
		KERNEL, (vir_bytes) &irq, sizeof(irq));
	/* Add the IRQ. */
	return priv_add_irq(rp, irq);
```

**功能**：注册中断请求线

**用途**：驱动程序需要处理硬件中断

```
中断处理流程：

┌─────────────────────────────────────────────────────────────────────┐
│  硬件中断发生                                                        │
│      │                                                              │
│      ▼                                                              │
│  CPU 收到中断信号                                                    │
│      │                                                              │
│      ▼                                                              │
│  内核查找中断向量表                                                  │
│      │                                                              │
│      ▼                                                              │
│  调用对应 IRQ 的处理程序                                             │
│      │                                                              │
│      ▼                                                              │
│  如果进程注册了这个 IRQ：                                            │
│  - 调用进程的中断处理程序                                            │
│  - 返回后继续执行被中断的代码                                        │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 24. SYS_PRIV_QUERY_MEM

```c
  case SYS_PRIV_QUERY_MEM:
  {
	phys_bytes addr, limit;
  	struct priv *sp;
	/* See if a certain process is allowed to map in certain physical
	 * memory.
	 */
	addr = (phys_bytes) m_ptr->m_lsys_krn_sys_privctl.phys_start;
	limit = addr + (phys_bytes) m_ptr->m_lsys_krn_sys_privctl.phys_len - 1;
	if(limit < addr)
		return EPERM;
	if(!(sp = priv(rp)))
		return EPERM;
	for(i = 0; i < sp->s_nr_mem_range; i++) {
		if(addr >= sp->s_mem_tab[i].mr_base &&
		   limit <= sp->s_mem_tab[i].mr_limit)
			return OK;
	}
	return EPERM;
  }
```

**功能**：查询进程是否有权限访问指定物理内存

**参数**：

| 参数 | 含义 |
|------|------|
| phys_start | 要查询的起始物理地址 |
| phys_len | 要查询的内存长度 |

**返回值**：

```
OK - 有权限访问
EPERM - 没有权限访问
```

**检查逻辑**：

```
遍历进程的所有内存范围：
for (i = 0; i < s_nr_mem_range; i++) {
    if (addr >= mr_base && limit <= mr_limit) {
        return OK;  // 在范围内，有权限
    }
}
return EPERM;  // 不在任何范围内，没有权限
```

---

## 25. SYS_PRIV_UPDATE_SYS

```c
  case SYS_PRIV_UPDATE_SYS:
	/* Update the privilege structure of a system process. */
	if(!m_ptr->m_lsys_krn_sys_privctl.arg_ptr) return EINVAL;

	/* Copy privilege structure from caller */
	if((r=data_copy(caller->p_endpoint,
		m_ptr->m_lsys_krn_sys_privctl.arg_ptr, KERNEL,
		(vir_bytes) &priv, sizeof(priv))) != OK)
		return r;

	/* Override settings in existing privilege structure. */
	if((r = update_priv(rp, &priv)) != OK) {
		return r;
	}

	return(OK);
```

**功能**：更新系统进程的特权结构

**与 SYS_PRIV_SET_SYS 的区别**：

```
SYS_PRIV_SET_SYS：
- 为进程分配新的特权结构
- 复制调用者的特权结构作为基础
- 然后更新

SYS_PRIV_UPDATE_SYS：
- 更新已存在的特权结构
- 不分配新的特权结构
- 只更新指定的字段
```

---

## 26. 默认处理

```c
  default:
	printf("do_privctl: bad request %d\n",
		m_ptr->m_lsys_krn_sys_privctl.request);
	return EINVAL;
```

**无效请求处理**：

```
打印错误信息到控制台
返回 EINVAL
```

---

## 27. update_priv 函数

```c
static int update_priv(struct proc *rp, struct priv *priv)
{
/* Update the privilege structure of a given process. */

  int i;

  /* Copy flags and signal managers. */
  priv(rp)->s_flags = priv->s_flags;
  priv(rp)->s_init_flags = priv->s_init_flags;
  priv(rp)->s_sig_mgr = priv->s_sig_mgr;
  priv(rp)->s_bak_sig_mgr = priv->s_bak_sig_mgr;
```

**复制基本字段**：

```
s_flags - 特权标志
s_init_flags - 初始化标志
s_sig_mgr - 主信号管理器
s_bak_sig_mgr - 备用信号管理器
```

---

## 28. 复制 IRQ

```c
  /* Copy IRQs. */
  if(priv->s_flags & CHECK_IRQ) {
  	if (priv->s_nr_irq < 0 || priv->s_nr_irq > NR_IRQ)
  		return EINVAL;
  	priv(rp)->s_nr_irq= priv->s_nr_irq;
  	for (i= 0; i<priv->s_nr_irq; i++)
  	{
  		priv(rp)->s_irq_tab[i]= priv->s_irq_tab[i];
#if PRIV_DEBUG
  		printf("do_privctl: adding IRQ %d for %d\n",
  			priv(rp)->s_irq_tab[i], rp->p_endpoint);
#endif
  	}
  }
```

**CHECK_IRQ 标志**：

```
CHECK_IRQ 表示进程需要检查 IRQ 权限

如果设置了这个标志：
1. 验证 IRQ 数量有效（0 <= nr_irq <= NR_IRQ）
2. 复制 IRQ 表
3. 调试模式下打印日志

如果没有设置这个标志：
- IRQ 相关字段保持不变
```

---

## 29. 复制 I/O 范围

```c
  /* Copy I/O ranges. */
  if(priv->s_flags & CHECK_IO_PORT) {
  	if (priv->s_nr_io_range < 0 || priv->s_nr_io_range > NR_IO_RANGE)
  		return EINVAL;
  	priv(rp)->s_nr_io_range= priv->s_nr_io_range;
  	for (i= 0; i<priv->s_nr_io_range; i++)
  	{
  		priv(rp)->s_io_tab[i]= priv->s_io_tab[i];
#if PRIV_DEBUG
  		printf("do_privctl: adding I/O range [%x..%x] for %d\n",
  			priv(rp)->s_io_tab[i].ior_base,
  			priv(rp)->s_io_tab[i].ior_limit,
  			rp->p_endpoint);
#endif
  	}
  }
```

**CHECK_IO_PORT 标志**：

```
类似 CHECK_IRQ，用于 I/O 端口范围权限
```

---

## 30. 复制内存范围

```c
  /* Copy memory ranges. */
  if(priv->s_flags & CHECK_MEM) {
  	if (priv->s_nr_mem_range < 0 || priv->s_nr_mem_range > NR_MEM_RANGE)
  		return EINVAL;
  	priv(rp)->s_nr_mem_range= priv->s_nr_mem_range;
  	for (i= 0; i<priv->s_nr_mem_range; i++)
  	{
  		priv(rp)->s_mem_tab[i]= priv->s_mem_tab[i];
#if PRIV_DEBUG
  		printf("do_privctl: adding mem range [%x..%x] for %d\n",
  			priv(rp)->s_mem_tab[i].mr_base,
  			priv(rp)->s_mem_tab[i].mr_limit,
  			rp->p_endpoint);
#endif
  	}
  }
```

**CHECK_MEM 标志**：

```
用于内存访问权限
```

---

## 31. 复制其他字段

```c
  /* Copy trap mask. */
  priv(rp)->s_trap_mask = priv->s_trap_mask;

  /* Copy target mask. */
#if PRIV_DEBUG
  printf("do_privctl: Setting ipc target mask for %d:");
  for (i=0; i < NR_SYS_PROCS; i += BITCHUNK_BITS) {
  	printf(" %08x", get_sys_bits(priv->s_ipc_to, i));
  }
  printf("\n");
#endif

  fill_sendto_mask(rp, &priv->s_ipc_to);

  /* Copy kernel call mask. */
  memcpy(priv(rp)->s_k_call_mask, priv->s_k_call_mask,
  	sizeof(priv(rp)->s_k_call_mask));

  return OK;
}
```

**复制 IPC 目标和内核调用掩码**：

```
s_trap_mask - 允许的陷阱（系统调用）
s_ipc_to - 允许的 IPC 目标进程
s_k_call_mask - 允许的内核调用
```

---

## 32. 流程图

```
┌─────────────────────────────────────────────────────────────────────┐
│                    do_privctl 完整流程                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. 权限检查                                                        │
│     priv(caller)->s_flags & SYS_PROC ?                              │
│         ├──► 否：return EPERM                                       │
│         └──► 是：继续                                                │
│                                                                     │
│  2. 端点验证                                                        │
│     endpt == SELF ?                                                 │
│         ├──► 是：使用调用者的端点                                    │
│         └──► 否：验证指定的端点                                      │
│                                                                     │
│  3. switch(request)                                                 │
│     ├─► SYS_PRIV_ALLOW       → 清除 RTS_NO_PRIV                     │
│     ├─► SYS_PRIV_YIELD       → 清除目标 + 设置自己 RTS_NO_PRIV       │
│     ├─► SYS_PRIV_DISALLOW    → 设置 RTS_NO_PRIV                     │
│     ├─► SYS_PRIV_CLEAR_IPC_REFS → clear_ipc_refs()                  │
│     ├─► SYS_PRIV_SET_SYS     → 分配 + 复制 + 清除待处理消息          │
│     ├─► SYS_PRIV_SET_USER    → 链接到 USER_PRIV_ID                  │
│     ├─► SYS_PRIV_ADD_IO      → priv_add_io()                       │
│     ├─► SYS_PRIV_ADD_MEM     → priv_add_mem()                      │
│     ├─► SYS_PRIV_ADD_IRQ     → priv_add_irq()                       │
│     ├─► SYS_PRIV_QUERY_MEM   → 检查内存范围                         │
│     └─► SYS_PRIV_UPDATE_SYS  → update_priv()                        │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 33. 特权层次总结

```
┌─────────────────────────────────────────────────────────────────────┐
│                    MINIX3 特权层次                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  用户进程（USER_PRIV_ID）                                    │   │
│  │  - 所有用户进程共享同一个特权结构                              │   │
│  │  - 特权最低                                                  │   │
│  │  - 不能访问 I/O 端口、特殊内存、IRQ                          │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              ▲                                      │
│                              │                                      │
│                              │ SYS_PRIV_SET_USER                    │
│                              │                                      │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  系统进程（每个有独立特权结构）                               │   │
│  │  - RS, PM, VM, VFS                                         │   │
│  │  - 驱动程序                                                 │   │
│  │  - 可以添加 I/O、内存、IRQ 权限                              │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  内核（无特权结构）                                           │   │
│  │  - 可以做任何事情                                            │   │
│  │  - 不受特权系统限制                                          │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 34. 要点总结

1. **SYS_PRIV_SET_SYS** 是核心 - 设置系统进程特权
2. **RTS_NO_PRIV** 标志控制进程是否能运行
3. **资源权限** 需要单独添加（ADD_IO/ADD_MEM/ADD_IRQ）
4. **用户进程共享特权** - 节省资源

---

## 35. 灾难预演

### 如果忘记权限检查

```
后果：
1. 任何进程可以修改其他进程特权
2. 恶意进程可以提升自己的特权
3. 系统安全被破坏
```

### 如果不清除待处理消息

```
后果：
1. 新进程收到发给父进程的消息
2. 进程状态混乱
3. 可能导致死锁或崩溃
```

### 如果不检查资源范围

```
后果：
1. 进程可能访问未授权的资源
2. 驱动程序可能访问错误的硬件
3. 系统不稳定或安全漏洞
```

---

## 36. 互动自测

1. **问题**：为什么用户进程共享 USER_PRIV_ID？
   **答案**：用户进程特权较低，不需要隔离，共享可以节省特权结构资源。

2. **问题**：SYS_PRIV_YIELD 和 SYS_PRIV_ALLOW 的区别？
   **答案**：ALLOW 只允许目标运行，YIELD 同时挂起调用者自己。

3. **问题**：为什么要清除待处理消息？
   **答案**：新进程不应该继承父进程的 IPC 状态，否则会收到发给父进程的消息。

4. **问题**：CHECK_IRQ 标志的作用？
   **答案**：表示进程需要 IRQ 权限，只有在标志设置时才会复制 IRQ 表。

5. **问题**：为什么系统进程需要独立的特权结构？
   **答案**：系统进程需要访问受保护的资源（I/O、内存、IRQ），需要隔离管理。

---

## 37. Rust 重构建议

```rust
bitflags! {
    pub struct PrivFlags: u32 {
        const SYS_PROC = 0x01;
        const CHECK_IRQ = 0x02;
        const CHECK_IO_PORT = 0x04;
        const CHECK_MEM = 0x08;
        const DYN_PRIV_ID = 0x10;
    }
}

pub enum PrivCtlError {
    PermissionDenied,
    InvalidEndpoint,
    PrivAllocationFailed,
    InvalidResourceRange,
    InvalidPrivId,
}

pub enum PrivCtlRequest {
    Allow,
    Yield,
    Disallow,
    ClearIpcRefs,
    SetSys { priv_struct: Option<PrivStruct> },
    SetUser,
    AddIo { range: IoRange },
    AddMem { range: MemRange },
    AddIrq { irq: u32 },
    QueryMem { addr: PhysAddr, len: usize },
    UpdateSys { priv_struct: PrivStruct },
}

pub fn do_privctl(
    caller: &Proc,
    request: &PrivctlRequest,
) -> Result<(), PrivCtlError> {
    if !caller.has_flag(PrivFlags::SYS_PROC) {
        return Err(PrivCtlError::PermissionDenied);
    }

    match request {
        PrivCtlRequest::Allow => {
            ensure!(target.is_no_priv() && target.has_priv());
            target.unset_no_priv();
        }
        PrivCtlRequest::Yield => {
            ensure!(target.is_no_priv() && target.has_priv());
            caller.set_no_priv();
            target.unset_no_priv();
        }
        PrivCtlRequest::Disallow => {
            ensure!(!target.is_no_priv());
            target.set_no_priv();
        }
        PrivCtlRequest::ClearIpcRefs => {
            clear_ipc_refs(target, ErrCode::DeadSrcDst);
        }
        PrivCtlRequest::SetSys { priv_struct } => {
            let priv_id = priv_struct.as_ref()
                .map(|p| if p.flags.contains(PrivFlags::DYN_PRIV_ID) {
                    None
                } else {
                    Some(p.id)
                })
                .unwrap_or(None);

            target.allocate_priv(priv_id)?;
            target.copy_privileges_from(caller);

            if let Some(ps) = priv_struct {
                target.update_priv(ps)?;
            }
        }
        PrivCtlRequest::SetUser => {
            target.link_to_user_priv();
        }
        // ... 其他请求
    }
    Ok(())
}
```

---

**文档版本**: 2026-03-30