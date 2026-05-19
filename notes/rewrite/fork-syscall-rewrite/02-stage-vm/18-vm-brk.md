# 18-vm-brk: VM_BRK 服务

> **分类**: VM服务  
> **源码**: `minix3/minix/servers/vm/break.c`  
> **说明**: VM 对外提供的堆调整服务，用于扩展或收缩进程堆空间

---

## 1. 概述

### 1.1 VM_BRK 服务的作用

**brk 的核心功能**

| 功能 | 说明 |
|------|------|
| **设置堆顶** | 将堆顶设置为指定地址 |
| **扩展堆** | 增加堆空间，分配新内存页 |
| **收缩堆** | 减少堆空间，释放内存页 |
| **查询堆顶** | 参数为 0 时返回当前堆顶 |

**进程地址空间布局**（低地址→高地址）：text → data → bss → heap（↑向上增长）→ gap → stack（↓向下增长）。堆和栈从 gap 两端相向增长，若相遇则进程被杀死（ENOMEM）。

### 1.2 堆的增长方向

- **扩展堆**（`brk(new_addr)` 其中 `new_addr > 当前堆顶`）：堆向高地址扩展，分配新的虚拟区域和物理页
- **收缩堆**（`brk(new_addr)` 其中 `new_addr < 当前堆顶`）：堆向低地址收缩，释放虚拟区域和物理页
- **危险情况**：堆顶超过栈底时，进程被杀死（ENOMEM）

### 1.3 与 Minix3 的对应关系

| 功能 | Minix3 源文件 | 函数 |
|------|--------------|------|
| brk 入口 | `break.c:44` | `do_brk()` |
| 实际调整 | `break.c:62` | `real_brk()` |
| 区域扩展 | `region.c:1002` | `map_region_extend_upto_v()` |
| 区域查找 | `region.c` (via `regionavl`) | `region_search()` |
| 内存类型调整 | `memtype.h:21` | `ev_resize` 回调 |

> **注意**: Minix3 的 vmproc 结构体中**没有** `vm_brk`、`vm_data_top`、`vm_stack_low` 等堆专用字段。堆顶地址隐含在数据段 vir_region 的 `vaddr + length` 中，由 `map_region_extend_upto_v` 通过 AVL 树查找区域并扩展来管理。vmproc 中与内存统计相关的字段是 `vm_total` 和 `vm_total_max`（见 [vmproc.h](../../../../../minix3/minix/servers/vm/vmproc.h)）。

### 1.4 brk 与 sbrk 的关系

| 接口 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| `brk(addr)` | 绝对地址 | 成功返回 0，失败返回 -1 | 设置堆顶到指定地址 |
| `sbrk(incr)` | 增量（可正可负） | 成功返回旧堆顶，失败返回 -1 | 增量调整堆大小 |

sbrk 基于 brk 实现：`old_brk = _brksize; if (brk(old_brk + incr) == 0) return old_brk; else return -1;`

---

## 2. C 源码分析

> 本章分析 Minix3 中 brk 相关的 C 源码实现。注意 Minix3 运行在 x86-32 位架构上，`vir_bytes` 为 `u32`，地址空间为 32 位；minix-rs 目标为 x86-64，地址空间为 64 位，相关差异在各节中标注。

### 2.1 IPC 接口与调用者

VM_BRK 消息由用户进程直接发送给 VM，不需要经过 PM。这是与 VM_FORK、VM_EXEC_NEWMEM 等需要 PM 协调的服务不同之处。

**调用路径**: 用户进程 → `brk(addr)`/`sbrk(incr)` → libc 封装（设置 `m.m_lc_vm_brk.addr`，调用 `_syscall(VM_PROC_NR, VM_BRK, &m)`） → VM `do_brk()` → `real_brk()` → 返回结果给用户进程

**Minix3 libc 实现**

```c
/* minix3/minix/lib/libc/sys/brk.c */

extern char *_brksize;  // 当前堆顶（全局变量）

int brk(void *addr)
{
    message m;

    // 只有请求的地址与当前堆顶不同时才调用 VM
    if (addr != _brksize) {
        memset(&m, 0, sizeof(m));
        m.m_lc_vm_brk.addr = addr;
        if (_syscall(VM_PROC_NR, VM_BRK, &m) < 0)
            return -1;
        _brksize = addr;  // 更新本地记录
    }
    return 0;
}

/* minix3/minix/lib/libc/sys/sbrk.c */

void *sbrk(intptr_t incr)
{
    char *newsize, *oldsize;

    oldsize = _brksize;
    newsize = _brksize + incr;

    // 溢出检查
    if ((incr > 0 && newsize < oldsize) ||
        (incr < 0 && newsize > oldsize))
        return (void *)-1;

    // 调用 brk
    if (brk(newsize) == 0)
        return oldsize;  // 返回旧堆顶
    else
        return (void *)-1;
}
```

**调用者类型**

| 调用者 | 说明 |
|--------|------|
| **用户进程** | 通过 brk()/sbrk() 系统调用直接请求 VM |
| **RS (Reincarnation Server)** | 启动时可能调用 VM_BRK |
| **malloc 实现** | 用户态内存分配器使用 sbrk 获取内存 |

**与 PM 的关系**

brk 不需要 PM 参与，原因：
1. brk 只修改调用者自身的地址空间
2. 不涉及进程创建或销毁
3. 不需要 PM 维护的进程状态同步

### 2.2 消息类型

VM_BRK 是 VM 服务处理堆调整请求的消息类型。

**消息定义**

```c
/* minix3/minix/include/minix/com.h:627,636 */
#define VM_RQ_BASE      0xC00
#define VM_BRK          (VM_RQ_BASE+2)   // brk 请求消息类型
```

**消息结构**

```c
/* minix3/minix/include/minix/ipc.h:922-925 */
typedef struct {
	void		*addr;
	uint8_t		padding[52];
} mess_lc_vm_brk;

/* 消息联合体中的位置 */
union message {
    // ...
    mess_lc_vm_brk m_lc_vm_brk;
    // ...
};
```

**消息类型编号**

| 编号 | 宏定义 | 说明 |
|------|--------|------|
| 0xC00 | VM_EXIT | 进程退出 |
| 0xC01 | VM_FORK | fork 请求 |
| 0xC02 | VM_BRK | 堆调整请求 ◄─── 本文档 |
| 0xC03 | VM_EXEC_NEWMEM | exec 内存设置 |
| 0xC05 | VM_WILLEXIT | 即将退出通知 |
| 0xC0A | VM_MMAP | 内存映射 |
| 0xC11 | VM_MUNMAP | 取消映射 |

**消息处理流程**

```c
/* minix3/minix/servers/vm/main.c:545 */
CALLMAP(VM_BRK, do_brk),    // VM_BRK 消息由 do_brk 处理
```

### 2.3 请求参数

> **32位 vs 64位差异**: Minix3 中 `vir_bytes` 为 `u32`（32位地址空间），`mess_lc_vm_brk.addr` 为 `void *`（也是32位）。minix-rs 中地址为 `u64`（64位地址空间），IPC 消息中的地址字段需要相应扩展。

#### 2.3.1 新堆顶地址

`m_lc_vm_brk.addr` 字段指定请求的新堆顶地址。

**参数含义**

| addr 值 | 含义 |
|---------|------|
| `NULL (0)` | 查询当前堆顶，不修改 |
| `> 当前堆顶` | 扩展堆，增加内存 |
| `< 当前堆顶` | 收缩堆，释放内存 |
| `= 当前堆顶` | 无操作，直接返回成功 |

**地址验证**

1. 地址必须 >= 数据段顶部（data_top）：不能低于进程的代码/数据段
2. 地址必须 < 栈底（stack_low）：不能与栈区域重叠
3. 地址必须符合页对齐（内部处理）：VM 会向上取整到页边界

错误情况：
- `addr < data_top` → EINVAL（低于数据段）
- `addr >= stack_low` → ENOMEM（与栈冲突）
- 超出虚拟内存限制 → ENOMEM

