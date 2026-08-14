# 17-vm-brk: VM_BRK 服务

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
| **扩展堆** | 增加堆空间，扩展虚拟区域（物理页延迟分配） |
| **收缩堆** | Minix3 中收缩被静默忽略（返回成功但不释放内存）；Rust 实现支持真正收缩 |
| **查询堆顶** | 参数为 0 时返回当前堆顶 |

**进程地址空间布局**（低地址→高地址）：text → data → bss → heap（↑向上增长）→ gap → stack（↓向下增长）。堆和栈从 gap 两端相向增长，若相遇则进程被杀死（ENOMEM）。

### 1.2 堆的增长方向

- **扩展堆**（`brk(new_addr)` 其中 `new_addr > 当前堆顶`）：堆向高地址扩展，扩展虚拟区域长度（物理页延迟分配，首次访问时通过缺页异常分配）
- **收缩堆**（`brk(new_addr)` 其中 `new_addr < 当前堆顶`）：**Minix3 中收缩被静默忽略**——`anon_resize` 对 `l <= vr->length` 直接返回 OK，不释放任何内存，堆顶不变但调用成功。这是 Minix3 的设计选择（见 §2.7.2 anon_resize 分析）。Rust 实现已支持真正收缩（见 §3.2 设计决策）
- **危险情况**：堆顶超过栈底时，进程被杀死（ENOMEM）

### 1.3 与 Minix3 的对应关系

| 功能 | Minix3 源文件 | 函数 |
|------|--------------|------|
| brk 入口 | `break.c:46` | `do_brk()` |
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

### 1.5 与 malloc 的关系

用户态 malloc 通过 brk/sbrk 获取堆内存，层次关系为：

应用程序 → `malloc(size)`/`free(ptr)` → 用户态内存分配器（管理已分配内存块、合并/分割空闲块、需要更多内存时调用 sbrk） → `sbrk(incr)`/`brk(addr)` → libc 系统调用封装（设置消息结构、`_syscall(VM_PROC_NR, VM_BRK, &m)`、维护 `_brksize` 全局变量） → VM 服务（管理进程地址空间、扩展/收缩堆区域、分配/释放物理页面）

**关键点**：

1. **malloc 是用户态分配器**：在 brk 获取的堆空间内管理内存块，调用者不直接使用 brk
2. **brk 是内核态服务**：VM 管理进程地址空间，扩展/收缩堆区域
3. **_brksize 全局变量**：libc 维护的当前堆顶，brk 成功后更新（libc 实现见 §2.1）
4. **malloc 通常不收缩**：free 后通常不调用 brk 收缩，而是缓存已分配内存供后续 malloc 使用

> **libc brk/sbrk 实现见 §2.1**，brk 与 sbrk 的接口对比见 §1.4。

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
/* minix3/minix/include/minix/ipc.h:921-924 */
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
| `0` 或极小值 | Minix3 中导致 ENOMEM（无法找到 vaddr < 0 的区域） |
| `> 当前堆顶` | 扩展堆，增加内存 |
| `< 当前堆顶` | 收缩堆，释放内存 |
| `= 当前堆顶` | 无操作，直接返回成功 |

**Minix3 的限制检查机制**

Minix3 的 brk **没有显式的地址范围验证**（如 data_top/stack_low 检查）。限制检查通过 `map_region_extend_upto_v` 的区域查找和冲突检查间接实现：

1. **区域查找失败** → ENOMEM：`region_search(&vmp->vm_regions_avl, offset, AVL_LESS)` 找不到起始地址 < offset 的区域时返回 ENOMEM。这隐含了"地址不能低于数据段"的约束——若 addr 低于所有区域的起始地址，查找必然失败
2. **堆栈冲突** → ENOMEM：`nextvr->vaddr < offset` 检查堆扩展是否会侵入下一个区域（通常是栈）。这隐含了"地址不能与栈重叠"的约束
3. **页对齐**（内部处理）：VM 通过 `roundup(offset, VM_PAGE_SIZE)` 向上取整到页边界

> **注意**：Minix3 的 vmproc 结构体中没有 `vm_brk`、`vm_data_top`、`vm_stack_low` 等堆专用字段（见 §1.3）。堆顶地址隐含在数据段 vir_region 的 `vaddr + length` 中。

错误情况（Minix3 实际行为）：
- `region_search(AVL_LESS)` 找不到区域 → ENOMEM（地址低于所有区域，或进程无数据段区域）
- `nextvr->vaddr < offset` → ENOMEM（堆扩展与下一个区域冲突，通常是栈）
- `realloc(physblocks)` 失败 → ENOMEM（内存不足）
- `vm_isokendpt()` 失败 → EINVAL（endpoint 无效）

