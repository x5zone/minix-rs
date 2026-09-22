# servers/vfs/smap.c 逐行讲解

## 文件概述

**文件路径**: `servers/vfs/smap.c`  
**模块归属**: VFS（虚拟文件系统服务器）  
**核心功能**: 实现套接字驱动映射表，管理套接字域（PF_INET, PF_INET6等）与套接字驱动的映射关系

---

## 逐行讲解

### 1. 文件头注释

```c
/*
 * This file contains the table with socket driver mappings.  One socket driver
 * may implement multiple domains (e.g., PF_INET and PF_INET6).  For this
 * reason, we assign a unique number to each socket driver, and use a "socket
 * device map" table (smap) that maps from those numbers to information about
 * socket drivers.  This number is combined with a per-driver socket identifier
 * to form a globally unique socket ID (64-bit, stored as dev_t).  In addition,
 * we use a table that maps from PF_xxx domains to socket drivers (pfmap).
 */
```

**第1-9行**: 文件头注释  
- **socket driver mappings**: 套接字驱动映射
- **多个域**: 一个驱动可实现多个域（如 PF_INET 和 PF_INET6）
- **唯一编号**: 每个套接字驱动有唯一编号
- **全局唯一 socket ID**: 64位，由驱动编号和套接字ID组成
- **pfmap**: 域到驱动的映射表

**设计原因**: 
- **多域支持**: 一个驱动可处理多个协议族
- **唯一标识**: 全局唯一的套接字ID

---

### 2. 包含头文件

```c
#include "fs.h"
#include <sys/socket.h>
#include <assert.h>
```

**第11-14行**: 包含头文件  
- `fs.h`: VFS 主头文件
- `sys/socket.h`: 套接字相关定义（PF_INET 等）
- `assert.h`: 断言宏

---

### 3. 静态数据结构

```c
static struct smap smap[NR_SOCKDEVS];
static struct smap *pfmap[PF_MAX];
```

**第16-17行**: 静态数据结构  
- `smap[NR_SOCKDEVS]`: 套接字驱动映射表
- `pfmap[PF_MAX]`: 协议族到驱动的映射表

**内存布局**:
```
静态数据段:
┌─────────────────────────────────┐
│ smap[0]                         │
│  ├─ smap_num = 1                │
│  ├─ smap_endpt = NONE           │
│  └─ ...                         │
├─────────────────────────────────┤
│ smap[1]                         │
│  └─ ...                         │
└─────────────────────────────────┘

pfmap:
┌─────────────────────────────────┐
│ pfmap[0] = NULL (PF_UNSPEC)     │
│ pfmap[1] = &smap[x] (PF_LOCAL)  │
│ pfmap[2] = &smap[y] (PF_INET)   │
│ ...                             │
└─────────────────────────────────┘
```

**设计原因**: 
- **静态分配**: 避免动态内存分配
- **双重映射**: 支持通过编号或域查找驱动

---

### 4. init_smap 函数

```c
/*
 * Initialize the socket device map table.
 */
void
init_smap(void)
{
	unsigned int i;

	for (i = 0; i < __arraycount(smap); i++) {
		/*
		 * The smap numbers are one-based so as to ensure that no
		 * socket will have the device number NO_DEV, which would
		 * create problems with eg the select code.
		 */
		smap[i].smap_num = i + 1;
		smap[i].smap_endpt = NONE;
	}

	memset(pfmap, 0, sizeof(pfmap));
}
```

**第19-37行**: 初始化套接字映射表  
- **遍历**: 遍历所有 smap 条目
- **编号**: `smap_num` 从 1 开始（避免 0，因为 `NO_DEV` 可能是 0）
- **端点**: 初始化为 `NONE`
- **清空 pfmap**: 将所有指针设为 NULL

**设计原因**: 
- **避免冲突**: `smap_num` 从 1 开始，避免与 `NO_DEV` 冲突
- **初始状态**: 所有驱动未注册

---

### 5. smap_map 函数

```c
/*
 * Register a socket driver.  This action can only be requested by RS.  The
 * process identified by the given DS label 'label' and endpoint 'endpt' is to
 * be responsible for sockets created in the domains as given in the 'domains'
 * array, which contains 'ndomains' elements.  Return OK upon successful
 * registration, or an error code otherwise.
 */
int
smap_map(const char * label, endpoint_t endpt, const int * domains,
	unsigned int ndomains)
{
	struct smap *sp;
	unsigned int i, num = 0;
	int domain;

	if (ndomains <= 0 || ndomains > NR_DOMAIN)
		return EINVAL;
```

**第39-57行**: 注册套接字驱动  
- **参数**: 
  - `label`: DS 标签（如 "inet")
  - `endpt`: 驱动端点
  - `domains`: 支持的域数组（如 [PF_INET, PF_INET6]）
  - `ndomains`: 域数量