**Minix3 源码中的处理**

do_brk 的完整实现见第3章 3.1 节。其核心逻辑是：先通过 `vm_isokendpt` 验证调用者 endpoint，再调用 `real_brk` 执行实际调整。

### 2.4 返回结果

#### 2.4.1 实际堆顶地址

brk 系统调用的返回值表示操作结果：

**返回值含义**

| 返回值 | 含义 |
|--------|------|
| `0` | 成功，堆顶已设置到请求地址 |
| `-1` | 失败，errno 设置为错误码 |

**libc 层的处理**

```c
/* minix3/minix/lib/libc/sys/brk.c */
int brk(void *addr)
{
    message m;

    if (addr != _brksize) {
        memset(&m, 0, sizeof(m));
        m.m_lc_vm_brk.addr = addr;
        if (_syscall(VM_PROC_NR, VM_BRK, &m) < 0)
            return -1;  // 失败
        _brksize = addr;  // 更新本地记录
    }
    return 0;  // 成功
}
```

**实际堆顶可能不同的原因**

1. **页对齐**：请求地址 0x401234（未对齐），实际堆顶 0x402000（向上取整到页边界）。VM 内部：`offset = roundup(offset, VM_PAGE_SIZE)`
2. **内存限制**：请求地址 0x800000（需要分配新页），分配失败，实际堆顶保持原值，返回 -1，errno = ENOMEM
3. **与栈冲突**：请求地址 0x7FFF0000，栈底 0x7FFF1000，与栈冲突，实际堆顶保持原值，返回 -1，errno = ENOMEM

#### 2.4.2 错误码

| 错误码 | 说明 | 触发条件 |
|--------|------|---------|
| `ENOMEM` | 内存不足 | 无法分配新页面、超出虚拟内存限制、与栈冲突 |
| `EINVAL` | 参数无效 | endpoint 验证失败 |

**错误处理示例**

```c
/* 用户态代码 */
if (brk(new_addr) != 0) {
    switch (errno) {
    case ENOMEM:
        // 内存不足或地址冲突
        fprintf(stderr, "brk: out of memory\n");
        break;
    case EINVAL:
        // 不应该发生在正常调用中
        fprintf(stderr, "brk: invalid argument\n");
        break;
    }
}
```

---

### 2.5 do_brk - 主处理函数

do_brk 是 VM_BRK 消息的处理入口，负责验证调用者并调用实际调整函数。

**Minix3 源码实现**

```c
/* minix3/minix/servers/vm/break.c:44-57 */
int do_brk(message *msg)
{
/* Perform the brk(addr) system call.
 * The parameter, 'addr' is the new virtual address in D space.
 */
	int proc;

	// 1. 验证调用者 endpoint
	if (vm_isokendpt(msg->m_source, &proc) != OK) {
		printf("VM: bogus endpoint VM_BRK %d\n", msg->m_source);
		return EINVAL;
	}

	// 2. 调用 real_brk 执行实际调整
	return real_brk(&vmproc[proc], (vir_bytes) msg->m_lc_vm_brk.addr);
}
```

**处理流程**

1. `vm_isokendpt(msg->m_source, &proc)` — 验证调用者 endpoint（检查有效性、进程是否在使用中、endpoint 与 slot 是否匹配），失败返回 EINVAL
2. `real_brk(&vmproc[proc], (vir_bytes) msg->m_lc_vm_brk.addr)` — 委托给 real_brk 执行实际调整

> **32位 vs 64位差异**: Minix3 中 `(vir_bytes) msg->m_lc_vm_brk.addr` 将 `void *` 转为 `u32`。在 64 位 minix-rs 中，地址为 `u64`，消息结构中的 `addr` 字段需要相应扩展为 64 位。

**关键点说明**

| 步骤 | 说明 |
|------|------|
| **endpoint 验证** | 确保调用者是有效的进程 |
| **获取进程结构** | 通过 slot 索引 vmproc 数组 |
| **委托处理** | 实际调整由 real_brk 完成 |

### 2.6 real_brk - 实际调整

real_brk 执行实际的堆调整操作，调用区域扩展函数。

**Minix3 源码实现**

```c
/* minix3/minix/servers/vm/break.c:62-69 */
int real_brk(struct vmproc *vmp, vir_bytes v)
{
	// 调用区域扩展函数，将堆区域扩展到地址 v
	if(map_region_extend_upto_v(vmp, v) == OK) {
		return OK;
	}

	return(ENOMEM);
}
```

**处理逻辑**

调用 `map_region_extend_upto_v(vmp, v)` 将堆区域扩展到地址 v。成功返回 OK，失败返回 ENOMEM。

**map_region_extend_upto_v 核心逻辑**

```c
/* minix3/minix/servers/vm/region.c:1002-1060 */
int map_region_extend_upto_v(struct vmproc *vmp, vir_bytes v)
{
	vir_bytes offset = v, limit, extralen;
	struct vir_region *vr, *nextvr;
	struct phys_region **newpr;
	int newslots, prevslots, addedslots, r;

	/* 1. 页对齐 */
	offset = roundup(offset, VM_PAGE_SIZE);

	/* 2. 查找可扩展的区域（起始地址 < offset 的最大区域） */
	if(!(vr = region_search(&vmp->vm_regions_avl, offset, AVL_LESS))) {
		printf("VM: nothing to extend\n");
		return ENOMEM;
	}

	/* 3. 如果新地址已在区域内，无需操作 */
	if(vr->vaddr + vr->length >= v) return OK;

	/* 4. 计算需要扩展的大小 */
	assert(vr->vaddr <= offset);
	newslots = phys_slot(offset - vr->vaddr);
	prevslots = phys_slot(vr->length);
	assert(newslots >= prevslots);
	addedslots = newslots - prevslots;
	limit = vr->vaddr + vr->length;
	extralen = offset - limit;
	assert(extralen > 0);

	/* 5. 检查是否会与下一个区域冲突 */
	if((nextvr = getnextvr(vr))) {
		assert(offset <= nextvr->vaddr);
	}
	if(nextvr && nextvr->vaddr < offset) {
		printf("VM: can't grow into next region\n");
		return ENOMEM;
	}

	/* 6a. 内存类型没有 ev_resize 回调：直接映射匿名内存 */
	if(!vr->def_memtype->ev_resize) {
		if(!map_page_region(vmp, limit, 0, extralen,
			VR_WRITABLE | VR_ANON,
			0, &mem_type_anon)) {
			printf("resize: couldn't put anon memory there\n");
			return ENOMEM;
		}
		return OK;
	}

	/* 6b. 内存类型有 ev_resize 回调：扩展 physblocks 数组并调用回调 */
	if(!(newpr = realloc(vr->physblocks,
		newslots * sizeof(struct phys_region *)))) {
		printf("VM: map_region_extend_upto_v: realloc failed\n");
		return ENOMEM;
	}

	vr->physblocks = newpr;
	memset(vr->physblocks + prevslots, 0,
		addedslots * sizeof(struct phys_region *));

	r = vr->def_memtype->ev_resize(vmp, vr, offset - vr->vaddr);

	return r;
}
```

### 2.7 堆区域管理

#### 2.7.1 查找堆区域

堆区域通过 AVL 树搜索来定位，使用 AVL_LESS 搜索类型。

**AVL 搜索类型**

```c
/* minix3/minix/servers/vm/cavl_if.h */
typedef enum {
    AVL_EQUAL = 1,           // 精确匹配
    AVL_LESS = 2,            // 小于
    AVL_GREATER = 4,         // 大于
    AVL_LESS_EQUAL = 3,      // 小于或等于
    AVL_GREATER_EQUAL = 5    // 大于或等于
} avl_search_type;
```

**堆区域查找逻辑**

`region_search(&vmp->vm_regions_avl, offset, AVL_LESS)` 查找起始地址 < offset 的最大区域。