**Minix3 源码中的处理**

do_brk 的完整实现见 §2.5。其核心逻辑是：先通过 `vm_isokendpt` 验证调用者 endpoint，再调用 `real_brk` 执行实际调整。

### 2.4 返回结果

#### 2.4.1 实际堆顶地址

brk 系统调用的返回值表示操作结果：

**返回值含义**

| 返回值 | 含义 |
|--------|------|
| `0` | 成功，堆顶已设置到请求地址 |
| `-1` | 失败，errno 设置为错误码 |

**libc 层的处理**

libc 的 `brk()` 封装在成功时更新 `_brksize` 全局变量，失败时返回 -1 并设置 errno。完整实现见 §2.1。

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
/* minix3/minix/servers/vm/break.c:46-57 */
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
/* minix3/minix/servers/vm/region.c:1002-1061 */
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

> AVL 树的完整实现分析见 [13-region-avl.md](13-region-avl.md)，本节聚焦 brk 调用路径中的 AVL_LESS 搜索语义。

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
/* minix3/minix/servers/vm/region.c:1002-1061 */
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

> VirRegion 和 PhysBlock 的完整数据结构分析见 [11-region-mapping.md](11-region-mapping.md)，本节聚焦 brk 扩展路径中的 physblocks realloc 和 ev_resize 回调。

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

> **Direct Map 影响分析**：brk 的物理页分配通过缺页处理程序间接完成，调用方不受 direct map 影响。缺页处理程序内部已简化为 `alloc_phys() → vm_phys_to_virt()`（见 15-pagefault.md），brk 代码无需修改。这是 direct map 统一性的体现——物理页分配的简化在底层完成，上层调用者透明受益。

**延迟分配的完整链路**

brk 扩展区域后，新页没有物理内存。当用户首次访问这些页时触发缺页异常，由缺页处理程序分配物理页：

```
brk(0x500000) → 区域扩展到 0x500000
  │
  │  （新页 0x400000-0x500000 无物理映射）
  │
  ▼  用户写入 0x420000
CPU #PF (P=0, 页不存在)
  │
  ▼
handle_pagefault():
  ├── regions.find(0x420000) → 找到堆区域
  ├── physblocks[page_idx] → None（未分配）
  ├── page_alloc.alloc_pfn() → new_pfn
  ├── PageFrames[new_pfn].refcount = 1
  ├── physblocks[page_idx] = Some(PageSlot { pfn: new_pfn, ... })
  └── page_table.map(0x420000, new_pfn) → 页表映射
```

**brk 扩展后的区域状态**（以 `physblocks: Vec<Option<PageSlot>>` 模型表示）：

```
brk 前:
  VirRegion { vaddr: 0x200000, length: 0x200000 }
  physblocks: [Some(slot0), Some(slot1), ..., Some(slot7)]  ← 8 页，全部有物理页
  页表: 0x200000-0x3FF000 全部映射

brk(0x500000) 后:
  VirRegion { vaddr: 0x200000, length: 0x300000 }     ← length 增加
  physblocks: [Some(slot0), ..., Some(slot7), None, None, None, None]  ← 新页为 None
  页表: 0x200000-0x3FF000 映射，0x400000-0x4FF000 未映射

首次访问 0x420000 后:
  VirRegion { vaddr: 0x200000, length: 0x300000 }
  physblocks: [Some(slot0), ..., Some(slot7), None, Some(slot9), None, None]  ← slot9 已分配
  页表: 0x200000-0x3FF000 映射，0x420000 映射，其余未映射
```

**brk 收缩后的区域状态**：

```
brk 前:
  VirRegion { vaddr: 0x200000, length: 0x300000 }
  physblocks: [Some(slot0), ..., Some(slot11)]  ← 12 页
  页表: 0x200000-0x4FF000 全部映射

brk(0x400000) 后（Rust 实现，Minix3 不支持收缩）:
  VirRegion { vaddr: 0x200000, length: 0x200000 }     ← length 减少
  physblocks: [Some(slot0), ..., Some(slot7)]          ← 8 页（Vec 已 resize）
  页表: 0x200000-0x3FF000 映射，0x400000-0x4FF000 已 unmap
  物理页: slot8-slot11 对应的 PageFrames refcount 递减，refcount==0 的已 free_pfn
```

#### 2.7.3 anon_resize — brk 扩展的实际执行者

> 内存类型回调机制的完整分析见 [12-memtype.md](12-memtype.md)，本节聚焦 `anon_resize` 在 brk 路径中的行为。

> **关键发现**：brk 扩展路径中，`map_region_extend_upto_v` 调用 `vr->def_memtype->ev_resize` 回调。对于堆区域（匿名内存类型 `mem_type_anon`），该回调是 `anon_resize`，它才是真正修改 `vr->length` 的函数。理解 `anon_resize` 的行为是理解 brk 收缩被静默忽略的关键。

