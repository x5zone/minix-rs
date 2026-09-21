# servers/pm/misc.c 逐行讲解

> **文件路径**: `minix3/minix/servers/pm/misc.c`
> **核心功能**: 杂项系统调用实现
> **代码行数**: 447 行

---

## 文件概述

### 是什么（功能说明）

这个文件实现了多个杂项系统调用，包括：

| 系统调用 | 功能 |
|---------|------|
| `do_reboot` | 重启/关机系统 |
| `do_getsysinfo` | 获取 PM 数据结构副本 |
| `do_getprocnr` | 通过 PID 查找端点 |
| `do_getepinfo` | 通过端点获取进程信息 |
| `do_getsetpriority` | 获取/设置进程优先级 |
| `do_svrctl` | 进程管理器控制 |
| `do_getrusage` | 获取资源使用统计 |
| `do_sysuname` | 获取/设置系统名称信息 |

### 为什么（设计原因）

**杂项系统调用的特点**:
- 不适合归类到其他文件
- 功能相对独立
- 代码量适中，可以合并到一个文件

**微内核设计**: 
- `do_getsysinfo` 允许系统服务获取 PM 的内部数据
- `do_getprocnr` 和 `do_getepinfo` 用于服务间通信

### 什么情景使用（应用场景）

| 系统调用 | 使用场景 |
|---------|---------|
| `reboot()` | 系统关机、重启 |
| `getpriority()` | 查看进程优先级 |
| `setpriority()` | 调整进程优先级 |
| `getrusage()` | 性能分析、资源监控 |
| `uname()` | 获取系统信息 |

---

## 头文件分析

```c
/* Miscellaneous system calls.				Author: Kees J. Bot
 *								31 Mar 2000
 * The entry points into this file are:
 *   do_reboot: kill all processes, then reboot system
 *   do_getsysinfo: request copy of PM data structure  (Jorrit N. Herder)
 *   do_getprocnr: lookup endpoint by process ID
 *   do_getepinfo: get the pid/uid/gid of a process given its endpoint
 *   do_getsetpriority: get/set process priority
 *   do_svrctl: process manager control
 *   do_getrusage: obtain process resource usage information
 */
```

**注释翻译**: "杂项系统调用。入口点：do_reboot（杀死所有进程后重启）、do_getsysinfo（请求 PM 数据结构副本）、do_getprocnr（通过进程 ID 查找端点）、do_getepinfo（通过端点获取进程的 pid/uid/gid）、do_getsetpriority（获取/设置进程优先级）、do_svrctl（进程管理器控制）、do_getrusage（获取进程资源使用信息）。"

```c
#include "pm.h"
#include <minix/callnr.h>
#include <signal.h>
#include <sys/svrctl.h>
#include <sys/reboot.h>
#include <sys/resource.h>
#include <sys/utsname.h>
#include <minix/com.h>
#include <minix/config.h>
#include <minix/sysinfo.h>
#include <minix/type.h>
#include <minix/ds.h>
#include <machine/archtypes.h>
#include <lib.h>
#include <assert.h>
#include "mproc.h"
#include "kernel/proc.h"
```

**讲解**: 包含必要的头文件：
- `sys/reboot.h`: 重启标志定义（`RB_POWERDOWN` 等）
- `sys/resource.h`: 资源使用结构 `struct rusage`
- `sys/utsname.h`: 系统名称结构 `struct utsname`
- `minix/sysinfo.h`: `SI_PROC_TAB` 等常量
- `minix/ds.h`: 数据存储服务接口

---

## 全局数据结构

### uname 信息

```c
/* START OF COMPATIBILITY BLOCK */
struct utsname uts_val = {
  OS_NAME,		/* system name */
  "noname",		/* node/network name */
  OS_RELEASE,		/* O.S. release (e.g. 3.3.0) */
  OS_VERSION,		/* O.S. version (e.g. Minix 3.3.0 (GENERIC)) */
#if defined(__i386__)
  "i386",		/* machine (cpu) type */
#elif defined(__arm__)
  "evbarm",		/* machine (cpu) type */
#else
#error			/* oops, no 'uname -mk' */
#endif
};
```

**注释翻译**: 
- "system name": 系统名称
- "node/network name": 节点/网络名称
- "O.S. release": 操作系统发行版本
- "O.S. version": 操作系统版本
- "machine (cpu) type": 机器（CPU）类型

**讲解**: `struct utsname` 存储 `uname` 系统调用返回的信息：

| 字段 | 示例值 | 含义 |
|------|--------|------|
| `sysname` | "Minix" | 操作系统名称 |
| `nodename` | "noname" | 主机名 |
| `release` | "3.3.0" | 内核版本 |
| `version` | "Minix 3.3.0 (GENERIC)" | 完整版本 |
| `machine` | "i386" 或 "evbarm" | 硬件架构 |

