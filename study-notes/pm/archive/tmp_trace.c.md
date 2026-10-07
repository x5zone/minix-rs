# servers/pm/trace.c 逐行讲解

> **文件路径**: `minix3/minix/servers/pm/trace.c`
> **核心功能**: ptrace 调试系统调用实现
> **代码行数**: 276 行

---

## 文件概述

### 是什么（功能说明）

这个文件实现了进程管理器的调试支持，通过 `ptrace` 系统调用提供进程调试功能。

**支持的调试命令**:

| 命令 | 功能 | 处理位置 |
|------|------|---------|
| `T_STOP` | 停止进程 | 不暴露给用户 |
| `T_OK` | 允许父进程跟踪 | PM |
| `T_GETINS` | 读取指令空间 | 系统任务 |
| `T_GETDATA` | 读取数据空间 | 系统任务 |
| `T_GETUSER` | 读取用户进程表 | 系统任务 |
| `T_SETINS` | 设置指令空间 | 系统任务 |
| `T_SETDATA` | 设置数据空间 | 系统任务 |
| `T_SETUSER` | 设置用户进程表 | 系统任务 |
| `T_RESUME` | 恢复执行 | PM + 系统任务 |
| `T_EXIT` | 退出进程 | PM |
| `T_STEP` | 单步执行 | PM + 系统任务 |
| `T_SYSCALL` | 跟踪系统调用 | PM + 系统任务 |
| `T_ATTACH` | 附加到进程 | PM |
| `T_DETACH` | 分离进程 | PM + 系统任务 |
| `T_SETOPT` | 设置跟踪选项 | PM |
| `T_GETRANGE` | 获取值范围 | PM |
| `T_SETRANGE` | 设置值范围 | PM |

### 为什么（设计原因）

**分层设计**:
- **PM 处理**: 权限检查、进程状态管理
- **系统任务处理**: 底层内存访问、寄存器操作

**安全考虑**:
- 严格的权限检查防止未授权调试
- 保护关键系统进程（PM、VM）不被调试

### 什么情景使用（应用场景）

| 场景 | 使用命令 |
|------|---------|
| 调试器启动被调试程序 | `T_OK` |
| 调试器附加到运行中的进程 | `T_ATTACH` |
| 设置断点 | `T_SETINS` |
| 单步执行 | `T_STEP` |
| 查看变量 | `T_GETDATA` |
| 修改变量 | `T_SETDATA` |

---

## 头文件分析

```c
/* This file handles the process manager's part of debugging, using the
 * ptrace system call. Most of the commands are passed on to the system
 * task for completion.
 *
 * The debugging commands available are:
 * T_STOP	stop the process
 * T_OK		enable tracing by parent for this process
 * T_GETINS	return value from instruction space
 * T_GETDATA	return value from data space
 * T_GETUSER	return value from user process table
 * T_SETINS	set value in instruction space
 * T_SETDATA	set value in data space
 * T_SETUSER	set value in user process table
 * T_RESUME	resume execution
 * T_EXIT	exit
 * T_STEP	set trace bit
 * T_SYSCALL	trace system call
 * T_ATTACH	attach to an existing process
 * T_DETACH	detach from a traced process
 * T_SETOPT	set trace options
 * T_GETRANGE	get range of values
 * T_SETRANGE	set range of values
 *
 * The T_OK, T_ATTACH, T_EXIT, and T_SETOPT commands are handled here, and the
 * T_RESUME, T_STEP, T_SYSCALL, and T_DETACH commands are partially handled
 * here and completed by the system task. The rest are handled entirely by the
 * system task.
 */
```

**注释翻译**: "这个文件处理进程管理器的调试部分，使用 ptrace 系统调用。大多数命令传递给系统任务完成。可用的调试命令：T_STOP（停止进程）、T_OK（允许父进程跟踪）、... T_OK、T_ATTACH、T_EXIT 和 T_SETOPT 命令在这里处理，T_RESUME、T_STEP、T_SYSCALL 和 T_DETACH 命令在这里部分处理并由系统任务完成。其余命令完全由系统任务处理。"