**Minix3 源码**

```c
/* minix3/minix/servers/vm/mem_anon.c:115-125 */
static int anon_resize(struct vmproc *vmp, struct vir_region *vr, vir_bytes l)
{
	/* Shrinking not implemented; silently ignored.
	 * (Which is ok for brk().)
	 */
	if(l <= vr->length)
		return OK;

        assert(vr);
        assert(vr->flags & VR_ANON);
        assert(!(l % VM_PAGE_SIZE));

        USE(vr, vr->length = l;);

	return OK;
}
```

**行为分析**

| 输入 | 行为 | 返回值 |
|------|------|--------|
| `l > vr->length`（扩展） | 设置 `vr->length = l`，仅修改虚拟区域长度 | OK |
| `l <= vr->length`（收缩或无变化） | **静默忽略**，不做任何操作 | OK |
| `l <= vr->length`（收缩） | 不释放物理页、不更新页表、不修改 physblocks | OK |

**关键洞察**：

1. **扩展时只修改 length**：`anon_resize` 不分配物理页、不更新页表。物理页在缺页时分配（延迟分配 / demand paging），页表在缺页处理时更新。brk 只扩展虚拟地址空间的"承诺"，不立即兑现物理内存
2. **收缩时静默忽略**：Minix3 的 brk **不真正收缩**。当 `brk(addr)` 传入的地址小于当前堆顶时，`real_brk` → `map_region_extend_upto_v` → `anon_resize`，`anon_resize` 判断 `l <= vr->length` 直接返回 OK。堆顶不变，但调用返回成功
3. **为什么 Minix3 不实现 brk 收缩**：
   - `anon_resize` 源码注释明确说 "Which is ok for brk()"
   - 用户态 malloc 通常不调用 brk 收缩，而是缓存已分配的内存
   - 收缩需要释放物理页、更新页表、调整 physblocks，实现复杂
   - 静默忽略收缩对 POSIX 语义是可接受的（brk 成功返回但实际不释放）

**其他内存类型的 ev_resize 行为**

| 内存类型 | ev_resize 实现 | 行为 |
|---------|---------------|------|
| `mem_type_anon` | `anon_resize` | 扩展修改 length，收缩静默忽略 |
| `mem_type_anon_contig` | `anon_contig_resize` | 直接返回 ENOMEM（物理连续内存不可调整大小） |
| `mem_type_cache` | `cache_resize` | 直接返回 ENOMEM（缓存块不可调整大小） |

#### 2.7.4 收缩堆

> 物理页引用计数和释放机制的完整分析见 [10-phys-pagestate.md](10-phys-pagestate.md)，本节聚焦 brk 收缩路径中的 `map_subfree` → `pb_unreferenced` 调用链。

> **重要说明**：Minix3 的 brk **不真正收缩堆**。`real_brk` 只调用 `map_region_extend_upto_v`，后者通过 `anon_resize` 对收缩请求静默忽略（返回 OK 但不操作）。本节分析的 `map_unmap_region` **不是 brk 的调用路径**，而是用于 `munmap` 等场景的区域释放函数。此处保留分析是因为 Rust 实现已支持真正的 brk 收缩（见 §3.2），需要理解区域收缩的完整机制。

**map_unmap_region — 区域释放函数**

`map_unmap_region` 是通用的区域释放函数，用于 `munmap`、`exit` 等场景，**不被 brk 调用**。

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

> **注意**: 若 brk 收缩走 `map_unmap_region`，则走的是 `offset + len == r->length` 分支（从尾部收缩），这是最简单的情况，只需减少 `r->length`。从头部收缩（`offset == 0`）需要 `ev_lowshrink` 回调支持，且涉及 `vaddr` 调整和 `physblocks` 移位，更为复杂。但再次强调，Minix3 的 brk 路径不调用此函数。
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

**收缩 vs 扩展（Minix3 实际行为）**

| 操作 | 物理页面 | 区域元数据 | Minix3 brk 行为 |
|------|---------|-----------|----------------|
| **扩展** | 延迟分配（缺页时） | `anon_resize` 增加 length | Y 真正执行 |
| **收缩** | 不释放 | `anon_resize` 静默忽略 | 注意: 返回 OK 但不操作 |

**收缩 vs 扩展（Rust 实现）**

| 操作 | 物理页面 | 区域元数据 | 说明 |
|------|---------|-----------|------|
| **扩展** | 延迟分配（缺页时） | 增加 length | 与 Minix3 一致 |
| **收缩** | 释放物理页、递减 refcount | 减少 length、truncate physblocks | Rust 设计增强，Minix3 不支持 |