示例：若地址空间有 Region A (text, 0x0000-0x1000)、Region B (data, 0x1000-0x2000)、Region C (heap, 0x2000-0x3000)、Region D (stack, 0x7000-0x8000)，当 offset = 0x4000 时，AVL_LESS 搜索返回 Region C (heap)，因为其 vaddr (0x2000) < 0x4000 且是满足条件的最大区域。

**Minix3 源码**

```c
/* minix3/minix/servers/vm/region.c:1002-1060 */
int map_region_extend_upto_v(struct vmproc *vmp, vir_bytes v)
{
    vir_bytes offset = v;
    struct vir_region *vr;

    offset = roundup(offset, VM_PAGE_SIZE);

    // 查找地址小于 offset 的最大区域
    if(!(vr = region_search(&vmp->vm_regions_avl, offset, AVL_LESS))) {
        printf("VM: nothing to extend\n");
        return ENOMEM;
    }

    // vr 现在指向堆区域（或数据段区域）
    // ...
}
```

**为什么堆区域总是可找到**

1. 进程启动时，数据段区域已存在
2. 堆区域紧随数据段之后
3. 堆区域和数据段可能是同一个区域（取决于内存布局）

#### 2.7.2 扩展堆

扩展堆需要分配新的物理页面并更新区域元数据。

**扩展流程**

1. 计算需要的额外槽位：`newslots = phys_slot(offset - vr->vaddr)`, `prevslots = phys_slot(vr->length)`, `addedslots = newslots - prevslots`, `extralen = offset - limit`
2. 检查内存类型：若无 `ev_resize` 回调，直接 `map_page_region(..., VR_WRITABLE | VR_ANON)` 映射匿名内存；若有回调，继续步骤 3-4
3. 扩展 physblocks 数组：`realloc(vr->physblocks, newslots * sizeof(...))`, `memset(newpr + prevslots, 0, ...)`
4. 调用内存类型回调：`vr->def_memtype->ev_resize(vmp, vr, offset - vr->vaddr)`

**Minix3 源码**

```c
/* minix3/minix/servers/vm/region.c */
int map_region_extend_upto_v(struct vmproc *vmp, vir_bytes v)
{
    // ... 前面的查找代码 ...

    limit = vr->vaddr + vr->length;
    extralen = offset - limit;

    // 情况 1: 内存类型没有 resize 回调
    if(!vr->def_memtype->ev_resize) {
        // 直接映射匿名内存
        if(!map_page_region(vmp, limit, 0, extralen,
            VR_WRITABLE | VR_ANON,
            0, &mem_type_anon)) {
            printf("resize: couldn't put anon memory there\n");
            return ENOMEM;
        }
        return OK;
    }

    // 情况 2: 扩展 physblocks 数组
    if(!(newpr = realloc(vr->physblocks,
        newslots * sizeof(struct phys_region *)))) {
        printf("VM: map_region_extend_upto_v: realloc failed\n");
        return ENOMEM;
    }

    vr->physblocks = newpr;
    memset(vr->physblocks + prevslots, 0,
        addedslots * sizeof(struct phys_region *));

    // 调用内存类型的 resize 回调
    r = vr->def_memtype->ev_resize(vmp, vr, offset - vr->vaddr);

    return r;
}
```

**物理页面分配**

扩展时，新页面按需分配：
1. 首次访问触发缺页异常
2. 缺页处理程序分配实际物理页
3. 支持延迟分配（lazy allocation）

> **方案四标注**：brk 的物理页分配通过缺页处理程序间接完成，调用方不受 direct map 影响。缺页处理程序内部已简化为 `alloc_phys() → vm_phys_to_virt()`（见 16-pagefault.md），brk 代码无需修改。这是 direct map 统一性的体现——物理页分配的简化在底层完成，上层调用者透明受益。

#### 2.7.3 收缩堆

收缩堆需要释放物理页面并更新区域元数据。

**收缩流程**

1. 计算要释放的区域：从新堆顶到当前堆顶的区域需要释放，`shrink_len = current_brk - new_brk`
2. 取消物理页面映射：`map_subfree(r, offset, len)` — 减少物理块引用计数，归零时释放物理页
3. 更新区域长度：`r->length -= shrink_len`（从尾部收缩的典型情况）
4. 更新页表：`pt_writemap(vmp, &vmp->vm_pt, regionstart, MAP_NONE, len, 0, WMF_OVERWRITE)` 取消映射

**Minix3 源码**

```c
/* minix3/minix/servers/vm/region.c:1065-1148 */
int map_unmap_region(struct vmproc *vmp, struct vir_region *r,
	vir_bytes offset, vir_bytes len)
{
	vir_bytes regionstart;
	int freeslots = phys_slot(len);

	if(offset+len > r->length || (len % VM_PAGE_SIZE)) {
		printf("VM: bogus length 0x%lx\n", len);
		return EINVAL;
	}

	regionstart = r->vaddr + offset;

	/* 取消物理页面的引用 */
	map_subfree(r, offset, len);

	/* 根据释放位置更新区域 */
	if(r->length == len) {
		/* 整个区域消失 */
		region_remove(&vmp->vm_regions_avl, r->vaddr);
		map_free(r);
	} else if(offset == 0) {
		/* 从头部收缩（需要 ev_lowshrink 回调支持） */
		if(!r->def_memtype->ev_lowshrink) {
			printf("VM: low-shrinking not implemented for %s\n",
				r->def_memtype->name);
			return EINVAL;
		}
		/* ... 调整 vaddr、physblocks、length ... */
	} else if(offset + len == r->length) {
		/* 从尾部收缩（brk 收缩的典型情况） */
		r->length -= len;
	}

	/* 更新页表：取消映射 */
	if(pt_writemap(vmp, &vmp->vm_pt, regionstart,
	  MAP_NONE, len, 0, WMF_OVERWRITE) != OK) {
	    printf("VM: map_unmap_region: pt_writemap failed\n");
	    return ENOMEM;
	}

	return OK;
}
```

> **注意**: brk 收缩走的是 `offset + len == r->length` 分支（从尾部收缩），这是最简单的情况，只需减少 `r->length`。从头部收缩（`offset == 0`）需要 `ev_lowshrink` 回调支持，且涉及 `vaddr` 调整和 `physblocks` 移位，更为复杂。
>
> **32位 vs 64位差异**: Minix3 中 `vir_bytes` 为 `u32`，`phys_slot()` 宏将字节数转换为页面槽位数。在 64 位 minix-rs 中，`vir_bytes` 为 `u64`，需要确保 `phys_slot` 的计算不会溢出，且 `realloc` 的 `newslots * sizeof(...)` 不会在 32 位 `size_t` 下溢出。

**物理页面释放**

```c
/* minix3/minix/servers/vm/region.c:527-563 */
static int map_subfree(struct vir_region *region,
	vir_bytes start, vir_bytes len)
{
	struct phys_region *pr;
	vir_bytes end = start+len;
	vir_bytes voffset;

	for(voffset = start; voffset < end; voffset+=VM_PAGE_SIZE) {
		if(!(pr = physblock_get(region, voffset)))
			continue;
		assert(pr->offset >= start);
		assert(pr->offset < end);
		pb_unreferenced(region, pr, 1);
		SLABFREE(pr);
	}

	return OK;
}
```

> **注意**: `map_subfree` 是 `region.c` 中的 `static` 函数，不是 `memory.c` 中的。它通过 `pb_unreferenced` 减少物理块引用计数（而非直接操作 `refcount` 字段），引用计数归零时由 `pb_unreferenced` 内部释放物理页。
>
> **32位 vs 64位差异**: Minix3 中 `vir_bytes` 为 `u32`，物理地址也为 32 位。在 64 位 minix-rs 中，物理地址和虚拟地址均为 64 位，`pb_unreferenced` 的实现需要正确处理 64 位地址。

**收缩 vs 扩展**

| 操作 | 物理页面 | 区域元数据 |
|------|---------|-----------|
| **扩展** | 延迟分配（缺页时） | 增加 length |
| **收缩** | 立即释放 | 减少 length |

### 2.8 限制检查

