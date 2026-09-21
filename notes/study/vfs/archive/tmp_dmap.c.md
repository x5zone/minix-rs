# servers/vfs/dmap.c 逐行讲解

## 文件概述

**文件路径**: `servers/vfs/dmap.c`  
**模块归属**: VFS（虚拟文件系统服务器）  
**核心功能**: 管理设备到驱动的映射表

---

## 逐行讲解

### 1. 文件头注释

```c
/* This file contains the table with device <-> driver mappings. It also
 * contains some routines to dynamically add and/ or remove device drivers
 * or change mappings.
 */
```

**第1-4行**: 文件头注释  
- **设备映射表**: 设备到驱动的映射
- **动态管理**: 动态添加、删除、修改映射

**设计原因**: 设备驱动管理

---

### 2. 包含头文件

```c
#include "fs.h"
#include <assert.h>
#include <string.h>
#include <stdlib.h>
#include <ctype.h>
#include <unistd.h>
#include <minix/callnr.h>
#include <minix/ds.h>
```

**第6-15行**: 包含头文件  
- `fs.h`: VFS 主头文件
- `assert.h`: 断言宏
- `string.h`: 字符串操作
- `stdlib.h`: 标准库
- `ctype.h`: 字符类型
- `unistd.h`: POSIX 系统调用
- `minix/callnr.h`: 系统调用号
- `minix/ds.h`: 数据存储服务

---

### 3. 设备映射表

```c
/* The order of the entries in the table determines the mapping between major
 * device numbers and device drivers. Character and block devices
 * can be intermixed at random.  The ordering determines the device numbers in
 * /dev. Note that the major device numbers used in /dev are NOT the same as
 * the process numbers of the device drivers. See <minix/dmap.h> for mappings.
 */

struct dmap dmap[NR_DEVICES];
```

**第17-25行**: 设备映射表  
- **dmap**: 设备映射表数组
- **大小**: `NR_DEVICES`
- **主设备号**: 表的索引对应主设备号
- **混合**: 字符设备和块设备可以混合

**设计原因**: 
- **映射**: 主设备号到驱动端点的映射
- **动态**: 支持动态修改

---

### 4. lock_dmap 函数

```c
/*===========================================================================*
 *				lock_dmap		 		     *
 *===========================================================================*/
void lock_dmap(struct dmap *dp)
{
/* Lock a driver */
	struct worker_thread *org_self;
	int r;

	assert(dp != NULL);
	assert(dp->dmap_driver != NONE);

	org_self = worker_suspend();

	if ((r = mutex_lock(&dp->dmap_lock)) != 0)
		panic("unable to get a lock on dmap: %d\n", r);

	worker_resume(org_self);
}
```

**第27-45行**: 锁定设备映射  
- **参数**: `dp` 设备映射指针
- **断言**: 检查指针有效性和驱动已映射
- **挂起**: 调用 `worker_suspend` 挂起当前线程
- **锁定**: 调用 `mutex_lock` 锁定互斥锁
- **恢复**: 调用 `worker_resume` 恢复线程

**设计原因**: 
- **互斥**: 防止并发访问
- **死锁避免**: 挂起线程避免死锁

---

### 5. unlock_dmap 函数

```c
/*===========================================================================*
 *				unlock_dmap		 		     *
 *===========================================================================*/
void unlock_dmap(struct dmap *dp)
{
/* Unlock a driver */
	int r;

	assert(dp != NULL);

	if ((r = mutex_unlock(&dp->dmap_lock)) != 0)
		panic("unable to unlock dmap lock: %d\n", r);
}
```

**第47-59行**: 解锁设备映射  
- **参数**: `dp` 设备映射指针
- **断言**: 检查指针有效性
- **解锁**: 调用 `mutex_unlock` 解锁互斥锁

**设计原因**: 释放锁

---

### 6. map_driver 函数