### 2.8 限制检查

> vmproc 结构体的完整字段分析见 [01-vmproc-struct.md](01-vmproc-struct.md)，本节聚焦 brk 相关的 `vm_total`/`vm_total_max` 统计和栈碰撞检查。

#### 2.8.1 vm_total_max

vm_total_max 记录进程使用过的最大虚拟内存量，用于资源统计。

**数据结构**

```c
/* minix3/minix/servers/vm/vmproc.h:14-33 */
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
/* minix3/minix/servers/vm/region.c:112-130 */
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

### 3.1 IPC 消息类型设计

与 VM_FORK 等其他服务保持一致的三层分离架构（Transport + Codec + Semantic）。

**语义层类型**（`minix-types/src/ipc/vm.rs`）

```rust
/// PM → VM: brk 请求
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmBrkIn {
    pub endpoint: Endpoint,
    pub new_addr: VirBytes,
}

/// VM → PM: brk 响应
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmBrkOut {
    pub new_addr: VirBytes,
}
```

**Codec 层**（`DecodeFromM1` / `EncodeToM1`）

```rust
impl DecodeFromM1 for VmBrkIn {
    fn decode(m1: &MessageM1) -> Self {
        Self {
            endpoint: Endpoint(m1.m1i1),
            new_addr: VirBytes(m1.m1p1),
        }
    }
}

impl EncodeToM1 for VmBrkOut {
    fn encode(&self, m1: &mut MessageM1) {
        m1.m1i1 = self.new_addr.0 as i32;
    }
}
```

**内部类型**（`os/servers/vm/src/brk.rs`）

```rust
pub(crate) struct BrkRequest {
    pub endpoint: Endpoint,
    pub new_brk_addr: VirBytes,
}

pub(crate) struct BrkResponse {
    pub new_brk_addr: VirBytes,
}
```

dispatcher 将 `VmBrkIn` 转换为 `BrkRequest`，将 `BrkResponse` 转换为 `VmBrkOut`。

### 3.2 堆区域管理

> **设计决策：为什么不用独立 HeapState？** Minix3 没有独立的堆状态结构，堆顶隐含在 vir_region 的 `vaddr + length` 中（§1.3 已说明）。Rust 实现同样不引入独立 HeapState，而是通过 `ActiveProc::region_top()` 获取当前堆顶（对应 Minix3 的 `vm_region_top` 字段），理由：
> 1. **与 Minix3 语义对齐**：Minix3 的 `vm_region_top` 就是堆顶，Rust 的 `vm_region_top` 字段语义相同
> 2. **避免冗余状态**：独立 HeapState 的 `current_brk` 与 `vm_region_top` 语义重复，需要额外同步逻辑
> 3. **简化实现**：`region_top()` 直接返回 `vm_region_top`，无需维护额外状态

> **设计增强：brk 收缩** — Minix3 的 brk 不支持收缩（`anon_resize` 对 `l <= vr->length` 静默忽略，返回 OK 但不操作，见 §2.7.3）。Rust 实现支持真正的 brk 收缩，理由：
> 1. **内存回收**：收缩时释放物理页，减少内存占用（Minix3 的静默忽略导致内存无法回收）
> 2. **语义正确性**：POSIX 允许 brk 收缩成功但实际不释放，但真正收缩更符合用户预期
> 3. **实现可行**：Rust 的 `VirRegion::split()` + `free_region_pages()` 使收缩实现更安全
> 4. **兼容性**：外部行为不变——收缩成功返回新地址，失败返回错误，与 Minix3 的"静默成功"在错误码层面兼容

**堆顶判断逻辑**

```rust
// handle_brk 中的三路分支（对应 Minix3 的 real_brk → map_region_extend_upto_v）
let current_brk = active.region_top();
let requested = request.new_brk_addr;

if requested.0 < current_brk.0 {
    shrink_heap(&mut active, page_alloc, frames, requested)  // 收缩
} else if requested.0 > current_brk.0 {
    grow_heap(&mut active, page_alloc, frames, requested)             // 扩展
} else {
    Ok(BrkResponse { new_brk_addr: current_brk })             // 无变化
}
```

与 Minix3 的 `map_region_extend_upto_v` 对比：Minix3 只处理扩展（收缩被 `anon_resize` 静默忽略），Rust 增加了收缩路径。

### 3.3 错误处理

使用 `Result<BrkResponse, BrkError>` 进行类型安全的错误处理。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrkError {
    ProcessNotFound,   // vm_isokendpt 或 get_active 失败
    OutOfMemory,       // 内存不足或区域冲突
}

impl BrkError {
    pub(crate) fn to_errno(&self) -> i32 {
        match self {
            Self::ProcessNotFound => ESRCH,
            Self::OutOfMemory => ENOMEM,
        }
    }
}
```

