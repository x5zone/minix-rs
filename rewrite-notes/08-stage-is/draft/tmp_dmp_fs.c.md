# IS Server - dmp_fs.c 逐行讲解

## 文件概述

**文件路径**: `minix/servers/is/dmp_fs.c`

**核心功能**: VFS（虚拟文件系统）数据结构的调试转储，包括VFS进程表和设备映射表。

---

## 头文件包含

```c
/* This file contains procedures to dump to FS' data structures.
 *
 * The entry points into this file are
 *   dtab_dump:   	display device <-> driver mappings
 *   fproc_dump:   	display FS process table
 *
 * Created:
 *   Oct 01, 2004:	by Jorrit N. Herder
 */

#include "inc.h"
#include "mfs/const.h"
#include "vfs/const.h"
#include "vfs/fproc.h"
#include "vfs/dmap.h"
#include <minix/dmap.h>

struct fproc fproc[NR_PROCS];
struct dmap dmap[NR_DEVICES];
```

**逐行讲解**:
- 包含VFS相关头文件
- `struct fproc fproc[NR_PROCS]`：VFS进程表副本
- `struct dmap dmap[NR_DEVICES]`：设备映射表副本

---

## fproc_dmp函数

```c
/*===========================================================================*
 *				fproc_dmp				     *
 *===========================================================================*/
void
fproc_dmp(void)
{
  struct fproc *fp;
  int i, j, nfds, n=0;
  static int prev_i;

  if (getsysinfo(VFS_PROC_NR, SI_PROC_TAB, fproc, sizeof(fproc)) != OK) {
	printf("Error obtaining table from VFS. Perhaps recompile IS?\n");
	return;
  }
```

**逐行讲解**:
- `void fproc_dmp(void)`：显示VFS进程表
  - 按Shift+F3触发
- `getsysinfo(VFS_PROC_NR, SI_PROC_TAB, ...)`：从VFS获取进程表

---

```c
  printf("File System (FS) process table dump\n");
  printf("-nr- -pid- -tty- -umask- --uid-- --gid-- -ldr-fds-sus-rev-proc-\n");
  for (i=prev_i; i<NR_PROCS; i++) {
  	fp = &fproc[i];
  	if (fp->fp_pid <= 0) continue;
  	if (++n > 22) break;
	for (j = nfds = 0; j < OPEN_MAX; j++)
		if (fp->fp_filp[j] != NULL) nfds++;
	printf("%3d  %4d  %2d/%d  0x%05x %2d (%2d) %2d (%2d) %3d %3d %3d %3d ",
		i, fp->fp_pid,
		major(fp->fp_tty), minor(fp->fp_tty),
		fp->fp_umask,
		fp->fp_realuid, fp->fp_effuid, fp->fp_realgid, fp->fp_effgid,
		!!(fp->fp_flags & FP_SESLDR), nfds,
		fp->fp_blocked_on, !!(fp->fp_flags & FP_REVIVED)
	);
	if (fp->fp_blocked_on == FP_BLOCKED_ON_CDEV)
		printf("%4d\n", fp->fp_cdev.endpt);
	/* TODO: for FP_BLOCKED_ON_SDEV we do not have the endpoint.. */
	else
		printf(" nil\n");
  }
  if (i >= NR_PROCS) i = 0;
  else printf("--more--\r");
  prev_i = i;
}
```

**逐行讲解**:
- 遍历VFS进程表
- 统计打开的文件描述符数量
- 打印每个进程的信息：
  - 索引、PID、TTY设备
  - umask、UID、GID
  - 是否为会话领导者
  - 打开的文件描述符数
  - 阻塞状态、是否被唤醒
  - 阻塞的设备端点

---

## dtab_dmp函数

```c
/*===========================================================================*
 *				dtab_dmp				     *
 *===========================================================================*/
void
dtab_dmp(void)
{
    int i;

    if (getsysinfo(VFS_PROC_NR, SI_DMAP_TAB, dmap, sizeof(dmap)) != OK) {
        printf("Error obtaining table from VFS. Perhaps recompile IS?\n");
        return;
    }

    printf("File System (FS) device <-> driver mappings\n");
    printf("    Label     Major Driver ept\n");
    printf("------------- ----- ----------\n");
    for (i=0; i<NR_DEVICES; i++) {
        if (dmap[i].dmap_driver == NONE) continue;
        printf("%13s %5d %10d\n", dmap[i].dmap_label, i, dmap[i].dmap_driver);
    }
}
```

**逐行讲解**:
- `void dtab_dmp(void)`：显示设备映射表
  - 按Shift+F4触发
- `getsysinfo(VFS_PROC_NR, SI_DMAP_TAB, ...)`：获取设备映射表
- 打印每个设备的映射信息：
  - 驱动标签
  - 主设备号
  - 驱动端点

**设备映射**:
```
主设备号 → 驱动程序端点
0 → /dev/ram (内存驱动)
1 → /dev/log (日志驱动)
...
```

---

## 要点总结

1. **VFS进程表**: 记录每个进程的文件系统状态，包括打开的文件、UID/GID等。

2. **设备映射**: 将设备号映射到驱动程序端点，实现设备独立性。

3. **阻塞状态**: 显示进程在文件系统操作中的阻塞状态。

---

## 灾难预演

**如果VFS进程表结构与IS不匹配会怎样？**

数据解析错误，显示错误信息，可能访问无效内存。

**如果设备映射表损坏会怎样？**

显示的映射信息不正确，可能显示不存在的驱动端点。