```c
/*===========================================================================*
 *				map_driver		 		     *
 *===========================================================================*/
static int map_driver(const char label[LABEL_MAX], devmajor_t major,
	endpoint_t proc_nr_e)
{
/* Add a new device driver mapping in the dmap table. If the proc_nr is set to
 * NONE, we're supposed to unmap it.
 */
  size_t len;
  struct dmap *dp;

  /* Get pointer to device entry in the dmap table. */
  if (major < 0 || major >= NR_DEVICES) return(ENODEV);
  dp = &dmap[major];

  /* Check if we're supposed to unmap it. */
 if (proc_nr_e == NONE) {
	/* Even when a driver is now unmapped and is shortly to be mapped in
	 * due to recovery, invalidate associated filps if they're character
	 * special files. More sophisticated recovery mechanisms which would
	 * reduce the need to invalidate files are possible, but would require
	 * cooperation of the driver and more recovery framework between RS,
	 * VFS, and DS.
	 */
	invalidate_filp_by_char_major(major);
	dp->dmap_driver = NONE;
	return(OK);
  }

  if (label != NULL) {
	len = strlen(label);
	if (len+1 > sizeof(dp->dmap_label)) {
		printf("VFS: map_driver: label too long: %zu\n", len);
		return(EINVAL);
	}
	strlcpy(dp->dmap_label, label, sizeof(dp->dmap_label));
  }

  /* Store driver I/O routines based on type of device */
  dp->dmap_driver = proc_nr_e;

  return(OK);
}
```

**第61-107行**: 映射设备驱动  
- **参数**: 
  - `label`: 驱动标签
  - `major`: 主设备号
  - `proc_nr_e`: 驱动端点
- **检查**: 主设备号是否有效
- **取消映射**: 如果端点是 `NONE`，取消映射
- **标签**: 如果提供标签，拷贝标签
- **映射**: 设置驱动端点

**设计原因**: 
- **动态**: 动态添加/删除驱动
- **恢复**: 支持驱动恢复

---

### 7. do_mapdriver 函数

```c
/*===========================================================================*
 *				do_mapdriver		 		     *
 *===========================================================================*/
int do_mapdriver(void)
{
/* Create a device->driver mapping. RS will tell us which major is driven by
 * this driver, what type of device it is (regular, TTY, asynchronous, clone,
 * etc), and its label. This label is registered with DS, and allows us to
 * retrieve the driver's endpoint.
 */
  const int *domains;
  int r, slot, ndomains;
  devmajor_t major;
  endpoint_t endpoint;
  vir_bytes label_vir;
  size_t label_len;
  char label[LABEL_MAX];
  struct fproc *rfp;

  /* Only RS can map drivers. */
  if (who_e != RS_PROC_NR) return(EPERM);

  label_vir = job_m_in.m_lsys_vfs_mapdriver.label;
  label_len = job_m_in.m_lsys_vfs_mapdriver.labellen;
  major = job_m_in.m_lsys_vfs_mapdriver.major;
  ndomains = job_m_in.m_lsys_vfs_mapdriver.ndomains;
  domains = job_m_in.m_lsys_vfs_mapdriver.domains;

  /* Get the label */
  if (label_len > sizeof(label)) { /* Can we store this label? */
	printf("VFS: do_mapdriver: label too long\n");
	return(EINVAL);
  }
  r = sys_vircopy(who_e, label_vir, SELF, (vir_bytes) label, label_len,
	CP_FLAG_TRY);
  if (r != OK) {
	printf("VFS: do_mapdriver: sys_vircopy failed: %d\n", r);
	return(EINVAL);
  }
  if (label[label_len-1] != '\0') {
	printf("VFS: do_mapdriver: label not null-terminated\n");
	return(EINVAL);
  }

  /* Now we know how the driver is called, fetch its endpoint */
  r = ds_retrieve_label_endpt(label, &endpoint);
  if (r != OK) {
	printf("VFS: do_mapdriver: label '%s' unknown\n", label);
```

**第109-160行**: 处理 mapdriver 系统调用  
- **权限检查**: 只有 RS 可以映射驱动
- **提取参数**: 从消息中提取标签、主设备号等
- **拷贝标签**: 从用户空间拷贝标签
- **检查标签**: 检查标签长度和终止符
- **获取端点**: 从 DS 获取驱动端点

**设计原因**: 
- **权限**: 只有 RS 可以映射驱动
- **标签**: 使用标签查找驱动端点

---

## 要点总结

### 1. 核心知识点

1. **设备映射表**: 主设备号到驱动端点的映射
2. **动态管理**: 动态添加、删除、修改映射
3. **互斥锁**: 保护设备映射表

### 2. 设计亮点

- **动态**: 支持动态驱动管理
- **恢复**: 支持驱动恢复
- **标签**: 使用标签查找驱动

### 3. 内存模型