| BrkError 变体 | 对应 Minix3 错误 | errno | 触发场景 |
|---------------|-----------------|-------|---------|
| `ProcessNotFound` | `vm_isokendpt()` → `EINVAL`/`EDEADEPT` | ESRCH | endpoint 无效或进程不活跃 |
| `OutOfMemory` | `real_brk` → `ENOMEM` | ENOMEM | 区域扩展失败或堆与栈区域冲突 |

**与 Minix3 的关键差异**：Minix3 的 `do_brk` 对 endpoint 验证失败返回 `EINVAL`，Rust 返回 `ESRCH`（更精确地表示"进程不存在"）。Minix3 的 `real_brk` 只返回 `ENOMEM`，Rust 的 `OutOfMemory` 同时覆盖 Minix3 的 `ENOMEM`（内存不足）和 `nextvr->vaddr < offset`（堆与栈区域冲突）两种场景。

> **设计说明**：早期版本曾定义 `InvalidAddress` 变体用于地址验证，但 Minix3 源码中 brk 路径无显式地址范围检查（见 §2.3），地址限制由 `region_search` 和 `nextvr` 冲突检查间接实现，因此删除了此变体。

### 3.4 进程表操作

brk 使用 `VmProcTable` 的 typestate 视图获取进程访问，与 fork（§3.5）相同的模式：

| 方法 | 对应 Minix3 | 返回类型 | 说明 |
|------|------------|---------|------|
| `vm_isokendpt(endpoint)` | `vm_isokendpt()` | `Result<UserSlot, EndptError>` | endpoint → slot 映射 |
| `get_active(slot)` | `&vmproc[proc]` + flags 检查 | `Option<ActiveProc>` | 获取活跃进程视图 |

与 fork 不同的是：brk 不需要 `get_empty()`（不创建新进程），只需定位调用者自身。

### 3.5 区域扩展策略

> **设计决策：优先扩展现有区域而非创建新区域** — Minix3 的 `map_region_extend_upto_v` 通过 `realloc(physblocks)` + `anon_resize` 扩展已有区域的 length，不创建新区域。Rust 实现对齐此语义：`grow_heap` 优先调用 `VirRegion::extend()` 扩展现有堆区域的 physblocks Vec，仅首次 brk（`find_mut(current_top)` 返回 None）时创建新 VirRegion。

**与 Minix3 的关键差异**：

| 方面 | Minix3 | Rust |
|------|--------|------|
| 扩展方式 | `map_region_extend_upto_v` 扩展已有区域的 length + realloc physblocks | `VirRegion::extend()` 扩展已有区域的 physblocks Vec，语义对齐 |
| 区域冲突检查 | `nextvr->vaddr < offset` → ENOMEM | `find_overlap(current_top, new_end)` → `OutOfMemory` |
| 首次 brk | `region_search(AVL_LESS)` 找不到区域 → ENOMEM | `find_mut(current_top)` 返回 None → 创建新 VirRegion |
| 物理页分配 | 延迟分配（缺页时） | 延迟分配（缺页时），一致 |
| ev_resize 回调 | 调用 `vr->def_memtype->ev_resize`（即 `anon_resize`，只更新 length） | `extend()` 内部直接更新 length + physblocks，等价于 `anon_resize` |
| 页对齐 | `roundup(offset, VM_PAGE_SIZE)` | `((grow_len + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE` |

### 3.6 区域收缩策略

> **设计决策：为什么用 split + remove 而非 truncate physblocks？** Minix3 的 `map_unmap_region` 通过 `r->length -= len` + `pt_writemap` 实现收缩。Rust 使用 `VirRegion::split()` 将区域拆分为保留部分和释放部分，理由：
> 1. **类型安全**：`split()` 返回两个独立的 `VirRegion`，生命周期清晰，不存在部分初始化状态
> 2. **RAII 兼容**：释放部分（right）作为独立对象，`free_region_pages` 可以完整处理
> 3. **语义对齐**：最终效果与 Minix3 一致——释放超出新堆顶的物理页，保留未超出部分

### 3.7 物理页释放策略

> **与 Minix3 对齐**: Minix3 的 `shrink_region` 遍历 `phys_blocks`，对每个 `PhysBlock` 调用 `decrement_refcount()`，当 `refcount` 降为 0 时调用 `free_physical_page()`。Rust 的 `free_range` → `unmap_page` 递减 refcount，refcount=0 时返回 `(pfn, memtype)`，调用者执行 `ev_unreference` + `free_pfn`，语义完全对齐。

**brk 与 exit 的闭环**