**设计思路讲解**: 
- 调试需要两个层次：进程管理（PM）和底层访问（系统任务）
- PM 负责权限检查和状态管理
- 系统任务负责实际的内存和寄存器访问

```c
#include "pm.h"
#include <minix/com.h>
#include <minix/callnr.h>
#include <sys/ptrace.h>
#include <sys/wait.h>
#include <signal.h>
#include "mproc.h"
```

**讲解**: 包含必要的头文件：
- `sys/ptrace.h`: ptrace 命令定义
- `sys/wait.h`: wait 状态码定义
- `signal.h`: 信号定义

---

## 函数详解

### do_trace 函数

```c
/*===========================================================================*
 *				do_trace  				     *
 *===========================================================================*/
int
do_trace(void)
{
  register struct mproc *child;
  struct ptrace_range pr;
  int i, r, req;

  req = m_in.m_lc_pm_ptrace.req;
```

**讲解**: 处理 `ptrace()` 系统调用。

**局部变量**:
- `child`: 指向被调试进程的 mproc 结构
- `pr`: ptrace 范围结构，用于批量数据传输
- `req`: 请求类型

```c
  /* The T_OK call is made by the child fork of the debugger before it execs
   * the process to be traced. The T_ATTACH call is made by the debugger itself
   * to attach to an existing process.
   */
  switch (req) {
  case T_OK:		/* enable tracing by parent for this proc */
	if (mp->mp_tracer != NO_TRACER) return(EBUSY);

	mp->mp_tracer = mp->mp_parent;
	mp->mp_reply.m_pm_lc_ptrace.data = 0;
	return(OK);
```

**注释翻译**: "T_OK 调用由调试器的子进程 fork 在 exec 被跟踪进程之前发出。T_ATTACH 调用由调试器本身发出，用于附加到现有进程。"

**讲解**: 处理 `T_OK`（`PT_TRACE_ME`）请求。

**典型使用流程**:
```
调试器进程
    |
    v fork()
调试器父进程          调试器子进程
    |                      |
    |                      v ptrace(T_OK)
    |                      | 设置 mp_tracer = 父进程
    |                      v exec(被调试程序)
    |                      |
    v wait() <-------------+ 子进程停止
    |
    v ptrace(T_STEP/...)
```

**检查**: 
- `mp->mp_tracer != NO_TRACER`: 进程已被跟踪，返回 `EBUSY`

**设置**:
- `mp->mp_tracer = mp->mp_parent`: 设置跟踪者为父进程

```c
  case T_ATTACH:	/* attach to an existing process */
	if ((child = find_proc(m_in.m_lc_pm_ptrace.pid)) == NULL) return(ESRCH);
	if (child->mp_flags & EXITING) return(ESRCH);
```

**注释翻译**: "附加到现有进程"

**讲解**: 处理 `T_ATTACH`（`PT_ATTACH`）请求。

**查找目标进程**:
- `find_proc()`: 通过 PID 查找进程
- 检查进程是否正在退出

```c
	/* For non-root processes, user and group ID must match. */
	if (mp->mp_effuid != SUPER_USER &&
		(mp->mp_effuid != child->mp_effuid ||
		 mp->mp_effgid != child->mp_effgid ||
		 child->mp_effuid != child->mp_realuid ||
		 child->mp_effgid != child->mp_realgid)) return(EPERM);
```

**注释翻译**: "对于非 root 进程，用户和组 ID 必须匹配。"

**讲解**: 权限检查（非 root 用户）:
1. 调试器的 effective UID 必须等于目标进程的 effective UID
2. 调试器的 effective GID 必须等于目标进程的 effective GID
3. 目标进程的 effective UID 必须等于 real UID（防止调试 setuid 程序）
4. 目标进程的 effective GID 必须等于 real GID

**为什么这样设计**: 防止普通用户调试 setuid/setgid 程序获取提升的权限。