#### 2.8.1 vm_total_max

vm_total_max 记录进程使用过的最大虚拟内存量，用于资源统计。

**数据结构**

```c
/* minix3/minix/servers/vm/vmproc.h:14-32 */
struct vmproc {
	int		vm_flags;
	endpoint_t	vm_endpoint;
	pt_t		vm_pt;
	struct boot_image *vm_boot;
	region_avl vm_regions_avl;
	vir_bytes  vm_region_top;
	int vm_acl;
	int vm_slot;
#if VMSTATS
	int vm_bytecopies;
#endif
	vir_bytes	vm_total;       /* 当前虚拟内存使用量 */
	vir_bytes	vm_total_max;   /* 历史最大虚拟内存使用量 */
	u64_t		vm_minor_page_fault;
	u64_t		vm_major_page_fault;
};
```

**统计更新**

`vm_total` 在物理页面分配/释放时更新（通过 `pb_unreferenced` 等路径），`vm_total_max` 在 `vm_total` 增加时同步更新。具体更新点分散在 `pb.c`、`region.c` 等文件中，而非在 brk 代码路径中直接操作。

**用途**

| 用途 | 说明 |
|------|------|
| **资源统计** | getrusage() 返回 ru_maxrss |
| **性能分析** | 了解进程内存使用峰值 |
| **监控** | 通过 VM_INFO 查询 |

**与 brk 的关系**

- brk 扩展堆：扩展虚拟区域 → 物理页面延迟分配 → 缺页时更新 vm_total
- brk 收缩堆：释放物理页面 → 更新 vm_total → vm_total_max 保持不变（历史峰值）

示例：T0 初始(vm_total=0, max=0) → T1 brk+4K(4K, 4K) → T2 brk+8K(12K, 12K) → T3 brk-4K(8K, 12K不变) → T4 brk+16K(24K, 24K)

**注意**: Minix3 的 brk 实现不直接检查 vm_total 限制，而是依赖物理内存分配时的限制检查。

#### 2.8.2 与栈碰撞检查

堆向上增长，栈向下增长，需要防止两者重叠。

**碰撞检测机制**

堆向上增长，栈向下增长，碰撞条件为新堆顶 >= 栈区域起始地址。Minix3 通过 `getnextvr(vr)` 获取堆区域的下一个区域（通常是栈），检查 `nextvr->vaddr < offset` 来判断是否冲突。

**Minix3 检测方式**

```c
/* minix3/minix/servers/vm/region.c */
int map_region_extend_upto_v(struct vmproc *vmp, vir_bytes v)
{
    // ...
    struct vir_region *vr, *nextvr;

    // 查找堆区域
    vr = region_search(&vmp->vm_regions_avl, offset, AVL_LESS);

    // 获取下一个区域（可能是栈）
    if((nextvr = getnextvr(vr))) {
        assert(offset <= nextvr->vaddr);
    }

    // 检查是否会与下一个区域冲突
    if(nextvr && nextvr->vaddr < offset) {
        printf("VM: can't grow into next region\n");
        return ENOMEM;
    }
    // ...
}
```

**getnextvr 函数**

```c
/* minix3/minix/servers/vm/region.c:112-128 */
static struct vir_region *getnextvr(struct vir_region *vr)
{
	struct vir_region *nextvr;
	region_iter v_iter;
	SLABSANE(vr);
	/* 先精确匹配当前区域，再递增迭代器获取下一个 */
	region_start_iter(&vr->parent->vm_regions_avl, &v_iter,
		vr->vaddr, AVL_EQUAL);
	assert(region_get_iter(&v_iter));
	assert(region_get_iter(&v_iter) == vr);
	region_incr_iter(&v_iter);
	nextvr = region_get_iter(&v_iter);
	if(!nextvr) return NULL;
	SLABSANE(nextvr);
	assert(vr->parent == nextvr->parent);
	assert(vr->vaddr < nextvr->vaddr);
	assert(vr->vaddr + vr->length <= nextvr->vaddr);
	return nextvr;
}
```

**区域不变量**

```c
// 每个区域之间必须有空隙
assert(vr->vaddr + vr->length <= nextvr->vaddr);
```

**碰撞处理**

| 情况 | 处理 |
|------|------|
| 堆扩展到栈区域 | 返回 ENOMEM |
| 栈扩展到堆区域 | 栈扩展失败 |
| 正常扩展 | 允许扩展 |

**安全间隙**

Minix3 不强制要求堆和栈之间有最小间隙，但区域边界检查确保不会重叠。

---

## 3. Rust 设计决策

### 3.1 BrkRequest/BrkResponse

使用类型安全的 IPC 消息结构，与 VM_FORK 等其他服务保持一致。

**请求结构**

```rust
/* os/vm/src/ipc/brk.rs */

/// brk 系统调用请求
#[derive(Debug, Clone, Copy)]
pub struct BrkRequest {
    /// 新堆顶地址
    /// - 0: 查询当前堆顶
    /// - 其他: 设置新堆顶
    pub addr: VAddr,
}

impl BrkRequest {
    /// 从 IPC 消息解析
    pub fn from_message(msg: &Message) -> Self {
        Self {
            addr: VAddr::new(msg.m_lc_vm_brk.addr as usize),
        }
    }

    /// 是否为查询操作
    pub fn is_query(&self) -> bool {
        self.addr.is_null()
    }
}
```

**响应结构**

```rust
/// brk 系统调用响应
#[derive(Debug, Clone, Copy)]
pub struct BrkResponse {
    /// 操作结果
    pub result: Result<VAddr, BrkError>,
}

impl BrkResponse {
    /// 成功响应
    pub fn success(new_brk: VAddr) -> Self {
        Self {
            result: Ok(new_brk),
        }
    }

    /// 错误响应
    pub fn error(err: BrkError) -> Self {
        Self {
            result: Err(err),
        }
    }

    /// 转换为 IPC 返回值
    pub fn to_return_value(&self) -> i32 {
        match &self.result {
            Ok(_) => 0,        // brk 成功返回 0
            Err(e) => e.to_errno(),
        }
    }
}
```

**错误类型**

```rust
/// brk 错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrkError {
    /// 内存不足
    OutOfMemory,
    /// 无效的 endpoint
    InvalidEndpoint,
    /// 与其他区域冲突
    RegionConflict,
    /// 地址无效
    InvalidAddress,
}

impl BrkError {
    pub fn to_errno(&self) -> i32 {
        match self {
            BrkError::OutOfMemory => -(errno::ENOMEM as i32),
            BrkError::InvalidEndpoint => -(errno::EINVAL as i32),
            BrkError::RegionConflict => -(errno::ENOMEM as i32),
            BrkError::InvalidAddress => -(errno::EINVAL as i32),
        }
    }
}
```

**消息类型定义**

```rust
/* os/vm/src/ipc/mod.rs */

/// VM 服务消息类型
#[derive(Debug, Clone, Copy)]
pub enum VmRequest {
    Exit(ExitRequest),
    Fork(ForkRequest),
    Brk(BrkRequest),      // brk 请求
    ExecNewmem(ExecNewmemRequest),
    // ...
}

impl VmRequest {
    /// 从原始消息解析
    pub fn from_message(msg: &Message) -> Option<Self> {
        match msg.m_type {
            VM_BRK => Some(VmRequest::Brk(BrkRequest::from_message(msg))),
            // ...
            _ => None,
        }
    }
}
```

### 3.2 堆区域管理

堆区域作为进程地址空间的一部分，通过 AVL 树进行管理。

**进程堆状态**