brk 和 exit 形成内存生命周期闭环：brk 扩展虚拟地址空间（延迟分配物理页），exit 释放所有内存（包括 brk 扩展的区域）。

```
brk 扩展: add_total(extralen) + region.length += extralen
  │
  │  （进程运行，使用堆内存；部分页通过缺页分配了物理页）
  │
  ▼
exit 释放: free_region_pages() → free_range() → unmap_page 递减 refcount
  └── refcount==0 且非缓存页 → ev_unreference + free_pfn
  └── sub_total 由 VmProc::clear() 中 vm_total = default 隐式处理
```

brk 的延迟分配意味着退出时可能有些 physblocks 是 `None`（从未访问），这些不需要释放物理页。只有 `Some(PageSlot { pfn, ... })` 的槽位才需要递减 refcount 和释放物理页。这就是 brk 和 exit 的协作：brk 承诺虚拟地址空间，exit 兑现物理内存回收。

---

## 4. 实现详解

### 4.1 消息处理入口

dispatcher 从 IPC 消息解码出 `VmBrkIn`，转换为 `BrkRequest`，调用 `handle_brk`。

```rust
// dispatcher 收到 VM_BRK 后的调用路径
pub(crate) fn dispatch_brk(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    request: VmBrkIn,
) -> VmReply {
    let brk_req = brk::BrkRequest {
        endpoint: request.endpoint,
        new_brk_addr: request.new_addr,
    };

    match brk::handle_brk(table, page_alloc, frames, &brk_req) {
        Ok(response) => VmReply::Brk(VmBrkOut {
            new_addr: response.new_brk_addr,
        }),
        Err(e) => VmReply::Error(Self::brk_error_to_vm_error(e)),
    }
}
```

dispatcher 只做消息转换和错误映射，核心逻辑在 `brk::handle_brk`（§4.2）。

### 4.2 handle_brk 编排

`handle_brk` 是 brk 的核心编排函数，对应 Minix3 的 `do_brk()` + `real_brk()`。

```rust
pub(crate) fn handle_brk(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    request: &BrkRequest,
) -> Result<BrkResponse, BrkError> {
    // 阶段1: 验证调用者 — endpoint → slot → ActiveProc 视图
    let slot = table.vm_isokendpt(request.endpoint)
        .map_err(|_| BrkError::ProcessNotFound)?;

    let mut active = table.get_active(slot)
        .ok_or(BrkError::ProcessNotFound)?;

    // 阶段2: 判断操作类型（扩展/收缩/无变化）
    let current_brk = active.region_top();
    let requested = request.new_brk_addr;

    if requested.0 < current_brk.0 {
        shrink_heap(&mut active, page_alloc, frames, requested)
    } else if requested.0 > current_brk.0 {
        grow_heap(&mut active, page_alloc, frames, requested)
    } else {
        Ok(BrkResponse { new_brk_addr: current_brk })
    }
}
```

**与 Minix3 的对应关系**：

| Minix3 | Rust | 说明 |
|--------|------|------|
| `vm_isokendpt(msg->m_source, &proc)` | `table.vm_isokendpt(request.endpoint)` | 验证调用者 endpoint |
| `real_brk(&vmproc[proc], addr)` | `grow_heap` / `shrink_heap` | 执行堆调整 |
| `map_region_extend_upto_v(vmp, v)` | `grow_heap` 扩展现有区域 | 扩展堆区域 |
| `nextvr->vaddr < offset` → ENOMEM | `find_overlap` → `OutOfMemory` | 堆栈冲突检查 |
| `anon_resize` 静默忽略收缩 | `shrink_heap` 真正释放物理页 | 收缩行为差异 |

### 4.3 堆扩展

`grow_heap` 对应 Minix3 的 `map_region_extend_upto_v`，实现方式与 Minix3 对齐：优先扩展现有堆区域，仅在没有可扩展区域时创建新区域。