```c
	/* Only root may trace system servers. */
	if (mp->mp_effuid != SUPER_USER && (child->mp_flags & PRIV_PROC))
		return(EPERM);

	/* System servers may not trace anyone. They can use sys_trace(). */
	if (mp->mp_flags & PRIV_PROC) return(EPERM);
```

**注释翻译**: "只有 root 可以跟踪系统服务器。系统服务器不能跟踪任何人。它们可以使用 sys_trace()。"

**讲解**: 
- 系统服务器（`PRIV_PROC`）有特殊权限，只有 root 可以调试
- 系统服务器不能作为调试器（防止权限提升）

```c
	/* Can't trace self, PM or VM. */
	if (child == mp || child->mp_endpoint == PM_PROC_NR ||
		child->mp_endpoint == VM_PROC_NR) return(EPERM);

	/* Can't trace a process that is already being traced. */
	if (child->mp_tracer != NO_TRACER) return(EBUSY);
```

**注释翻译**: "不能跟踪自己、PM 或 VM。不能跟踪已被跟踪的进程。"

**讲解**: 保护关键进程：
- 不能调试自己（死锁）
- 不能调试 PM（进程管理器）
- 不能调试 VM（虚拟内存管理器）

```c
	child->mp_tracer = who_p;
	child->mp_trace_flags = TO_NOEXEC;

	sig_proc(child, SIGSTOP, TRUE /*trace*/, FALSE /* ksig */);

	mp->mp_reply.m_pm_lc_ptrace.data = 0;
	return(OK);
```

**讲解**: 设置跟踪关系：
1. `mp_tracer = who_p`: 设置跟踪者为调用者
2. `mp_trace_flags = TO_NOEXEC`: 设置跟踪选项（exec 时不发送信号）
3. 发送 `SIGSTOP` 信号停止目标进程

**TO_NOEXEC 标志**: 当被调试进程执行 exec 时，不发送 SIGTRAP 信号。

```c
  case T_STOP:		/* stop the process */
	/* This call is not exposed to user programs, because its effect can be
	 * achieved better by sending the traced process a signal with kill(2).
	 */
	return(EINVAL);
```

**注释翻译**: "这个调用不暴露给用户程序，因为通过 kill(2) 发送信号可以更好地实现其效果。"

**讲解**: `T_STOP` 不暴露给用户，使用 `kill(pid, SIGSTOP)` 替代。

```c
  case T_READB_INS:	/* special hack for reading text segments */
	if (mp->mp_effuid != SUPER_USER) return(EPERM);
	if ((child = find_proc(m_in.m_lc_pm_ptrace.pid)) == NULL) return(ESRCH);
	if (child->mp_flags & EXITING) return(ESRCH);

	r = sys_trace(req, child->mp_endpoint, m_in.m_lc_pm_ptrace.addr,
		&m_in.m_lc_pm_ptrace.data);
	if (r != OK) return(r);

	mp->mp_reply.m_pm_lc_ptrace.data = m_in.m_lc_pm_ptrace.data;
	return(OK);
```

**注释翻译**: "读取代码段的特殊技巧"

**讲解**: 处理 `T_READB_INS` 请求（读取代码段）。

**权限**: 只有 root 可以使用（因为涉及代码段修改检测）。

**调用**: `sys_trace()` 让系统任务执行实际的读取操作。

```c
  case T_WRITEB_INS:	/* special hack for patching text segments */
	if (mp->mp_effuid != SUPER_USER) return(EPERM);
	if ((child = find_proc(m_in.m_lc_pm_ptrace.pid)) == NULL) return(ESRCH);
	if (child->mp_flags & EXITING) return(ESRCH);

#if 0
	/* Should check for shared text */

	/* Make sure the text segment is not used as a source for shared
	 * text.
	 */
	child->mp_ino = 0;
	child->mp_dev = 0;
	child->mp_ctime = 0;
#endif

	r = sys_trace(req, child->mp_endpoint, m_in.m_lc_pm_ptrace.addr,
		&m_in.m_lc_pm_ptrace.data);
	if (r != OK) return(r);

	mp->mp_reply.m_pm_lc_ptrace.data = m_in.m_lc_pm_ptrace.data;
	return(OK);
  }
```