```rust
/* os/vm/src/process/heap.rs */

/// 进程堆状态
#[derive(Debug)]
pub struct HeapState {
    /// 堆起始地址（数据段顶部）
    pub start: VAddr,
    /// 当前堆顶地址
    pub current_brk: VAddr,
    /// 历史最大堆大小
    pub max_brk: VAddr,
}

impl HeapState {
    /// 创建新的堆状态
    pub fn new(data_top: VAddr) -> Self {
        Self {
            start: data_top,
            current_brk: data_top,
            max_brk: data_top,
        }
    }

    /// 当前堆大小
    pub fn size(&self) -> usize {
        self.current_brk.value() - self.start.value()
    }

    /// 计算新堆顶
    pub fn calculate_new_brk(&self, requested: VAddr) -> HeapAdjustment {
        if requested.value() < self.start.value() {
            // 低于数据段，无效
            HeapAdjustment::Invalid
        } else if requested.value() < self.current_brk.value() {
            // 收缩堆
            HeapAdjustment::Shrink {
                old_brk: self.current_brk,
                new_brk: requested,
                freed_size: self.current_brk.value() - requested.value(),
            }
        } else if requested.value() > self.current_brk.value() {
            // 扩展堆
            HeapAdjustment::Expand {
                old_brk: self.current_brk,
                new_brk: requested,
                added_size: requested.value() - self.current_brk.value(),
            }
        } else {
            // 无变化
            HeapAdjustment::NoChange
        }
    }
}

/// 堆调整类型
#[derive(Debug, Clone, Copy)]
pub enum HeapAdjustment {
    /// 无需调整
    NoChange,
    /// 扩展堆
    Expand {
        old_brk: VAddr,
        new_brk: VAddr,
        added_size: usize,
    },
    /// 收缩堆
    Shrink {
        old_brk: VAddr,
        new_brk: VAddr,
        freed_size: usize,
    },
    /// 无效请求
    Invalid,
}
```

**堆区域查找**

```rust
/* os/vm/src/region/heap.rs */

impl ProcessMemory {
    /// 查找堆区域
    pub fn find_heap_region(&self, addr: VAddr) -> Option<&VirRegion> {
        // 使用 AVL 树查找地址小于 addr 的最大区域
        self.regions.search(addr, AvlSearchType::Less)
    }

    /// 扩展堆区域到指定地址
    pub fn extend_heap(&mut self, new_brk: VAddr) -> Result<(), BrkError> {
        let page_aligned = new_brk.page_align_up();

        // 查找堆区域
        let heap_region = self.find_heap_region(page_aligned)
            .ok_or(BrkError::OutOfMemory)?;

        // 检查是否会与下一个区域冲突
        if let Some(next) = self.regions.next_region(heap_region) {
            if page_aligned.value() > next.vaddr().value() {
                return Err(BrkError::RegionConflict);
            }
        }

        // 执行扩展
        self.region_extend_upto(heap_region, page_aligned)
    }
}
```

**与进程结构的关系**

```rust
/* os/vm/src/process/mod.rs */

/// VM 进程结构
pub struct VmProcess {
    /// 进程标识
    pub endpoint: Endpoint,
    /// 地址空间
    pub memory: ProcessMemory,
    /// 堆状态
    pub heap: HeapState,
    /// 当前虚拟内存使用量
    pub total_memory: usize,
    /// 历史最大虚拟内存使用量
    pub max_memory: usize,
}
```

### 3.3 错误处理

使用 Rust 的 Result 类型进行类型安全的错误处理。

**错误分类**

```rust
/* os/vm/src/error/brk.rs */

/// brk 操作错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrkError {
    /// 内存不足
    /// - 物理内存耗尽
    /// - 超出虚拟内存限制
    OutOfMemory,

    /// 无效的 endpoint
    /// - 进程不存在
    /// - endpoint 与 slot 不匹配
    InvalidEndpoint,

    /// 区域冲突
    /// - 与栈区域重叠
    /// - 与其他映射区域冲突
    RegionConflict,

    /// 无效地址
    /// - 低于数据段起始
    /// - 地址空间无效
    InvalidAddress,
}

impl BrkError {
    /// 转换为 errno
    pub fn to_errno(&self) -> i32 {
        match self {
            BrkError::OutOfMemory => -(errno::ENOMEM as i32),
            BrkError::InvalidEndpoint => -(errno::EINVAL as i32),
            BrkError::RegionConflict => -(errno::ENOMEM as i32),
            BrkError::InvalidAddress => -(errno::EINVAL as i32),
        }
    }

    /// 错误描述
    pub fn description(&self) -> &'static str {
        match self {
            BrkError::OutOfMemory => "Out of memory",
            BrkError::InvalidEndpoint => "Invalid endpoint",
            BrkError::RegionConflict => "Region conflict",
            BrkError::InvalidAddress => "Invalid address",
        }
    }
}

impl core::fmt::Display for BrkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "brk error: {}", self.description())
    }
}
```

**错误处理流程**

```rust
/* os/vm/src/handler/brk.rs */

impl VmHandler {
    /// 处理 brk 请求
    pub fn handle_brk(&mut self, request: BrkRequest) -> BrkResponse {
        // 1. 验证 endpoint
        let process = match self.process_table.find_by_endpoint(request.caller) {
            Some(p) => p,
            None => return BrkResponse::error(BrkError::InvalidEndpoint),
        };

        // 2. 验证地址
        if !self.is_valid_brk_address(process, request.addr) {
            return BrkResponse::error(BrkError::InvalidAddress);
        }

        // 3. 执行调整
        match self.adjust_heap(process, request.addr) {
            Ok(new_brk) => BrkResponse::success(new_brk),
            Err(e) => BrkResponse::error(e),
        }
    }

    /// 验证 brk 地址
    fn is_valid_brk_address(&self, process: &VmProcess, addr: VAddr) -> bool {
        // 地址不能低于数据段顶部
        if addr.value() < process.heap.start.value() {
            return false;
        }

        // 地址必须在有效范围内
        if addr.value() > process.memory.max_user_address() {
            return false;
        }

        true
    }
}
```

**错误恢复**

```rust
impl VmHandler {
    /// 调整堆（带错误恢复）
    fn adjust_heap(&mut self, process: &mut VmProcess, new_brk: VAddr) 
        -> Result<VAddr, BrkError> 
    {
        let adjustment = process.heap.calculate_new_brk(new_brk);

        match adjustment {
            HeapAdjustment::NoChange => Ok(process.heap.current_brk),

            HeapAdjustment::Expand { old_brk, new_brk, added_size } => {
                // 尝试扩展
                match self.try_expand_heap(process, new_brk, added_size) {
                    Ok(()) => {
                        process.heap.current_brk = new_brk;
                        if new_brk.value() > process.heap.max_brk.value() {
                            process.heap.max_brk = new_brk;
                        }
                        Ok(new_brk)
                    }
                    Err(e) => {
                        // 扩展失败，保持原状态
                        Err(e)
                    }
                }
            }

            HeapAdjustment::Shrink { old_brk, new_brk, freed_size } => {
                // 收缩通常不会失败
                self.shrink_heap(process, new_brk, freed_size);
                process.heap.current_brk = new_brk;
                Ok(new_brk)
            }

            HeapAdjustment::Invalid => Err(BrkError::InvalidAddress),
        }
    }
}
```

**日志记录**

```rust
impl VmHandler {
    fn log_brk_result(&self, process: &VmProcess, request: &BrkRequest, 
                      result: &Result<VAddr, BrkError>) 
    {
        match result {
            Ok(new_brk) => {
                trace!("brk: process {} set brk to {:#x}", 
                       process.endpoint, new_brk.value());
            }
            Err(e) => {
                warn!("brk: process {} failed: {} (requested {:#x})", 
                      process.endpoint, e.description(), request.addr.value());
            }
        }
    }
}
```

---

## 4. 实现详解

### 4.1 消息处理入口

do_brk 是 VM_BRK 消息的处理入口函数。

**函数签名**

```rust
/* os/vm/src/handler/brk.rs */

impl VmHandler {
    /// 处理 VM_BRK 消息
    /// 
    /// # 参数
    /// - `msg`: 原始 IPC 消息
    /// 
    /// # 返回
    /// 操作结果，成功返回 0，失败返回负的 errno
    pub fn do_brk(&mut self, msg: &Message) -> i32 {
        // 解析请求
        let request = BrkRequest::from_message(msg);

        // 处理请求
        let response = self.handle_brk(request);

        // 返回结果
        response.to_return_value()
    }
}
```