- **权限检查**: 只能由 RS（重启服务器）调用
- **参数验证**: 检查域数量是否有效

**设计原因**: RS 负责服务注册，VFS 记录映射关系

---

### 6. 查找现有条目

```c
	/*
	 * See if there is already a socket device map entry for this label.
	 * If so, the socket driver is probably being restarted, and we should
	 * overwrite its previous entry.
	 */
	sp = NULL;
	for (i = 0; i < __arraycount(smap); i++) {
		if (smap[i].smap_endpt != NONE &&
		    !strcmp(smap[i].smap_label, label)) {
			sp = &smap[i];
			break;
		}
	}
```

**第59-71行**: 查找现有条目  
- **遍历**: 遍历所有 smap 条目
- **匹配**: 查找标签匹配且已注册的条目
- **重启场景**: 如果找到，说明驱动正在重启

**设计原因**: 支持驱动热重启

---

### 7. 验证域

```c
	/*
	 * See if all given domains are valid and not already reserved by a
	 * socket driver other than (if applicable) this driver's old instance.
	 */
	for (i = 0; i < ndomains; i++) {
		domain = domains[i];
		if (domain < 0 || domain >= __arraycount(pfmap))
			return EINVAL;
		if (domain == PF_UNSPEC)
			return EINVAL;
		if (pfmap[domain] != NULL && pfmap[domain] != sp)
			return EBUSY;
	}
```

**第73-85行**: 验证域  
- **遍历**: 遍历所有请求的域
- **范围检查**: 域必须在有效范围内
- **PF_UNSPEC**: 不允许注册 PF_UNSPEC
- **冲突检查**: 域不能已被其他驱动占用

**设计原因**: 防止域冲突

---

### 8. 分配空闲条目

```c
	/*
	 * If we are not about to replace an existing socket device map entry,
	 * find a free entry, returning an error if all entries are in use.
	 */
	if (sp == NULL) {
		for (num = 0; num < __arraycount(smap); num++)
			if (smap[num].smap_endpt == NONE)
				break;

		if (num == __arraycount(smap))
			return ENOMEM;
	} else
		num = (unsigned int)(sp - smap);
```

**第87-100行**: 分配空闲条目  
- **查找空闲**: 如果不是替换，查找空闲条目
- **内存不足**: 如果所有条目都在使用，返回 `ENOMEM`
- **计算索引**: 如果是替换，计算现有条目的索引

**设计原因**: 静态分配，有限资源

---

### 9. 清理旧实例

```c
	/*
	 * At this point, the registration will succeed, and we can start
	 * modifying tables.  Just to be sure, unmap the domain mappings for
	 * the old instance, in case it is somehow registered with a different
	 * set of domains.  Also, if the endpoint of the service has changed,
	 * cancel any operations involving the previous endpoint and invalidate
	 * any preexisting sockets.  However, for stateful restarts where the
	 * service endpoint does not change, leave things as is.
	 */
	if (sp != NULL) {
		if (sp->smap_endpt != endpt) {
			/*
			 * For stateless restarts, it is common that the new
			 * endpoint is made ready before the old endpoint is
			 * exited, so we cannot wait for the exit handling code
			 * to do these steps, as they rely on the old socket
			 * mapping still being around.
			 */
			unsuspend_by_endpt(sp->smap_endpt);

			invalidate_filp_by_sock_drv(sp->smap_num);
		}

		for (i = 0; i < __arraycount(pfmap); i++)
			if (pfmap[i] == sp)
				pfmap[i] = NULL;
	}
```

**第102-127行**: 清理旧实例  
- **端点变化**: 如果端点变化，取消旧端点的操作
- **失效套接字**: 使旧驱动创建的套接字失效
- **清除映射**: 清除旧的域映射

**设计原因**: 支持有状态和无状态重启

---

### 10. 初始化新条目

```c
	/*
	 * Initialize the socket driver map entry, and set up the domain map
	 * entries.
	 */
	sp = &smap[num];
	sp->smap_endpt = endpt;
	strlcpy(sp->smap_label, label, sizeof(sp->smap_label));
	sp->smap_sel_busy = FALSE;
	sp->smap_sel_filp = NULL;

	for (i = 0; i < ndomains; i++)
		pfmap[domains[i]] = sp;

	return OK;
}
```

**第129-144行**: 初始化新条目  
- **设置端点**: 存储驱动端点
- **复制标签**: 存储驱动标签
- **初始化 select**: 初始化 select 相关字段
- **建立映射**: 为每个域建立映射

**设计原因**: 完成驱动注册

---

### 11. smap_unmap_by_endpt 函数