**注释翻译**: "修改代码段的特殊技巧。应该检查共享代码。确保代码段不被用作共享代码的源。"

**讲解**: 处理 `T_WRITEB_INS` 请求（修改代码段）。

**注释掉的代码**: 用于处理共享代码段的情况。修改共享代码段会影响所有使用该代码的进程。

**为什么禁用**: 共享代码段处理复杂，当前实现跳过。

```c
  /* All the other calls are made by the tracing process to control execution
   * of the child. For all these calls, the child must be stopped.
   */
  if ((child = find_proc(m_in.m_lc_pm_ptrace.pid)) == NULL) return(ESRCH);
  if (child->mp_flags & EXITING) return(ESRCH);
  if (child->mp_tracer != who_p) return(ESRCH);
  if (!(child->mp_flags & TRACE_STOPPED)) return(EBUSY);
```

**注释翻译**: "所有其他调用由跟踪进程发出，用于控制子进程的执行。对于所有这些调用，子进程必须已停止。"

**讲解**: 其他命令的前置条件检查：
1. 目标进程必须存在
2. 目标进程未退出
3. 调用者是目标进程的跟踪者
4. 目标进程已停止（`TRACE_STOPPED` 标志）

```c
  switch (req) {
  case T_EXIT:		/* exit */
	child->mp_flags |= TRACE_EXIT;

	/* Defer the exit if the traced process has a call pending. */
	if (child->mp_flags & (VFS_CALL | EVENT_CALL))
		child->mp_exitstatus = m_in.m_lc_pm_ptrace.data; /* save it */
	else
		exit_proc(child, m_in.m_lc_pm_ptrace.data,
			FALSE /*dump_core*/);

	/* Do not reply to the caller until VFS has processed the exit
	 * request.
	 */
	return(SUSPEND);
```

**注释翻译**: "退出。如果被跟踪进程有待处理的调用，延迟退出。在 VFS 处理完退出请求之前不要回复调用者。"

**讲解**: 处理 `T_EXIT`（`PT_KILL`）请求。

**流程**:
1. 设置 `TRACE_EXIT` 标志
2. 如果进程有 VFS 或 EVENT 调用待处理，保存退出状态
3. 否则立即调用 `exit_proc()` 退出
4. 返回 `SUSPEND`，等待 VFS 完成

**为什么延迟**: 防止在 VFS 调用期间退出导致状态不一致。

```c
  case T_SETOPT:	/* set trace options */
	child->mp_trace_flags = m_in.m_lc_pm_ptrace.data;

	mp->mp_reply.m_pm_lc_ptrace.data = 0;
	return(OK);
```

**注释翻译**: "设置跟踪选项"

**讲解**: 处理 `T_SETOPT` 请求。

**跟踪选项**:
- `TO_NOEXEC`: exec 时不发送信号

```c
  case T_GETRANGE:
  case T_SETRANGE:	/* get/set range of values */
	r = sys_datacopy(who_e, m_in.m_lc_pm_ptrace.addr, SELF, (vir_bytes)&pr,
		(phys_bytes)sizeof(pr));
	if (r != OK) return(r);

	if (pr.pr_space != TS_INS && pr.pr_space != TS_DATA) return(EINVAL);
	if (pr.pr_size == 0 || pr.pr_size > LONG_MAX) return(EINVAL);
```

**注释翻译**: "获取/设置值范围"

**讲解**: 处理 `T_GETRANGE` 和 `T_SETRANGE` 请求（批量数据传输）。

**参数验证**:
- `pr_space`: 必须是 `TS_INS`（代码段）或 `TS_DATA`（数据段）
- `pr_size`: 必须大于 0 且不超过 `LONG_MAX`

```c
	if (req == T_GETRANGE)
		r = sys_vircopy(child->mp_endpoint, (vir_bytes) pr.pr_addr,
			who_e, (vir_bytes) pr.pr_ptr,
			(phys_bytes) pr.pr_size, 0);
	else
		r = sys_vircopy(who_e, (vir_bytes) pr.pr_ptr,
			child->mp_endpoint, (vir_bytes) pr.pr_addr,
			(phys_bytes) pr.pr_size, 0);

	if (r != OK) return(r);

	mp->mp_reply.m_pm_lc_ptrace.data = 0;
	return(OK);
```

