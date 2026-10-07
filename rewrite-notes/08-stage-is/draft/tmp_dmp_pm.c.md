# IS Server - dmp_pm.c 逐行讲解

## 文件概述

**文件路径**: `minix/servers/is/dmp_pm.c`

**核心功能**: PM（进程管理器）数据结构的调试转储，包括PM进程表和信号处理信息。

**设计思路**: 通过`getsysinfo`系统调用从PM获取数据副本，然后在IS中格式化输出。

---

## 头文件包含

```c
/* This file contains procedures to dump to PM' data structures.
 *
 * The entry points into this file are
 *   mproc_dmp:   	display PM process table
 *
 * Created:
 *   May 11, 2005:	by Jorrit N. Herder
 */

#include "inc.h"
#include "../pm/mproc.h"
#include <minix/timers.h>
#include <minix/config.h>
#include <minix/type.h>

struct mproc mproc[NR_PROCS];
```

**逐行讲解**:
- 包含PM的头文件`../pm/mproc.h`
- `struct mproc mproc[NR_PROCS]`：PM进程表副本
  - 静态存储，大小为`NR_PROCS * sizeof(struct mproc)`

---

## flags_str函数

```c
/*===========================================================================*
 *				mproc_dmp				     *
 *===========================================================================*/
static char *flags_str(int flags)
{
	static char str[12];
	str[0] = (flags & WAITING) ? 'W' : '-';
	str[1] = (flags & ZOMBIE)  ? 'Z' : '-';
	str[2] = (flags & ALARM_ON)  ? 'A' : '-';
	str[3] = (flags & EXITING) ? 'E' : '-';
	str[4] = (flags & TRACE_STOPPED)  ? 'T' : '-';
	str[5] = (flags & SIGSUSPENDED)  ? 'U' : '-';
	str[6] = (flags & VFS_CALL) ? 'F' : '-';
	str[7] = (flags & PROC_STOPPED) ? 's' : '-';
	str[8] = (flags & PRIV_PROC)  ? 'p' : '-';
	str[9] = (flags & PARTIAL_EXEC) ? 'x' : '-';
	str[10] = (flags & DELAY_CALL) ? 'd' : '-';
	str[11] = '\0';

	return str;
}
```

**逐行讲解**:
- `static char *flags_str(int flags)`：将PM进程标志转换为字符串
  - W：等待子进程
  - Z：僵尸进程
  - A：闹钟开启
  - E：正在退出
  - T：被ptrace停止
  - U：信号挂起
  - F：VFS调用中
  - s：进程停止
  - p：特权进程
  - x：部分exec
  - d：延迟调用

---

## mproc_dmp函数

```c
void
mproc_dmp(void)
{
  struct mproc *mp;
  int i, n=0;
  static int prev_i = 0;

  if (getsysinfo(PM_PROC_NR, SI_PROC_TAB, mproc, sizeof(mproc)) != OK) {
	printf("Error obtaining table from PM. Perhaps recompile IS?\n");
	return;
  }
```

**逐行讲解**:
- `void mproc_dmp(void)`：显示PM进程表
  - 按Shift+F1触发
- `getsysinfo(PM_PROC_NR, SI_PROC_TAB, mproc, sizeof(mproc))`：从PM获取进程表
  - `PM_PROC_NR`：PM的端点
  - `SI_PROC_TAB`：请求进程表
- `static int prev_i = 0`：记录上次显示位置，支持分页

---

```c
  printf("Process manager (PM) process table dump\n");
  printf("-process- -nr-pnr-tnr- --pid--ppid--pgrp- -uid--  -gid--  -nice- -flags-----\n");
  for (i=prev_i; i<NR_PROCS; i++) {
  	mp = &mproc[i];
  	if (mp->mp_pid == 0 && i != PM_PROC_NR) continue;
  	if (++n > 22) break;
  	printf("%8.8s %4d%4d%4d  %5d %5d %5d  ",
  		mp->mp_name, i, mp->mp_parent, mp->mp_tracer, mp->mp_pid, mproc[mp->mp_parent].mp_pid, mp->mp_procgrp);
  	printf("%2d(%2d)  %2d(%2d)   ",
  		mp->mp_realuid, mp->mp_effuid, mp->mp_realgid, mp->mp_effgid);
  	printf(" %3d  %s  ",
  		mp->mp_nice, flags_str(mp->mp_flags));
  	printf("\n");
  }
  if (i >= NR_PROCS) i = 0;
  else printf("--more--\r");
  prev_i = i;
}
```