```c
/*
 * The process with the given endpoint has exited.  If the endpoint identifies
 * a socket driver, deregister the driver and invalidate any sockets it owned.
 */
void
smap_unmap_by_endpt(endpoint_t endpt)
{
	struct smap *sp;
	unsigned int i;

	if ((sp = get_smap_by_endpt(endpt)) == NULL)
		return;

	/*
	 * Invalidation requires that the smap entry still be around, so do
	 * this before clearing the endpoint.
	 */
	invalidate_filp_by_sock_drv(sp->smap_num);

	sp->smap_endpt = NONE;

	for (i = 0; i < __arraycount(pfmap); i++)
		if (pfmap[i] == sp)
			pfmap[i] = NULL;
}
```

**第146-170行**: 根据端点注销驱动  
- **查找驱动**: 调用 `get_smap_by_endpt` 查找驱动
- **失效套接字**: 使驱动创建的套接字失效
- **清除端点**: 设置端点为 `NONE`
- **清除映射**: 清除域映射

**设计原因**: 驱动退出时清理

---

### 12. smap_endpt_up 函数

```c
/*
 * The given endpoint has announced itself as a socket driver.
 */
void
smap_endpt_up(endpoint_t endpt)
{
	struct smap *sp;

	if ((sp = get_smap_by_endpt(endpt)) == NULL)
		return;

	/*
	 * The announcement indicates that the socket driver has either started
	 * anew or restarted statelessly.  In the second case, none of its
	 * previously existing sockets will have survived, so mark them as
	 * invalid.
	 */
	invalidate_filp_by_sock_drv(sp->smap_num);
}
```

**第172-189行**: 驱动上线通知  
- **查找驱动**: 调用 `get_smap_by_endpt` 查找驱动
- **失效套接字**: 如果是无状态重启，使旧套接字失效

**设计原因**: 支持驱动重启

---

### 13. make_smap_dev 函数

```c
/*
 * Construct a device number that combines the entry number of the given socket
 * map and the given per-driver socket identifier, thus constructing a unique
 * identifier for the socket.  Generally speaking, we use the dev_t type
 * because the value is stored as special device number (sdev) on a socket node
 * on PFS.  We use our own bit division rather than the standard major/minor
 * division because this simplifies using each half as a 32-bit value.  The
 * block/character device numbers and socket device numbers are in different
 * namespaces, and numbers may overlap (even though this is currently
 * practically impossible), so one must always test the file type first.
 */
dev_t
make_smap_dev(struct smap * sp, sockid_t sockid)
{

	assert(sp->smap_endpt != NONE);
	assert(sockid >= 0);

	return (dev_t)(((uint64_t)sp->smap_num << 32) | (uint32_t)sockid);
}
```

**第191-210行**: 构造套接字设备号  
- **参数**: `sp` 驱动映射条目，`sockid` 驱动内套接字ID
- **组合**: 高 32 位是驱动编号，低 32 位是套接字ID
- **返回**: 64 位设备号

**内存布局**:
```
dev_t (64位):
┌─────────────────────────────────┬─────────────────────────────────┐
│ 高 32 位: smap_num              │ 低 32 位: sockid                │
└─────────────────────────────────┴─────────────────────────────────┘
```

**设计原因**: 
- **全局唯一**: 驱动编号 + 套接字ID = 全局唯一
- **简化解析**: 高低位分离，易于提取

---

### 14. get_smap_by_dev 函数

```c
/*
 * Return a pointer to the smap structure for the socket driver associated with
 * the socket device number.  In addition, if the given socket ID pointer is
 * not NULL, store the per-driver socket identifier in it.  Return NULL if the
 * given socket device number is not a socket for a valid socket driver.
 */
struct smap *
get_smap_by_dev(dev_t dev, sockid_t * sockidp)
{
	struct smap *sp;
	unsigned int num;
	sockid_t id;

	num = (unsigned int)(dev >> 32);
	id = (sockid_t)(dev & ((1ULL << 32) - 1));
	if (num == 0 || num > __arraycount(smap) || id < 0)
		return NULL;

	sp = &smap[num - 1];
	assert(sp->smap_num == num);

	if (sp->smap_endpt == NONE)
		return NULL;

	if (sockidp != NULL)
		*sockidp = id;
	return sp;
}
```

**第212-237行**: 根据设备号获取驱动  
- **参数**: `dev` 设备号，`sockidp` 输出参数（可选）
- **提取**: 从设备号提取驱动编号和套接字ID
- **验证**: 检查编号和ID是否有效
- **返回**: 返回驱动映射条目和套接字ID

**设计原因**: 解析全局唯一的套接字设备号

---

### 15. get_smap_by_endpt 函数