**讲解**: 执行批量数据复制：
- `T_GETRANGE`: 从被调试进程复制到调试器
- `T_SETRANGE`: 从调试器复制到被调试进程

```c
  case T_DETACH:	/* detach from traced process */
	if (m_in.m_lc_pm_ptrace.data < 0 || m_in.m_lc_pm_ptrace.data >= _NSIG)
		return(EINVAL);

	child->mp_tracer = NO_TRACER;
```

**注释翻译**: "从被跟踪进程分离"

**讲解**: 处理 `T_DETACH`（`PT_DETACH`）请求。

**参数验证**: 信号值必须在有效范围内。

**清除跟踪关系**: `mp_tracer = NO_TRACER`

```c
	/* Let all tracer-pending signals through the filter. */
	for (i = 1; i < _NSIG; i++) {
		if (sigismember(&child->mp_sigtrace, i)) {
			sigdelset(&child->mp_sigtrace, i);
			check_sig(child->mp_pid, i, FALSE /* ksig */);
		}
	}
```

**注释翻译**: "让所有跟踪器待处理的信号通过过滤器"

**讲解**: 分离时，发送所有被拦截的信号给进程。

**mp_sigtrace**: 存储被跟踪器拦截的信号。

```c
	if (m_in.m_lc_pm_ptrace.data > 0) {		/* issue signal */
		sig_proc(child, m_in.m_lc_pm_ptrace.data, TRUE /*trace*/,
			FALSE /* ksig */);
	}

	/* Resume the child as if nothing ever happened. */
	child->mp_flags &= ~TRACE_STOPPED;
	child->mp_trace_flags = 0;

	check_pending(child);

	break;
```

**注释翻译**: "恢复子进程，就像什么都没发生过一样"

**讲解**: 
1. 如果指定了信号，发送给进程
2. 清除 `TRACE_STOPPED` 标志
3. 清除跟踪选项
4. 检查待处理的信号

```c
  case T_RESUME:
  case T_STEP:
  case T_SYSCALL:	/* resume execution */
	if (m_in.m_lc_pm_ptrace.data < 0 || m_in.m_lc_pm_ptrace.data >= _NSIG)
		return(EINVAL);

	if (m_in.m_lc_pm_ptrace.data > 0) {		/* issue signal */
		sig_proc(child, m_in.m_lc_pm_ptrace.data, FALSE /*trace*/,
			FALSE /* ksig */);
	}
```

**注释翻译**: "恢复执行"

**讲解**: 处理 `T_RESUME`（继续）、`T_STEP`（单步）、`T_SYSCALL`（系统调用跟踪）请求。

**参数**: 可以指定一个信号发送给进程。

```c
	/* If there are any other signals waiting to be delivered,
	 * feign a successful resumption.
	 */
	for (i = 1; i < _NSIG; i++) {
		if (sigismember(&child->mp_sigtrace, i)) {
			mp->mp_reply.m_pm_lc_ptrace.data = 0;
			return(OK);
		}
	}

	child->mp_flags &= ~TRACE_STOPPED;

	check_pending(child);

	break;
  }
```

**注释翻译**: "如果有其他信号等待传递，假装恢复成功"

**讲解**: 
- 如果有待处理的信号，不立即恢复执行
- 清除 `TRACE_STOPPED` 标志
- 检查待处理的信号

```c
  r = sys_trace(req, child->mp_endpoint, m_in.m_lc_pm_ptrace.addr,
	&m_in.m_lc_pm_ptrace.data);
  if (r != OK) return(r);

  mp->mp_reply.m_pm_lc_ptrace.data = m_in.m_lc_pm_ptrace.data;
  return(OK);
}
```

**讲解**: 对于需要系统任务处理的命令，调用 `sys_trace()` 执行底层操作。

---