**条件编译**: 根据目标架构选择正确的机器类型字符串。

```c
static char *uts_tbl[] = {
#if defined(__i386__)
  "i386",		/* architecture */
#elif defined(__arm__)
  "evbarm",		/* architecture */
#endif
  NULL,			/* No kernel architecture */
  uts_val.machine,
  NULL,			/* No hostname */
  uts_val.nodename,
  uts_val.release,
  uts_val.version,
  uts_val.sysname,
  NULL,			/* No bus */			/* No bus */
};
/* END OF COMPATIBILITY BLOCK */
```

**讲解**: `uts_tbl` 是一个字符串指针数组，用于索引各个 uname 字段。某些字段（如 kernel architecture、hostname、bus）在 Minix 中未实现，设为 `NULL`。

### 系统调用统计

```c
#if ENABLE_SYSCALL_STATS
unsigned long calls_stats[NR_PM_CALLS];
#endif
```

**讲解**: 如果启用了系统调用统计（`ENABLE_SYSCALL_STATS`），则记录每个系统调用的调用次数。

---

## 函数详解

### do_sysuname 函数

```c
/*===========================================================================*
 *				do_sysuname				     *
 *===========================================================================*/
int
do_sysuname(void)
{
/* Set or get uname strings. */
  int r;
  size_t n;
  char *string;

  if (m_in.m_lc_pm_sysuname.field >= __arraycount(uts_tbl)) return(EINVAL);

  string = uts_tbl[m_in.m_lc_pm_sysuname.field];
  if (string == NULL)
	return EINVAL;	/* Unsupported field */
```

**注释翻译**: "设置或获取 uname 字符串。"

**讲解**: 处理 `uname()` 系统调用。

**参数验证**:
1. 检查字段索引是否越界
2. 检查字段是否支持（非 `NULL`）

```c
  switch (m_in.m_lc_pm_sysuname.req) {
  case 0:
	/* Copy an uname string to the user. */
	n = strlen(string) + 1;
	if (n > m_in.m_lc_pm_sysuname.len) n = m_in.m_lc_pm_sysuname.len;
	r = sys_datacopy(SELF, (vir_bytes)string, mp->mp_endpoint,
		m_in.m_lc_pm_sysuname.value, (phys_bytes)n);
	if (r < 0) return(r);
	break;

  default:
	return(EINVAL);
  }
  /* Return the number of bytes moved. */
  return(n);
}
```

**注释翻译**: "将 uname 字符串复制给用户。返回移动的字节数。"

**讲解**: 
- `req == 0`: 获取操作
- 计算字符串长度（包括结尾的 `\0`）
- 如果字符串比用户缓冲区长，截断
- 复制到用户空间
- 返回实际复制的字节数

---

### do_getsysinfo 函数

```c
/*===========================================================================*
 *				do_getsysinfo			       	     *
 *===========================================================================*/
int
do_getsysinfo(void)
{
  vir_bytes src_addr, dst_addr;
  size_t len;

  /* This call leaks important information. In the future, requests from
   * non-system processes should be denied.
   */
  if (mp->mp_effuid != 0)
  {
	printf("PM: unauthorized call of do_getsysinfo by proc %d '%s'\n",
		mp->mp_endpoint, mp->mp_name);
	sys_diagctl_stacktrace(mp->mp_endpoint);
	return EPERM;
  }
```

**注释翻译**: "这个调用泄露重要信息。将来，应该拒绝非系统进程的请求。"

**讲解**: 权限检查——只有超级用户（UID 0）可以调用。

**安全措施**:
- 打印警告信息
- 记录调用者的堆栈跟踪
- 返回 `EPERM`（权限不足）

```c
  switch(m_in.m_lsys_getsysinfo.what) {
  case SI_PROC_TAB:			/* copy entire process table */
        src_addr = (vir_bytes) mproc;
        len = sizeof(struct mproc) * NR_PROCS;
        break;
#if ENABLE_SYSCALL_STATS
  case SI_CALL_STATS:
  	src_addr = (vir_bytes) calls_stats;
  	len = sizeof(calls_stats);
  	break;
#endif
  default:
  	return(EINVAL);
  }
```

**注释翻译**: "复制整个进程表"

**讲解**: 根据请求类型确定数据源：
- `SI_PROC_TAB`: 复制整个 PM 进程表（`mproc` 数组）
- `SI_CALL_STATS`: 复制系统调用统计