```c
/*
 * Return a pointer to the smap structure for the socket driver with the given
 * endpoint.  Return NULL if the endpoint does not identify a socket driver.
 */
struct smap *
get_smap_by_endpt(endpoint_t endpt)
{
	unsigned int i;

	/*
	 * TODO: this function is used rather frequently, so it would be nice
```

**第239-252行**: 根据端点获取驱动  
- **参数**: `endpt` 端点号
- **遍历**: 遍历所有 smap 条目
- **匹配**: 找到端点匹配的条目

**设计原因**: 通过端点查找驱动

---

## 要点总结

### 1. 核心知识点

1. **套接字驱动映射**: 一个驱动可处理多个协议族
2. **全局唯一ID**: 64位设备号 = 驱动编号（高32位）+ 套接字ID（低32位）
3. **双重映射**: 支持通过编号或域查找驱动

### 2. 设计亮点

- **热重启支持**: 驱动可重启，VFS 自动清理旧状态
- **有状态/无状态**: 区分有状态和无状态重启
- **资源管理**: 静态分配，有限资源

### 3. 内存模型

```
静态数据段:
┌─────────────────────────────────┐
│ smap[0]                         │
│  ├─ smap_num = 1                │
│  ├─ smap_endpt = 100            │
│  ├─ smap_label = "inet"         │
│  └─ ...                         │
├─────────────────────────────────┤
│ pfmap[PF_INET] = &smap[0]       │
│ pfmap[PF_INET6] = &smap[0]      │
└─────────────────────────────────┘
```

---

## 灾难预演

### 场景 1: 域冲突

**后果**: 
- 两个驱动注册同一域
- 套接字操作发送到错误的驱动

**症状**: 套接字创建失败或行为异常

### 场景 2: 设备号溢出

**后果**: 
- 套接字ID 超过 32 位
- 设备号冲突

**症状**: 套接字ID 错误

### 场景 3: 驱动重启失败

**后果**: 
- 旧套接字未失效
- 操作发送到已终止的驱动

**症状**: 套接字操作返回错误

---

## 互动自测

### 问题 1: 设备号构成

**问**: 为什么使用 64 位设备号？

**答**: 
- 高 32 位：驱动编号（最多 2^32 个驱动）
- 低 32 位：套接字ID（每个驱动最多 2^32 个套接字）
- 全局唯一，避免冲突

### 问题 2: 热重启

**问**: VFS 如何支持驱动热重启？

**答**: 
- 检测标签匹配的现有条目
- 如果端点变化，失效旧套接字
- 更新映射关系

### 问题 3: 域映射

**问**: 为什么需要 `pfmap`？

**答**: 
- 快速查找：通过协议族找到驱动
- 避免遍历：O(1) 查找
- 支持多域：一个驱动可处理多个协议族

---

## Rust 实现对比

### C 版本（原始）

```c
dev_t
make_smap_dev(struct smap * sp, sockid_t sockid)
{
	assert(sp->smap_endpt != NONE);
	assert(sockid >= 0);

	return (dev_t)(((uint64_t)sp->smap_num << 32) | (uint32_t)sockid);
}
```

### Rust 版本（安全抽象）

```rust
struct Smap {
    smap_num: u32,
    smap_endpt: Option<i32>,
    smap_label: [u8; LABEL_MAX],
}

impl Smap {
    fn make_dev(&self, sockid: u32) -> Result<u64, i32> {
        if self.smap_endpt.is_none() {
            return Err(EINVAL);
        }

        Ok(((self.smap_num as u64) << 32) | (sockid as u64))
    }
}
```

### 关键改进

1. **Option**: 使用 `Option` 而非 `NONE` 常量
2. **Result**: 使用 `Result` 返回错误
3. **类型安全**: 使用 `u32` 和 `u64` 明确位宽

---

## 理论关联

### 1. 套接字抽象

**操作系统概念**: 套接字是网络通信的端点

**Minix3 实现**:
- 套接字驱动是用户态服务
- VFS 提供统一接口
- 通过 IPC 与驱动通信

### 2. 协议族

**操作系统概念**: 协议族定义套接字的通信域

**Minix3 实现**:
- `PF_INET`: IPv4
- `PF_INET6`: IPv6
- `PF_LOCAL`: 本地套接字

### 3. 微内核架构

**操作系统概念**: 服务在用户态运行，通过 IPC 通信

**Minix3 实现**:
- 套接字驱动是独立进程
- VFS 通过端点号与驱动通信
- RS 负责驱动生命周期管理

---

## 总结

`smap.c` 实现了 Minix3 VFS 的套接字驱动映射管理。通过双重映射表、全局唯一设备号、热重启支持等设计，实现了灵活、可靠的套接字服务管理。理解 `smap` 和 `pfmap` 是理解 VFS 套接字层的关键。