```
设备映射表:
┌─────────────────────────────────┐
│ dmap[0]                         │
│  ├─ dmap_driver = NONE          │
│  ├─ dmap_label = ""             │
│  └─ dmap_lock                   │
├─────────────────────────────────┤
│ dmap[major]                     │
│  ├─ dmap_driver = driver_endpt  │
│  ├─ dmap_label = "driver_name"  │
│  └─ dmap_lock                   │
└─────────────────────────────────┘
```

---

## 灾难预演

### 场景 1: 无效主设备号

**后果**: 
- 返回 `ENODEV`
- 映射失败

**症状**: 驱动映射失败

### 场景 2: 标签过长

**后果**: 
- 返回 `EINVAL`
- 映射失败

**症状**: 驱动映射失败

### 场景 3: 权限不足

**后果**: 
- 返回 `EPERM`
- 映射失败

**症状**: 驱动映射失败

---

## 互动自测

### 问题 1: 设备映射表

**问**: 设备映射表的作用是什么？

**答**: 
- **映射**: 主设备号到驱动端点的映射
- **查找**: 根据主设备号找到驱动
- **管理**: 管理设备驱动

### 问题 2: 动态管理

**问**: 为什么需要动态管理？

**答**: 
- **热插拔**: 支持设备热插拔
- **恢复**: 支持驱动崩溃恢复
- **灵活**: 灵活配置系统

### 问题 3: RS

**问**: 为什么只有 RS 可以映射驱动？

**答**: 
- **安全**: 防止未授权映射
- **集中**: RS 集中管理驱动
- **协调**: RS 协调驱动启动

---

## Rust 实现对比

### C 版本（原始）

```c
static int map_driver(const char label[LABEL_MAX], devmajor_t major,
	endpoint_t proc_nr_e)
{
  size_t len;
  struct dmap *dp;

  if (major < 0 || major >= NR_DEVICES) return(ENODEV);
  dp = &dmap[major];

  if (proc_nr_e == NONE) {
	invalidate_filp_by_char_major(major);
	dp->dmap_driver = NONE;
	return(OK);
  }

  if (label != NULL) {
	len = strlen(label);
	if (len+1 > sizeof(dp->dmap_label)) {
		printf("VFS: map_driver: label too long: %zu\n", len);
		return(EINVAL);
	}
	strlcpy(dp->dmap_label, label, sizeof(dp->dmap_label));
  }

  dp->dmap_driver = proc_nr_e;

  return(OK);
}
```

### Rust 版本（安全抽象）

```rust
fn map_driver(label: Option<&str>, major: devmajor_t, proc_nr_e: endpoint_t) -> Result<(), i32> {
    if major < 0 || major >= NR_DEVICES {
        return Err(ENODEV);
    }
    let dp = &mut dmap[major as usize];

    if proc_nr_e == NONE {
        invalidate_filp_by_char_major(major);
        dp.dmap_driver = NONE;
        return Ok(());
    }

    if let Some(l) = label {
        if l.len() + 1 > dp.dmap_label.len() {
            return Err(EINVAL);
        }
        dp.dmap_label[..l.len()].copy_from_slice(l.as_bytes());
        dp.dmap_label[l.len()] = 0;
    }

    dp.dmap_driver = proc_nr_e;

    Ok(())
}
```

### 关键改进

1. **Option**: 使用 `Option` 表示可能为空的标签
2. **Result**: 使用 `Result` 返回错误
3. **安全**: 使用安全拷贝

---

## 理论关联

### 1. 设备驱动

**操作系统概念**: 设备驱动管理具体设备

**Minix3 实现**:
- 驱动是用户态进程
- 通过设备映射表查找驱动
- 动态管理驱动

### 2. 主设备号

**操作系统概念**: 主设备号标识驱动

**Minix3 实现**:
- 主设备号作为设备映射表索引
- 次设备号标识具体设备
- 设备号 = 主设备号 + 次设备号

### 3. 动态管理

**操作系统概念**: 动态管理支持热插拔

**Minix3 实现**:
- RS 管理驱动生命周期
- VFS 管理设备映射
- DS 提供标签查找

---

## 总结

`dmap.c` 实现了 Minix3 VFS 的设备映射表管理。通过动态管理、互斥锁、标签查找等设计，实现了灵活、安全的设备驱动管理。理解设备映射表的作用和管理机制是理解 VFS 设备管理的核心。