### trace_stop 函数

```c
/*===========================================================================*
 *				trace_stop				     *
 *===========================================================================*/
void
trace_stop(register struct mproc *rmp, int signo)
{
/* A traced process got a signal so stop it. */

  register struct mproc *rpmp = mproc + rmp->mp_tracer;
  int r;

  r = sys_trace(T_STOP, rmp->mp_endpoint, 0L, (long *) 0);
  if (r != OK) panic("sys_trace failed: %d", r);
```

**注释翻译**: "被跟踪的进程收到信号，停止它。"

**讲解**: 当被跟踪进程收到信号时，停止它并通知调试器。

**参数**:
- `rmp`: 被跟踪进程
- `signo`: 导致停止的信号

**调用系统任务**: `sys_trace(T_STOP, ...)` 停止进程。

```c
  rmp->mp_flags |= TRACE_STOPPED;
  if (wait_test(rpmp, rmp)) {
	/* TODO: rusage support */

	sigdelset(&rmp->mp_sigtrace, signo);

	rpmp->mp_flags &= ~WAITING;	/* parent is no longer waiting */
	rpmp->mp_reply.m_pm_lc_wait4.status = W_STOPCODE(signo);
	reply(rmp->mp_tracer, rmp->mp_pid);
  }
}
```

**讲解**: 
1. 设置 `TRACE_STOPPED` 标志
2. 检查调试器是否在等待（`wait_test()`）
3. 如果在等待，构造停止状态码（`W_STOPCODE`）
4. 回复调试器

**W_STOPCODE**: 宏，构造表示进程停止的 wait 状态码。

---

## 要点总结

### 核心知识点

1. **ptrace 分层处理**: PM 处理权限和状态，系统任务处理底层访问

2. **跟踪关系**: `mp_tracer` 字段记录跟踪者，`TRACE_STOPPED` 标志表示停止状态

3. **权限检查**: 非 root 用户只能调试自己的进程，不能调试 setuid/setgid 程序

### 关键标志

| 标志 | 含义 |
|------|------|
| `TRACE_STOPPED` | 进程因跟踪而停止 |
| `TRACE_EXIT` | 跟踪器强制进程退出 |
| `TO_NOEXEC` | exec 时不发送信号 |
| `NO_TRACER` | 进程未被跟踪 |

---

## 灾难预演

### 场景 1: 调试 PM 进程

**如果允许调试 PM**:
```c
// if (child->mp_endpoint == PM_PROC_NR) return(EPERM);
```

**后果**: 
- 调试器可以修改 PM 的代码和数据
- 可能破坏进程管理的完整性
- 系统崩溃

### 场景 2: 跳过权限检查

**如果删除权限检查**:
```c
// if (mp->mp_effuid != SUPER_USER && ...) return(EPERM);
```

**后果**: 
- 普通用户可以调试 setuid 程序
- 可能获取 root 权限
- 系统安全被破坏

### 场景 3: 不检查 TRACE_STOPPED

**如果删除停止状态检查**:
```c
// if (!(child->mp_flags & TRACE_STOPPED)) return(EBUSY);
```

**后果**: 
- 调试器可以在进程运行时修改其内存
- 可能导致竞态条件和数据损坏

---

## 互动自测

### 问题 1: T_OK 和 T_ATTACH 有什么区别？

**答案**:
- **T_OK**: 子进程主动允许父进程跟踪，在 fork 后、exec 前调用
- **T_ATTACH**: 调试器强制附加到运行中的进程
- **时机**: T_OK 用于启动调试，T_ATTACH 用于运行时附加

### 问题 2: 为什么不能调试 setuid 程序？

**答案**: 
- setuid 程序运行时 effective UID 不等于 real UID
- 如果允许调试，可以通过修改内存获取提升的权限
- 安全漏洞

### 问题 3: trace_stop 函数何时被调用？

**答案**: 
- 当被跟踪进程收到信号时
- 在 `sig_proc()` 函数中检测到进程被跟踪
- 调用 `trace_stop()` 停止进程并通知调试器

---

## Rust 实现对比

