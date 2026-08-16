# IS Server - dmp_ds.c 逐行讲解

## 文件概述

**文件路径**: `minix/servers/is/dmp_ds.c`

**核心功能**: DS（数据存储）服务的数据结构转储，显示数据存储内容。

---

## 头文件包含

```c
#include "inc.h"
#include "../ds/store.h"

#define LINES 22

static struct data_store noxfer_ds_store[NR_DS_KEYS];
```

**逐行讲解**:
- 包含DS的头文件
- `struct data_store noxfer_ds_store[NR_DS_KEYS]`：数据存储副本
  - `noxfer`前缀表示不传输指针数据（只存储结构体本身）

---

## data_store_dmp函数

```c
void
data_store_dmp(void)
{
  struct data_store *p;
  static int prev_i = 0;
  int i, n = 0;

  if (getsysinfo(DS_PROC_NR, SI_DATA_STORE, noxfer_ds_store, sizeof(noxfer_ds_store)) != OK) {
	printf("Error obtaining table from DS. Perhaps recompile IS?\n");
	return;
  }
```

**逐行讲解**:
- `void data_store_dmp(void)`：显示DS数据存储
  - 按Shift+F8触发
- `getsysinfo(DS_PROC_NR, SI_DATA_STORE, ...)`：获取数据存储

---

```c
  printf("Data store contents:\n");
  printf("-slot- -----------key----------- -----owner----- ---type--- ----value---\n");
  for(i = prev_i; i < NR_DS_KEYS && n < LINES; i++) {
	p = &noxfer_ds_store[i];
	if(!(p->flags & DSF_IN_USE))
		continue;

	printf("%6d %-25s %-15s ", i, p->key, p->owner);
	switch(p->flags & DSF_MASK_TYPE) {
	case DSF_TYPE_U32:
		printf("%-10s %12u\n", "U32", p->u.u32);
		break;
	case DSF_TYPE_STR:
		printf("%-10s %12s\n", "STR", (char*) p->u.mem.data);
		break;
	case DSF_TYPE_MEM:
		printf("%-10s %12zu\n", "MEM", p->u.mem.length);
		break;
	case DSF_TYPE_LABEL:
		printf("%-10s %12u\n", "LABEL", p->u.u32);
		break;
	default:
		return;
	}

	n++;
  }

  if (i >= NR_DS_KEYS) i = 0;
  else printf("--more--\r");
  prev_i = i;
}
```

**逐行讲解**:
- 遍历数据存储
- 跳过未使用的槽
- 根据类型打印数据：
  - U32：32位无符号整数
  - STR：字符串（注意：指针可能无效）
  - MEM：内存块（显示长度）
  - LABEL：标签（显示端点）

**DS数据存储输出示例**:
```
Data store contents:
-slot- -----------key----------- -----owner----- ---type--- ----value---
     0 system.vm_phys             rs             LABEL              5
     1 vm.addr                    vm             U32          12345678
     2 system.pm_pid              rs             U32              44
     3 system.uptime              pm             U32           123456
```

---

## 要点总结

1. **数据存储**: DS存储系统范围的键值对数据。

2. **类型区分**: 支持多种数据类型（U32、字符串、内存、标签）。

3. **所有权**: 每个数据条目有所有者，用于权限控制。

---

## 灾难预演

**如果DS数据存储中的指针无效会怎样？**

对于STR和MEM类型，指针可能指向无效地址。访问可能导致段错误。IS使用`noxfer`版本，不传输指针数据。

**如果键名过长会怎样？**

键名被截断为`DS_MAX_KEYLEN`（通常40字节）。