```c
  if (len != m_in.m_lsys_getsysinfo.size)
	return(EINVAL);

  dst_addr = m_in.m_lsys_getsysinfo.where;
  return sys_datacopy(SELF, src_addr, who_e, dst_addr, len);
}
```

**讲解**: 
- 验证请求的大小与实际数据大小匹配
- 复制数据到调用者地址空间

**使用场景**: 系统诊断工具（如 `ps`、`top`）需要获取进程表信息。

---

### do_getprocnr 函数

```c
/*===========================================================================*
 *				do_getprocnr			             *
 *===========================================================================*/
int do_getprocnr(void)
{
  register struct mproc *rmp;

  /* This check should be replaced by per-call ACL checks. */
  if (who_e != RS_PROC_NR) {
	printf("PM: unauthorized call of do_getprocnr by %d\n", who_e);
	return EPERM;
  }
```

**注释翻译**: "这个检查应该被替换为每次调用的 ACL 检查。"

**讲解**: 权限检查——只有重启服务（RS）可以调用。

**RS（Reincarnation Server）**: 负责启动和监控系统服务。

```c
  if ((rmp = find_proc(m_in.m_lsys_pm_getprocnr.pid)) == NULL)
	return(ESRCH);

  mp->mp_reply.m_pm_lsys_getprocnr.endpt = rmp->mp_endpoint;
  return(OK);
}
```

**讲解**: 
- 通过 PID 查找进程
- 找到则返回进程端点
- 找不到返回 `ESRCH`（进程不存在）

**使用场景**: RS 需要通过 PID 找到进程端点，以便发送消息。

---

### do_getepinfo 函数

```c
/*===========================================================================*
 *				do_getepinfo			             *
 *===========================================================================*/
int do_getepinfo(void)
{
  struct mproc *rmp;
  endpoint_t ep;
  int r, slot, ngroups;

  ep = m_in.m_lsys_pm_getepinfo.endpt;
  if (pm_isokendpt(ep, &slot) != OK)
	return(ESRCH);
  rmp = &mproc[slot];
```

**讲解**: 通过端点获取进程信息。

**参数验证**: `pm_isokendpt()` 验证端点有效性并获取进程表槽位。

```c
  mp->mp_reply.m_pm_lsys_getepinfo.uid = rmp->mp_realuid;
  mp->mp_reply.m_pm_lsys_getepinfo.euid = rmp->mp_effuid;
  mp->mp_reply.m_pm_lsys_getepinfo.gid = rmp->mp_realgid;
  mp->mp_reply.m_pm_lsys_getepinfo.egid = rmp->mp_effgid;
  mp->mp_reply.m_pm_lsys_getepinfo.ngroups = ngroups = rmp->mp_ngroups;
```

**讲解**: 填充回复消息：
- Real UID / Effective UID
- Real GID / Effective GID
- 补充组数量

```c
  if (ngroups > m_in.m_lsys_pm_getepinfo.ngroups)
	ngroups = m_in.m_lsys_pm_getepinfo.ngroups;
  if (ngroups > 0) {
	if ((r = sys_datacopy(SELF, (vir_bytes)rmp->mp_sgroups, who_e,
	    m_in.m_lsys_pm_getepinfo.groups, ngroups * sizeof(gid_t))) != OK)
		return(r);
  }
  return(rmp->mp_pid);
}
```

**讲解**: 
- 限制复制的组数量不超过调用者请求的大小
- 复制补充组列表
- 返回进程 PID

**使用场景**: VFS 或其他服务需要获取进程的身份信息。

---

### do_reboot 函数

```c
/*===========================================================================*
 *				do_reboot				     *
 *===========================================================================*/
int
do_reboot(void)
{
  message m;

  /* Check permission to abort the system. */
  if (mp->mp_effuid != SUPER_USER) return(EPERM);
```

**注释翻译**: "检查中止系统的权限。"

**讲解**: 只有超级用户可以调用 `reboot()`。

```c
  /* See how the system should be aborted. */
  abort_flag = m_in.m_lc_pm_reboot.how;
```

**注释翻译**: "查看系统应该如何中止。"

**讲解**: `abort_flag` 是全局变量，存储重启方式：
- `RB_HALT`: 停机
- `RB_POWERDOWN`: 关机
- `RB_AUTOBOOT`: 重启

```c
  /* notify readclock (some arm systems power off via RTC alarms) */
  if (abort_flag & RB_POWERDOWN) {
	endpoint_t readclock_ep;
	if (ds_retrieve_label_endpt("readclock.drv", &readclock_ep) == OK) {
		message m; /* no params to set, nothing we can do if it fails */
		_taskcall(readclock_ep, RTCDEV_PWR_OFF, &m);
	}
  }
```

**注释翻译**: "通知 readclock（某些 ARM 系统通过 RTC 闹钟关机）"