```rust
fn grow_heap(
    active: &mut ActiveProc<'_>,
    _page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    new_brk: VirBytes,
) -> Result<BrkResponse, BrkError> {
    let current_top = active.region_top();
    let grow_len = new_brk.0 - current_top.0;

    if grow_len == 0 {
        return Ok(BrkResponse { new_brk_addr: current_top });
    }

    let aligned_len = VirBytes(((grow_len + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE);
    let new_end = VirBytes(current_top.0 + aligned_len.0);

    // 区域冲突检查：防止堆扩展侵入栈或其他区域（对应 Minix3 nextvr->vaddr < offset）
    if active.regions().find_overlap(current_top, new_end).is_some() {
        return Err(BrkError::OutOfMemory);
    }

    // 优先扩展现有区域（对应 Minix3 realloc physblocks + anon_resize）
    if let Some(top_region) = active.regions_mut().find_mut(current_top) {
        top_region.extend(aligned_len)
            .map_err(|_| BrkError::OutOfMemory)?;
    } else if let Some(top_region) = active.regions_mut().find_mut_by_end(current_top) {
        // find_mut 要求 contains_addr(addr)，而 addr == end_addr() 不满足。
        // find_mut_by_end 通过 end_addr() == target 查找边界相邻的区域。
        top_region.extend(aligned_len)
            .map_err(|_| BrkError::OutOfMemory)?;
    } else {
        // 首次 brk：没有可扩展的区域，创建新区域
        let new_region = VirRegion::with_memtype(
            current_top,
            aligned_len,
            VrFlags::WRITABLE | VrFlags::ANON,
            &MEM_TYPE_ANON,
        );
        active.regions_mut().insert(new_region);
    }

    active.add_total(aligned_len);
    active.set_region_top(new_end);

    Ok(BrkResponse { new_brk_addr: new_brk })
}
```

> **三路分支说明**：`find_mut` 查找 `contains_addr(addr)` 的区域（适用于常规扩展），`find_mut_by_end` 查找 `end_addr() == target` 的区域（适用于 `addr` 恰好等于区域尾地址的边界情况，此时 `contains_addr` 返回 false）。这是 BTreeMap 模型与 Minix3 AVL 树行为差异的补偿——AVL 树中 `region_search(AVL_LESS)` 能找到 `vaddr <= addr` 的最近区域，而 BTreeMap 的 `range(..=addr).next_back()` 后还需额外的包含性检查。`find_mut_by_end` 方法在 [RegionMap](os/servers/vm/src/region/region_map.rs) 中定义，消除了原有的 `find_less + filter + get_mut` 三次查找。

### 4.4 堆收缩

`shrink_heap` 是 Rust 新增功能，Minix3 的 brk 不支持收缩。实现逻辑：遍历区域树，移除或拆分超出新堆顶的区域。

```rust
fn shrink_heap(
    active: &mut ActiveProc<'_>,
    _page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    new_brk: VirBytes,
) -> Result<BrkResponse, BrkError> {
    let current_top = active.region_top();

    if new_brk.0 >= current_top.0 {
        return Ok(BrkResponse { new_brk_addr: current_top });
    }

    // 收集需要移除和需要拆分的区域
    let mut regions_to_remove = alloc::vec::Vec::new();
    let mut regions_to_shrink = alloc::vec::Vec::new();

    for region in active.regions().iter() {
        if region.vaddr.0 >= new_brk.0 {
            regions_to_remove.push(region.vaddr);
        } else if region.end_addr().0 > new_brk.0 && region.vaddr.0 < new_brk.0 {
            regions_to_shrink.push(region.vaddr);
        }
    }

    // 拆分跨越新堆顶的区域
    for vaddr in regions_to_shrink {
        if let Some(region) = active.regions_mut().remove(vaddr) {
            let split_point = VirBytes(new_brk.0 - region.vaddr.0);
            let region_len = region.length;
            if split_point.0 > 0 && split_point.0 < region_len.0 {
                match region.split(split_point) {
                    Ok((left, right)) => {
                        let freed_len = right.length;
                        {
                            let page_table = active.page_table_mut();
                            free_region_pages(right, page_table, frames, page_alloc);
                        }
                        active.sub_total(VirBytes(freed_len.0));
                        active.regions_mut().insert(left);
                    }
                    Err(_) => {
                        active.sub_total(VirBytes(region_len.0));
                    }
                }
            } else {
                active.regions_mut().insert(region);
            }
        }
    }

    // 移除完全超出新堆顶的区域
    for vaddr in regions_to_remove {
        if let Some(region) = active.regions_mut().remove(vaddr) {
            let freed_len = region.length;
            {
                let page_table = active.page_table_mut();
                free_region_pages(region, page_table, frames, page_alloc);
            }
            active.sub_total(VirBytes(freed_len.0));
        }
    }

    active.set_region_top(new_brk);

    Ok(BrkResponse { new_brk_addr: new_brk })
}
```

**收缩策略**：

1. **完全超出**：区域起始地址 >= 新堆顶 → 整个移除并释放物理页
2. **跨越边界**：区域起始 < 新堆顶 < 区域结束 → `VirRegion::split()` 拆分，保留左半部分，释放右半部分
3. **完全在内**：区域结束 <= 新堆顶 → 不操作

**free_region_pages** — 释放区域物理页