### ptrace 请求枚举

```rust
#![no_std]

use core::result::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtraceRequest {
    Stop,
    Ok,
    GetIns,
    GetData,
    GetUser,
    SetIns,
    SetData,
    SetUser,
    Resume,
    Exit,
    Step,
    Syscall,
    Attach,
    Detach,
    SetOpt,
    GetRange,
    SetRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceSpace {
    Ins,
    Data,
}

#[derive(Debug)]
pub struct PtraceRange {
    pub space: TraceSpace,
    pub addr: usize,
    pub ptr: usize,
    pub size: usize,
}
```

### 跟踪状态

```rust
pub const NO_TRACER: i32 = 0;

#[derive(Debug, Clone, Copy, Default)]
pub struct TraceState {
    pub tracer: i32,
    pub trace_flags: u32,
    pub stopped: bool,
    pub exit_pending: bool,
}

impl TraceState {
    pub fn is_traced(&self) -> bool {
        self.tracer != NO_TRACER
    }
}
```

### ptrace 处理函数

```rust
#[derive(Debug)]
pub enum PtraceError {
    InvalidRequest,
    ProcessNotFound,
    PermissionDenied,
    ProcessBusy,
    InvalidSignal,
    SystemError(i32),
}

pub fn do_trace(
    req: PtraceRequest,
    pid: i32,
    addr: usize,
    data: i32,
    current_uid: u32,
    is_root: bool,
    is_priv_proc: bool,
) -> Result<i32, PtraceError> {
    match req {
        PtraceRequest::Ok => {
            if current_tracer != NO_TRACER {
                return Err(PtraceError::ProcessBusy);
            }
            current_tracer = parent_index;
            Ok(0)
        }
        
        PtraceRequest::Attach => {
            let child = find_proc(pid).ok_or(PtraceError::ProcessNotFound)?;
            
            if child.is_exiting() {
                return Err(PtraceError::ProcessNotFound);
            }
            
            if !is_root {
                if current_uid != child.eff_uid 
                    || current_gid != child.eff_gid
                    || child.eff_uid != child.real_uid
                    || child.eff_gid != child.real_gid {
                    return Err(PtraceError::PermissionDenied);
                }
                
                if child.is_priv_proc {
                    return Err(PtraceError::PermissionDenied);
                }
            }
            
            if is_priv_proc {
                return Err(PtraceError::PermissionDenied);
            }
            
            if child.is_self || child.is_pm || child.is_vm {
                return Err(PtraceError::PermissionDenied);
            }
            
            if child.is_traced() {
                return Err(PtraceError::ProcessBusy);
            }
            
            child.tracer = caller_index;
            child.trace_flags = TO_NOEXEC;
            send_signal(child, SIGSTOP);
            Ok(0)
        }
        
        PtraceRequest::Detach => {
            if data < 0 || data >= NSIG {
                return Err(PtraceError::InvalidSignal);
            }
            
            child.tracer = NO_TRACER;
            
            for sig in 1..NSIG {
                if child.sigtrace.has(sig) {
                    child.sigtrace.remove(sig);
                    send_signal(child.pid, sig);
                }
            }
            
            if data > 0 {
                send_signal(child, data);
            }
            
            child.stopped = false;
            child.trace_flags = 0;
            check_pending(child);
            Ok(0)
        }
        
        _ => {
            sys_trace(req, child.endpoint, addr, data)
        }
    }
}
```

### Rust 实现的优势

1. **类型安全**: 使用枚举表示请求类型，编译时检查。

2. **错误处理**: 使用 `Result<T, E>` 显式处理错误。

3. **状态封装**: `TraceState` 封装跟踪状态，提供清晰接口。

4. **不可变引用**: 使用 `&self` 和 `&mut self` 区分只读和修改操作。

### Rust 实现的权衡

1. **运行时开销**: 枚举匹配比 C 的 switch 有轻微开销。

2. **与 C 交互**: 需要使用 `unsafe` 块与现有 C 代码交互。

3. **复杂性**: 类型安全的实现可能增加代码量。