**消息分发**

```rust
/* os/vm/src/main.rs */

impl VmServer {
    /// 消息处理循环
    pub fn run(&mut self) -> ! {
        loop {
            // 接收消息
            let msg = self.receive_message();

            // 根据消息类型分发
            let result = match msg.m_type {
                VM_BRK => self.handler.do_brk(&msg),
                VM_FORK => self.handler.do_fork(&msg),
                VM_EXIT => self.handler.do_exit(&msg),
                // ...
                _ => {
                    warn!("unknown message type: {}", msg.m_type);
                    -(errno::EINVAL as i32)
                }
            };

            // 发送响应
            self.send_response(result);
        }
    }
}
```

**处理流程**

1. 解析请求：`BrkRequest::from_message(msg)` — 提取 addr 字段和 caller endpoint
2. 处理请求：`handle_brk(request)` — 验证 endpoint、验证地址、执行堆调整
3. 构造响应：`BrkResponse { result: Ok/Err }` — 成功返回 0，失败返回负的 errno

### 4.2 进程查找

通过消息来源 endpoint 查找进程结构。

**进程查找实现**

```rust
/* os/vm/src/process/table.rs */

impl ProcessTable {
    /// 通过 endpoint 查找进程
    /// 
    /// # 参数
    /// - `endpoint`: 进程的 endpoint
    /// 
    /// # 返回
    /// 成功返回进程的可变引用，失败返回 None
    pub fn find_by_endpoint(&mut self, endpoint: Endpoint) 
        -> Option<&mut VmProcess> 
    {
        // 验证 endpoint 有效性
        if !self.is_valid_endpoint(endpoint) {
            return None;
        }

        // 计算 slot
        let slot = self.endpoint_to_slot(endpoint);

        // 检查 slot 有效性
        if slot >= self.processes.len() {
            return None;
        }

        let process = &mut self.processes[slot];

        // 验证进程在使用中且 endpoint 匹配
        if process.flags.contains(VmProcessFlags::INUSE) 
            && process.endpoint == endpoint 
        {
            Some(process)
        } else {
            None
        }
    }

    /// 验证 endpoint 有效性
    fn is_valid_endpoint(&self, endpoint: Endpoint) -> bool {
        // endpoint 必须是有效的进程 endpoint
        endpoint.is_valid() && !endpoint.is_none()
    }

    /// 从 endpoint 计算 slot
    fn endpoint_to_slot(&self, endpoint: Endpoint) -> usize {
        // Minix3 的 endpoint 编码方式
        // slot = (endpoint - FIRST_USER_PROC) 或类似计算
        endpoint.slot()
    }
}
```

**endpoint 验证**

```rust
/* os/vm/src/ipc/endpoint.rs */

impl Endpoint {
    /// 检查 endpoint 是否有效
    pub fn is_valid(&self) -> bool {
        // 检查 endpoint 是否在有效范围内
        self.value >= MIN_ENDPOINT && self.value <= MAX_ENDPOINT
    }

    /// 获取进程 slot
    pub fn slot(&self) -> usize {
        // 从 endpoint 提取 slot
        // Minix3 编码: endpoint 包含 slot 信息
        ((self.value - MIN_ENDPOINT) as usize) & SLOT_MASK
    }
}
```

**查找流程**

1. 验证 endpoint 格式：`is_valid_endpoint(endpoint)` — 检查是否在有效范围内、是否为 NONE，无效返回 None
2. 计算 slot：`slot = endpoint.slot()`
3. 检查进程状态：INUSE 标志是否设置、endpoint 是否匹配，匹配返回 `Some(&mut process)`，不匹配返回 None

**错误处理**

```rust
impl VmHandler {
    fn lookup_process(&mut self, endpoint: Endpoint) 
        -> Result<&mut VmProcess, BrkError> 
    {
        match self.process_table.find_by_endpoint(endpoint) {
            Some(process) => Ok(process),
            None => {
                warn!("brk: invalid endpoint {}", endpoint);
                Err(BrkError::InvalidEndpoint)
            }
        }
    }
}
```

### 4.3 地址验证

验证新堆顶地址的合法性。

**地址验证实现**

```rust
/* os/vm/src/handler/brk.rs */

impl VmHandler {
    /// 验证 brk 地址
    /// 
    /// # 检查项
    /// 1. 地址不能低于数据段顶部
    /// 2. 地址不能与栈区域冲突
    /// 3. 地址必须在用户空间范围内
    pub fn validate_brk_address(&self, process: &VmProcess, addr: VAddr) 
        -> Result<(), BrkError> 
    {
        // 1. 检查地址不能低于数据段顶部
        if addr.value() < process.heap.start.value() {
            warn!("brk: address {:#x} below data top {:#x}", 
                  addr.value(), process.heap.start.value());
            return Err(BrkError::InvalidAddress);
        }

        // 2. 检查地址必须在用户空间范围内
        if addr.value() > process.memory.max_user_address() {
            warn!("brk: address {:#x} exceeds user space", addr.value());
            return Err(BrkError::InvalidAddress);
        }

        // 3. 检查是否与栈区域冲突
        if self.would_conflict_with_stack(process, addr) {
            warn!("brk: address {:#x} conflicts with stack", addr.value());
            return Err(BrkError::RegionConflict);
        }

        Ok(())
    }

    /// 检查是否会与栈区域冲突
    fn would_conflict_with_stack(&self, process: &VmProcess, addr: VAddr) -> bool {
        // 查找堆区域
        let heap_region = match process.memory.find_heap_region(addr) {
            Some(r) => r,
            None => return true,  // 找不到区域，视为冲突
        };

        // 获取下一个区域
        if let Some(next) = process.memory.next_region(heap_region) {
            // 如果下一个区域是栈，检查是否会重叠
            let page_aligned = addr.page_align_up();
            if page_aligned.value() > next.vaddr().value() {
                return true;
            }
        }

        false
    }
}
```

**地址验证流程**

1. 检查 `addr >= process.heap.start`（地址 >= 数据段顶部），失败返回 InvalidAddress
2. 检查 `addr <= max_user_address`（地址 <= 用户空间最大地址），失败返回 InvalidAddress
3. 检查 `page_align_up(addr) < stack_region.vaddr`（不与栈区域冲突），失败返回 RegionConflict

**边界情况处理**

```rust
impl VmHandler {
    /// 处理边界情况
    fn handle_edge_cases(&self, process: &VmProcess, addr: VAddr) 
        -> Option<BrkResponse> 
    {
        // NULL 地址：查询当前堆顶
        if addr.is_null() {
            return Some(BrkResponse::success(process.heap.current_brk));
        }

        // 地址等于当前堆顶：无需操作
        if addr == process.heap.current_brk {
            return Some(BrkResponse::success(addr));
        }

        // 地址在当前堆范围内（收缩）：直接处理
        if addr.value() < process.heap.current_brk.value() {
            // 收缩操作通常不会失败
            return None;  // 继续正常处理
        }

        None  // 继续正常处理
    }
}
```

### 4.4 堆调整

执行实际的堆扩展或收缩操作。

**堆调整实现**