```rust
fn free_region_pages(
    region: VirRegion,
    page_table: &mut PageTable,
    frames: &mut PageFrames,
    page_alloc: &mut VmPageAllocator,
) {
    // 从页表取消映射
    let page_count = (region.length.0 / PAGE_SIZE) as usize;
    for i in 0..page_count {
        let vaddr = VirBytes(region.vaddr.0 + (i as u64) * PAGE_SIZE);
        let _ = page_table.unmap(vaddr);
    }

    // 递减 refcount 并释放物理页（对应 Minix3 pb_unreferenced + free_physical_page）
    let mut region = region;
    let pending = region.free_range(frames, VirBytes(0), region.length);
    for (pfn, mt) in pending {
        mt.ev_unreference(frames, pfn);
        page_alloc.free_pfn(pfn);
    }
}
```

> 收缩策略的设计决策见 §3.6，物理页释放策略见 §3.7。

### 4.5 返回结果

构造 `BrkResponse` 返回给 dispatcher，由 dispatcher 转换为 `VmReply::Brk(VmBrkOut)`。

**成功路径**：`Ok(BrkResponse { new_brk_addr })` → `VmReply::Brk(VmBrkOut { new_addr })` → 编码为 M1 消息返回
**失败路径**：`Err(BrkError)` → `VmReply::Error(VmError)` → 返回错误码

**与 libc 的交互**

libc 的 `brk()` 收到 VM 返回后更新 `_brksize` 全局变量（实现见 §2.1），用户态通过 `_brksize` 获取当前堆顶。

---

## 5. 测试要点

> 本章描述 Rust 实现需要测试的维度和关键场景，而非罗列测试代码。

### 5.1 测试维度

| 维度 | 测试重点 |
|------|---------|
| **正常路径** | 扩展、收缩、无变化、查询堆顶 |
| **边界条件** | 页对齐、零地址、堆顶等于数据段顶 |
| **错误路径** | 无效 endpoint、地址低于数据段、与栈冲突、内存不足 |
| **错误码对齐** | 验证 BrkError 到 errno 的映射与 Minix3 一致 |
| **状态一致性** | 扩展/收缩后 region_top 与区域长度一致 |
| **物理页面** | 收缩后物理页引用计数正确、vm_total 正确更新 |

### 5.2 关键测试场景

**堆扩展**

| 场景 | 输入 | 预期结果 |
|------|------|---------|
| 小扩展（+1 页） | addr = current_brk + PAGE_SIZE | 成功 |
| 大扩展（+N 页） | addr = current_brk + N*PAGE_SIZE | 成功 |
| 非对齐地址 | addr 未页对齐 | 成功（内部向上取整） |
| 连续扩展 | 多次调用 | 每次成功，region_top 递增 |
| 无变化 | addr = current_brk | 成功，无操作 |
| NULL 地址查询 | addr = 0 | 返回当前堆顶 |

**堆收缩**

> **Minix3 行为差异**：Minix3 的 brk 收缩被 `anon_resize` 静默忽略（返回 OK 但不释放内存，见 §2.7.3）。下表测试场景针对 Rust 实现（支持真正收缩），需额外测试 Minix3 兼容模式（收缩返回成功但不操作）。

| 场景 | 初始堆顶 | 目标堆顶 | Rust 预期结果 | Minix3 行为 |
|------|---------|---------|-------------|------------|
| 小收缩 | current_brk | current_brk - PAGE_SIZE | 成功，释放物理页 | 返回成功，不操作 |
| 大收缩 | current_brk | data_top | 成功（堆大小=0） | 返回成功，不操作 |
| 部分收缩 | current_brk | current_brk - N*PAGE_SIZE | 成功，物理页释放 | 返回成功，不操作 |
| 收缩后 vm_total | 有已分配物理页 | 收缩释放区域 | vm_total 减少 | vm_total 不变 |

**错误处理**

| 场景 | 输入 | 预期错误 |
|------|------|---------|
| 无效 endpoint | 不存在的进程 | ProcessNotFound → ESRCH |
| 与栈冲突 | page_align(addr) >= stack_region.vaddr | OutOfMemory → ENOMEM |
| 内存不足 | 请求超过可用物理内存 | OutOfMemory → ENOMEM |
| 区域查找失败 | addr 低于所有区域 | OutOfMemory → ENOMEM |

**错误码对齐验证**

| BrkError 变体 | 期望 errno | 说明 |
|---------------|-----------|------|
| ProcessNotFound | ESRCH | Minix3 do_brk 中 vm_isokendpt 失败返回 EINVAL，Rust 使用更精确的 ESRCH |
| OutOfMemory | ENOMEM | 与 Minix3 real_brk 返回值一致；同时覆盖堆栈冲突场景（对应 Minix3 nextvr->vaddr < offset → ENOMEM）和区域查找失败（对应 Minix3 region_search 找不到区域 → ENOMEM） |

---

## 6. 参见

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