**讲解**: 如果是关机操作，通知 readclock 驱动程序。某些 ARM 平台需要通过 RTC（实时时钟）控制器来切断电源。

```c
  /* Order matters here. When VFS is told to reboot, it exits all its
   * processes, and then would be confused if they're exited again by
   * SIGKILL. So first kill, then reboot.
   */

  check_sig(-1, SIGKILL, FALSE /* ksig*/); /* kill all users except init */
  sys_stop(INIT_PROC_NR);		   /* stop init, but keep it around */
```

**注释翻译**: "顺序很重要。当 VFS 被告知重启时，它会退出所有进程，如果它们再次被 SIGKILL 退出，VFS 会困惑。所以先杀死，再重启。"

**讲解**: 
1. `check_sig(-1, SIGKILL, FALSE)`: 向所有进程发送 `SIGKILL`（除了 init）
2. `sys_stop(INIT_PROC_NR)`: 停止 init 进程，但保留它

**为什么保留 init**: init 进程需要在系统重启时执行清理操作。

```c
  /* Tell VFS to reboot */
  memset(&m, 0, sizeof(m));
  m.m_type = VFS_PM_REBOOT;

  tell_vfs(&mproc[VFS_PROC_NR], &m);

  return(SUSPEND);			/* don't reply to caller */
}
```

**注释翻译**: "告诉 VFS 重启。不要回复调用者。"

**讲解**: 
- 发送 `VFS_PM_REBOOT` 消息给 VFS
- VFS 会同步文件系统、关闭所有文件
- 返回 `SUSPEND`，不回复调用者（系统即将重启）

**重启流程**:
```
用户调用 reboot()
       |
       v
PM 检查权限
       |
       v
通知 readclock（如果是关机）
       |
       v
发送 SIGKILL 给所有进程
       |
       v
停止 init 进程
       |
       v
通知 VFS 同步并重启
       |
       v
VFS 完成后通知内核重启
```

---

### do_getsetpriority 函数

```c
/*===========================================================================*
 *							do_getsetpriority			     *
 *===========================================================================*/
int
do_getsetpriority(void)
{
	int r, arg_which, arg_who, arg_pri;
	struct mproc *rmp;

	arg_which = m_in.m_lc_pm_priority.which;
	arg_who = m_in.m_lc_pm_priority.who;
	arg_pri = m_in.m_lc_pm_priority.prio;	/* for SETPRIORITY */
```

**注释翻译**: "用于 SETPRIORITY"

**讲解**: 处理 `getpriority()` 和 `setpriority()` 系统调用。

**参数**:
- `which`: 优先级类型（`PRIO_PROCESS`、`PRIO_PGRP`、`PRIO_USER`）
- `who`: 目标进程/进程组/用户的 ID
- `prio`: 新的优先级值（仅用于 `setpriority`）

```c
	/* Code common to GETPRIORITY and SETPRIORITY. */

	/* Only support PRIO_PROCESS for now. */
	if (arg_which != PRIO_PROCESS)
		return(EINVAL);
```

**注释翻译**: "GETPRIORITY 和 SETPRIORITY 的公共代码。目前只支持 PRIO_PROCESS。"

**讲解**: Minix 目前只支持进程级别的优先级操作。

```c
	if (arg_who == 0)
		rmp = mp;
	else
		if ((rmp = find_proc(arg_who)) == NULL)
			return(ESRCH);
```

**讲解**: 
- 如果 `who == 0`，操作当前进程
- 否则查找目标进程

```c
	if (mp->mp_effuid != SUPER_USER &&
	   mp->mp_effuid != rmp->mp_effuid && mp->mp_effuid != rmp->mp_realuid)
		return EPERM;
```

**讲解**: 权限检查：
- 超级用户可以操作任何进程
- 普通用户只能操作自己的进程

```c
	/* If GET, that's it. */
	if (call_nr == PM_GETPRIORITY) {
		return(rmp->mp_nice - PRIO_MIN);
	}
```

**注释翻译**: "如果是 GET，就这样。"

**讲解**: `getpriority()` 返回进程的 nice 值（偏移 `PRIO_MIN` 使其从 0 开始）。

**nice 值范围**: `PRIO_MIN`（-20，最高优先级）到 `PRIO_MAX`（20，最低优先级）。

```c
	/* Only root is allowed to reduce the nice level. */
	if (rmp->mp_nice > arg_pri && mp->mp_effuid != SUPER_USER)
		return(EACCES);
```

**注释翻译**: "只有 root 被允许降低 nice 值（提高优先级）。"

**讲解**: 普通用户只能提高 nice 值（降低优先级），不能降低 nice 值（提高优先级）。

