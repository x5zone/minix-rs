# IS Server - dmp_rs.c 逐行讲解

## 文件概述

**文件路径**: `minix/servers/is/dmp_rs.c`

**核心功能**: RS（重启动服务器）数据结构的调试转储，显示系统服务进程表。

---

## 头文件包含

```c
/* This file contains procedures to dump RS data structures.
 *
 * The entry points into this file are
 *   rproc_dump:   	display RS system process table
 *
 * Created:
 *   Oct 03, 2005:	by Jorrit N. Herder
 */

#include "inc.h"
#include <minix/timers.h>
#include <minix/rs.h>
#include "kernel/priv.h"
#include "../rs/const.h"
#include "../rs/type.h"

struct rprocpub rprocpub[NR_SYS_PROCS];
struct rproc rproc[NR_SYS_PROCS];
```

**逐行讲解**:
- 包含RS相关头文件
- `struct rprocpub rprocpub[NR_SYS_PROCS]`：RS公共进程表副本
- `struct rproc rproc[NR_SYS_PROCS]`：RS私有进程表副本

---

## s_flags_str函数

```c
static char *s_flags_str(int flags, int sys_flags)
{
	static char str[10];
	str[0] = (flags & RS_ACTIVE)        ? 'A' : '-';
	str[1] = (flags & RS_UPDATING)      ? 'U' : '-';
	str[2] = (flags & RS_EXITING)       ? 'E' : '-';
	str[3] = (flags & RS_NOPINGREPLY)   ? 'N' : '-';
	str[4] = (sys_flags & SF_USE_COPY)  ? 'C' : '-';
	str[5] = (sys_flags & SF_USE_REPL)  ? 'R' : '-';
	str[6] = '\0';

	return(str);
}
```

**逐行讲解**:
- `static char *s_flags_str(...)`：将RS进程标志转换为字符串
  - A：活跃
  - U：正在更新
  - E：正在退出
  - N：无ping回复
  - C：使用副本
  - R：使用副本（备用）

---

## rproc_dmp函数

```c
/*===========================================================================*
 *				rproc_dmp				     *
 *===========================================================================*/
void
rproc_dmp(void)
{
  struct rproc *rp;
  struct rprocpub *rpub;
  int i, n=0;
  static int prev_i=0;

  if (getsysinfo(RS_PROC_NR, SI_PROCPUB_TAB, rprocpub, sizeof(rprocpub)) != OK
	|| getsysinfo(RS_PROC_NR, SI_PROC_TAB, rproc, sizeof(rproc)) != OK) {
	printf("Error obtaining table from RS. Perhaps recompile IS?\n");
	return;
  }
```

**逐行讲解**:
- `void rproc_dmp(void)`：显示RS进程表
  - 按Shift+F6触发
- 获取RS的公共和私有进程表

---

```c
  printf("Reincarnation Server (RS) system process table dump\n");
  printf("----label---- endpoint- -pid- flags- -dev- -T- alive_tm starts command\n");
  for (i=prev_i; i<NR_SYS_PROCS; i++) {
  	rp = &rproc[i];
  	rpub = &rprocpub[i];
  	if (! (rp->r_flags & RS_IN_USE)) continue;
  	if (++n > 22) break;
	printf("%13s %9d %5d %6s %4d %4lu %8u %5dx %s",
  		rpub->label, rpub->endpoint, rp->r_pid,
		s_flags_str(rp->r_flags, rpub->sys_flags), rpub->dev_nr,
		(unsigned long) rp->r_period,
		(unsigned int) rp->r_alive_tm, rp->r_restarts,
		rp->r_args
  	);
	printf("\n");
  }
  if (i >= NR_SYS_PROCS) i = 0;
  else printf("--more--\r");
  prev_i = i;
}
```

**逐行讲解**:
- 遍历RS进程表
- 跳过未使用的槽
- 打印每个系统服务的信息：
  - 标签、端点、PID
  - 标志、设备号
  - 周期、存活时间
  - 重启次数、命令行参数

**RS进程表输出示例**:
```
Reincarnation Server (RS) system process table dump
----label---- endpoint- -pid- flags- -dev- -T- alive_tm starts command
           rs         4    42  A----   -1    0      123     0x /sbin/rs
           vm         5    43  A----   -1    0      123     0x /sbin/vm
           pm         6    44  A----   -1    0      123     0x /sbin/pm
         sched         7    45  A----   -1    0      123     0x /sbin/sched
          vfs         8    46  A----   -1    0      123     0x /sbin/vfs
```

---

## 要点总结

1. **系统服务管理**: RS管理所有系统服务的启动、监控和重启。

2. **重启统计**: 记录服务的存活时间和重启次数，用于诊断问题。

3. **服务状态**: 显示服务的当前状态（活跃、更新中、退出中等）。

---

## 灾难预演

**如果RS进程表损坏会怎样？**

显示错误的服务信息，可能导致诊断困难。

**如果服务频繁重启会怎样？**

`r_restarts`字段会显示较高的值，提示可能存在问题。
