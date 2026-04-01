# 特权安全系统调用

> **模块定位**: 进程权限管理与内存授权机制
> 
> **核心文件**:
> - `minix3/minix/kernel/system/do_privctl.c` (371行)
> - `minix3/minix/kernel/system/do_setgrant.c` (30行)

---

## 模块架构总览

### 两个系统调用的协作关系

```
┌─────────────────────────────────────────────────────────────────────┐
│                     特权安全模块全景图                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  特权结构管理 (谁有什么权限)                                  │  │
│  │  ┌────────────────┐        ┌────────────────┐               │  │
│  │  │ do_privctl     │───────►│ 特权分配       │               │  │
│  │  │ (SYS_PRIVCTL)  │        │ - 系统进程特权 │               │  │
│  │  └────────────────┘        │ - 用户进程特权 │               │  │
│  │                            │ - 运行控制     │               │  │
│  │                            │ - 资源权限     │               │  │
│  │                            └────────────────┘               │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                              │                                      │
│                              │ 特权结构指针                         │
│                              ▼                                      │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  授权表管理 (内存访问授权)                                    │  │
│  │  ┌────────────────┐        ┌────────────────┐               │  │
│  │  │ do_setgrant    │───────►│ Grant Table    │               │  │
│  │  │ (SYS_SETGRANT) │        │ - 内存授权表   │               │  │
│  │  └────────────────┘        │ - 安全访问     │               │  │
│  │                            └────────────────┘               │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 核心设计理念

**1. 特权层次结构**

Minix3 采用分层的特权模型：

```
┌─────────────────────────────────────────────────────────────────────┐
│  特权层次结构                                                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  用户进程（低特权）                                                   │
│  ├── 共享 USER_PRIV_ID 特权结构                                      │
│  ├── 无独立特权结构                                                  │
│  └── 受限的系统调用访问                                              │
│                                                                     │
│  系统进程（高特权）                                                   │
│  ├── RS（重生服务器）- 最高特权                                      │
│  ├── PM（进程管理器）- 进程管理特权                                  │
│  ├── VM（虚拟内存）- 内存管理特权                                    │
│  ├── VFS（文件系统）- 文件系统特权                                   │
│  └── 驱动程序 - 设备访问特权                                         │
│      └── 每个系统进程有独立的特权结构                                │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**2. Grant 机制的本质**

```
Grant = "内核背书的跨进程指针"
      = (granter, grantee, addr, len, rights)
      = "A 允许 B 访问我这段内存"
```

**核心用途**: 受控的一次性内存访问（非长期共享）

| 机制 | 特点 | 适用场景 |
|------|------|---------|
| shared memory | 长期映射 | 高频大数据共享 |
| **grant** | 短期、受控、按需授权 | 安全的跨进程数据传递 |

**3. 三层隔离模型**

```
用户进程        不可信
服务器进程      半可信
内核            完全可信
```

Grant 把"是否允许访问"的决策集中在内核。

---

## 特权结构管理机制

### do_privctl - 特权控制接口