**为什么这样设计**: 防止普通用户通过提高优先级来独占 CPU 资源。

```c
	/* We're SET, and it's allowed.
	 *
	 * The value passed in is currently between PRIO_MIN and PRIO_MAX.
	 * We have to scale this between MIN_USER_Q and MAX_USER_Q to match
	 * the kernel's scheduling queues.
	 */

	if ((r = sched_nice(rmp, arg_pri)) != OK) {
		return r;
	}

	rmp->mp_nice = arg_pri;
	return(OK);
}
```

**注释翻译**: "我们是 SET，并且被允许。传入的值在 PRIO_MIN 和 PRIO_MAX 之间。我们需要将其缩放到 MIN_USER_Q 和 MAX_USER_Q 之间，以匹配内核的调度队列。"

**讲解**: 
1. 调用 `sched_nice()` 更新调度器
2. 更新进程的 nice 值
3. 返回成功

**nice 值与调度优先级的关系**: nice 值越高，调度优先级越低。

---

### do_svrctl 函数

```c
/*===========================================================================*
 *				do_svrctl				     *
 *===========================================================================*/
int do_svrctl(void)
{
  unsigned long req;
  int s;
  vir_bytes ptr;
#define MAX_LOCAL_PARAMS 2
  static struct {
  	char name[30];
  	char value[30];
  } local_param_overrides[MAX_LOCAL_PARAMS];
  static int local_params = 0;

  req = m_in.m_lc_svrctl.request;
  ptr = m_in.m_lc_svrctl.arg;
```

**讲解**: 处理服务器控制请求。

**本地参数覆盖**: `local_param_overrides` 数组存储 PM 本地的参数覆盖，最多 2 个。

```c
  /* Is the request indeed for the PM? ('M' is old and being phased out) */
  if (IOCGROUP(req) != 'P' && IOCGROUP(req) != 'M') return(EINVAL);
```

**注释翻译**: "请求确实是给 PM 的吗？（'M' 是旧的，正在逐步淘汰）"

**讲解**: `IOCGROUP(req)` 提取请求的组别：
- `'P'`: PM 请求
- `'M'`: 旧的 PM 请求（兼容性）

```c
  /* Control operations local to the PM. */
  switch(req) {
  case OPMSETPARAM:
  case OPMGETPARAM:
  case PMSETPARAM:
  case PMGETPARAM: {
```

**注释翻译**: "PM 本地的控制操作。"

**讲解**: 处理参数设置/获取请求。

```c
      struct sysgetenv sysgetenv;
      char search_key[64];
      char *val_start;
      size_t val_len;
      size_t copy_len;

      /* Copy sysgetenv structure to PM. */
      if (sys_datacopy(who_e, ptr, SELF, (vir_bytes) &sysgetenv,
              sizeof(sysgetenv)) != OK) return(EFAULT);
```

**注释翻译**: "将 sysgetenv 结构复制到 PM。"

**讲解**: 从用户空间复制参数结构。

```c
      /* Set a param override? */
      if (req == PMSETPARAM || req == OPMSETPARAM) {
  	if (local_params >= MAX_LOCAL_PARAMS) return ENOSPC;
  	if (sysgetenv.keylen <= 0
  	 || sysgetenv.keylen >=
  	 	 sizeof(local_param_overrides[local_params].name)
  	 || sysgetenv.vallen <= 0
  	 || sysgetenv.vallen >=
  	 	 sizeof(local_param_overrides[local_params].value))
  		return EINVAL;

          if ((s = sys_datacopy(who_e, (vir_bytes) sysgetenv.key,
            SELF, (vir_bytes) local_param_overrides[local_params].name,
               sysgetenv.keylen)) != OK)
               	return s;
          if ((s = sys_datacopy(who_e, (vir_bytes) sysgetenv.val,
            SELF, (vir_bytes) local_param_overrides[local_params].value,
              sysgetenv.vallen)) != OK)
               	return s;
            local_param_overrides[local_params].name[sysgetenv.keylen] = '\0';
            local_param_overrides[local_params].value[sysgetenv.vallen] = '\0';

  	local_params++;

  	return OK;
      }
```

**注释翻译**: "设置参数覆盖？"

**讲解**: 设置参数：
1. 检查是否还有空间
2. 验证键和值的长度
3. 复制键和值
4. 添加字符串结束符
5. 增加计数

```c
      if (sysgetenv.keylen == 0) {	/* copy all parameters */
          val_start = monitor_params;
          val_len = sizeof(monitor_params);
      }
      else {				/* lookup value for key */
```

**注释翻译**: "复制所有参数" / "查找键对应的值"