```rust
/* os/vm/src/handler/brk.rs */

impl VmHandler {
    /// 执行堆调整
    pub fn adjust_heap(&mut self, process: &mut VmProcess, new_brk: VAddr) 
        -> Result<VAddr, BrkError> 
    {
        let adjustment = process.heap.calculate_new_brk(new_brk);

        match adjustment {
            HeapAdjustment::NoChange => {
                Ok(process.heap.current_brk)
            }

            HeapAdjustment::Expand { old_brk, new_brk, added_size } => {
                self.expand_heap(process, old_brk, new_brk, added_size)
            }

            HeapAdjustment::Shrink { old_brk, new_brk, freed_size } => {
                self.shrink_heap(process, old_brk, new_brk, freed_size)
            }

            HeapAdjustment::Invalid => {
                Err(BrkError::InvalidAddress)
            }
        }
    }

    /// 扩展堆
    fn expand_heap(&mut self, process: &mut VmProcess, 
                   old_brk: VAddr, new_brk: VAddr, added_size: usize) 
        -> Result<VAddr, BrkError> 
    {
        let page_aligned = new_brk.page_align_up();

        // 查找堆区域
        let heap_region = process.memory.find_heap_region(page_aligned)
            .ok_or(BrkError::OutOfMemory)?;

        // 检查区域冲突
        if let Some(next) = process.memory.next_region(heap_region) {
            if page_aligned.value() > next.vaddr().value() {
                return Err(BrkError::RegionConflict);
            }
        }

        // 执行区域扩展
        process.memory.extend_region(heap_region, page_aligned)?;

        // 更新堆状态
        process.heap.current_brk = new_brk;
        if new_brk.value() > process.heap.max_brk.value() {
            process.heap.max_brk = new_brk;
        }

        Ok(new_brk)
    }

    /// 收缩堆
    fn shrink_heap(&mut self, process: &mut VmProcess,
                   old_brk: VAddr, new_brk: VAddr, freed_size: usize) 
        -> Result<VAddr, BrkError> 
    {
        let page_aligned = new_brk.page_align_up();

        // 查找堆区域
        let heap_region = process.memory.find_heap_region(old_brk)
            .ok_or(BrkError::OutOfMemory)?;

        // 释放物理页面
        process.memory.shrink_region(heap_region, page_aligned)?;

        // 更新堆状态
        process.heap.current_brk = new_brk;

        Ok(new_brk)
    }
}
```

**区域扩展实现**

```rust
impl ProcessMemory {
    pub fn extend_region(
        &mut self,
        region: &mut VirRegion,
        new_end: VirBytes,
        frames: &mut PageFrames,
    ) -> Result<(), BrkError> {
        let new_length = VirBytes(new_end.get() - region.vaddr.get());
        let old_length = region.length;

        if new_length.get() <= old_length.get() {
            return Ok(());
        }

        let added_pages = ((new_length.get() - old_length.get() + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
        region.physblocks.extend((0..added_pages).map(|_| None));

        region.length = new_length;

        Ok(())
    }

    pub fn shrink_region(
        &mut self,
        region: &mut VirRegion,
        new_end: VirBytes,
        frames: &mut PageFrames,
    ) -> Result<(), BrkError> {
        let new_length = VirBytes(new_end.get() - region.vaddr.get());
        let old_length = region.length;

        if new_length.get() >= old_length.get() {
            return Ok(());
        }

        let freed_pages = ((old_length.get() - new_length.get() + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
        let keep_pages = (new_length.get() / PAGE_SIZE) as usize;

        for page_idx in keep_pages..(keep_pages + freed_pages) {
            if let Some(slot) = region.physblocks[page_idx].take() {
                if slot.is_mapped() {
                    let state = frames.get_mut(slot.pfn).unwrap();
                    state.refcount -= 1;
                    if state.refcount == 0
                        && !state.flags.contains(PageFlags::IN_CACHE)
                    {
                        if let Some(mt) = slot.memtype {
                            mt.ev_unreference(frames, slot);
                        }
                    }
                }
            }
        }

        region.physblocks.truncate(keep_pages);
        region.length = new_length;

        Ok(())
    }
}
```

> **方案 A 简化**: Minix3 的 `shrink_region` 需要遍历 `phys_blocks`，对每个 `PhysBlock` 调用 `decrement_refcount()`，当 `refcount` 降为 0 时调用 `free_physical_page()`。方案 A 使用 `VirRegion.unmap_page()` 统一处理，递减 `PageFrames.states[pfn].refcount`，由 `MemType.ev_unreference()` 决定是否释放物理页。

**调整流程**

1. 计算调整类型：`calculate_new_brk(new_brk)` → NoChange / Expand / Shrink
2. Expand 路径：查找堆区域 → 检查冲突 → 扩展区域（`physblocks.extend`，新增 `None` 槽位）
3. Shrink 路径：查找堆区域 → 释放物理页（递减 `PageFrames.states[pfn].refcount`，归零时 `ev_unreference`） → `physblocks.truncate` → 更新区域长度
4. 更新堆状态：`current_brk = new_brk`, `max_brk = max(max_brk, new_brk)`

### 4.5 返回结果

构造响应消息返回给调用者。

**响应构造**

```rust
/* os/vm/src/handler/brk.rs */

impl VmHandler {
    /// 构造 brk 响应
    pub fn build_response(&self, result: Result<VAddr, BrkError>) -> BrkResponse {
        match result {
            Ok(new_brk) => {
                trace!("brk: success, new_brk = {:#x}", new_brk.value());
                BrkResponse::success(new_brk)
            }
            Err(e) => {
                warn!("brk: failed: {}", e.description());
                BrkResponse::error(e)
            }
        }
    }
}
```

**BrkResponse 实现**

```rust
/* os/vm/src/ipc/brk.rs */

impl BrkResponse {
    /// 成功响应
    pub fn success(new_brk: VAddr) -> Self {
        Self {
            result: Ok(new_brk),
        }
    }

    /// 错误响应
    pub fn error(err: BrkError) -> Self {
        Self {
            result: Err(err),
        }
    }

    /// 转换为 IPC 返回值
    /// 
    /// # 返回值
    /// - 成功: 0
    /// - 失败: 负的 errno 值
    pub fn to_return_value(&self) -> i32 {
        match &self.result {
            Ok(_) => 0,
            Err(e) => e.to_errno(),
        }
    }

    /// 获取新堆顶地址（用于日志）
    pub fn new_brk(&self) -> Option<VAddr> {
        self.result.ok()
    }
}
```

**响应流程**

1. 成功路径：`Ok(new_brk)` → 记录成功日志 → `BrkResponse::success(new_brk)` → `to_return_value()` 返回 0
2. 失败路径：`Err(error)` → 记录错误日志 → `BrkResponse::error(err)` → `to_return_value()` 返回负的 errno

**完整处理函数**

```rust
impl VmHandler {
    /// 完整的 brk 处理函数
    pub fn handle_brk(&mut self, request: BrkRequest) -> BrkResponse {
        // 1. 查找进程
        let process = match self.process_table.find_by_endpoint(request.caller) {
            Some(p) => p,
            None => return BrkResponse::error(BrkError::InvalidEndpoint),
        };

        // 2. 处理边界情况
        if let Some(response) = self.handle_edge_cases(process, request.addr) {
            return response;
        }

        // 3. 验证地址
        if let Err(e) = self.validate_brk_address(process, request.addr) {
            return BrkResponse::error(e);
        }

        // 4. 执行堆调整
        let result = self.adjust_heap(process, request.addr);

        // 5. 构造响应
        self.build_response(result)
    }
}
```

**与 libc 的交互**

```c
/* 用户态 libc */
int brk(void *addr)
{
    message m;
    memset(&m, 0, sizeof(m));
    m.m_lc_vm_brk.addr = addr;
    
    // 发送请求并接收响应
    if (_syscall(VM_PROC_NR, VM_BRK, &m) < 0)
        return -1;  // errno 已设置
    
    _brksize = addr;  // 更新本地记录
    return 0;
}
```

---

## 5. 专题：与 malloc 的关系

### 5.1 libc brk

用户态 malloc 通过 brk/sbrk 获取堆内存。

**malloc 与 brk 的层次关系**

应用程序 → `malloc(size)`/`free(ptr)` → 用户态内存分配器（管理已分配内存块、合并/分割空闲块、需要更多内存时调用 sbrk） → `sbrk(incr)`/`brk(addr)` → libc 系统调用封装（设置消息结构、`_syscall(VM_PROC_NR, VM_BRK, &m)`、维护 `_brksize` 全局变量） → VM 服务（管理进程地址空间、扩展/收缩堆区域、分配/释放物理页面）

**Minix3 libc 实现**

