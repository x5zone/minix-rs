# IS Server - dmp_vm.c 逐行讲解

## 文件概述

**文件路径**: `minix/servers/is/dmp_vm.c`

**核心功能**: VM（虚拟内存）服务的数据结构转储，显示内存使用和进程内存映射。

---

## 头文件包含

```c
/* Debugging dump procedures for the VM server. */

#include "inc.h"
#include <sys/mman.h>
#include <minix/vm.h>
#include <minix/timers.h>
#include "kernel/proc.h"

#define LINES 24
```

**逐行讲解**:
- 包含VM相关头文件
- `#define LINES 24`：每页显示行数

---

## print_region函数

```c
static void print_region(struct vm_region_info *vri, int *n)
{
  static int vri_count, vri_prev_set;
  static struct vm_region_info vri_prev;
  int is_repeat;

  /* part of a contiguous identical run? */
  is_repeat =
	vri &&
  	vri_prev_set &&
	vri->vri_prot == vri_prev.vri_prot &&
	vri->vri_flags == vri_prev.vri_flags &&
	vri->vri_length == vri_prev.vri_length &&
	vri->vri_addr == vri_prev.vri_addr + vri_prev.vri_length;
  if (vri) {
  	vri_prev_set = 1;
	vri_prev = *vri;
  } else {
	vri_prev_set = 0;
  }
  if (is_repeat) {
	vri_count++;
	return;
  }

  if (vri_count > 0) {
	printf("  (contiguously repeated %d more times)\n", vri_count);
	(*n)++;
	vri_count = 0;
  }

  /* NULL indicates the end of a list of mappings, nothing else to do */
  if (!vri) return;

  printf("  %08lx-%08lx %c%c%c (%lu kB)\n", vri->vri_addr,
	vri->vri_addr + vri->vri_length,
	(vri->vri_prot & PROT_READ) ? 'r' : '-',
	(vri->vri_prot & PROT_WRITE) ? 'w' : '-',
	(vri->vri_prot & PROT_EXEC) ? 'x' : '-',
	vri->vri_length / 1024L);
  (*n)++;
}
```

**逐行讲解**:
- `static void print_region(...)`：打印内存区域信息
  - 支持合并连续相同的区域
- 检查是否与上一个区域连续且属性相同
- 如果是重复，计数但不打印
- 打印区域地址范围、权限、大小

**内存区域显示**:
```
  08048000-08049000 r-x (4 kB)
  08049000-0804a000 rw- (4 kB)
  (contiguously repeated 3 more times)
```

---

## vm_dmp函数

```c
void
vm_dmp(void)
{
  static struct proc proc[NR_TASKS + NR_PROCS];
  static struct vm_region_info vri[LINES];
  struct vm_stats_info vsi;
  struct vm_usage_info vui;
  static int prev_i = -1;
  static vir_bytes prev_base = 0;
  int r, r2, i, j, first, n = 0;

  if (prev_i == -1) {
	if ((r = vm_info_stats(&vsi)) != OK) {
		printf("IS: warning: couldn't talk to VM: %d\n", r);
		return;
	}

	printf("Total %lu kB, free %lu kB, largest free %lu kB, cached %lu kB\n",
		vsi.vsi_total * (vsi.vsi_pagesize / 1024),
		vsi.vsi_free * (vsi.vsi_pagesize / 1024),
		vsi.vsi_largest * (vsi.vsi_pagesize / 1024),
		vsi.vsi_cached * (vsi.vsi_pagesize / 1024));
	n++;
	printf("\n");
	n++;

  	prev_i++;
  }
```

**逐行讲解**:
- `void vm_dmp(void)`：显示VM状态
  - 按F8触发
- `vm_info_stats(&vsi)`：获取VM统计信息
- 打印总内存、空闲内存、最大连续空闲、缓存

---

```c
  if ((r = sys_getproctab(proc)) != OK) {
	printf("IS: warning: couldn't get copy of process table: %d\n", r);
	return;
  }

  for (i = prev_i; i < NR_TASKS + NR_PROCS && n < LINES; i++, prev_base = 0) {
	if (i < NR_TASKS || isemptyp(&proc[i])) continue;

	/* The first batch dump for each process contains a header line. */
	first = prev_base == 0;

	r = vm_info_region(proc[i].p_endpoint, vri, LINES - first, &prev_base);

	if (r < 0) {
		printf("Process %d (%s): error %d\n",
			proc[i].p_endpoint, proc[i].p_name, r);
		n++;
		continue;
	}

	if (first) {
		/* The entire batch should fit on the screen. */
		if (n + 1 + r > LINES) {
			prev_base = 0;	/* restart on next page */
			break;
		}

		if ((r2 = vm_info_usage(proc[i].p_endpoint, &vui)) != OK) {
			printf("Process %d (%s): error %d\n",
				proc[i].p_endpoint, proc[i].p_name, r2);
			n++;
			continue;
		}

		printf("Process %d (%s): total %lu kB, common %lu kB, "
			"shared %lu kB\n",
			proc[i].p_endpoint, proc[i].p_name,
			vui.vui_total / 1024L, vui.vui_common / 1024L,
			vui.vui_shared / 1024L);
		n++;
	}
```

**逐行讲解**:
- 遍历进程表
- 对每个进程获取内存区域信息
- `vm_info_region`：获取进程的内存映射
- `vm_info_usage`：获取进程的内存使用统计
- 打印进程内存使用情况

---

```c
	while (r > 0) {
		for (j = 0; j < r; j++) {
			print_region(&vri[j], &n);
		}

		if (LINES - n - 1 <= 0) break;
		r = vm_info_region(proc[i].p_endpoint, vri, LINES - n - 1,
			&prev_base);

		if (r < 0) {
			printf("Process %d (%s): error %d\n",
				proc[i].p_endpoint, proc[i].p_name, r);
			n++;
		}
	}
	print_region(NULL, &n);

	if (n > LINES) printf("IS: internal error\n");
	if (n == LINES) break;

	/* This may have to wipe out the "--more--" from below. */
	printf("        \n");
	n++;
  }

  if (i >= NR_TASKS + NR_PROCS) {
	i = -1;
	prev_base = 0;
  }
  else printf("--more--\r");
  prev_i = i;
}
```

**逐行讲解**:
- 循环获取并打印内存区域
- 支持分页显示
- 处理错误情况

**VM转储输出示例**:
```
Total 524288 kB, free 262144 kB, largest free 131072 kB, cached 32768 kB

Process 0 (kernel): total 4096 kB, common 0 kB, shared 0 kB
  c0000000-c0010000 r-x (64 kB)
  c0010000-c0020000 rw- (64 kB)
  
Process 1 (pm): total 2048 kB, common 512 kB, shared 256 kB
  08048000-08049000 r-x (4 kB)
  08049000-0804a000 rw- (4 kB)
```

---

## 要点总结

1. **内存统计**: 显示系统总内存、空闲内存、缓存等信息。

2. **进程内存**: 显示每个进程的内存使用和映射区域。

3. **区域合并**: 合并连续相同的内存区域，减少输出。

---

## 灾难预演

**如果VM服务不可用会怎样？**

`vm_info_stats`返回错误，无法显示内存信息。IS打印警告并返回。

**如果进程内存映射过多会怎样？**

分页显示，每页24行。用户需要多次按键查看完整信息。