**讲解**: 
- 如果 `keylen == 0`，返回所有参数
- 否则查找特定键的值

```c
      	  int p;
          /* Try to get a copy of the requested key. */
          if (sysgetenv.keylen > sizeof(search_key)) return(EINVAL);
          if ((s = sys_datacopy(who_e, (vir_bytes) sysgetenv.key,
                  SELF, (vir_bytes) search_key, sysgetenv.keylen)) != OK)
              return(s);

          /* Make sure key is null-terminated and lookup value.
           * First check local overrides.
           */
          search_key[sysgetenv.keylen-1]= '\0';
          for(p = 0; p < local_params; p++) {
          	if (!strcmp(search_key, local_param_overrides[p].name)) {
          		val_start = local_param_overrides[p].value;
          		break;
          	}
          }
          if (p >= local_params && (val_start = find_param(search_key)) == NULL)
               return(ESRCH);
          val_len = strlen(val_start) + 1;
      }
```

**注释翻译**: "确保键以 null 结尾并查找值。首先检查本地覆盖。"

**讲解**: 
1. 复制搜索键
2. 先在本地覆盖中查找
3. 如果没找到，在 monitor 参数中查找
4. 计算值长度

```c
      /* See if it fits in the client's buffer. */
      if (val_len > sysgetenv.vallen)
      	return E2BIG;

      /* Value found, make the actual copy (as far as possible). */
      copy_len = MIN(val_len, sysgetenv.vallen);
      if ((s=sys_datacopy(SELF, (vir_bytes) val_start,
              who_e, (vir_bytes) sysgetenv.val, copy_len)) != OK)
          return(s);

      return OK;
  }

  default:
	return(EINVAL);
  }
}
```

**注释翻译**: "查看是否适合客户端的缓冲区。值找到后，进行实际复制（尽可能多）。"

**讲解**: 
1. 检查缓冲区大小
2. 复制值到用户空间

---

### do_getrusage 函数

```c
/*===========================================================================*
 *				do_getrusage				     *
 *===========================================================================*/
int
do_getrusage(void)
{
	clock_t user_time, sys_time;
	struct rusage r_usage;
	int r, children;

	if (m_in.m_lc_pm_rusage.who != RUSAGE_SELF &&
	    m_in.m_lc_pm_rusage.who != RUSAGE_CHILDREN)
		return EINVAL;
```

**讲解**: 处理 `getrusage()` 系统调用。

**参数验证**: 只支持 `RUSAGE_SELF`（当前进程）和 `RUSAGE_CHILDREN`（子进程）。

```c
	/*
	 * TODO: first relay the call to VFS.  As is, VFS does not have any
	 * fields it can fill with meaningful values, but this may change in
	 * the future.  In that case, PM would first have to use the tell_vfs()
	 * system to get those values from VFS, and do the rest here upon
	 * getting the response.
	 */

	memset(&r_usage, 0, sizeof(r_usage));
```

**注释翻译**: "TODO: 首先将调用转发给 VFS。目前 VFS 没有任何可以填充有意义值的字段，但这可能会在未来改变。"

**讲解**: 初始化 `rusage` 结构，清零所有字段。

```c
	children = (m_in.m_lc_pm_rusage.who == RUSAGE_CHILDREN);

	/*
	 * Get system times.  For RUSAGE_SELF, get the times for the calling
	 * process from the kernel.  For RUSAGE_CHILDREN, we already have the
	 * values we should return right here.
	 */
	if (!children) {
		if ((r = sys_times(who_e, &user_time, &sys_time, NULL,
		    NULL)) != OK)
			return r;
	} else {
		user_time = mp->mp_child_utime;
		sys_time = mp->mp_child_stime;
	}
```

**注释翻译**: "获取系统时间。对于 RUSAGE_SELF，从内核获取调用进程的时间。对于 RUSAGE_CHILDREN，我们已经有了应该返回的值。"

**讲解**: 
- `RUSAGE_SELF`: 从内核获取当前进程的用户时间和系统时间
- `RUSAGE_CHILDREN`: 使用 PM 保存的子进程累计时间

**子进程时间**: 当子进程退出时，PM 会将其 CPU 时间累加到父进程的 `mp_child_utime` 和 `mp_child_stime` 字段。

```c
	/* In both cases, convert from clock ticks to microseconds. */
	set_rusage_times(&r_usage, user_time, sys_time);

	/* Get additional fields from VM. */
	if ((r = vm_getrusage(who_e, &r_usage, children)) != OK)
		return r;

	/* Finally copy the structure to the caller. */
	return sys_datacopy(SELF, (vir_bytes)&r_usage, who_e,
	    m_in.m_lc_pm_rusage.addr, (vir_bytes)sizeof(r_usage));
}
```