```c
/* minix3/minix/lib/libc/sys/brk.c */

extern char *_brksize;  // 当前堆顶（全局变量）

int brk(void *addr)
{
    message m;

    // 只有请求的地址与当前堆顶不同时才调用 VM
    if (addr != _brksize) {
        memset(&m, 0, sizeof(m));
        m.m_lc_vm_brk.addr = addr;
        if (_syscall(VM_PROC_NR, VM_BRK, &m) < 0)
            return -1;
        _brksize = addr;
    }
    return 0;
}
```

**malloc 的典型实现**

```c
/* 简化的 malloc 实现示意 */

struct block_header {
    size_t size;
    int free;
    struct block_header *next;
};

static struct block_header *free_list = NULL;
static void *heap_start = NULL;

void *malloc(size_t size)
{
    // 1. 对齐请求大小
    size = ALIGN(size + sizeof(struct block_header));

    // 2. 在空闲链表中查找合适的块
    struct block_header *block = find_free_block(size);
    if (block) {
        block->free = 0;
        return (void *)(block + 1);
    }

    // 3. 没有合适的块，需要扩展堆
    void *new_mem = sbrk(size);
    if (new_mem == (void *)-1) {
        return NULL;  // 内存不足
    }

    // 4. 初始化新块
    block = (struct block_header *)new_mem;
    block->size = size;
    block->free = 0;
    block->next = NULL;

    return (void *)(block + 1);
}

void free(void *ptr)
{
    if (!ptr) return;

    struct block_header *block = (struct block_header *)ptr - 1;
    block->free = 1;

    // 可选：合并相邻的空闲块
    // 可选：如果堆末尾有大块空闲，收缩堆
}
```

**_brksize 全局变量**

```c
/* _brksize 由 libc 维护 */
char *_brksize;

/* 在进程启动时初始化 */
void _init_brk(void)
{
    // 通过 brk(0) 查询当前堆顶
    message m;
    memset(&m, 0, sizeof(m));
    m.m_lc_vm_brk.addr = 0;
    _syscall(VM_PROC_NR, VM_BRK, &m);
    // _brksize 在 brk 中被设置
}
```

### 5.2 sbrk 实现

sbrk 是基于 brk 的便捷封装，用于增量调整堆。

**Minix3 sbrk 实现**

```c
/* minix3/minix/lib/libc/sys/sbrk.c */

#include <unistd.h>

extern char *_brksize;

void *sbrk(intptr_t incr)
{
    char *newsize, *oldsize;

    // 保存旧堆顶
    oldsize = _brksize;
    
    // 计算新堆顶
    newsize = _brksize + incr;

    // 溢出检查
    if ((incr > 0 && newsize < oldsize) ||
        (incr < 0 && newsize > oldsize))
        return (void *)-1;

    // 调用 brk
    if (brk(newsize) == 0)
        return oldsize;  // 返回旧堆顶
    else
        return (void *)-1;
}
```

**sbrk 与 brk 的区别**

| 接口 | 参数 | 返回值 | 用途 |
|------|------|--------|------|
| `brk(addr)` | 绝对地址 | 0 (成功) 或 -1 (失败) | 设置堆顶到指定地址 |
| `sbrk(incr)` | 增量（可正可负） | 旧堆顶地址 或 -1 (失败) | 增量调整堆大小 |

示例：`brk((void*)0x10000)` 设置堆顶到 0x10000；`sbrk(4096)` 扩展 4KB 并返回扩展前的堆顶。

**sbrk 的典型用法**

```c
/* malloc 使用 sbrk 获取内存 */
void *malloc(size_t size)
{
    // ... 省略查找空闲块的逻辑 ...

    // 需要更多内存
    void *mem = sbrk(size);
    if (mem == (void *)-1) {
        errno = ENOMEM;
        return NULL;
    }

    return mem;
}

/* 查询当前堆顶 */
void *current_brk = sbrk(0);  // incr = 0，返回当前堆顶
```

**sbrk(0) 的特殊用途**

```c
// 查询当前堆顶，不修改
void *get_current_brk(void)
{
    return sbrk(0);
}

// 检查堆是否可以扩展
int can_expand_heap(size_t size)
{
    void *current = sbrk(0);
    void *test = sbrk(size);
    if (test == (void *)-1) {
        return 0;  // 无法扩展
    }
    sbrk(-size);  // 恢复
    return 1;
}
```

---

## 6. 测试要点

> 本章描述 Rust 实现需要测试的维度和关键场景，而非罗列测试代码。

### 6.1 测试维度

| 维度 | 测试重点 |
|------|---------|
| **正常路径** | 扩展、收缩、无变化、查询堆顶 |
| **边界条件** | 页对齐、零地址、堆顶等于数据段顶 |
| **错误路径** | 无效 endpoint、地址低于数据段、与栈冲突、内存不足 |
| **错误码对齐** | 验证 BrkError 到 errno 的映射与 Minix3 一致 |
| **状态一致性** | 扩展/收缩后 HeapState 与区域长度一致 |
| **物理页面** | 收缩后物理页引用计数正确、vm_total 正确更新 |

### 6.2 关键测试场景

**堆扩展**

| 场景 | 输入 | 预期结果 |
|------|------|---------|
| 小扩展（+1 页） | addr = current_brk + PAGE_SIZE | 成功 |
| 大扩展（+N 页） | addr = current_brk + N*PAGE_SIZE | 成功 |
| 非对齐地址 | addr 未页对齐 | 成功（内部向上取整） |
| 连续扩展 | 多次调用 | 每次成功，max_brk 递增 |
| 无变化 | addr = current_brk | 成功，无操作 |
| NULL 地址查询 | addr = 0 | 返回当前堆顶 |

**堆收缩**

| 场景 | 初始堆顶 | 目标堆顶 | 预期结果 |
|------|---------|---------|---------|
| 小收缩 | current_brk | current_brk - PAGE_SIZE | 成功 |
| 大收缩 | current_brk | data_top | 成功（堆大小=0） |
| 部分收缩 | current_brk | current_brk - N*PAGE_SIZE | 成功，物理页释放 |
| 收缩后 vm_total | 有已分配物理页 | 收缩释放区域 | vm_total 减少 |

**错误处理**

| 场景 | 输入 | 预期错误 |
|------|------|---------|
| 无效 endpoint | 不存在的进程 | InvalidEndpoint → EINVAL |
| 地址低于数据段 | addr < data_top | InvalidAddress → EINVAL |
| 与栈冲突 | page_align(addr) >= stack_region.vaddr | RegionConflict → ENOMEM |
| 内存不足 | 请求超过可用物理内存 | OutOfMemory → ENOMEM |

**错误码对齐验证**

| BrkError 变体 | 期望 errno | 说明 |
|---------------|-----------|------|
| OutOfMemory | ENOMEM | 与 Minix3 real_brk 返回值一致 |
| InvalidEndpoint | EINVAL | 与 Minix3 do_brk 中 vm_isokendpt 失败一致 |
| RegionConflict | ENOMEM | 与 Minix3 "can't grow into next region" 一致 |
| InvalidAddress | EINVAL | Rust 设计新增，Minix3 无此独立检查 |

---

## 7. 参见

- [01-vmproc-struct.md](01-vmproc-struct.md) - vmproc 结构体定义（vm_total, vm_total_max 等字段）
- [02-vmproc-table.md](02-vmproc-table.md) - 进程表与 endpoint 验证（vm_isokendpt）
- [12-memtype.md](12-memtype.md) - 内存类型与 ev_resize/ev_lowshrink 回调
- [11-region-mapping.md](11-region-mapping.md) - VirRegion + PageSlot 页映射
- [13-region-avl.md](13-region-avl.md) - AVL 树搜索（region_search, AVL_LESS）
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作（pt_writemap）
- [10-phys-pagestate.md](10-phys-pagestate.md) - 物理页状态与引用计数（unmap_page）
- [00-vm-overview.md](00-vm-overview.md) - VM 服务总览

---

*分类: VM服务 | IPC接口: VM_BRK | 调用者: 用户进程（不经过PM）*
