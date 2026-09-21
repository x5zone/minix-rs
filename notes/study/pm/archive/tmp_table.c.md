# servers/pm/table.c 逐行讲解

## 文件概述

**文件路径**: `servers/pm/table.c`  
**模块归属**: PM（进程管理服务器）  
**核心功能**: 定义 PM 系统调用分发表

---

## 逐行讲解

### 1. 文件头注释

```c
/* This file contains the table used to map system call numbers onto the
 * routines that perform them.
 */
```

**第1-3行**: 文件头注释  
- **系统调用映射表**: 将系统调用号映射到执行函数

**设计原因**: 集中管理系统调用分发

---

### 2. 定义和包含

```c
#define _TABLE

#include "pm.h"
#include <minix/callnr.h>
#include <signal.h>
#include "mproc.h"
```

**第5-10行**: 定义和包含  
- `_TABLE`: 定义宏，用于条件编译
- `pm.h`: PM 主头文件
- `minix/callnr.h`: 系统调用号
- `signal.h`: 信号定义
- `mproc.h`: PM 进程结构

---

### 3. 宏定义

```c
#define CALL(n)	[((n) - PM_BASE)]
```

**第12行**: 宏定义  
- **CALL(n)**: 将系统调用号转换为数组索引
- **PM_BASE**: PM 系统调用基址

**设计原因**: 简化数组索引计算

---

### 4. 系统调用分发表

```c
int (* const call_vec[NR_PM_CALLS])(void) = {
	CALL(PM_EXIT)		= do_exit,		/* _exit(2) */
	CALL(PM_FORK)		= do_fork,		/* fork(2) */
	CALL(PM_WAIT4)		= do_wait4,		/* wait4(2) */
	CALL(PM_GETPID)		= do_get,		/* get[p]pid(2) */
	CALL(PM_SETUID)		= do_set,		/* setuid(2) */
	CALL(PM_GETUID)		= do_get,		/* get[e]uid(2) */
	CALL(PM_STIME)		= do_stime,		/* stime(2) */
	CALL(PM_PTRACE)		= do_trace,		/* ptrace(2) */
	CALL(PM_SETGROUPS)	= do_set,		/* setgroups(2) */
	CALL(PM_GETGROUPS)	= do_get,		/* getgroups(2) */
	CALL(PM_KILL)		= do_kill,		/* kill(2) */
	CALL(PM_SETGID)		= do_set,		/* setgid(2) */
	CALL(PM_GETGID)		= do_get,		/* get[e]gid(2) */
	CALL(PM_EXEC)		= do_exec,		/* execve(2) */
	CALL(PM_SETSID)		= do_set,		/* setsid(2) */
	CALL(PM_GETPGRP)	= do_get,		/* getpgrp(2) */
	CALL(PM_ITIMER)		= do_itimer,		/* [gs]etitimer(2) */
	CALL(PM_GETMCONTEXT)	= do_getmcontext,	/* getmcontext(2) */
	CALL(PM_SETMCONTEXT)	= do_setmcontext,	/* setmcontext(2) */
	CALL(PM_SIGACTION)	= do_sigaction,		/* sigaction(2) */
	CALL(PM_SIGSUSPEND)	= do_sigsuspend,	/* sigsuspend(2) */
	CALL(PM_SIGPENDING)	= do_sigpending,	/* sigpending(2) */
	CALL(PM_SIGPROCMASK)	= do_sigprocmask,	/* sigprocmask(2) */
	CALL(PM_SIGRETURN)	= do_sigreturn,		/* sigreturn(2) */
	CALL(PM_SYSUNAME)	= do_sysuname,		/* sysuname(2) */
	CALL(PM_GETPRIORITY)	= do_getsetpriority,	/* getpriority(2) */
	CALL(PM_SETPRIORITY)	= do_getsetpriority,	/* setpriority(2) */
	CALL(PM_GETTIMEOFDAY)	= do_time,		/* gettimeofday(2) */
	CALL(PM_SETEUID)	= do_set,		/* geteuid(2) */
	CALL(PM_SETEGID)	= do_set,		/* setegid(2) */
	CALL(PM_ISSETUGID)	= do_get,		/* issetugid */
	CALL(PM_GETSID)		= do_get,		/* getsid(2) */
	CALL(PM_CLOCK_GETRES)	= do_getres,		/* clock_getres(2) */
	CALL(PM_CLOCK_GETTIME)	= do_gettime,		/* clock_gettime(2) */
	CALL(PM_CLOCK_SETTIME)	= do_settime,		/* clock_settime(2) */
	CALL(PM_GETRUSAGE)	= do_getrusage,		/* getrusage(2) */
	CALL(PM_REBOOT)		= do_reboot,		/* reboot(2) */
	CALL(PM_SVRCTL)		= do_svrctl,		/* svrctl(2) */
	CALL(PM_SPROF)		= do_sprofile,		/* sprofile(2) */
	CALL(PM_PROCEVENTMASK)	= do_proceventmask,	/* proceventmask(2) */
	CALL(PM_SRV_FORK)	= do_srv_fork,		/* srv_fork(2) */
	CALL(PM_SRV_KILL)	= do_srv_kill,		/* srv_kill(2) */
	CALL(PM_EXEC_NEW)	= do_newexec,
	CALL(PM_EXEC_RESTART)	= do_execrestart,
	CALL(PM_GETEPINFO)	= do_getepinfo,		/* getepinfo(2) */
	CALL(PM_GETPROCNR)	= do_getprocnr,		/* getprocnr(2) */
	CALL(PM_GETSYSINFO)	= do_getsysinfo		/* getsysinfo(2) */
};
```