**注释翻译**: "在两种情况下，都将时钟滴答转换为微秒。从 VM 获取额外字段。最后将结构复制给调用者。"

**讲解**: 
1. `set_rusage_times()`: 将时钟滴答转换为微秒，存入 `rusage` 结构
2. `vm_getrusage()`: 从 VM 获取内存使用信息
3. 复制结果到用户空间

**struct rusage 结构**:
```c
struct rusage {
    struct timeval ru_utime;    // 用户 CPU 时间
    struct timeval ru_stime;    // 系统 CPU 时间
    long ru_maxrss;             // 最大驻留集大小
    long ru_ixrss;              // 共享内存大小
    long ru_idrss;              // 非共享数据大小
    long ru_isrss;              // 非共享栈大小
    long ru_minflt;             // 页面错误（无需 I/O）
    long ru_majflt;             // 页面错误（需要 I/O）
    long ru_nswap;              // 交换次数
    long ru_inblock;            // 块输入操作
    long ru_oublock;            // 块输出操作
    long ru_msgsnd;             // 发送的消息
    long ru_msgrcv;             // 接收的消息
    long ru_nsignals;           // 接收的信号
    long ru_nvcsw;              // 自愿上下文切换
    long ru_nivcsw;             // 非自愿上下文切换
};
```

---

## 要点总结

### 核心知识点

1. **reboot 流程**: 杀死所有进程 → 停止 init → 通知 VFS → 内核重启

2. **优先级管理**: nice 值范围 -20 到 20，普通用户只能提高 nice 值

3. **资源统计**: `getrusage()` 获取 CPU 时间、内存使用等信息

### 关键函数

| 函数 | 功能 |
|------|------|
| `check_sig(-1, SIGKILL, FALSE)` | 向所有进程发送 SIGKILL |
| `sched_nice()` | 更新调度器优先级 |
| `sys_times()` | 从内核获取 CPU 时间 |
| `vm_getrusage()` | 从 VM 获取内存使用信息 |

---

## 灾难预演

### 场景 1: reboot 权限检查缺失

**如果删除权限检查**:
```c
// if (mp->mp_effuid != SUPER_USER) return(EPERM);
```

**后果**: 普通用户可以重启系统，导致服务中断和数据丢失。

### 场景 2: setpriority 不限制优先级提升

**如果删除限制**:
```c
// if (rmp->mp_nice > arg_pri && mp->mp_effuid != SUPER_USER)
//     return(EACCES);
```

**后果**: 普通用户可以提高进程优先级，可能独占 CPU 资源，影响系统稳定性。

### 场景 3: getsysinfo 不检查权限

**如果删除权限检查**:
```c
// if (mp->mp_effuid != 0) return EPERM;
```

**后果**: 普通用户可以获取整个进程表，泄露敏感信息（如其他用户的进程信息）。

---

## 互动自测

### 问题 1: reboot 系统调用的完整流程是什么？

**答案**:
1. 检查调用者是否是超级用户
2. 设置 `abort_flag` 表示重启类型
3. 如果是关机，通知 readclock 驱动
4. 向所有进程发送 `SIGKILL`（除了 init）
5. 停止 init 进程
6. 通知 VFS 同步文件系统并重启
7. 内核执行重启

### 问题 2: 为什么普通用户只能提高 nice 值？

**答案**: 
- 提高 nice 值 = 降低优先级 = 更少 CPU 时间
- 降低 nice 值 = 提高优先级 = 更多 CPU 时间
- 如果允许普通用户提高优先级，可能导致资源争抢和系统不稳定

### 问题 3: getrusage() 如何获取子进程的资源使用？

**答案**: 
- 子进程退出时，PM 将其 CPU 时间累加到父进程的 `mp_child_utime` 和 `mp_child_stime`
- `getrusage(RUSAGE_CHILDREN)` 返回这些累计值
- 只统计已退出的子进程

---

## Rust 实现对比

### 系统信息结构

```rust
#![no_std]

use core::result::Result;

#[derive(Debug, Clone)]
pub struct UtsName {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
}

impl UtsName {
    pub fn new() -> Self {
        let mut sysname = [0u8; 65];
        let mut nodename = [0u8; 65];
        let mut release = [0u8; 65];
        let mut version = [0u8; 65];
        let mut machine = [0u8; 65];
        
        copy_str(&mut sysname, "Minix");
        copy_str(&mut nodename, "noname");
        copy_str(&mut release, "3.3.0");
        copy_str(&mut version, "Minix 3.3.0 (GENERIC)");
        
        #[cfg(target_arch = "x86")]
        copy_str(&mut machine, "i386");
        #[cfg(target_arch = "arm")]
        copy_str(&mut machine, "evbarm");
        
        Self { sysname, nodename, release, version, machine }
    }
}

fn copy_str(dest: &mut [u8], src: &str) {
    let bytes = src.as_bytes();
    let len = core::cmp::min(bytes.len(), dest.len() - 1);
    dest[..len].copy_from_slice(&bytes[..len]);
}
```