**源代码位置**: [do_privctl.c](file://../minix3/minix/kernel/system/do_privctl.c)

**核心功能**: 控制系统进程特权和运行状态

#### 逐行代码分析

**源码位置**: [do_privctl.c:1-371](file://../minix3/minix/kernel/system/do_privctl.c#L1-L371)

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

#include "kernel/system.h"
#include <signal.h>
#include <string.h>
#include <minix/endpoint.h>

#if USE_PRIVCTL

#define PRIV_DEBUG 0

static int update_priv(struct proc *rp, struct priv *priv);

/*===========================================================================*
 *				do_privctl				     *
 *===========================================================================*/
int do_privctl(struct proc * caller, message * m_ptr)
{
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

  /* Check whether caller is allowed to make this call. */
  if (! (priv(caller)->s_flags & SYS_PROC)) return(EPERM);
  
  if(m_ptr->m_lsys_krn_sys_privctl.endpt == SELF) 
      okendpt(caller->p_endpoint, &proc_nr);
  else if(!isokendpt(m_ptr->m_lsys_krn_sys_privctl.endpt, &proc_nr))
      return(EINVAL);
  
  rp = proc_addr(proc_nr);

  switch(m_ptr->m_lsys_krn_sys_privctl.request)
  {
  case SYS_PRIV_ALLOW:
	if (!RTS_ISSET(rp, RTS_NO_PRIV) || priv(rp)->s_proc_nr == NONE) {
		return(EPERM);
	}
	RTS_UNSET(rp, RTS_NO_PRIV);
	return(OK);

  case SYS_PRIV_YIELD:
	if (!RTS_ISSET(rp, RTS_NO_PRIV) || priv(rp)->s_proc_nr == NONE) {
		return(EPERM);
	}
	RTS_SET(caller, RTS_NO_PRIV);
	RTS_UNSET(rp, RTS_NO_PRIV);
	return(OK);
    ...
  }
}
```

**逐行解析**:

| 行号 | 代码 | 内存位置 | 作用 |
|------|------|---------|------|
| 1-8 | 注释 | - | 文档说明：系统调用类型、参数定义 |
| 10-13 | `#include` | - | 引入系统头文件、信号、字符串、端点定义 |
| 15 | `#if USE_PRIVCTL` | - | 条件编译：是否启用 PRIVCTL |
| 17 | `#define PRIV_DEBUG 0` | - | 调试开关（0=关闭） |
| 19 | `static int update_priv(...)` | - | 前向声明：更新特权结构的辅助函数 |
| 26-35 | 变量声明 | 栈 | 局部变量：进程指针、进程号、特权ID等 |

**关键代码段详解**:

**1. 权限检查 (第37-38行)**

```c
/* Check whether caller is allowed to make this call. */
if (! (priv(caller)->s_flags & SYS_PROC)) return(EPERM);
```

**为什么只有系统进程才能调用？**

```
特权控制是高风险操作:
┌─────────────────────────────────────────────────────────────────────┐
│  如果允许用户进程调用:                                               │
│      ├─► 用户进程可以提升自己的权限                                  │
│      ├─► 用户进程可以访问任意内存                                    │
│      ├─► 用户进程可以访问任意 I/O 端口                               │
│      └─► 系统安全完全崩溃                                            │
│                                                                     │
│  限制为系统进程:                                                     │
│      ├─► 只有 RS (Reincarnation Server) 可以创建新服务              │
│      ├─► 只有 PM (Process Manager) 可以管理进程特权                 │
│      └─► 系统安全得到保障                                            │
└─────────────────────────────────────────────────────────────────────┘
```

**SYS_PROC 标志位**:

源码位置: [priv.h](file://../minix3/minix/include/minix/priv.h)

```c
#define SYS_PROC        0x0001	/* system process */
```

**2. 端点验证 (第40-44行)**

```c
if(m_ptr->m_lsys_krn_sys_privctl.endpt == SELF) 
    okendpt(caller->p_endpoint, &proc_nr);
else if(!isokendpt(m_ptr->m_lsys_krn_sys_privctl.endpt, &proc_nr))
    return(EINVAL);

rp = proc_addr(proc_nr);
```

**SELF 宏的作用**:

```c
#define SELF    -1      /* indicate self process */
```

允许调用者操作自己：
- `sys_privctl(SELF, ...)` → 操作调用者自己
- `sys_privctl(other_endpoint, ...)` → 操作其他进程

**3. SYS_PRIV_ALLOW - 允许进程运行 (第48-54行)**

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

**宏定义源码**: [minix/com.h:342](file://../minix3/minix/include/minix/com.h#L342)

```c
#define SYS_PRIV_ALLOW		1	/* Allow process to run */
```

**条件检查详解**:

```
条件 1: !RTS_ISSET(rp, RTS_NO_PRIV)
  └─► 进程没有被禁止运行
  └─► 如果进程已经可以运行，返回 EPERM

条件 2: priv(rp)->s_proc_nr == NONE
  └─► 进程没有关联的特权结构
  └─► 如果特权结构未设置，返回 EPERM

只有满足:
  ├─► 进程被禁止运行 (RTS_NO_PRIV 已设置)
  └─► 特权结构已设置 (s_proc_nr != NONE)
才允许清除 RTS_NO_PRIV 标志
```

**RTS_NO_PRIV 标志的作用**:

```
进程创建时的特权初始化流程:
┌─────────────────────────────────────────────────────────────────────┐
│  1. fork() 创建新进程                                               │
│     ├─► 新进程继承父进程的特权结构                                   │
│     └─► 如果父进程是系统进程，新进程被标记为 RTS_NO_PRIV            │
│                                                                     │
│  2. RS 调用 sys_privctl(SET_SYS) 设置特权                           │
│     ├─► 分配独立的特权结构                                          │
│     └─► 进程仍然被标记为 RTS_NO_PRIV                                │
│                                                                     │
│  3. RS 调用 sys_privctl(ALLOW) 允许运行                             │
│     ├─► 清除 RTS_NO_PRIV 标志                                       │
│     └─► 进程开始运行                                                │
└─────────────────────────────────────────────────────────────────────┘
```

**4. SYS_PRIV_YIELD - 让权运行 (第56-63行)**

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

**宏定义源码**: [minix/com.h:352](file://../minix3/minix/include/minix/com.h#L352)

```c
#define SYS_PRIV_YIELD	       10	/* Allow process to run and suspend */
```

**YIELD vs ALLOW 的区别**:

| 操作 | 目标进程 | 调用者 | 使用场景 |
|------|---------|--------|---------|
| `ALLOW` | 开始运行 | 继续运行 | RS 启动服务后继续工作 |
| `YIELD` | 开始运行 | 被挂起 | RS 启动服务后等待服务完成初始化 |

**YIELD 的典型使用场景**:

```
RS 启动新服务流程:
┌─────────────────────────────────────────────────────────────────────┐
│  1. RS 调用 sys_privctl(SET_SYS, new_service)                      │
│     └─► 设置新服务的特权结构                                        │
│                                                                     │
│  2. RS 调用 sys_privctl(YIELD, new_service)                        │
│     ├─► 新服务开始运行                                              │
│     ├─► RS 被挂起 (RTS_NO_PRIV)                                    │
│     └─► RS 等待新服务初始化完成                                     │
│                                                                     │
│  3. 新服务初始化完成后，通知 RS                                     │
│     └─► RS 被唤醒，继续工作                                         │
└─────────────────────────────────────────────────────────────────────┘
```

**5. SYS_PRIV_DISALLOW - 禁止运行 (第65-70行)**

```c
case SYS_PRIV_DISALLOW:
    /* Disallow process from running. */
    if (RTS_ISSET(rp, RTS_NO_PRIV)) return(EPERM);
    RTS_SET(rp, RTS_NO_PRIV);
    return(OK);
```

**宏定义源码**: [minix/com.h:343](file://../minix3/minix/include/minix/com.h#L343)

```c
#define SYS_PRIV_DISALLOW	2	/* Disallow process to run */
```

**使用场景**:

- PM 需要停止进程时
- 进程需要被重启时
- 进程需要被更新时

**6. SYS_PRIV_SET_SYS - 设置系统进程特权 (第72-170行)**

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

    /* Make sure this process has its own privileges structure. */
    if ((i=get_priv(rp, priv_id)) != OK)
    {
        printf("do_privctl: unable to allocate priv_id %d: %d\n",
            priv_id, i);
        return(i);
    }
    priv_id = priv(rp)->s_id;		/* backup privilege id */
    *priv(rp) = *priv(caller);		/* copy from caller */
    priv(rp)->s_id = priv_id;		/* restore privilege id */
    priv(rp)->s_proc_nr = proc_nr;		/* reassociate process nr */
    ...
```

**特权结构复制流程**:

```
┌─────────────────────────────────────────────────────────────────────┐
│  步骤 1: 备份特权 ID                                                │
│      priv_id = priv(rp)->s_id;                                      │
│      └─► 保存目标进程的特权 ID                                      │
│                                                                     │
│  步骤 2: 复制特权结构                                               │
│      *priv(rp) = *priv(caller);                                     │
│      └─► 从调用者复制整个特权结构                                   │
│      └─► 包括所有权限、标志、资源                                   │
│                                                                     │
│  步骤 3: 恢复特权 ID                                                │
│      priv(rp)->s_id = priv_id;                                      │
│      └─► 恢复目标进程的特权 ID                                      │
│      └─► 避免特权 ID 冲突                                           │
│                                                                     │
│  步骤 4: 重新关联进程号                                             │
│      priv(rp)->s_proc_nr = proc_nr;                                 │
│      └─► 特权结构指向正确的进程                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**为什么要备份和恢复特权 ID？**

```
特权结构复制问题:
┌─────────────────────────────────────────────────────────────────────┐
│  调用者 (RS):                                                       │
│      priv(RS)->s_id = RS_PRIV_ID                                    │
│      priv(RS)->s_proc_nr = RS_PROC_NR                               │
│                                                                     │
│  目标进程 (新服务):                                                  │
│      priv(new)->s_id = NEW_PRIV_ID                                  │
│      priv(new)->s_proc_nr = NEW_PROC_NR                             │
│                                                                     │
│  如果直接复制:                                                       │
│      *priv(new) = *priv(RS);                                        │
│      ├─► priv(new)->s_id = RS_PRIV_ID  ← 错误！                     │
│      └─► priv(new)->s_proc_nr = RS_PROC_NR ← 错误！                 │
│                                                                     │
│  正确做法:                                                           │
│      priv_id = priv(new)->s_id;           // 备份                   │
│      *priv(new) = *priv(RS);              // 复制                   │
│      priv(new)->s_id = priv_id;           // 恢复                   │
│      priv(new)->s_proc_nr = NEW_PROC_NR;  // 重新关联               │
└─────────────────────────────────────────────────────────────────────┘
```

**清除待处理消息 (第122-134行)**:

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

**为什么需要清除？**

```
新进程启动时的问题:
┌─────────────────────────────────────────────────────────────────────┐
│  特权结构从调用者复制而来:                                           │
│      ├─► 调用者可能有未处理的异步消息                               │
│      ├─► 调用者可能有未处理的通知                                   │
│      ├─► 调用者可能有未处理的中断                                   │
│      └─► 调用者可能有未处理的信号                                   │
│                                                                     │
│  如果不清除:                                                         │
│      ├─► 新进程会收到错误的消息                                     │
│      ├─► 新进程会处理不属于它的中断                                 │
│      └─► 系统行为不可预测                                           │
│                                                                     │
│  解决方案:                                                           │
│      ├─► 清除所有待处理消息                                         │
│      ├─► 重置所有定时器                                             │
│      └─► 确保新进程从干净状态开始                                   │
└─────────────────────────────────────────────────────────────────────┘
```

**设置默认值 (第137-162行)**:

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

/* Set defaults for resources */
priv(rp)->s_nr_io_range= 0;
priv(rp)->s_nr_mem_range= 0;
priv(rp)->s_nr_irq= 0;
priv(rp)->s_grant_table= 0;
priv(rp)->s_grant_entries= 0;
...
```

**默认值的含义**:

| 字段 | 默认值 | 含义 |
|------|--------|------|
| `s_flags` | `DSRV_F` | 默认服务标志 |
| `s_trap_mask` | `DSRV_T` | 允许的系统调用陷阱 |
| `s_ipc_to` | `DSRV_M` | 允许通信的目标 |
| `s_k_call_mask` | `DSRV_KC` | 允许的内核调用 |
| `s_sig_mgr` | `DSRV_SM` | 默认信号管理器 |
| `s_nr_io_range` | `0` | 无 I/O 端口权限 |
| `s_nr_mem_range` | `0` | 无内存访问权限 |
| `s_nr_irq` | `0` | 无中断权限 |

**7. SYS_PRIV_SET_USER - 设置用户进程特权 (第172-181行)**

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

**用户进程特权共享机制**:

```
┌─────────────────────────────────────────────────────────────────────┐
│  用户进程特权结构共享                                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  系统进程 (独立特权结构):                                            │
│      RS      → priv[RS_PRIV_ID]                                     │
│      PM      → priv[PM_PRIV_ID]                                     │
│      VFS     → priv[VFS_PRIV_ID]                                    │
│      ...                                                            │
│                                                                     │
│  用户进程 (共享特权结构):                                            │
│      bash    ─┐                                                     │
│      ls      ─┼─► priv[USER_PRIV_ID]                                │
│      grep   ─┘  (所有用户进程共享)                                  │
│                                                                     │
│  优势:                                                               │
│      ├─► 节省特权结构空间                                           │
│      ├─► 所有用户进程有相同的权限                                   │
│      └─► 简化权限管理                                               │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**USER_PRIV_ID 宏定义**:

```c
#define USER_PRIV_ID  static_priv_id(ROOT_USR_PROC_NR)
```

**8. SYS_PRIV_ADD_IO - 添加 I/O 端口范围 (第183-198行)**

```c
case SYS_PRIV_ADD_IO:
    if (RTS_ISSET(rp, RTS_NO_PRIV))
        return(EPERM);

    /* Get the I/O range */
    data_copy(caller->p_endpoint, m_ptr->m_lsys_krn_sys_privctl.arg_ptr,
        KERNEL, (vir_bytes) &io_range, sizeof(io_range));
    /* Add the I/O range */
    return priv_add_io(rp, &io_range);
```

**I/O 端口范围结构**:

```c
struct io_range {
    port_t ior_base;    /*起始 I/O 端口 */
    port_t ior_limit;   /* 结束 I/O 端口 */
};
```

**使用场景**:

- 驱动程序需要访问设备 I/O 端口
- 例如：网卡驱动需要访问 0x3000-0x30FF

**9. SYS_PRIV_ADD_MEM - 添加内存范围 (第200-211行)**

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

**内存范围结构**:

```c
struct minix_mem_range {
    phys_bytes mr_base;    /* 起始物理地址 */
    phys_bytes mr_limit;   /* 结束物理地址 */
};
```

**使用场景**:

- 驱动程序需要访问设备内存
- 例如：显卡驱动需要访问显存区域

**10. SYS_PRIV_ADD_IRQ - 添加中断 (第213-226行)**

```c
case SYS_PRIV_ADD_IRQ:
    if (RTS_ISSET(rp, RTS_NO_PRIV))
        return(EPERM);

    data_copy(caller->p_endpoint, m_ptr->m_lsys_krn_sys_privctl.arg_ptr,
        KERNEL, (vir_bytes) &irq, sizeof(irq));
    /* Add the IRQ. */
    return priv_add_irq(rp, irq);
```

**使用场景**:

- 驱动程序需要处理设备中断
- 例如：网卡驱动需要处理 IRQ 1

**11. SYS_PRIV_QUERY_MEM - 查询内存权限 (第228-248行)**

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

**查询逻辑**:

```
检查物理地址是否在允许范围内:
┌─────────────────────────────────────────────────────────────────────┐
│  输入: addr, len                                                    │
│  输出: OK (允许) 或 EPERM (不允许)                                  │
│                                                                     │
│  检查流程:                                                           │
│      for each memory range in s_mem_tab:                           │
│          if (addr >= mr_base && limit <= mr_limit):                │
│              return OK                                              │
│      return EPERM                                                   │
│                                                                     │
│  示例:                                                               │
│      允许范围: 0x1000-0x1FFF                                        │
│      查询: addr=0x1500, len=0x100                                   │
│      limit = 0x1500 + 0x100 - 1 = 0x15FF                           │
│      检查: 0x1500 >= 0x1000 && 0x15FF <= 0x1FFF                    │
│      结果: OK (允许)                                                │
└─────────────────────────────────────────────────────────────────────┘
```

#### 主要请求类型对比

| 请求 | 功能 | 调用者 | 使用场景 |
|------|------|--------|---------|
| `SYS_PRIV_ALLOW` | 允许进程运行 | RS | 服务启动完成 |
| `SYS_PRIV_YIELD` | 允许目标运行，挂起自己 | RS | RS 启动新服务 |
| `SYS_PRIV_DISALLOW` | 禁止进程运行 | PM | 进程需要停止 |
| `SYS_PRIV_SET_SYS` | 设置系统进程特权 | RS | 创建系统进程 |
| `SYS_PRIV_SET_USER` | 设置用户进程特权 | RS | 创建用户进程 |
| `SYS_PRIV_ADD_IO` | 添加 I/O 端口范围 | 驱动程序 | 设备驱动初始化 |
| `SYS_PRIV_ADD_MEM` | 添加内存范围 | 驱动程序 | 内存映射设备 |
| `SYS_PRIV_ADD_IRQ` | 添加 IRQ | 驱动程序 | 中断处理 |

#### 特权结构复制机制

```c
/* 设置系统进程特权时的关键步骤 */
priv_id = priv(rp)->s_id;     // 备份特权 ID
*priv(rp) = *priv(caller);    // 复制调用者的特权结构
priv(rp)->s_id = priv_id;     // 恢复特权 ID
priv(rp)->s_proc_nr = proc_nr; // 重新关联进程号
```

**为什么要备份和恢复特权 ID？**

```
特权结构复制流程:
┌─────────────────────────────────────────────────────────────────────┐
│  调用者 (RS 进程)                                                    │
│  ┌────────────────────────────────────┐                            │
│  │ priv(RS)                           │                            │
│  │  - s_id = RS_PRIV_ID               │                            │
│  │  - s_proc_nr = RS_PROC_NR          │                            │
│  │  - s_flags = SYS_PROC | ...        │                            │
│  └────────────────────────────────────┘                            │
│              │                                                      │
│              │ *priv(rp) = *priv(caller)  (复制整个结构)            │
│              ▼                                                      │
│  目标进程 (新服务)                                                   │
│  ┌────────────────────────────────────┐                            │
│  │ priv(rp) (复制后)                  │                            │
│  │  - s_id = RS_PRIV_ID  ← 错误！     │  需要恢复                  │
│  │  - s_proc_nr = RS_PROC_NR ← 错误！ │  需要恢复                  │
│  │  - s_flags = SYS_PROC | ...        │  正确                      │
│  └────────────────────────────────────┘                            │
│              │                                                      │
│              │ 恢复 s_id 和 s_proc_nr                               │
│              ▼                                                      │
│  ┌────────────────────────────────────┐                            │
│  │ priv(rp) (最终)                    │                            │
│  │  - s_id = NEW_PRIV_ID  ← 正确      │                            │
│  │  - s_proc_nr = NEW_PROC_NR ← 正确  │                            │
│  │  - s_flags = SYS_PROC | ...        │  正确                      │
│  └────────────────────────────────────┘                            │
└─────────────────────────────────────────────────────────────────────┘
```

#### 清除待处理消息

```c
/* 清除所有待处理的消息和事件 */
for (i=0; i< NR_SYS_CHUNKS; i++)
    priv(rp)->s_asyn_pending.chunk[i] = 0;  // 异步消息
for (i=0; i< NR_SYS_CHUNKS; i++)
    priv(rp)->s_notify_pending.chunk[i] = 0; // 通知
priv(rp)->s_int_pending = 0;                  // 中断
sigemptyset(&priv(rp)->s_sig_pending);       // 信号
reset_kernel_timer(&priv(rp)->s_alarm_timer); // 闹钟
priv(rp)->s_asyntab = -1;                     // asynsend 表
```

**为什么要清除？**

```
新进程启动时，特权结构可能包含旧数据:
┌─────────────────────────────────────────────────────────────────────┐
│  问题场景:                                                           │
│  1. 特权结构从调用者复制而来                                         │
│  2. 调用者可能有未处理的消息                                         │
│  3. 如果不清除，新进程会收到错误的消息                               │
│                                                                     │
│  解决方案:                                                           │
│  - 清除所有待处理消息                                                │
│  - 重置所有定时器                                                    │
│  - 确保新进程从干净状态开始                                          │
└─────────────────────────────────────────────────────────────────────┘
```

#### 关键宏定义

```c
// minix3/minix/include/minix/com.h:342-352
#define SYS_PRIV_ALLOW      1   // 允许进程运行
#define SYS_PRIV_DISALLOW   2   // 禁止进程运行
#define SYS_PRIV_YIELD      10  // 允许目标运行，挂起自己

// minix3/minix/include/minix/priv.h
#define USER_PRIV_ID  static_priv_id(ROOT_USR_PROC_NR)
```

---

## 授权表管理机制

### do_setgrant - 授权表设置

**源代码位置**: [do_setgrant.c](file://../minix3/minix/kernel/system/do_setgrant.c)

**核心功能**: 设置进程的授权表

#### 逐行代码分析

**源码位置**: [do_setgrant.c:1-30](file://../minix3/minix/kernel/system/do_setgrant.c#L1-L30)

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_SETGRANT
 *
 * The parameters for this kernel call are:
 *   m_lsys_krn_sys_setgrant.addr    address of grant table in own address space
 *   m_lsys_krn_sys_setgrant.size    number of entries
 */

#include "kernel/system.h"
#include <minix/safecopies.h>

/*===========================================================================*
 *				do_setgrant				     *
 *===========================================================================*/
int do_setgrant(struct proc * caller, message * m_ptr)
{
	int r;

	/* Copy grant table set in priv. struct. */
	if (RTS_ISSET(caller, RTS_NO_PRIV) || !(priv(caller))) {
		r = EPERM;
	} else {
		_K_SET_GRANT_TABLE(caller,
			m_ptr->m_lsys_krn_sys_setgrant.addr,
			m_ptr->m_lsys_krn_sys_setgrant.size);
		r = OK;
	}

	return r;
}
```

**逐行解析**:

| 行号 | 代码 | 内存位置 | 作用 |
|------|------|---------|------|
| 1-6 | 注释 | - | 文档说明：系统调用类型、参数定义 |
| 8-9 | `#include` | - | 引入系统头文件、安全拷贝定义 |
| 18 | `int r;` | 栈，4字节 | 返回值变量 |

**关键代码段详解**:

**1. 权限检查 (第21-23行)**

```c
if (RTS_ISSET(caller, RTS_NO_PRIV) || !(priv(caller))) {
    r = EPERM;
}
```

**条件检查**:

```
条件 1: RTS_ISSET(caller, RTS_NO_PRIV)
  └─► 调用者被禁止运行
  └─► 特权结构可能未设置

条件 2: !(priv(caller))
  └─► 调用者没有特权结构
  └─► 不应该发生，但防御性检查

如果任一条件为真:
  └─► 返回 EPERM (权限错误)
```

**2. 设置授权表 (第24-27行)**

```c
_K_SET_GRANT_TABLE(caller,
    m_ptr->m_lsys_krn_sys_setgrant.addr,
    m_ptr->m_lsys_krn_sys_setgrant.size);
```

**_K_SET_GRANT_TABLE 宏**:

源码位置: [safecopies.h](file://../minix3/minix/include/minix/safecopies.h)

```c
#define _K_SET_GRANT_TABLE(p, addr, size)				\
	do {								\
		priv(p)->s_grant_table = (vir_bytes) (addr);		\
		priv(p)->s_grant_entries = (size);			\
		priv(p)->s_grant_endpoint = (p)->p_endpoint;		\
	} while(0)
```

**宏展开后的代码**:

```c
priv(caller)->s_grant_table = (vir_bytes) m_ptr->m_lsys_krn_sys_setgrant.addr;
priv(caller)->s_grant_entries = m_ptr->m_lsys_krn_sys_setgrant.size;
priv(caller)->s_grant_endpoint = caller->p_endpoint;
```

**三个字段的作用**:

| 字段 | 类型 | 含义 |
|------|------|------|
| `s_grant_table` | `vir_bytes` | 授权表在进程地址空间中的地址 |
| `s_grant_entries` | `int` | 授权表中的条目数量 |
| `s_grant_endpoint` | `endpoint_t` | 授权表所属进程的端点 |

**为什么需要 s_grant_endpoint？**

```
授权表验证流程:
┌─────────────────────────────────────────────────────────────────────┐
│  进程 A 调用 sys_setgrant(addr, size)                               │
│      ├─► priv(A)->s_grant_table = addr                              │
│      ├─► priv(A)->s_grant_entries = size                            │
│      └─► priv(A)->s_grant_endpoint = A->p_endpoint                  │
│                                                                     │
│  进程 B 调用 sys_safecopyfrom(A, grant_id, ...)                     │
│      ├─► 内核验证 grant_id                                          │
│      ├─► 内核检查 priv(A)->s_grant_endpoint == A->p_endpoint        │
│      │   └─► 确保授权表属于 A                                       │
│      └─► 内核执行安全拷贝                                           │
└─────────────────────────────────────────────────────────────────────┘
```

**授权表机制详解**:

**授权表是什么？**

```
授权表（Grant Table）= "内核背书的跨进程指针"

grant entry = {
    granter: 进程 A 的端点号,
    grantee: 进程 B 的端点号,
    addr: A 的内存地址,
    len: 内存长度,
    rights: READ/WRITE
}

含义: "A 允许 B 访问我这段内存"
```

**内存布局**:

```
进程 A 的地址空间:
┌────────────────────────────────────────────────────────────────────┐
│  代码段                                                            │
├────────────────────────────────────────────────────────────────────┤
│  数据段                                                            │
├────────────────────────────────────────────────────────────────────┤
│  堆                                                                │
├────────────────────────────────────────────────────────────────────┤
│  授权表 ←─────────────────────────────────────┐                    │
│  ┌──────────────────────────────────────┐    │                    │
│  │ grant[0]: (A, B, 0x1000, 4096, R)    │    │ 内核可访问          │
│  │ grant[1]: (A, C, 0x2000, 8192, RW)   │    │                    │
│  │ grant[2]: ...                        │    │                    │
│  └──────────────────────────────────────┘    │                    │
├──────────────────────────────────────────────┼────────────────────┤
│  栈                                        │                    │
└──────────────────────────────────────────────┼────────────────────┘
                                               │
                                               ▼
                                    内核通过授权表验证访问权限
```

**用户进程授权机制（关键设计）**

**用户进程共享 USER_PRIV_ID**

```c
// 所有普通用户进程共享同一个特权结构体
#define USER_PRIV_ID  static_priv_id(ROOT_USR_PROC_NR)
```

**共享带来的问题**:

```
┌─────────────────────────────────────────────────────────────────────┐
│  问题: 用户进程共享特权结构导致授权冲突                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  T1: 用户进程 A 调用 sys_setgrant()                                 │
│      └──► USER_PRIV_ID->s_grant_endpoint = A                       │
│                                                                     │
│  T2: 用户进程 B 调用 sys_setgrant()                                 │
│      └──► USER_PRIV_ID->s_grant_endpoint = B (覆盖了 A！)          │
│                                                                     │
│  T3: 验证 A 的授权时                                                │
│      └──► s_grant_endpoint != A->p_endpoint                        │
│      └──► 返回 ENOTREADY！A 的授权失效了！                          │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**真正的设计: 特权进程代理授权**

```
用户进程不直接使用授权表，而是通过特权进程（VFS、PM）代为创建授权！

┌─────────────────────────────────────────────────────────────────────┐
│  用户进程 A 调用 read(fd, buf, size)                                │
│      │                                                              │
│      ▼                                                              │
│  VFS 收到请求（VFS 是特权进程，有独立的 priv 结构）                 │
│      │                                                              │
│      ▼                                                              │
│  VFS 调用 cpf_grant_magic(文件系统进程, 用户进程A, buf, ...)       │
│      在 VFS 自己的授权表中创建授权                                  │
│      │                                                              │
│      ▼                                                              │
│  文件系统进程调用 sys_safecopy(grant_id)                           │
│      │                                                              │
│      ▼                                                              │
│  内核验证 VFS 的授权表，直接从用户进程内存复制数据                  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

#### 三种授权类型

| 类型 | 函数 | 用途 | 调用者 |
|------|------|------|--------|
| `CPF_DIRECT` | `cpf_grant_direct()` | 授权自己的内存给别人 | 特权进程 |
| `CPF_INDIRECT` | `cpf_grant_indirect()` | 转授权 | 特权进程 |
| `CPF_MAGIC` | `cpf_grant_magic()` | 授权别人的内存给第三方 | **仅特权进程** |

**Magic Grant 的实现**:

```c
// lib/libsys/safecopies.c
cp_grant_id_t cpf_grant_magic(endpoint_t who_to, endpoint_t who_from,
    vir_bytes addr, size_t bytes, int access)
{
    // 创建"魔法授权"：允许 who_to 访问 who_from 的内存
    grants[g].cp_u.cp_magic.cp_who_to = who_to;      // 文件系统进程
    grants[g].cp_u.cp_magic.cp_who_from = who_from;  // 用户进程
    grants[g].cp_u.cp_magic.cp_start = addr;         // 用户进程的缓冲区地址
    grants[g].cp_u.cp_magic.cp_length = bytes;       // 长度
    grants[g].cp_u.cp_magic.cp_access = access;      // READ/WRITE
}
```

---

## Grant 机制深度分析

### 为什么需要 Grant？

**微内核场景**: `用户进程 A ──► VFS ──► 文件系统进程`

| 方案 | 做法 | 问题 |
|------|------|------|
| ❌ 直接传指针 | 把 buf 指针传过去 | 不安全，FS 可乱读 A 的所有内存 |
| ❌ 内核 copy | A → kernel → FS | 多一次 copy、cache 污染 |
| ✅ MINIX 解法 | A → VFS → grant → FS | 验证 + 临时授权 + 直接拷贝 |

### Grant 验证流程

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Grant 验证流程                                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. 文件系统进程调用 sys_safecopy(grant_id, ...)                    │
│     │                                                               │
│     ▼                                                               │
│  2. 内核查找 grant_id 对应的授权条目                                │
│     │                                                               │
│     ├─► 检查 granter (VFS) 是否有授权表                             │
│     ├─► 检查 grant_id 是否在有效范围内                              │
│     ├─► 检查 grantee (FS) 是否匹配                                  │
│     └─► 检查访问权限 (READ/WRITE)                                   │
│     │                                                               │
│     ▼                                                               │
│  3. 验证通过，执行内存拷贝                                          │
│     │                                                               │
│     └─► 从用户进程 A 的内存直接拷贝到 FS 的缓冲区                   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 优缺点分析

| 优点 | 缺点 |
|------|------|
| 安全性极高，不暴露裸指针 | 复杂度高，理解成本高 |
| 权限显式，没有隐式共享 | 有额外开销（lookup + 检查） |
| 非常适合微内核 | 不适合高频小数据 |
| 支持复杂授权（magic grant） | cache/NUMA 不友好 |

### 与 Linux 对比

| 维度 | MINIX (grant) | Linux |
|------|---------------|-------|
| **安全** | 强（显式授权） | 中（隐式共享） |
| **性能** | 中（额外检查） | 强（直接访问） |
| **抽象** | 显式授权 | 隐式共享 |
| **适用场景** | 微内核 | 宏内核 |

### 哲学评价

> **Grant 是"安全优先"的设计，而不是"性能优先"**

- **微内核世界观**: 非常优雅（capability-based security, least privilege）
- **现代多核性能世界**: 不够极致（cache 比 security 更贵）

---

## 模块级 Rust 重构建议

### 1. 特权结构的类型安全设计

```rust
use core::ptr::NonNull;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivId(u16);

impl PrivId {
    pub const USER: Self = PrivId(0);
    pub const DYNAMIC_START: Self = PrivId(1);
    pub const MAX: Self = PrivId(NR_SYS_PROCS as u16);
}

#[derive(Debug, Clone)]
pub struct Priv {
    pub s_id: PrivId,
    pub s_proc_nr: ProcNr,
    pub s_flags: PrivFlags,
    pub s_trap_mask: TrapMask,
    pub s_k_call_mask: KernelCallMask,
    pub s_io_range: Vec<IoRange, MAX_IO_RANGES>,
    pub s_mem_range: Vec<MemRange, MAX_MEM_RANGES>,
    pub s_irq_list: Vec<u8, MAX_IRQS>,
    pub s_grant_table: Option<GrantTable>,
    pub s_notify_pending: SysMap,
    pub s_asyn_pending: SysMap,
    pub s_int_pending: u32,
    pub s_sig_pending: SigSet,
    pub s_alarm_timer: KernelTimer,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PrivFlags: u32 {
        const SYS_PROC = 0x01;
        const CHECK_IRQ = 0x02;
        const CHECK_IO_PORT = 0x04;
        const CHECK_MEM = 0x08;
        const DYN_PRIV_ID = 0x10;
    }
}
```

### 2. 特权请求的类型安全枚举

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivctlRequest {
    Allow,
    Yield,
    Disallow,
    SetSys,
    SetUser,
    AddIo(IoRange),
    AddMem(MemRange),
    AddIrq(u8),
    ClearIpcRefs,
}

#[derive(Debug, Clone, Copy)]
pub struct IoRange {
    pub base: u16,
    pub len: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct MemRange {
    pub base: PhysAddr,
    pub len: usize,
}
```

### 3. 特权控制的类型安全实现

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivctlError {
    PermissionDenied,
    InvalidEndpoint,
    PrivAllocationFailed,
    InvalidResourceRange,
    AlreadyRunning,
    NotRunning,
}

pub fn do_privctl(
    caller: &Proc,
    request: PrivctlRequest,
    target_ep: Endpoint,
) -> Result<(), PrivctlError> {
    // 权限检查：只有系统进程才能调用
    if !caller.privilege().flags.contains(PrivFlags::SYS_PROC) {
        return Err(PrivctlError::PermissionDenied);
    }
    
    let target = validate_endpoint(target_ep)
        .map_err(|_| PrivctlError::InvalidEndpoint)?;
    
    match request {
        PrivctlRequest::Allow => {
            if !target.has_rts_flag(RtsFlags::NO_PRIV) {
                return Err(PrivctlError::AlreadyRunning);
            }
            if target.privilege().proc_nr == NONE {
                return Err(PrivctlError::PrivAllocationFailed);
            }
            target.clear_rts_flag(RtsFlags::NO_PRIV);
            Ok(())
        }
        
        PrivctlRequest::Yield => {
            if !target.has_rts_flag(RtsFlags::NO_PRIV) {
                return Err(PrivctlError::AlreadyRunning);
            }
            if target.privilege().proc_nr == NONE {
                return Err(PrivctlError::PrivAllocationFailed);
            }
            caller.set_rts_flag(RtsFlags::NO_PRIV);
            target.clear_rts_flag(RtsFlags::NO_PRIV);
            Ok(())
        }
        
        PrivctlRequest::Disallow => {
            if target.has_rts_flag(RtsFlags::NO_PRIV) {
                return Err(PrivctlError::NotRunning);
            }
            target.set_rts_flag(RtsFlags::NO_PRIV);
            Ok(())
        }
        
        PrivctlRequest::SetSys => {
            if !target.has_rts_flag(RtsFlags::NO_PRIV) {
                return Err(PrivctlError::AlreadyRunning);
            }
            setup_system_privilege(&target, caller)?;
            Ok(())
        }
        
        PrivctlRequest::SetUser => {
            if !target.has_rts_flag(RtsFlags::NO_PRIV) {
                return Err(PrivctlError::AlreadyRunning);
            }
            target.set_privilege(PrivId::USER);
            Ok(())
        }
        
        PrivctlRequest::AddIo(range) => {
            if target.has_rts_flag(RtsFlags::NO_PRIV) {
                return Err(PrivctlError::NotRunning);
            }
            target.privilege_mut()
                .add_io_range(range)
                .map_err(|_| PrivctlError::InvalidResourceRange)?;
            Ok(())
        }
        
        PrivctlRequest::AddMem(range) => {
            if target.has_rts_flag(RtsFlags::NO_PRIV) {
                return Err(PrivctlError::NotRunning);
            }
            target.privilege_mut()
                .add_mem_range(range)
                .map_err(|_| PrivctlError::InvalidResourceRange)?;
            Ok(())
        }
        
        PrivctlRequest::AddIrq(irq) => {
            if target.has_rts_flag(RtsFlags::NO_PRIV) {
                return Err(PrivctlError::NotRunning);
            }
            target.privilege_mut()
                .add_irq(irq)
                .map_err(|_| PrivctlError::InvalidResourceRange)?;
            Ok(())
        }
        
        PrivctlRequest::ClearIpcRefs => {
            clear_ipc_refs(&target, ErrorCode::DeadSrcDst);
            Ok(())
        }
    }
}

fn setup_system_privilege(target: &Proc, caller: &Proc) -> Result<(), PrivctlError> {
    let priv_id = get_dynamic_priv_id()
        .ok_or(PrivctlError::PrivAllocationFailed)?;
    
    // 复制调用者的特权结构
    let mut new_priv = caller.privilege().clone();
    
    // 恢复关键字段
    new_priv.s_id = priv_id;
    new_priv.s_proc_nr = target.proc_nr();
    
    // 清除待处理消息
    new_priv.s_notify_pending.clear();
    new_priv.s_asyn_pending.clear();
    new_priv.s_int_pending = 0;
    new_priv.s_sig_pending.clear();
    new_priv.s_alarm_timer.reset();
    
    // 设置默认值
    new_priv.s_flags = PrivFlags::SYS_PROC;
    new_priv.s_io_range.clear();
    new_priv.s_mem_range.clear();
    new_priv.s_irq_list.clear();
    new_priv.s_grant_table = None;
    
    target.set_privilege(new_priv);
    Ok(())
}
```

### 4. 授权表的类型安全设计

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrantId(u32);

#[derive(Debug, Clone, Copy)]
pub struct GrantEntry {
    pub granter: Endpoint,
    pub grantee: Endpoint,
    pub start: VirtAddr,
    pub length: usize,
    pub access: GrantAccess,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct GrantAccess: u32 {
        const READ = 0x01;
        const WRITE = 0x02;
    }
}

#[derive(Debug, Clone)]
pub struct GrantTable {
    entries: Vec<GrantEntry, MAX_GRANTS>,
    endpoint: Endpoint,
}

impl GrantTable {
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            entries: Vec::new(),
            endpoint,
        }
    }
    
    pub fn add_grant(&mut self, entry: GrantEntry) -> Result<GrantId, GrantError> {
        if self.entries.len() >= MAX_GRANTS {
            return Err(GrantError::TableFull);
        }
        
        let id = GrantId(self.entries.len() as u32);
        self.entries.push(entry);
        Ok(id)
    }
    
    pub fn verify(&self, grant_id: GrantId, grantee: Endpoint) -> Result<&GrantEntry, GrantError> {
        let entry = self.entries.get(grant_id.0 as usize)
            .ok_or(GrantError::InvalidId)?;
        
        if entry.granter != self.endpoint {
            return Err(GrantError::EndpointMismatch);
        }
        
        if entry.grantee != grantee {
            return Err(GrantError::PermissionDenied);
        }
        
        Ok(entry)
    }
    
    pub fn set_endpoint(&mut self, endpoint: Endpoint) {
        self.endpoint = endpoint;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantError {
    TableFull,
    InvalidId,
    EndpointMismatch,
    PermissionDenied,
}
```

### 5. 授权表设置的类型安全实现

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetGrantError {
    PermissionDenied,
    InvalidAddress,
    TableTooLarge,
}

pub fn do_setgrant(
    caller: &mut Proc,
    addr: VirtAddr,
    size: usize,
) -> Result<(), SetGrantError> {
    // 权限检查
    if caller.has_rts_flag(RtsFlags::NO_PRIV) {
        return Err(SetGrantError::PermissionDenied);
    }
    
    if caller.privilege().is_none() {
        return Err(SetGrantError::PermissionDenied);
    }
    
    // 验证大小
    if size > MAX_GRANTS {
        return Err(SetGrantError::TableTooLarge);
    }
    
    // 设置授权表
    caller.privilege_mut().set_grant_table(addr, size);
    
    Ok(())
}
```

### 6. Magic Grant 的类型安全实现

```rust
impl GrantTable {
    pub fn add_magic_grant(
        &mut self,
        who_to: Endpoint,
        who_from: Endpoint,
        addr: VirtAddr,
        length: usize,
        access: GrantAccess,
    ) -> Result<GrantId, GrantError> {
        // Magic grant: 允许 who_to 访问 who_from 的内存
        // 授权者 (self.endpoint) 是代理进程（如 VFS）
        
        let entry = GrantEntry {
            granter: who_from,  // 实际的内存所有者
            grantee: who_to,    // 被授权者
            start: addr,
            length,
            access,
        };
        
        self.add_grant(entry)
    }
}
```

---

## 现代 64 位硬件演进

### 特权环与硬件支持

```
32 位 x86:
  - Ring 0: 内核
  - Ring 1-2: 驱动程序（很少使用）
  - Ring 3: 用户进程

64 位 x86:
  - Ring 0: 内核
  - Ring 3: 用户进程
  - Ring 1-2 基本废弃
  - VT-x/AMD-V 提供额外的虚拟化特权层
```

### IOMMU/SMMU 支持

```
传统方案:
  - Grant 机制完全由软件实现
  - 每次访问都需要内核验证

现代硬件 (IOMMU/SMMU):
  - 硬件级别的 DMA 保护
  - 设备访问内存前通过 IOMMU 验证
  - 可以将 Grant 映射到 IOMMU 页表
  - 减少内核验证开销
```

### Rust 中的硬件抽象

```rust
#[cfg(feature = "iommu")]
pub struct IommuGrant {
    iommu_entry: IommuPageTableEntry,
    grant: GrantEntry,
}

impl IommuGrant {
    pub fn setup(&mut self) -> Result<(), IommuError> {
        // 配置 IOMMU 页表项
        self.iommu_entry.set_physical_addr(self.grant.start);
        self.iommu_entry.set_length(self.grant.length);
        self.iommu_entry.set_permissions(self.grant.access);
        Ok(())
    }
}
```

---

## 要点总结

### 核心知识点

1. **特权层次结构**: 系统进程有独立特权结构，用户进程共享 USER_PRIV_ID
2. **Grant 机制**: 内核背书的跨进程内存访问授权，支持三种类型（direct/indirect/magic）
3. **代理授权模式**: 用户进程通过特权进程（VFS/PM）代为创建授权，避免共享特权结构的问题

### 灾难预演

**场景 1: 删除权限检查**

```c
// 如果删除 do_privctl 中的权限检查
if (! (priv(caller)->s_flags & SYS_PROC)) return(EPERM);
```

后果:
- 任何进程都可以修改其他进程的特权
- 用户进程可以提升自己的权限
- 系统安全完全崩溃

**场景 2: 忘记恢复特权 ID**

```c
// 如果忘记恢复特权 ID
*priv(rp) = *priv(caller);
// 缺少: priv(rp)->s_id = priv_id;
```

后果:
- 多个进程共享同一个特权 ID
- 权限混乱，进程 A 可以访问进程 B 的资源
- 安全漏洞

**场景 3: 用户进程直接设置授权表**

```c
// 如果用户进程直接调用 sys_setgrant
```

后果:
- 多个用户进程共享 USER_PRIV_ID
- 授权表端点互相覆盖
- 授权验证失败，IPC 通信中断

### 互动自测

1. **问题**: 为什么用户进程共享 USER_PRIV_ID？
   **答案**: 用户进程特权较低，不需要单独的特权结构，共享可以节省资源。

2. **问题**: SYS_PRIV_YIELD 的作用是什么？
   **答案**: 允许目标进程运行，同时挂起调用者，用于 RS 启动新服务。

3. **问题**: Magic Grant 的作用是什么？
   **答案**: 允许特权进程（如 VFS）代理用户进程创建授权，避免用户进程共享特权结构的问题。

4. **问题**: 为什么特权结构复制后要恢复 s_id？
   **答案**: 避免多个进程共享同一个特权 ID，导致权限混乱。

5. **问题**: Grant 机制与 shared memory 的区别？
   **答案**: Grant 是短期、受控、按需授权，shared memory 是长期映射。Grant 更安全但有额外开销。

---

**文档版本**: 2026-03-31