**第14-62行**: 系统调用分发表  
- **类型**: 函数指针数组
- **大小**: `NR_PM_CALLS`
- **映射**: 系统调用号 → 处理函数
- **注释**: 每个条目都有对应的系统调用注释

**设计原因**: 
- **集中管理**: 集中管理系统调用分发
- **扩展性**: 易于添加新系统调用
- **清晰**: 每个系统调用都有注释

---

## 要点总结

### 1. 核心知识点

1. **系统调用分发**: 通过函数指针数组分发系统调用
2. **宏定义**: 使用宏简化数组索引
3. **集中管理**: 集中管理系统调用映射

### 2. 设计亮点

- **清晰**: 每个系统调用都有注释
- **扩展性**: 易于添加新系统调用
- **类型安全**: 使用函数指针类型

### 3. 内存模型

```
系统调用分发表:
┌─────────────────────────────────┐
│ call_vec[0] = do_exit           │
│ call_vec[1] = do_fork           │
│ call_vec[2] = do_wait4          │
│ ...                             │
│ call_vec[NR_PM_CALLS-1] = ...   │
└─────────────────────────────────┘
```

---

## 灾难预演

### 场景 1: 系统调用号越界

**后果**: 
- 数组越界访问
- 系统崩溃

**症状**: 系统崩溃

### 场景 2: 函数指针为空

**后果**: 
- 调用空指针
- 系统崩溃

**症状**: 系统崩溃

### 场景 3: 系统调用号冲突

**后果**: 
- 调用错误的函数
- 行为异常

**症状**: 系统调用行为异常

---

## 互动自测

### 问题 1: 系统调用分发

**问**: 为什么使用函数指针数组分发系统调用？

**答**: 
- **效率**: 直接索引，快速
- **清晰**: 集中管理，易于理解
- **扩展**: 易于添加新系统调用

### 问题 2: CALL 宏

**问**: CALL 宏的作用是什么？

**答**: 
- **索引**: 将系统调用号转换为数组索引
- **基址**: 减去 PM_BASE
- **简化**: 简化代码

### 问题 3: const

**问**: 为什么使用 const？

**答**: 
- **只读**: 防止修改
- **安全**: 提高安全性
- **优化**: 编译器优化

---

## Rust 实现对比

### C 版本（原始）

```c
int (* const call_vec[NR_PM_CALLS])(void) = {
	CALL(PM_EXIT)		= do_exit,
	CALL(PM_FORK)		= do_fork,
	CALL(PM_WAIT4)		= do_wait4,
	// ...
};
```

### Rust 版本（安全抽象）

```rust
type PmCallFn = fn() -> i32;

const CALL_VEC: [PmCallFn; NR_PM_CALLS] = [
    do_exit,    // PM_EXIT
    do_fork,    // PM_FORK
    do_wait4,   // PM_WAIT4
    // ...
];
```

### 关键改进

1. **类型别名**: 使用类型别名提高可读性
2. **const**: 使用 `const` 定义常量数组
3. **安全**: Rust 保证类型安全

---

## 理论关联

### 1. 系统调用

**操作系统概念**: 系统调用是用户态请求内核服务的接口

**Minix3 实现**:
- PM 处理进程管理相关系统调用
- 通过消息传递
- 函数指针数组分发

### 2. 进程管理

**操作系统概念**: 进程管理包括创建、退出、信号等

**Minix3 实现**:
- fork、exec、exit
- 信号处理
- 进程状态管理

### 3. 分发表

**操作系统概念**: 分发表将请求映射到处理函数

**Minix3 实现**:
- 函数指针数组
- 系统调用号索引
- 集中管理

---

## 总结

`table.c` 定义了 Minix3 PM 的系统调用分发表。通过函数指针数组、宏定义、集中管理等设计，实现了清晰、高效的系统调用分发。理解系统调用分发表是理解 PM 系统调用处理的核心。