### 资源使用结构

```rust
#[derive(Debug, Clone, Copy, Default)]
pub struct TimeVal {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

#[derive(Debug, Clone, Default)]
pub struct RUsage {
    pub ru_utime: TimeVal,      // 用户 CPU 时间
    pub ru_stime: TimeVal,      // 系统 CPU 时间
    pub ru_maxrss: i64,         // 最大驻留集大小
    pub ru_ixrss: i64,          // 共享内存大小
    pub ru_idrss: i64,          // 非共享数据大小
    pub ru_isrss: i64,          // 非共享栈大小
    pub ru_minflt: i64,         // 页面错误（无需 I/O）
    pub ru_majflt: i64,         // 页面错误（需要 I/O）
    pub ru_nswap: i64,          // 交换次数
    pub ru_inblock: i64,        // 块输入操作
    pub ru_oublock: i64,        // 块输出操作
    pub ru_msgsnd: i64,         // 发送的消息
    pub ru_msgrcv: i64,         // 接收的消息
    pub ru_nsignals: i64,       // 接收的信号
    pub ru_nvcsw: i64,          // 自愿上下文切换
    pub ru_nivcsw: i64,         // 非自愿上下文切换
}

pub const RUSAGE_SELF: i32 = 0;
pub const RUSAGE_CHILDREN: i32 = -1;
```

### 优先级管理

```rust
pub const PRIO_MIN: i32 = -20;
pub const PRIO_MAX: i32 = 20;
pub const PRIO_PROCESS: i32 = 0;

#[derive(Debug)]
pub enum PriorityError {
    InvalidProcess,
    PermissionDenied,
    InvalidValue,
}

pub fn getpriority(which: i32, who: i32, current_uid: u32) -> Result<i32, PriorityError> {
    if which != PRIO_PROCESS {
        return Err(PriorityError::InvalidValue);
    }
    
    let target = if who == 0 {
        current_uid
    } else {
        who as u32
    };
    
    // 查找进程并返回 nice 值
    Ok(0) // 默认 nice 值
}

pub fn setpriority(
    which: i32, 
    who: i32, 
    prio: i32,
    current_uid: u32,
    is_root: bool
) -> Result<(), PriorityError> {
    if which != PRIO_PROCESS {
        return Err(PriorityError::InvalidValue);
    }
    
    if prio < PRIO_MIN || prio > PRIO_MAX {
        return Err(PriorityError::InvalidValue);
    }
    
    // 普通用户只能提高 nice 值（降低优先级）
    // 这里需要比较当前 nice 值和新值
    
    Ok(())
}
```

### 重启控制

```rust
pub const RB_HALT: i32 = 0x0001;
pub const RB_POWERDOWN: i32 = 0x0002;
pub const RB_AUTOBOOT: i32 = 0x0004;

#[derive(Debug)]
pub enum RebootError {
    PermissionDenied,
    SystemError,
}

pub fn reboot(how: i32, is_root: bool) -> Result<!, RebootError> {
    if !is_root {
        return Err(RebootError::PermissionDenied);
    }
    
    // 1. 如果是关机，通知 readclock
    if (how & RB_POWERDOWN) != 0 {
        // notify_readclock();
    }
    
    // 2. 杀死所有进程（除了 init）
    // check_sig(-1, SIGKILL);
    
    // 3. 停止 init
    // sys_stop(INIT_PROC_NR);
    
    // 4. 通知 VFS
    // tell_vfs(VFS_PM_REBOOT);
    
    // 5. 内核重启（永不返回）
    loop {}
}
```

### Rust 实现的优势

1. **类型安全**: `UtsName`、`RUsage` 等结构有明确的类型定义。

2. **枚举错误**: 使用 `enum` 明确列出所有可能的错误情况。

3. **常量定义**: 使用 `const` 定义常量，编译时检查。

4. **条件编译**: 使用 `#[cfg(target_arch)]` 处理架构差异。

5. **永不返回**: `reboot()` 返回 `!` 类型，表示永不返回。

### Rust 实现的权衡

1. **固定大小数组**: 需要预先定义字符串缓冲区大小。

2. **错误处理开销**: 使用 `Result` 需要额外的内存存储错误信息。

3. **与 C 交互**: 需要使用 `unsafe` 块与现有 C 代码交互。