**逐行讲解**:
- 遍历PM进程表
- 跳过未使用的槽（pid=0且不是PM自身）
- 每页显示22行
- 打印每个进程的信息：
  - 名称、索引、父进程、跟踪者
  - PID、父PID、进程组
  - UID、GID（真实和有效）
  - nice值、标志

---

## sigaction_dmp函数

```c
/*===========================================================================*
 *				sigaction_dmp				     *
 *===========================================================================*/
void
sigaction_dmp(void)
{
  struct mproc *mp;
  int i, n=0;
  static int prev_i = 0;
  clock_t uptime;

  if (getsysinfo(PM_PROC_NR, SI_PROC_TAB, mproc, sizeof(mproc)) != OK) {
	printf("Error obtaining table from PM. Perhaps recompile IS?\n");
	return;
  }
  uptime = getticks();
```

**逐行讲解**:
- `void sigaction_dmp(void)`：显示信号处理信息
  - 按Shift+F2触发
- `getticks()`：获取系统运行时间
  - 用于计算闹钟剩余时间

---

```c
  printf("Process manager (PM) signal action dump\n");
  printf("-process- -nr- --ignore- --catch- --block- -pending- -alarm---\n");
  for (i=prev_i; i<NR_PROCS; i++) {
  	mp = &mproc[i];
  	if (mp->mp_pid == 0 && i != PM_PROC_NR) continue;
  	if (++n > 22) break;
  	printf("%8.8s  %3d  ", mp->mp_name, i);
	printf(" %08x %08x %08x ",
		mp->mp_ignore.__bits[0], mp->mp_catch.__bits[0],
		mp->mp_sigmask.__bits[0]);
	printf("%08x  ", mp->mp_sigpending.__bits[0]);
  	if (mp->mp_flags & ALARM_ON) printf("%8lu",
		(unsigned long) (mp->mp_timer.tmr_exp_time-uptime));
  	else printf("       -");
  	printf("\n");
  }
  if (i >= NR_PROCS) i = 0;
  else printf("--more--\r");
  prev_i = i;
}
```

**逐行讲解**:
- 打印每个进程的信号处理信息：
  - 忽略的信号位图
  - 捕获的信号位图
  - 阻塞的信号位图
  - 待处理的信号位图
  - 闹钟剩余时间（如果有）

**信号位图**:
```
mp_ignore.__bits[0] (32位):
┌───┬───┬───┬───┬───┬───┬───┬───┐
│ 0 │ 0 │ 1 │ 0 │ 0 │ 0 │ 0 │ 0 │ ...
└───┴───┴───┴───┴───┴───┴───┴───┘
  1   2   3   4   5   6   7   8

位3被设置，表示SIGQUIT被忽略
```

---

## 要点总结

1. **跨服务通信**: 通过`getsysinfo`从PM获取数据，展示服务间协作。

2. **分页显示**: 使用静态变量记录位置，实现分页功能。

3. **标志可视化**: 将位图和标志转换为可读字符串。

---

## 灾难预演

**如果PM进程表结构改变但IS未重新编译会怎样？**

`getsysinfo`复制的数据大小不匹配，可能导致：
1. 数据错位
2. 访问无效内存
3. 显示错误信息

**如果删除`mp->mp_pid == 0`检查会怎样？**

会显示大量未使用的进程槽，输出混乱，难以找到有效信息。

---

## 互动自测

1. **内存模型**: `mproc`数组存储在哪个内存段？大小是多少？

2. **设计选择**: 为什么使用`getsysinfo`而不是直接访问PM的内存？

3. **信号位图**: 如果`mp_ignore.__bits[0] = 0x00000008`，表示哪些信号被忽略？
