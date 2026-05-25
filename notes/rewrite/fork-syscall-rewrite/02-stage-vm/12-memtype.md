# 12-memtype: 内存类型系统

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/memtype.h`, `mem_anon.c`  
> **说明**: 多态内存类型系统，支持匿名内存、文件映射、物理内存等不同类型

---

## 1. 概述

### 1.1 内存类型系统的作用

**为什么需要内存类型系统？**

在操作系统中，不同来源和用途的内存有不同的行为特征：

| 内存类型 | 来源 | 分配时机 | 释放时机 | 特殊处理 |
|---------|------|---------|---------|---------|
| **匿名内存** | 按需分配 | 页错误时 | refcount 归零时 | 支持写时复制 |
| **文件映射** | 文件系统 | mmap 时 | munmap 时 | 需同步到磁盘 |
| **共享内存** | vm_remap 创建 | do_remap 时 | 所有映射进程退出/解除映射 | 多进程可见 |
| **物理映射** | 设备寄存器 | 驱动初始化 | 驱动卸载 | 不由 VM 管理 |

**核心问题**：如何用统一的接口管理这些行为各异的内存？

**Minix3 的解决方案**：使用**函数指针表**实现多态，每种内存类型实现一组回调函数。

### 1.2 多态设计

Minix3 使用函数指针表 `mem_type_t` 实现多态，包含 15 个回调字段（完整定义见 §2.1）：

```c
// memtype.h — 示意
typedef struct mem_type {
    const char *name;
    int (*ev_new)(struct vir_region *region);
    void (*ev_delete)(struct vir_region *region);
    int (*ev_reference)(struct phys_region *pr, struct phys_region *newpr);
    int (*ev_unreference)(struct phys_region *pr);
    int (*ev_pagefault)(struct vmproc *vmp, ...);
    // ... 其余 10 个回调
} mem_type_t;
```

**调用方式**：框架层通过 `phys_region.memtype->ev_xxx(...)` 统一调用，具体行为由 memtype 实例决定：

```c
int result = pr->memtype->ev_pagefault(vmp, region, ph, write, ...);
```

### 1.3 与前文的关系：框架与策略的分离

读者可能会疑惑：11-region-mapping 已经讲了 `map_page_region`（创建区域）、`split_region`（分割区域）、`map_pf`（页错误处理）、`map_copy_region`（fork 复制）等函数，为什么这里又出现 `ev_new`、`ev_split`、`ev_pagefault`、`ev_copy` 等同名回调？

**答案是职责分层**：11 文档讲的是**框架**（"何时调用"），12 文档讲的是**策略**（"做什么"）。

以页错误为例，Minix3 的调用链是：

```
map_pf()                              ← 11 文档：框架层
  ├── pb_new(MAP_NONE)                ← 创建空 phys_block
  ├── pb_reference(pb, ...)           ← 创建 phys_region，refcount++
  └── ph->memtype->ev_pagefault(...)  ← 12 文档：策略层
       ├── anon_pagefault()           ← 匿名内存：分配物理页 / CoW
       ├── mappedfile_pagefault()     ← 文件映射：查缓存 / 请求 VFS
       ├── cache_pagefault()          ← 缓存：从缓存索引获取
       └── ...
```

框架层负责**通用流程**（创建数据结构、管理引用计数、处理页表），策略层负责**类型特化行为**（匿名内存分配物理页、文件映射从磁盘读取、缓存从索引查找）。

所有回调都遵循这个模式：

| 回调 | 框架层（11 文档）何时调用 | 策略层（12 文档）做什么 |
|------|------------------------|----------------------|
| `ev_new` | `map_page_region` 创建 vir_region 骨架后 | 类型特化初始化（仅 anon_contig 实现：预分配连续物理页） |
| `ev_delete` | `map_free` 释放 vir_region 前 | 类型特化清理（如 shared_delete 减少 remaps） |
| `ev_reference` | `map_copy_region` fork 复制时，`pb_reference` 之后 | 引用通知（cache 更新索引、anon_contig 拒绝 fork） |
| `ev_unreference` | `pb_unreferenced` refcount 降为 0 时 | 释放物理页（anon 调用 `free_mem`，directphys 不释放） |
| `ev_pagefault` | `map_pf` 页错误时，创建 phys_region 之后 | 填充物理页（anon 分配 / file 读磁盘 / cache 查索引） |
| `ev_resize` | `resize_region` 调整区域大小时 | 类型特化调整（cache/anon_contig 拒绝 resize） |
| `ev_split` | `split_region` 分割区域时 | 类型特化分割（mappedfile 复制 fdref、shared 设置源） |
| `ev_copy` | `map_copy_region` fork 复制区域时 | 类型特化复制（mappedfile 复制文件信息、shared 设置源） |
| `ev_lowshrink` | `map_unmap_region` 从低端收缩时 | 类型特化收缩（cache 释放缓存页） |
| `writable` | `pr_writable` 判断页是否可写时 | 类型特化判断（anon: refcount==1 可写，directphys: 始终可写） |
| `ev_sanitycheck` | `SANITYCHECK` 宏遍历所有区域时 | 类型特化检查（各类型验证自身不变量） |
| `regionid` | `map_regionid` 获取区域标识时 | 类型特化 ID（shared 返回源区域 ID） |
| `refcount` | `map_refcount` 获取引用计数时 | 类型特化计数（shared: 1+remaps） |
| `pt_flags` | `map_page_region` 设置页表标志时 | 类型特化标志（directphys 不缓存、cache 写回） |

**一句话总结**：11 文档讲的是"壳怎么造"（数据结构的创建、引用计数管理、页表操作），12 文档讲的是"壳造好后，特定类型要不要往里面塞东西"（类型特化的初始化、填充、清理逻辑）。

### 1.4 全局内存类型实例

| Minix3 变量 | 用途 | 源文件 |
|-------------|------|--------|
| `mem_type_anon` | 普通堆内存、栈 | `mem_anon.c` |
| `mem_type_directphys` | 设备内存映射 | `mem_directphys.c` |
| `mem_type_anon_contig` | DMA 缓冲区（物理连续） | `mem_anon_contig.c` |
| `mem_type_cache` | 文件系统缓存 | `mem_cache.c` |
| `mem_type_mappedfile` | mmap 文件映射 | `mem_file.c` |
| `mem_type_shared` | 进程间共享内存 | `mem_shared.c` |

### 1.5 内存类型与区域的关系

**全局实例 + 指针引用**：每种内存类型在源码中定义一个全局 `struct mem_type` 实例（如 `mem_type_anon`），`vir_region` 和 `phys_region` 通过 `mem_type_t *` 指针指向这些全局实例。所有同类型的区域共享同一个实例，回调函数通过参数中的 `vir_region`/`phys_region` 区分不同区域，而非依赖实例自身的状态。

**两层 memtype 指针**：

1. **vir_region.def_memtype**：区域的默认内存类型，新分配的物理区域继承此类型
2. **phys_region.memtype**：物理区域的具体类型，可以覆盖默认类型

调用方式：`phys_region.memtype->ev_pagefault(vmp, region, pr, write)`

**设计理由**：

- 同一虚拟区域内的大多数页面使用相同的内存类型
- 少数特殊情况（如文件映射中的部分页面）可以有不同的类型
- 默认类型简化了常见情况的处理

---

## 2. C 源码分析

### 2.1 mem_type_t 结构体

**源码位置**: [`memtype.h`](../../../minix3/minix/servers/vm/memtype.h)

#### 2.1.1 结构体定义

```c
typedef struct mem_type {
    const char *name;                          /* 人类可读的名称 */
    int (*ev_new)(struct vir_region *region);  /* 创建区域回调 */
    void (*ev_delete)(struct vir_region *region); /* 删除区域回调 */
    int (*ev_reference)(struct phys_region *pr, struct phys_region *newpr); /* 引用回调 */
    int (*ev_unreference)(struct phys_region *pr); /* 取消引用回调 */
    int (*ev_pagefault)(struct vmproc *vmp, struct vir_region *region,
         struct phys_region *ph, int write, vfs_callback_t cb, void *state,
         int len, int *io);                    /* 页错误处理回调 */
    int (*ev_resize)(struct vmproc *vmp, struct vir_region *vr, vir_bytes len); /* 调整大小 */
    void (*ev_split)(struct vmproc *vmp, struct vir_region *vr,
            struct vir_region *r1, struct vir_region *r2); /* 分割区域 */
    int (*writable)(struct phys_region *pr);   /* 判断是否可写 */
    int (*ev_sanitycheck)(struct phys_region *pr, const char *file, int line); /* 完整性检查 */
    int (*ev_copy)(struct vir_region *vr, struct vir_region *newvr); /* 复制区域 */
    int (*ev_lowshrink)(struct vir_region *vr, vir_bytes len); /* 收缩低端地址 */
    u32_t (*regionid)(struct vir_region *vr);  /* 获取区域 ID */
    int (*refcount)(struct vir_region *vr);    /* 获取引用计数 */
    int (*pt_flags)(struct vir_region *vr);    /* 页表标志 */
} mem_type_t;
```

#### 2.1.2 NULL 回调的框架层处理

`mem_type_t` 中未赋值的回调为 NULL，框架层对其处理分两种：

| 处理方式 | 回调 | 框架行为 |
|---------|------|---------|
| NULL 时跳过 | `ev_new`, `ev_delete`, `ev_reference`, `ev_unreference`, `ev_copy`, `ev_sanitycheck`, `writable`, `regionid`, `refcount`, `pt_flags` | `if(ptr) ptr(...)`，NULL 则不调用 |
| NULL 时报错 | `ev_split`, `ev_lowshrink` | `if(!ptr) { printf("not implemented"); return EINVAL; }`，NULL 则拒绝操作 |

因此，`ev_split` 和 `ev_lowshrink` 即使无需特化逻辑，也必须注册空函数（如 `anon_split`、`anon_lowshrink`），否则框架会报错。其余回调为 NULL 时框架直接跳过，注册空函数与 NULL 效果相同。

#### 2.1.3 回调函数详解

**生命周期回调**

| 回调 | 触发时机 | 返回值 | 说明 |
|------|---------|--------|------|
| `ev_new` | 创建新虚拟区域 | `OK`/错误码 | 初始化区域类型特定数据 |
| `ev_delete` | 删除虚拟区域 | 无 | 清理类型特定资源 |
| `ev_copy` | 复制区域（fork） | `OK`/错误码 | 复制类型特定参数 |
| `ev_split` | 分割区域 | 无 | 处理区域分割后的状态 |

**物理内存回调**

| 回调 | 触发时机 | 返回值 | 说明 |
|------|---------|--------|------|
| `ev_reference` | 增加物理块引用 | `OK`/错误码 | CoW 时复制引用信息 |
| `ev_unreference` | 减少物理块引用 | `OK`/错误码 | 引用为 0 时释放内存 |

**页错误处理**

```c
int (*ev_pagefault)(
    struct vmproc *vmp,        // 发生错误的进程
    struct vir_region *region, // 所属虚拟区域
    struct phys_region *ph,    // 物理区域
    int write,                 // 是否写操作
    vfs_callback_t cb,         // VFS 回调（文件映射用）
    void *state,               // 回调状态
    int len,                   // 数据长度
    int *io                    // I/O 计数
);
```

**查询回调**

| 回调 | 用途 | 典型实现 |
|------|------|---------|
| `writable` | 判断物理区域是否可写 | 匿名内存：`refcount == 1` |
| `refcount` | 获取区域引用计数 | 匿名内存：`1 + remaps` |
| `regionid` | 获取区域唯一标识 | 返回 `region->id` |
| `pt_flags` | 获取页表标志 | ARM: 缓存/设备标志 |

**调整回调**

| 回调 | 触发时机 | 说明 |
|------|---------|------|
| `ev_resize` | `brk()` 扩展堆 | 匿名内存允许扩展 |
| `ev_lowshrink` | 收缩区域低端 | 释放不再使用的页面 |

**调试回调**

| 回调 | 用途 |
|------|------|
| `ev_sanitycheck` | 运行时完整性检查 |

#### 2.1.3 调用约定

**可选回调**

所有回调都是可选的（可为 `NULL`）。调用前需检查：

```c
// region.c:485
if (newregion->def_memtype->ev_new) {
    if (newregion->def_memtype->ev_new(newregion) != OK) {
        // 处理错误
    }
}
```

#### 2.1.4 多态接口设计

**接口统一性**

不同内存类型通过相同的接口实现不同行为：

```c
// 统一调用，不同行为
int result = pr->memtype->ev_pagefault(vmp, region, ph, write, ...);

// 匿名内存：分配新页或 CoW
// 物理映射：计算物理地址
// 文件映射：从文件读取
// 共享内存：查找源区域
```

**扩展性**

添加新内存类型只需：
1. 定义新的 `mem_type_t` 实例
2. 实现需要的回调函数
3. 在创建区域时指定类型

无需修改现有代码，符合开闭原则。

### 2.2 内存类型实例

#### 2.2.1 mem_type_anon - 匿名内存

**源码位置**: [`mem_anon.c`](../../../minix3/minix/servers/vm/mem_anon.c)

**什么是匿名内存？**

匿名内存是进程私有、不与任何文件关联的内存。典型用途：
- **堆内存**：`brk()`/`sbrk()` 分配
- **栈内存**：进程栈
- **mmap 匿名映射**：`MAP_ANONYMOUS`

**核心特点**：
1. **按需分配**：首次访问时才分配物理页
2. **写时复制 (CoW)**：fork 后共享物理页，写入时复制
3. **进程私有**：不与其他进程共享

**回调函数实现**（未列出即为 NULL，框架层跳过调用，下同）

| 回调 | 实现函数 | 功能 |
|------|---------|------|
| `ev_unreference` | `anon_unreference` | 引用为 0 时释放物理页 |
| `ev_pagefault` | `anon_pagefault` | 按需分配或 CoW |
| `ev_resize` | `anon_resize` | 允许扩展（收缩忽略） |
| `ev_sanitycheck` | `anon_sanitycheck` | 断言物理页已登记 |
| `ev_split` | `anon_split` | 空操作（必须注册，否则框架报错） |
| `ev_lowshrink` | `anon_lowshrink` | 空操作（必须注册，否则框架报错） |
| `writable` | `anon_writable` | refcount==1 且物理页存在则可写 |
| `regionid` | `anon_regionid` | 返回区域 ID |
| `refcount` | `anon_refcount` | 返回 `1 + remaps` |
| `pt_flags` | `anon_pt_flags` | ARM 返回缓存标志 |

**关键函数分析**

**anon_pagefault - 页错误处理**

```c
static int anon_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb, void *state,
    int len, int *io)
{
    phys_bytes new_page, new_page_cl;
    u32_t allocflags;

    allocflags = vrallocflags(region->flags);
    assert(ph->ph->refcount > 0);

    // 预分配一页（可能用于 CoW）
    if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) {
        printf("anon_pagefault: out of memory\n");
        return ENOMEM;
    }
    new_page = CLICK2ABS(new_page_cl);

    // 情况 1: 全新页面，从未分配
    if(ph->ph->phys == MAP_NONE) {
        ph->ph->phys = new_page;
        assert(ph->ph->phys != MAP_NONE);
        return OK;
    }

    // 情况 2: 只有一个引用，或非写操作
    if(ph->ph->refcount < 2 || !write) {
        /* memory is ready already */
        return OK;
    }

    // 情况 3: 多引用 + 写操作 = 需要 CoW
    assert(region->flags & VR_WRITABLE);
    return mem_cow(region, ph, new_page_cl, new_page);
}
```

**三种处理路径**：

1. **全新页面**（`phys == MAP_NONE`）：直接使用预分配的新页，返回 OK
2. **无需 CoW**（`refcount < 2` 或非写操作）：释放预分配页，返回 OK（页面已就绪）
3. **需要 CoW**（`refcount >= 2` 且写操作）：断言 `VR_WRITABLE`，调用 `mem_cow()` 执行写时复制

**anon_writable - 可写判断**

```c
static int anon_writable(struct phys_region *pr)
{
    assert(pr->ph->refcount > 0);
    
    // 未分配物理页，不可写
    if(pr->ph->phys == MAP_NONE)
        return 0;
    
    // 有 remaps（共享内存映射），可写
    if(pr->parent->remaps > 0)
        return 1;
    
    // 只有一个引用时才可写（CoW 语义）
    return pr->ph->refcount == 1;
}
```

**CoW 可写条件**：
- 物理页已分配
- 只有一个引用者（`refcount == 1`）
- 或者有 `remaps`（重映射）

**anon_unreference - 释放物理页**

```c
static int anon_unreference(struct phys_region *pr)
{
    // 前置条件：引用计数已为 0
    assert(pr->ph->refcount == 0);
    
    // 释放物理页
    if(pr->ph->phys != MAP_NONE)
        free_mem(ABS2CLICK(pr->ph->phys), 1);
    
    return OK;
}
```

**anon_resize - 区域扩展**

```c
static int anon_resize(struct vmproc *vmp, struct vir_region *vr, vir_bytes l)
{
    // 收缩操作忽略（对 brk() 安全）
    if(l <= vr->length)
        return OK;

    assert(vr);
    assert(vr->flags & VR_ANON);
    assert(!(l % VM_PAGE_SIZE));

    USE(vr, vr->length = l;);

    return OK;
}
```

**全局实例定义**

```c
// glo.h
EXTERN mem_type_t mem_type_anon;

// mem_anon.c
struct mem_type mem_type_anon = {
    .name = "anonymous memory",
    .ev_unreference = anon_unreference,
    .ev_pagefault = anon_pagefault,
    .ev_resize = anon_resize,
    .ev_sanitycheck = anon_sanitycheck,
    .ev_lowshrink = anon_lowshrink,
    .ev_split = anon_split,
    .regionid = anon_regionid,
    .writable = anon_writable,
    .refcount = anon_refcount,
    .pt_flags = anon_pt_flags,
};
```

**未实现的回调**

以下回调匿名内存不需要，保持 `NULL`：
- `ev_new`：无需初始化
- `ev_delete`：无需清理
- `ev_reference`：默认引用行为足够
- `ev_copy`：默认复制行为足够

#### 2.2.2 mem_type_directphys - 直接物理映射

**源码位置**: [`mem_directphys.c`](../../../minix3/minix/servers/vm/mem_directphys.c)

**什么是直接物理映射？**

直接物理映射允许进程直接访问指定的物理内存地址，VM 不负责分配或释放这些内存。典型用途：
- **设备寄存器映射**：GPU、网卡等设备的 MMIO 区域
- **DMA 缓冲区**：与设备共享的物理内存
- **物理内存观察**：调试工具直接查看物理内存

**核心特点**：
1. **不分配内存**：物理地址由调用者提供
2. **不释放内存**：取消映射时不释放物理页
3. **直接映射**：虚拟地址直接映射到指定物理地址

**与匿名内存的对比**

| 特性 | 匿名内存 | 直接物理映射 |
|------|---------|-------------|
| 物理页来源 | VM 按需分配 | 调用者提供 |
| 页错误处理 | 分配新页或 CoW | 计算物理地址 |
| 取消映射 | 释放物理页 | 不释放 |
| CoW 支持 | 支持 | 不支持 |
| 典型用途 | 堆、栈 | 设备内存 |

**回调函数实现**（仅 5 个非 NULL 回调）

| 回调 | 实现函数 | 功能 |
|------|---------|------|
| `ev_copy` | `phys_copy` | 复制物理地址参数 |
| `ev_unreference` | `phys_unreference` | 空操作（不释放） |
| `ev_pagefault` | `phys_pagefault` | 计算物理地址 |
| `writable` | `phys_writable` | 检查物理地址是否已设置 |
| `pt_flags` | `phys_pt_flags` | ARM 返回设备标志 |

**关键函数分析**

**phys_pagefault - 页错误处理**

```c
static int phys_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb, void *state,
    int len, int *io)
{
    phys_bytes arg = region->param.phys, phmem;
    
    // 前置条件检查
    assert(arg != MAP_NONE);          // 区域有物理基地址
    assert(ph->ph->phys == MAP_NONE); // 物理块尚未映射
    
    // 计算物理地址 = 基地址 + 页内偏移
    phmem = arg + ph->offset;
    assert(phmem != MAP_NONE);
    
    // 直接设置物理地址，无需分配
    ph->ph->phys = phmem;
    return OK;
}
```

**工作原理**：

物理地址计算方式：`phmem = region->param.phys + ph->offset`。即虚拟区域保存物理基地址（`param.phys`），每个 `phys_region` 的物理地址等于基地址加上该区域的页内偏移。例如，`param.phys = 0xFEC00000`，则 offset 为 0x1000 的页面映射到物理地址 0xFEC01000。

**phys_writable - 可写判断**

```c
static int phys_writable(struct phys_region *pr)
{
    assert(pr->ph->refcount > 0);
    // 物理地址已设置即可写
    return pr->ph->phys != MAP_NONE;
}
```

**phys_unreference - 空操作**

```c
static int phys_unreference(struct phys_region *pr)
{
    // 不释放物理内存，因为不由 VM 管理
    return OK;
}
```

**phys_copy - 复制区域**

```c
static int phys_copy(struct vir_region *vr, struct vir_region *newvr)
{
    // 复制物理基地址参数
    newvr->param.phys = vr->param.phys;
    return OK;
}
```

**辅助函数**

```c
// 设置区域的物理基地址
void phys_setphys(struct vir_region *vr, phys_bytes phys)
{
    vr->param.phys = phys;
}
```

**全局实例定义**

```c
struct mem_type mem_type_directphys = {
    .name = "physical memory mapping",
    .ev_copy = phys_copy,
    .ev_unreference = phys_unreference,
    .writable = phys_writable,
    .ev_pagefault = phys_pagefault,
    .pt_flags = phys_pt_flags,
};
```

**页表标志**

```c
static int phys_pt_flags(struct vir_region *vr)
{
#if defined(__arm__)
    // ARM 架构：设备内存需要特殊标志
    // 禁用缓存，使用设备内存属性
    return ARM_VM_PTE_DEVICE;
#else
    // x86: 默认标志即可
    return 0;
#endif
}
```

**硬件差异处理**：

| 架构 | 页表标志 | 说明 |
|------|---------|------|
| x86 | 0 | 使用默认的写组合或 UC 类型 |
| ARM | `ARM_VM_PTE_DEVICE` | 设备内存，禁用缓存 |

**使用示例**

```c
// 映射设备寄存器（假设地址 0xFEC00000，长度 64KB）
struct vir_region *vr;
vr = region_new(vaddr, 0x10000, VR_DIRECT);
phys_setphys(vr, 0xFEC00000);
vr->def_memtype = &mem_type_directphys;

// 进程访问 vaddr 时触发页错误
// phys_pagefault 计算物理地址并建立映射
```

**未实现的回调**

以下回调直接物理映射不需要：
- `ev_new`：无需初始化
- `ev_delete`：无需清理
- `ev_reference`：无特殊引用处理
- `ev_resize`：不支持调整大小
- `ev_split`：无分割处理
- `regionid`：使用默认
- `refcount`：使用默认

#### 2.2.3 mem_type_anon_contig - 连续匿名内存

**源码位置**: [`mem_anon_contig.c`](../../../minix3/minix/servers/vm/mem_anon_contig.c)

**什么是物理连续匿名内存？**

物理连续匿名内存是一种特殊的匿名内存，要求所有物理页在物理地址空间中连续。典型用途：
- **DMA 缓冲区**：设备 DMA 需要物理连续的内存
- **硬件描述符表**：某些硬件要求连续的描述符表
- **高性能 I/O**：减少 IOMMU 映射开销

**核心特点**：
1. **创建时全部分配**：`ev_new` 中一次性分配所有物理页
2. **物理地址连续**：所有页面物理地址连续
3. **不支持 fork**：`ev_reference` 返回错误
4. **不支持 resize**：无法扩展或收缩

**与普通匿名内存的对比**

| 特性 | 普通匿名内存 | 连续匿名内存 |
|------|-------------|-------------|
| 分配时机 | 页错误时按需分配 | 创建时一次性分配 |
| 物理连续性 | 不保证 | 保证 |
| fork 支持 | 支持（CoW） | 不支持 |
| resize 支持 | 支持 | 不支持 |
| 页错误 | 正常处理 | panic（不应发生） |
| 分配复杂度 | O(1) 每页 | O(n) 整体 |

**回调函数实现**（仅 9 个非 NULL 回调）

| 回调 | 实现函数 | 功能 |
|------|---------|------|
| `ev_new` | `anon_contig_new` | 一次性分配所有物理页 |
| `ev_reference` | `anon_contig_reference` | 返回错误（不支持 fork） |
| `ev_unreference` | `anon_contig_unreference` | 复用匿名内存实现 |
| `ev_pagefault` | `anon_contig_pagefault` | panic（不应发生） |
| `ev_resize` | `anon_contig_resize` | 返回错误 |
| `ev_split` | `anon_contig_split` | 空操作（必须注册，否则框架报错） |
| `ev_sanitycheck` | `anon_contig_sanitycheck` | 断言物理页已登记 |
| `writable` | `anon_contig_writable` | 复用匿名内存实现 |
| `pt_flags` | `anon_contig_pt_flags` | ARM 返回设备标志 |

**关键函数分析**

**anon_contig_new - 创建时分配**

```c
static int anon_contig_new(struct vir_region *region)
{
    u32_t allocflags;
    phys_bytes new_pages, new_page_cl, cur_ph;
    phys_bytes p, pages;

    allocflags = vrallocflags(region->flags);
    pages = region->length / VM_PAGE_SIZE;

    // 步骤 1: 创建所有 phys_block 和 phys_region
    for(p = 0; p < pages; p++) {
        struct phys_block *pb = pb_new(MAP_NONE);
        struct phys_region *pr = NULL;
        if(pb)
            pr = pb_reference(pb, p * VM_PAGE_SIZE, region, &mem_type_anon_contig);
        if(!pr) {
            if(pb) pb_free(pb);
            map_free(region);
            return ENOMEM;
        }
    }

    // 步骤 2: 一次性分配连续物理内存
    if((new_page_cl = alloc_mem(pages, allocflags)) == NO_MEM) {
        map_free(region);
        return ENOMEM;
    }

    cur_ph = new_pages = CLICK2ABS(new_page_cl);

    // 步骤 3: 设置每个页面的物理地址
    for(p = 0; p < pages; p++) {
        struct phys_region *pr = physblock_get(region, p * VM_PAGE_SIZE);
        assert(pr);
        assert(pr->ph);
        assert(pr->ph->phys == MAP_NONE);
        pr->ph->phys = cur_ph + pr->offset;  // 连续地址
    }

    return OK;
}
```

**分配流程**：

1. **创建 phys_block 结构**：为每个页面创建 `phys_block`（phys = MAP_NONE）和对应的 `phys_region`，通过 `pb_reference` 关联
2. **一次性分配连续物理内存**：调用 `alloc_mem(pages, allocflags)` 分配 `pages` 个连续 click
3. **设置每个页面的物理地址**：遍历所有 `phys_region`，设置 `pr->ph->phys = cur_ph + pr->offset`，确保物理地址连续

**anon_contig_pagefault - 不应发生**

```c
static int anon_contig_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb, void *state,
    int len, int *io)
{
    // 创建时已全部分配，不应发生页错误
    panic("anon_contig_pagefault: pagefault cannot happen");
}
```

**设计理由**：创建时已分配所有物理页，页错误不应发生。如果发生，说明有 bug。

**anon_contig_reference - 不支持 fork**

```c
static int anon_contig_reference(struct phys_region *pr,
    struct phys_region *newpr)
{
    printf("VM: cannot fork with physically contig memory.\n");
    return ENOMEM;
}
```

**为什么不支持 fork？**
- fork 需要复制物理区域引用
- 物理连续内存在 fork 后可能被 CoW 分裂
- 分裂后不再保证物理连续性

**anon_contig_resize - 不支持调整大小**

```c
static int anon_contig_resize(struct vmproc *vmp, struct vir_region *vr, vir_bytes l)
{
    printf("VM: cannot resize physically contiguous memory.\n");
    return ENOMEM;
}
```

**复用匿名内存实现**

```c
// 取消引用：直接调用匿名内存的实现
static int anon_contig_unreference(struct phys_region *pr)
{
    return mem_type_anon.ev_unreference(pr);
}

// 可写判断：直接调用匿名内存的实现
static int anon_contig_writable(struct phys_region *pr)
{
    return mem_type_anon.writable(pr);
}
```

**全局实例定义**

```c
struct mem_type mem_type_anon_contig = {
    .name = "anonymous memory (physically contiguous)",
    .ev_new = anon_contig_new,
    .ev_reference = anon_contig_reference,
    .ev_unreference = anon_contig_unreference,
    .ev_pagefault = anon_contig_pagefault,
    .ev_resize = anon_contig_resize,
    .ev_split = anon_contig_split,
    .ev_sanitycheck = anon_contig_sanitycheck,
    .writable = anon_contig_writable,
    .pt_flags = anon_contig_pt_flags,
};
```

**使用场景**

```c
// 驱动程序申请 DMA 缓冲区
// 通过 mmap 系统调用，指定 MAP_CONTIG 标志
void *dma_buffer = mmap(NULL, size, PROT_READ | PROT_WRITE,
                        MAP_ANONYMOUS | MAP_CONTIG, -1, 0);

// 内核处理
if (flags & MAP_CONTIG) {
    mt = &mem_type_anon_contig;  // 使用连续匿名内存类型
}
```

**限制总结**

| 操作 | 支持 | 原因 |
|------|------|------|
| 创建 | ✅ | 一次性分配连续物理内存 |
| 访问 | ✅ | 已映射，无页错误 |
| fork | ❌ | 会破坏物理连续性 |
| resize | ❌ | 无法保证扩展后连续 |
| CoW | ❌ | 会破坏物理连续性 |

#### 2.2.4 mem_type_cache - 磁盘缓存

**源码位置**: [`mem_cache.c`](../../../minix3/minix/servers/vm/mem_cache.c)

**什么是磁盘缓存？**

磁盘缓存是 VM 管理的页面缓存，用于加速文件系统 I/O。典型用途：
- **文件系统块缓存**：缓存磁盘块内容
- **元数据缓存**：inode、目录项等
- **读写缓冲**：文件系统读写操作的缓冲区

**核心特点**：
1. **由 VM 统一管理**：所有缓存页面在 VM 的数据结构中
2. **可被文件系统映射**：文件系统通过 `do_mapcache` 映射缓存
3. **可转换为缓存类型**：普通匿名内存可转为缓存类型
4. **按设备/偏移索引**：通过设备号和偏移量查找缓存页

**缓存数据结构**

```c
// cache.h
struct cached_page {
    dev_t dev;                  // 设备号
    u64_t dev_offset;           // 设备偏移
    ino_t ino;                  // inode 号
    u64_t ino_offset;           // inode 偏移
    int flags;                  // 标志（VMSF_ONCE 或 0）
    struct phys_block *page;    // 物理块
    struct cached_page *older;  // LRU 链表（较旧）
    struct cached_page *newer;  // LRU 链表（较新）
    struct cached_page *hash_next_dev; // 哈希链表（按设备）
    struct cached_page *hash_next_ino; // 哈希链表（按 inode）
};
```

**回调函数实现**（仅 8 个非 NULL 回调）

| 回调 | 实现函数 | 功能 |
|------|---------|------|
| `ev_reference` | `cache_reference` | 空操作（无需特化） |
| `ev_unreference` | `cache_unreference` | 复用匿名内存实现 |
| `ev_pagefault` | `cache_pagefault` | 链接预分配的缓存块 |
| `ev_resize` | `cache_resize` | 返回错误 |
| `ev_lowshrink` | `cache_lowshrink` | 空操作（必须注册，否则框架报错） |
| `ev_sanitycheck` | `cache_sanitycheck` | 断言物理页已登记 |
| `writable` | `cache_writable` | 物理页已分配则可写 |
| `pt_flags` | `cache_pt_flags` | ARM 返回缓存标志 |

**关键函数分析**

**cache_pagefault - 页错误处理**

```c
static int cache_pagefault(struct vmproc *vmp, struct vir_region *region, 
    struct phys_region *ph, int write, vfs_callback_t cb,
    void *state, int len, int *io)
{
    vir_bytes offset = ph->offset;
    
    // 前置条件
    assert(ph->ph->phys == MAP_NONE);      // 物理块未映射
    assert(region->param.pb_cache);        // 有预分配的缓存块
    
    // 解除原有链接
    pb_unreferenced(region, ph, 0);
    
    // 链接缓存块到 phys_region
    pb_link(ph, region->param.pb_cache, offset, region);
    
    // 清除临时指针
    region->param.pb_cache = NULL;

    return OK;
}
```

**工作原理**：

1. `do_mapcache` 调用时：查找 `cached_page`（`find_cached_page_bydev`），设置 `region->param.pb_cache = cached_page->page`，触发页错误 `map_pf()`
2. `cache_pagefault` 执行：调用 `pb_unreferenced()` 解除原有链接，调用 `pb_link()` 将缓存块链接到 `phys_region`，清除 `param.pb_cache`
3. 结果：`phys_region.ph` 指向缓存的物理块

**cache_writable - 可写判断**

```c
static int cache_writable(struct phys_region *pr)
{
    // 缓存块目前只被文件系统使用，物理页已分配即可写
    assert(pr->ph->refcount > 0);
    return pr->ph->phys != MAP_NONE;
}
```

**cache_unreference - 复用匿名内存**

```c
static int cache_unreference(struct phys_region *pr)
{
    // 直接调用匿名内存的实现
    return mem_type_anon.ev_unreference(pr);
}
```

**cache_resize - 不支持调整大小**

```c
static int cache_resize(struct vmproc *vmp, struct vir_region *vr, vir_bytes l)
{
    printf("VM: cannot resize cache blocks.\n");
    return ENOMEM;
}
```

**系统调用接口**

**do_mapcache - 映射缓存到文件系统**

```c
int do_mapcache(message *msg)
{
    dev_t dev = msg->m_vmmcp.dev;
    uint64_t dev_off = msg->m_vmmcp.dev_offset;
    
    // 1. 在调用者地址空间分配区域
    vr = map_page_region(caller, VM_MMAPBASE, VM_MMAPTOP,
        alloc_bytes, VR_ANON | VR_WRITABLE, 0, &mem_type_cache);
    
    // 2. 对每个页面
    for(offset = 0; offset < bytes; offset += VM_PAGE_SIZE) {
        // 查找缓存页
        hb = find_cached_page_bydev(dev, dev_off + offset,
            msg->m_vmmcp.ino, ino_off + offset, 1);
        
        // 缓存页不存在或标记为 VMSF_ONCE（一次性缓存），返回 ENOENT
        if(!hb || (hb->flags & VMSF_ONCE)) {
            map_unmap_region(caller, vr, 0, bytes);
            return ENOENT;
        }
        
        // 设置临时指针
        vr->param.pb_cache = hb->page;
        
        // 触发页错误，链接缓存块
        map_pf(caller, vr, offset, 1, NULL, NULL, 0, &io);
    }
    
    // 返回映射地址
    msg->m_vmmcp_reply.addr = (void *) vr->vaddr;
    return OK;
}
```

**do_setcache - 将内存标记为缓存**

```c
int do_setcache(message *msg)
{
    // 遍历指定范围的每个页面
    for(offset = 0; offset < bytes; offset += VM_PAGE_SIZE) {
        // 查找虚拟区域
        region = map_lookup(caller, v, &phys_region);
        
        // 去重：检查是否已有缓存条目
        if((hb = find_cached_page_bydev(dev, dev_off + offset,
            msg->m_vmmcp.ino, ino_off + offset, 1))) {
            if(hb->page != phys_region->ph || (hb->flags & VMSF_ONCE)) {
                // 旧缓存条目已过时，移除
                rmcache(hb);
            } else {
                // 缓存条目已存在且有效，跳过
                continue;
            }
        }
        
        // 检查类型必须是匿名内存
        if(phys_region->memtype != &mem_type_anon &&
           phys_region->memtype != &mem_type_anon_contig) {
            return EFAULT;
        }
        
        // 检查引用计数必须为 1（独占访问）
        if(phys_region->ph->refcount != 1) {
            return EFAULT;
        }
        
        // 更改内存类型为缓存
        phys_region->memtype = &mem_type_cache;
        
        // 添加到缓存索引
        addcache(dev, dev_off + offset, ino, ino_off, flags, phys_region->ph);
    }
    
    return OK;
}
```

**全局实例定义**

```c
struct mem_type mem_type_cache = {
    .name = "cache memory",
    .ev_reference = cache_reference,
    .ev_unreference = cache_unreference,
    .ev_resize = cache_resize,
    .ev_lowshrink = cache_lowshrink,
    .ev_sanitycheck = cache_sanitycheck,
    .ev_pagefault = cache_pagefault,
    .writable = cache_writable,
    .pt_flags = cache_pt_flags,
};
```

**缓存生命周期**：

1. **创建**：文件系统分配匿名内存 → `do_setcache` → 类型改为 cache
2. **使用**：文件系统读写 → `do_mapcache` → 映射到地址空间
3. **回收**：解除映射 → `ev_unreference` → 引用为 0 时释放
4. **失效**：`do_forgetcache` / `do_clearcache` → 从缓存索引移除

**do_forgetcache - 使指定范围的缓存失效**

```c
int do_forgetcache(message *msg)
{
    dev_t dev = msg->m_vmmcp.dev;
    uint64_t dev_off = msg->m_vmmcp.dev_offset;
    phys_bytes bytes = msg->m_vmmcp.pages * VM_PAGE_SIZE;

    for(offset = 0; offset < bytes; offset += VM_PAGE_SIZE) {
        // 按 (dev, dev_offset) 查找缓存页
        if((hb = find_cached_page_bydev(dev, dev_off + offset,
            VMC_NO_INODE, 0, 0)) != NULL)
            rmcache(hb);  // 从哈希表和 LRU 中移除，refcount--
    }

    return OK;
}
```

**do_clearcache - 使指定设备的全部缓存失效**

```c
int do_clearcache(message *msg)
{
    dev_t dev = msg->m_vmmcp.dev;

    // 遍历哈希表，移除该设备的所有缓存页
    clear_cache_bydev(dev);

    return OK;
}
```

`rmcache` 的核心逻辑（被 `do_forgetcache`、`do_clearcache`、`cache_freepages` 共用）：

1. 清除 `phys_block` 的 `PBF_INCACHE` 标志
2. 从 `cache_hash_bydev` 和 `cache_hash_byino` 哈希表中移除
3. `phys_block->refcount--`（因为缓存索引本身持有一个引用）
4. 从 LRU 链表中移除
5. 若 `refcount == 0`，释放物理页（`free_mem`）和 `phys_block`
6. 释放 `cached_page` 结构体

**与其他内存类型的关系**

| 操作 | 匿名内存 → 缓存 | 缓存 → 匿名内存 |
|------|----------------|----------------|
| 转换 | `do_setcache` | 不支持 |
| 原因 | 文件系统需要缓存 | 缓存应保持一致性 |

**限制总结**

| 操作 | 支持 | 说明 |
|------|------|------|
| 创建 | ✅ | 通过 `do_setcache` 从匿名内存转换 |
| 映射 | ✅ | 通过 `do_mapcache` 映射到文件系统 |
| fork | ✅ | `ev_reference` 返回 OK |
| resize | ❌ | 缓存大小固定 |
| 写入 | ✅ | 物理页已分配则可写 |

#### 2.2.5 mem_type_mappedfile - 文件映射

**源码位置**: [`mem_file.c`](../../../minix3/minix/servers/vm/mem_file.c)

**什么是文件映射？**

文件映射是将文件内容映射到进程地址空间的机制，通过 `mmap()` 系统调用实现。典型用途：
- **文件 I/O**：通过内存访问文件内容
- **共享库加载**：动态链接器加载 `.so` 文件
- **内存映射 I/O**：高效的大文件处理
- **进程间共享**：共享文件映射实现 IPC

**核心特点**：
1. **延迟加载**：页错误时从文件系统读取数据
2. **缓存复用**：优先使用已有的缓存页
3. **写时复制**：写入时转为匿名内存
4. **文件关联**：通过 `fdref` 维护文件引用

**文件映射数据结构**

```c
// vir_region->param.file
struct {
    int inited;             // 是否已初始化
    struct fdref *fdref;    // 文件描述符引用
    u64_t offset;           // 文件偏移
    u16_t clearend;         // 末尾清理字节数
};

// fdref - 文件描述符引用
struct fdref {
    int fd;                 // 文件描述符
    int refcount;           // 引用计数
    dev_t dev;              // 设备号
    ino_t ino;              // inode 号
    struct fdref *next;     // 全局链表
    int counting;           // 完整性检查标志
};
```

**回调函数实现**（仅 9 个非 NULL 回调）

| 回调 | 实现函数 | 功能 |
|------|---------|------|
| `ev_unreference` | `mappedfile_unreference` | 释放物理页 |
| `ev_pagefault` | `mappedfile_pagefault` | 从缓存或文件系统加载页面 |
| `ev_sanitycheck` | `mappedfile_sanitycheck` | 验证物理页 |
| `ev_copy` | `mappedfile_copy` | fork 时复制文件映射信息 |
| `ev_split` | `mappedfile_split` | 分割区域时调整偏移 |
| `ev_lowshrink` | `mappedfile_lowshrink` | 收缩时调整偏移 |
| `ev_delete` | `mappedfile_delete` | 释放 fdref 引用 |
| `writable` | `mappedfile_writable` | 返回 0（不可直接写） |
| `pt_flags` | `mappedfile_pt_flags` | ARM 返回缓存标志 |

**关键函数分析**

**mappedfile_pagefault - 页错误处理**

```c
// mem_file.c:84-165 (简化，保留核心逻辑)
static int mappedfile_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb,
    void *state, int statelen, int *io)
{
    u32_t allocflags;
    int procfd = region->param.file.fdref->fd;

    allocflags = vrallocflags(region->flags);

    assert(ph->ph->refcount > 0);
    assert(region->param.file.inited);
    assert(region->param.file.fdref);
    assert(region->param.file.fdref->dev != NO_DEV);

    if(ph->ph->phys == MAP_NONE) {
        struct cached_page *cp;
        u64_t referenced_offset =
            region->param.file.offset + ph->offset;
        if(region->param.file.fdref->ino == VMC_NO_INODE) {
            cp = find_cached_page_bydev(region->param.file.fdref->dev,
                referenced_offset, VMC_NO_INODE, 0, 1);
        } else {
            cp = find_cached_page_byino(region->param.file.fdref->dev,
                region->param.file.fdref->ino, referenced_offset, 1);
        }

        if(cp && (!cb || !(cp->flags & VMSF_ONCE))) {
            int result = OK;
            pb_unreferenced(region, ph, 0);
            pb_link(ph, cp->page, ph->offset, region);

            if(roundup(ph->offset+region->param.file.clearend,
                VM_PAGE_SIZE) >= region->length) {
                result = cow_block(vmp, region, ph,
                    region->param.file.clearend);
            } else if(result == OK && write) {
                result = cow_block(vmp, region, ph, 0);
            }

            if (result == OK && (cp->flags & VMSF_ONCE))
                rmcache(cp);

            return result;
        }

        if(!cb) {
            return EFAULT;
        }

        if(vfs_request(VMVFSREQ_FDIO, procfd, vmp, referenced_offset,
            VM_PAGE_SIZE, cb, NULL, state, statelen) != OK) {
            printf("VM: mappedfile_pagefault: vfs_request failed\n");
            return ENOMEM;
        }
        *io = 1;
        return SUSPEND;
    }

    if(!write) {
        return OK;
    }

    return cow_block(vmp, region, ph, 0);
}
```

**页错误处理流程**：

1. **物理页不存在**（`phys == MAP_NONE`）：
   - 查找缓存页（`find_cached_page_bydev` 或 `find_cached_page_byino`）
   - 缓存命中：链接缓存页（`pb_link`），若需要写入或末尾清理则执行 `cow_block`，否则返回 OK
   - 缓存未命中且无回调：返回 `EFAULT`
   - 缓存未命中且有回调：向 VFS 请求（`vfs_request`），返回 `SUSPEND`——此返回值挂起当前进程，VFS 完成回调后 VM 恢复处理该页错误
2. **物理页存在且非写操作**：返回 OK
3. **物理页存在且写操作**：执行 `cow_block`，转为匿名内存

**mappedfile_writable - 不可直接写**

```c
static int mappedfile_writable(struct phys_region *pr)
{
    // 文件映射不可直接写入，必须通过 COW
    return 0;
}
```

**mappedfile_copy - fork 复制**

```c
int mappedfile_copy(struct vir_region *vr, struct vir_region *newvr)
{
    // 复制文件映射信息到新区域
    mappedfile_setfile(newvr->parent, newvr, vr->param.file.fdref->fd,
        vr->param.file.offset,
        vr->param.file.fdref->dev, vr->param.file.fdref->ino,
        vr->param.file.clearend, 0, 0);
    
    return OK;
}
```

**mappedfile_split - 区域分割**

```c
static void mappedfile_split(struct vmproc *vmp, struct vir_region *vr,
    struct vir_region *r1, struct vir_region *r2)
{
    // 复制文件信息
    r1->param.file = vr->param.file;
    r2->param.file = vr->param.file;

    // 增加引用计数
    fdref_ref(vr->param.file.fdref, r1);
    fdref_ref(vr->param.file.fdref, r2);

    // 调整偏移
    r1->param.file.clearend = 0;          // 前半部分无末尾清理
    r2->param.file.offset += r1->length;  // 后半部分偏移增加
}
```

**全局实例定义**

```c
struct mem_type mem_type_mappedfile = {
    .name = "file-mapped memory",
    .ev_unreference = mappedfile_unreference,
    .ev_pagefault = mappedfile_pagefault,
    .ev_sanitycheck = mappedfile_sanitycheck,
    .ev_copy = mappedfile_copy,
    .writable = mappedfile_writable,
    .ev_split = mappedfile_split,
    .ev_lowshrink = mappedfile_lowshrink,
    .ev_delete = mappedfile_delete,
    .pt_flags = mappedfile_pt_flags,
};
```

**mmap 系统调用流程**：

1. 用户进程调用 `mmap(addr, len, prot, flags, fd, offset)`
2. VM 的 `do_mmap()` 验证参数，向 VFS 请求文件信息（`VMVFSREQ_FDLOOKUP`）
3. VFS 返回 `dev`, `ino`, `size`
4. VM 的 `mmap_file()` 调用 `mmap_region()` 创建虚拟区域，`mappedfile_setfile()` 设置文件信息（包括 `fdref_dedup_or_new()` 创建/复用 fdref，以及 prefill 预填充已有缓存页）
5. 返回映射地址

**与缓存系统的交互**

| 场景 | 行为 |
|------|------|
| 缓存命中 | 直接链接缓存页，无需 I/O |
| 缓存未命中 | 向 VFS 请求，VFS 填充缓存 |
| 写入 | COW 分配新页，转为匿名内存 |
| fork | 复制映射信息，共享 fdref |

**写时复制的意义**：

- **读取时**：多个进程共享同一缓存页，`phys_region` 指向 `cached_page`（只读）
- **写入时**：1. 分配新的物理页；2. 复制缓存页内容到新页；3. 修改 `phys_region` 指向新页；4. 类型改为 `mem_type_anon`
- **结果**：写入进程拥有私有副本，其他进程仍共享原缓存页，保证文件映射的写隔离

**限制总结**

| 操作 | 支持 | 说明 |
|------|------|------|
| 创建 | ✅ | 通过 `mmap()` 系统调用 |
| 读取 | ✅ | 页错误时加载，缓存命中则直接映射 |
| 写入 | ✅ | 通过 COW 机制，转为匿名内存 |
| fork | ✅ | 复制映射信息，共享 fdref |
| resize | ❌ | 不支持调整大小 |
| 直接写 | ❌ | 必须通过 COW |

#### 2.2.6 mem_type_shared - 共享内存

**源码位置**: [`mem_shared.c`](../../../minix3/minix/servers/vm/mem_shared.c)

**什么是共享内存？**

共享内存是多个进程可以同时访问的内存区域，是最快的进程间通信 (IPC) 方式。典型用途：
- **进程间通信**：高速数据交换
- **共享缓冲区**：生产者-消费者模式
- **共享数据结构**：多进程共享状态
- **零拷贝传输**：避免数据复制

**核心特点**：
1. **真正的共享**：多个进程映射同一物理内存
2. **基于源区域**：共享内存引用另一个进程的匿名内存
3. **延迟映射**：页错误时链接到源区域的物理页
4. **引用计数**：跟踪共享区域的引用数

**共享内存数据结构**

```c
// vir_region->param.shared
struct {
    endpoint_t ep;      // 源进程端点
    vir_bytes vaddr;    // 源区域虚拟地址
    int id;             // 源区域 ID
};

// 源区域 (匿名内存) 的 remaps 字段
// vir_region->remaps 记录有多少共享区域引用此区域
```

**回调函数实现**（仅 9 个非 NULL 回调）

| 回调 | 实现函数 | 功能 |
|------|---------|------|
| `ev_unreference` | `shared_unreference` | 复用匿名内存实现 |
| `ev_pagefault` | `shared_pagefault` | 链接到源区域的物理页 |
| `ev_sanitycheck` | `shared_sanitycheck` | 空操作（无需特化） |
| `ev_copy` | `shared_copy` | fork 时复制共享信息 |
| `ev_delete` | `shared_delete` | 减少源区域的 remaps 计数 |
| `regionid` | `shared_regionid` | 返回源区域 ID |
| `refcount` | `shared_refcount` | 返回 1 + remaps |
| `writable` | `shared_writable` | 物理页存在则可写 |
| `pt_flags` | `shared_pt_flags` | ARM 返回缓存标志 |

**关键函数分析**

**shared_pagefault - 页错误处理**

```c
static int shared_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb,
    void *state, int statelen, int *io)
{
    struct vir_region *src_region;
    struct vmproc *src_vmp;
    struct phys_region *pr;

    // 获取源进程和源区域
    if(getsrc(region, &src_vmp, &src_region) != OK) {
        return EINVAL;
    }

    // 物理页已存在，无需处理
    if(ph->ph->phys != MAP_NONE) {
        return OK;
    }

    // 释放空的物理块
    pb_free(ph->ph);

    // 获取源区域对应偏移的物理区域
    if(!(pr = physblock_get(src_region, ph->offset))) {
        // 源区域也没有物理页，先触发源区域的页错误
        int r;
        if((r=map_pf(src_vmp, src_region, ph->offset, write,
            NULL, NULL, 0, io)) != OK)
            return r;
        if(!(pr = physblock_get(src_region, ph->offset))) {
            panic("missing region after pagefault handling");
        }
    }

    // 链接到源区域的物理块
    pb_link(ph, pr->ph, ph->offset, region);

    return OK;
}
```

**页错误处理流程**：

1. `getsrc()` 获取源进程和源区域（根据 `param.shared.ep` 和 `param.shared.vaddr`）
2. 若物理页已存在（`ph->ph->phys != MAP_NONE`），直接返回 OK
3. 释放空的物理块（`pb_free(ph->ph)`）
4. 查找源区域对应偏移的物理区域（`physblock_get(src_region, ph->offset)`）
5. 若源区域也没有物理页：先触发源区域的页错误（`map_pf(src_vmp, src_region, ...)`），然后重试获取
6. 链接到源区域的物理块（`pb_link(ph, pr->ph, ph->offset, region)`）
7. 结果：进程 B 的 `phys_region` 指向进程 A 的物理块

**getsrc - 获取源区域**

```c
static int getsrc(struct vir_region *region,
    struct vmproc **vmp, struct vir_region **r)
{
    int srcproc;

    // 验证类型
    if(region->def_memtype != &mem_type_shared) {
        printf("shared region hasn't shared type but %s.\n",
            region->def_memtype->name);
        return EINVAL;
    }

    // 验证参数
    if(!region->param.shared.ep || !region->param.shared.vaddr) {
        printf("shared region has not defined source region.\n");
        return EINVAL;
    }

    // 查找源进程
    if(vm_isokendpt((endpoint_t) region->param.shared.ep, &srcproc) != OK) {
        printf("VM: shared memory with missing source process.\n");
        return EINVAL;
    }

    *vmp = &vmproc[srcproc];

    // 查找源区域
    if(!(*r=map_lookup(*vmp, region->param.shared.vaddr, NULL))) {
        printf("VM: shared memory with missing vaddr 0x%lx.\n",
            region->param.shared.vaddr);
        return EINVAL;
    }

    // 验证源区域类型
    if((*r)->def_memtype != &mem_type_anon) {
        printf("source region hasn't anon type but %s.\n",
            (*r)->def_memtype->name);
        return EINVAL;
    }

    // 验证 ID 匹配
    if(region->param.shared.id != (*r)->id) {
        printf("source region has no matching id\n");
        return EINVAL;
    }

    return OK;
}
```

**shared_setsource - 设置源区域**

```c
void shared_setsource(struct vir_region *vr, endpoint_t ep,
    struct vir_region *src_vr)
{
    struct vmproc *vmp;
    struct vir_region *srcvr;
    int id = src_vr->id;
    vir_bytes vaddr = src_vr->vaddr;

    assert(vr->def_memtype == &mem_type_shared);

    // 设置共享参数
    vr->param.shared.ep = ep;
    vr->param.shared.vaddr = vaddr;
    vr->param.shared.id = id;

    // 验证源区域
    if(getsrc(vr, &vmp, &srcvr) != OK)
        panic("initial getsrc failed");

    assert(srcvr == src_vr);

    // 增加引用计数
    srcvr->remaps++;
}
```

**shared_copy - fork 复制**

```c
static int shared_copy(struct vir_region *vr, struct vir_region *newvr)
{
    struct vmproc *vmp;
    struct vir_region *srcvr;

    // 获取源区域
    if(getsrc(vr, &vmp, &srcvr) != OK)
        panic("copy: original getsrc failed");

    // 为新区域设置相同的源
    shared_setsource(newvr, vr->param.shared.ep, srcvr);

    return OK;
}
```

**shared_refcount - 引用计数**

```c
static int shared_refcount(struct vir_region *vr)
{
    // 源区域本身 + 所有共享映射
    return 1 + vr->remaps;
}
```

**全局实例定义**

```c
struct mem_type mem_type_shared = {
    .name = "shared memory",
    .ev_copy = shared_copy,
    .ev_unreference = shared_unreference,
    .ev_pagefault = shared_pagefault,
    .ev_sanitycheck = shared_sanitycheck,
    .ev_delete = shared_delete,
    .regionid = shared_regionid,
    .refcount = shared_refcount,
    .writable = shared_writable,
    .pt_flags = shared_pt_flags,
};
```

**共享内存创建流程 (do_remap)**

> **源码位置**: [`mmap.c:366`](../../../minix3/minix/servers/vm/mmap.c#L366)（注意：`do_remap` 不在 `mem_shared.c` 中，而在 `mmap.c` 中）

```c
int do_remap(message *m)
{
    // 获取源进程和目标进程
    if ((r = vm_isokendpt(destination, &dn)) != OK) return EINVAL;
    if ((r = vm_isokendpt(source, &sn)) != OK) return EINVAL;

    dvmp = &vmproc[dn];  // 目标进程
    svmp = &vmproc[sn];  // 源进程

    // 查找源区域
    if (!(src_region = map_lookup(svmp, sa, NULL)))
        return EINVAL;

    // 验证源区域
    if(src_region->vaddr != sa) return EFAULT;
    if(size != src_region->length) return EFAULT;

    // 在目标进程创建共享区域
    flags = VR_SHARED;
    if(!readonly) flags |= VR_WRITABLE;

    vr = map_page_region(dvmp, da, 0, size, flags, 0, &mem_type_shared);

    // 设置源区域
    shared_setsource(vr, svmp->vm_endpoint, src_region);

    return OK;
}
```

**共享内存架构图**

```
┌──────────────────────────────────────────────────────────┐
│                    共享内存架构                            │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  进程 A (源)                                              │
│  ┌───────────────────────────────────────────────────┐   │
│  │ vir_region (匿名内存)                              │   │
│  │   - vaddr: 0x400000                               │   │
│  │   - def_memtype: &mem_type_anon                   │   │
│  │   - remaps: 2  ←────────────────┐                │   │
│  │   ┌───────────────────────┐     │                │   │
│  │   │ phys_region           │     │                │   │
│  │   │   - ph->phys: 0x12345000    │                │   │
│  │   │                         │────┼───────────┐   │   │
│  │   └───────────────────────┘     │           │   │   │
│  └───────────────────────────────────────────────────┘   │
│                                   │           │          │
│  进程 B (共享者 1)                 │           │          │
│  ┌───────────────────────────────────────────────────┐   │
│  │ vir_region (共享内存)              │           │   │
│  │   - vaddr: 0x500000               │           │   │
│  │   - def_memtype: &mem_type_shared │           │   │
│  │   - param.shared:                 │           │   │
│  │       ep: 进程A端点               │           │   │
│  │       vaddr: 0x400000 ────────────┘           │   │
│  │   ┌───────────────────────┐                   │   │
│  │   │ phys_region           │                   │   │
│  │   │   - ph->phys: 0x12345000                  │   │
│  │   │                       │←──────────────────┘   │
│  │   └───────────────────────┘                       │   │
│  └───────────────────────────────────────────────────┘   │
│                                                          │
│  进程 C (共享者 2)                                        │
│  ┌───────────────────────────────────────────────────┐   │
│  │ vir_region (共享内存)                              │   │
│  │   - vaddr: 0x600000                               │   │
│  │   - def_memtype: &mem_type_shared                 │   │
│  │   - param.shared:                                 │   │
│  │       ep: 进程A端点                               │   │
│  │       vaddr: 0x400000                             │   │
│  │   ┌───────────────────────┐                       │   │
│  │   │ phys_region           │                       │   │
│  │   │   - ph->phys: 0x12345000                      │   │
│  │   │                       │← 同一物理页           │   │
│  │   └───────────────────────┘                       │   │
│  └───────────────────────────────────────────────────┘   │
│                                                          │
│  物理内存: 0x12345000 被三个进程共享                       │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

**与匿名内存的关系**

| 特性 | 匿名内存 (源) | 共享内存 |
|------|-------------|---------|
| 类型 | `mem_type_anon` | `mem_type_shared` |
| 物理页所有权 | 拥有 | 引用源的物理页 |
| 可被共享 | ✅ (通过 remap) | ✅ |
| 写入 | CoW 或直接写 | 直接写（共享） |
| fork | CoW | 复制共享信息 |

**限制总结**

| 操作 | 支持 | 说明 |
|------|------|------|
| 创建 | ✅ | 通过 `do_remap` 从匿名内存创建 |
| 读取 | ✅ | 页错误时链接源区域的物理页 |
| 写入 | ✅ | 直接写入共享物理页 |
| fork | ✅ | 复制共享信息，增加 remaps |
| resize | ❌ | 不支持调整大小 |
| 独立存在 | ❌ | 必须引用源匿名内存区域 |

### 2.3 多态操作

#### 2.3.1 内存回收机制

**Minix3 没有传统的页面回收（evict/swap）机制**

通过分析 Minix3 VM 源码，发现 Minix3 采用了一种简化的内存管理策略：

| 特性 | Minix3 | 传统 Unix |
|------|--------|----------|
| Swap 空间 | ❌ 无 | ✅ 有 |
| 页面换出 | ❌ 无 | ✅ 有 |
| Pageout 守护进程 | ❌ 无 | ✅ 有 |
| 页面换入 | ❌ 无 | ✅ 有 |
| 内存压力处理 | 进程终止 | 换出页面 |

**内存回收通过引用计数实现**

Minix3 的内存回收完全依赖引用计数机制，通过 `ev_unreference` 回调实现：

```c
// pb.c - 物理块释放（无引用计数逻辑）
void pb_free(struct phys_block *pb)
{
    if(pb->phys != MAP_NONE)
        free_mem(ABS2CLICK(pb->phys), 1);
    SLABFREE(pb);
}

// pb.c - 物理块引用计数管理（解除引用时调用）
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb;

    pb = pr->ph;
    assert(pb->refcount > 0);
    USE(pb, pb->refcount--;);

    // 从链表中移除 phys_region
    if(pb->firstregion == pr) {
        USE(pb, pb->firstregion = pr->next_ph_list;);
    } else {
        // 遍历链表找到前驱
        struct phys_region *others;
        for(others = pb->firstregion; others;
            others = others->next_ph_list) {
            assert(others->ph == pb);
            if(others->next_ph_list == pr) {
                USE(others, others->next_ph_list = pr->next_ph_list;);
                break;
            }
        }
        assert(others);
    }

    // 引用计数为 0 时，调用 ev_unreference 回调并释放 phys_block
    if(pb->refcount == 0) {
        assert(!pb->firstregion);
        int r;
        if((r = pr->memtype->ev_unreference(pr)) != OK)
            panic("unref failed, %d", r);

        SLABFREE(pb);
    }

    pr->ph = NULL;

    if(rm) physblock_set(region, pr->offset, NULL);
}
```

**ev_unreference 回调实现**

各内存类型的 `ev_unreference` 实现：

| 内存类型 | 实现函数 | 行为 |
|---------|---------|------|
| `mem_type_anon` | `anon_unreference` | 引用为 0 时释放物理页 |
| `mem_type_directphys` | `phys_unreference` | 空操作（不管理物理页） |
| `mem_type_anon_contig` | 复用 `anon_unreference` | 引用为 0 时释放连续物理页 |
| `mem_type_cache` | 复用 `anon_unreference` | 引用为 0 时释放缓存页 |
| `mem_type_mappedfile` | `mappedfile_unreference` | 引用为 0 时释放物理页 |
| `mem_type_shared` | 复用 `anon_unreference` | 引用为 0 时释放物理页 |

**引用计数变化场景**：

增加引用（`ev_reference`）：
1. fork：父进程的 `phys_region` 被子进程引用
2. 共享内存：多个进程共享同一物理页
3. 缓存共享：多个文件系统请求同一缓存页

减少引用（`ev_unreference`）：
1. munmap：解除内存映射
2. 进程终止：释放所有内存区域
3. 区域收缩：释放被移除的部分
4. CoW：写时复制后解除对原页面的引用

引用计数 = 0 时：
1. 调用 `ev_unreference` 回调
2. 释放物理页（`free_mem`）
3. 释放 `phys_block` 结构

**内存压力处理**

当系统内存不足时，Minix3 的处理方式：

```c
// 分配失败时的处理
if((new_page_cl = alloc_mem(pages, allocflags)) == NO_MEM) {
    // 1. 尝试释放缓存页面
    // 2. 如果仍然失败，返回 ENOMEM
    // 3. 调用者可能终止进程
    return ENOMEM;
}
```

**与缓存系统的配合**

缓存页面通过 LRU 链表管理，但不会主动回收：

```c
// cache.c - 缓存页面查找
struct cached_page *find_cached_page_bydev(dev_t dev, u64_t dev_off,
    ino_t ino, u64_t ino_off, int touchlru)
{
    // 查找缓存页
    for(hb = cache_hash_bydev[makehash(dev, dev_off)]; hb; hb=hb->hash_next_dev) {
        if(hb->dev == dev && hb->dev_offset == dev_off) {
            // 如果 touchlru，更新 LRU 位置
            if(touchlru) {
                // 移动到 LRU 链表头部
            }
            return hb;
        }
    }
    return NULL;
}
```

**设计决策总结**

Minix3 选择不实现传统页面回收的原因：

1. **简化设计**：避免复杂的换入换出逻辑
2. **实时性**：避免页面换出导致的延迟
3. **嵌入式友好**：适合没有 swap 空间的嵌入式系统
4. **可靠性**：内存不足时行为可预测

#### 2.3.2 cow - 写时复制

**CoW 触发场景**

写时复制在以下场景触发：

| 场景 | 触发条件 | 处理方式 |
|------|---------|---------|
| fork 后写入 | `refcount > 1` 且写入 | 分配新页，复制数据 |
| 文件映射写入 | 写入文件映射区域 | 分配新页，复制数据，转为匿名内存 |
| 共享内存写入 | 不适用 | 共享内存直接写入，不触发 CoW |

**mem_cow 核心实现**

**源码位置**: [`pb.c`](../../../minix3/minix/servers/vm/pb.c)

```c
int mem_cow(struct vir_region *region,
    struct phys_region *ph, phys_bytes new_page_cl, phys_bytes new_page)
{
    struct phys_block *pb;

    // 如果没有提供新页面，则分配一个
    if(new_page == MAP_NONE) {
        u32_t allocflags;
        allocflags = vrallocflags(region->flags);

        if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM)
            return ENOMEM;

        new_page = CLICK2ABS(new_page_cl);
    }

    // 原页面必须存在
    assert(ph->ph->phys != MAP_NONE);

    // 复制原页面内容到新页面
    if(sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE) != OK) {
        panic("VM: abscopy failed\n");
        return EFAULT;
    }

    // 创建新的物理块
    if(!(pb = pb_new(new_page))) {
        free_mem(new_page_cl, 1);
        return ENOMEM;
    }

    // 解除对原物理块的引用
    pb_unreferenced(region, ph, 0);

    // 链接新物理块
    pb_link(ph, pb, ph->offset, region);

    // 类型转为匿名内存
    ph->memtype = &mem_type_anon;

    return OK;
}
```

**CoW 执行流程**：

1. 写入操作触发页错误，检查条件：`refcount > 1` 且 `write = true`
2. 若不满足条件，直接返回 OK
3. 若满足条件，执行 CoW：
   - 分配新物理页（`alloc_mem`）
   - 复制数据（`sys_abscopy`）
   - 创建新 `phys_block`
   - 解除原引用（`pb_unreferenced`）
   - 链接新块（`pb_link`）
   - 类型改为 `mem_type_anon`
4. 返回 OK，进程继续写入

**anon_pagefault 中的 CoW 判断**

```c
static int anon_pagefault(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, int write, vfs_callback_t cb, void *state,
    int len, int *io)
{
    phys_bytes new_page, new_page_cl;
    u32_t allocflags;

    allocflags = vrallocflags(region->flags);

    assert(ph->ph->refcount > 0);

    // 预分配一个页面（可能用于 CoW 或首次分配）
    if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) {
        printf("anon_pagefault: out of memory\n");
        return ENOMEM;
    }
    new_page = CLICK2ABS(new_page_cl);

    // 情况 1: 物理页不存在，首次分配
    if(ph->ph->phys == MAP_NONE) {
        ph->ph->phys = new_page;
        return OK;
    }

    // 情况 2: 不需要 CoW
    // - refcount < 2: 只有一个引用
    // - !write: 只读访问
    if(ph->ph->refcount < 2 || !write) {
        /* memory is ready already */
        return OK;
    }

    // ⚠ Minix3 源码缺陷：情况 2 中预分配的 new_page_cl 未被释放（free_mem），
    // 造成内存泄漏。每次只读页错误或独占页写错误都会泄漏一个页面。

    // 情况 3: 需要 CoW
    assert(region->flags & VR_WRITABLE);
    return mem_cow(region, ph, new_page_cl, new_page);
}
```

**anon_writable - 可写判断**

```c
static int anon_writable(struct phys_region *pr)
{
    assert(pr->ph->refcount > 0);

    // 物理页不存在，不可写
    if(pr->ph->phys == MAP_NONE)
        return 0;

    // 有共享映射，可写（写入会触发 CoW）
    if(pr->parent->remaps > 0)
        return 1;

    // 只有一个引用，可写
    return pr->ph->refcount == 1;
}
```

**fork 时的引用共享**

```c
// region.c - map_copy_region
for(p = 0; p < phys_slot(vr->length); p++) {
    struct phys_region *newph;

    if(!(ph = physblock_get(vr, p*VM_PAGE_SIZE))) continue;

    // 子进程引用父进程的物理块
    newph = pb_reference(ph->ph, ph->offset, newvr, vr->def_memtype);

    if(!newph) { map_free(newvr); return NULL; }

    // 调用类型的 ev_reference 回调
    if(ph->memtype->ev_reference)
        ph->memtype->ev_reference(ph, newph);
}
```

**fork 后的内存状态**

```
┌─────────────────────────────────────────────────────────────┐
│                    fork 后的内存状态                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   fork 前:                                                  │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 父进程 A                                             │   │
│   │   vir_region                                         │   │
│   │     └── phys_region                                  │   │
│   │           └── phys_block (refcount=1)               │   │
│   │                 └── phys: 0x12345000                │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   fork 后:                                                  │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 父进程 A                    子进程 B                 │   │
│   │   vir_region                 vir_region              │   │
│   │     └── phys_region           └── phys_region       │   │
│   │           │                         │                │   │
│   │           └─────────┬───────────────┘                │   │
│   │                     ↓                                │   │
│   │           phys_block (refcount=2)                    │   │
│   │                 └── phys: 0x12345000                 │   │
│   │                                                      │   │
│   │   页表: 只读              页表: 只读                  │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   父进程写入后 (CoW):                                        │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 父进程 A                    子进程 B                 │   │
│   │   vir_region                 vir_region              │   │
│   │     └── phys_region           └── phys_region       │   │
│   │           │                         │                │   │
│   │           ↓                         ↓                │   │
│   │   phys_block (refcount=1)   phys_block (refcount=1)  │   │
│   │     phys: 0x12346000         phys: 0x12345000        │   │
│   │                                                      │   │
│   │   页表: 读写              页表: 只读                  │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**文件映射的 CoW (cow_block)**

```c
// mem_file.c
static int cow_block(struct vmproc *vmp, struct vir_region *region,
    struct phys_region *ph, u16_t clearend)
{
    int r;

    // 执行 CoW
    if((r=mem_cow(region, ph, MAP_NONE, MAP_NONE)) != OK) {
        printf("mappedfile_pagefault: COW failed\n");
        return r;
    }

    // COW 后转为匿名内存
    // 注意：此赋值与 mem_cow() 内部的 ph->memtype = &mem_type_anon 冗余
    ph->memtype = &mem_type_anon;

    // 清理末尾字节（处理文件末尾不对齐）
    if(clearend) {
        phys_bytes phaddr = ph->ph->phys, po = VM_PAGE_SIZE-clearend;
        assert(clearend < VM_PAGE_SIZE);
        phaddr += po;
        if(sys_memset(NONE, 0, phaddr, clearend) != OK) {
            panic("cow_block: clearend failed\n");
        }
    }

    return OK;
}
```

**CoW 与内存类型的关系**

| 内存类型 | 支持 CoW | 触发条件 |
|---------|---------|---------|
| `mem_type_anon` | ✅ | fork 后写入 |
| `mem_type_directphys` | ❌ | 不支持 |
| `mem_type_anon_contig` | ❌ | 不支持 fork |
| `mem_type_cache` | ❌ | 不支持写入 |
| `mem_type_mappedfile` | ✅ | 写入文件映射 |
| `mem_type_shared` | ❌ | 直接写入共享 |

**性能优化**

Minix3 在 `anon_pagefault` 中预分配页面：

```c
// 预分配页面，避免在 CoW 路径中分配失败
if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) {
    return ENOMEM;
}

// 如果不需要 CoW，释放预分配的页面
if(ph->ph->refcount < 2 || !write) {
    free_mem(new_page_cl, 1);
    return OK;
}

// 需要 CoW，使用预分配的页面
return mem_cow(region, ph, new_page_cl, new_page);
```

#### 2.3.3 other 操作

**回调函数完整列表**

`mem_type` 结构体定义了所有回调函数：

```c
typedef struct mem_type {
    const char *name;                           // 类型名称
    int (*ev_new)(struct vir_region *region);   // 创建区域
    void (*ev_delete)(struct vir_region *region); // 删除区域
    int (*ev_reference)(struct phys_region *pr, struct phys_region *newpr); // 增加引用
    int (*ev_unreference)(struct phys_region *pr); // 减少引用
    int (*ev_pagefault)(...);                   // 页错误处理
    int (*ev_resize)(struct vmproc *vmp, struct vir_region *vr, vir_bytes len); // 调整大小
    void (*ev_split)(struct vmproc *vmp, struct vir_region *vr,
        struct vir_region *r1, struct vir_region *r2); // 分割区域
    int (*writable)(struct phys_region *pr);    // 可写判断
    int (*ev_sanitycheck)(struct phys_region *pr, const char *file, int line); // 一致性检查
    int (*ev_copy)(struct vir_region *vr, struct vir_region *newvr); // 复制区域
    int (*ev_lowshrink)(struct vir_region *vr, vir_bytes len); // 低地址收缩
    u32_t (*regionid)(struct vir_region *vr);   // 区域 ID
    int (*refcount)(struct vir_region *vr);     // 引用计数
    int (*pt_flags)(struct vir_region *vr);     // 页表标志
} mem_type_t;
```

**ev_new - 创建区域**

在创建新的虚拟区域时调用，用于类型特定的初始化。

```c
// region.c
if(newregion->def_memtype->ev_new) {
    if(newregion->def_memtype->ev_new(newregion) != OK) {
        // ev_new 会释放并移除区域
        return NULL;
    }
}
```

**实现示例**：

| 内存类型 | 实现 | 功能 |
|---------|------|------|
| `mem_type_anon` | 无 | 按需分配 |
| `mem_type_anon_contig` | `anon_contig_new` | 一次性分配所有连续物理页 |
| 其他 | 无 | 不需要特殊初始化 |

**ev_delete - 删除区域**

在删除虚拟区域时调用，用于类型特定的清理。

```c
// region.c
if(region->def_memtype->ev_delete)
    region->def_memtype->ev_delete(region);
```

**实现示例**：

| 内存类型 | 实现 | 功能 |
|---------|------|------|
| `mem_type_mappedfile` | `mappedfile_delete` | 释放 fdref 引用 |
| `mem_type_shared` | `shared_delete` | 减少源区域的 remaps 计数 |
| 其他 | 无 | 不需要特殊清理 |

**ev_split - 分割区域**

在区域分割时调用（如 munmap 中间部分）。

```c
// region.c - split_region
vr->def_memtype->ev_split(vmp, vr, r1, r2);

// mem_file.c - mappedfile_split
static void mappedfile_split(struct vmproc *vmp, struct vir_region *vr,
    struct vir_region *r1, struct vir_region *r2)
{
    // 复制文件信息
    r1->param.file = vr->param.file;
    r2->param.file = vr->param.file;

    // 增加引用计数
    fdref_ref(vr->param.file.fdref, r1);
    fdref_ref(vr->param.file.fdref, r2);

    // 调整偏移
    r1->param.file.clearend = 0;          // 前半部分无末尾清理
    r2->param.file.offset += r1->length;  // 后半部分偏移增加
}
```

**分割流程**：

区域分割发生在 `munmap` 中间部分时。原区域 VR 被分为 R1 和 R2：
- R1：保持原 vaddr，length = split_len
- R2：vaddr += split_len，`ev_split` 调整内部偏移（如文件映射的 `file.offset += r1->length`）

**ev_resize - 调整大小**

在区域扩展时调用（如 brk）。

```c
// mem_anon.c
static int anon_resize(struct vmproc *vmp, struct vir_region *vr, vir_bytes l)
{
    // 只支持扩展，不支持收缩
    if(l <= vr->length)
        return OK;

    USE(vr, vr->length = l;);
    return OK;
}

// mem_cache.c
static int cache_resize(struct vmproc *vmp, struct vir_region *vr, vir_bytes l)
{
    printf("VM: cannot resize cache blocks.\n");
    return ENOMEM;
}
```

**ev_lowshrink - 低地址收缩**

从区域低地址端收缩，用于特殊情况。

```c
// region.c - 低地址收缩逻辑（内联在 region 操作中）
if(!r->def_memtype->ev_lowshrink) {
    printf("VM: low-shrinking not implemented for %s\n",
        r->def_memtype->name);
    return EINVAL;
}

if(r->def_memtype->ev_lowshrink(r, len) != OK) {
    printf("VM: low-shrinking failed for %s\n",
        r->def_memtype->name);
    return EINVAL;
}

// 更新虚拟地址和长度
USE(r, r->vaddr += len;);
USE(r, r->length -= len;);

// mem_file.c
static int mappedfile_lowshrink(struct vir_region *vr, vir_bytes len)
{
    assert(vr->param.file.inited);
    vr->param.file.offset += len;  // 调整文件偏移
    return OK;
}
```

**ev_copy - 复制区域**

在 fork 时复制区域信息。

```c
// region.c - region_copy_slab
if(vr->def_memtype->ev_copy && (r=vr->def_memtype->ev_copy(vr, newvr)) != OK) {
    map_free(newvr);
    return NULL;
}

// mem_file.c
int mappedfile_copy(struct vir_region *vr, struct vir_region *newvr)
{
    assert(vr->param.file.inited);
    mappedfile_setfile(newvr->parent, newvr, vr->param.file.fdref->fd,
        vr->param.file.offset,
        vr->param.file.fdref->dev, vr->param.file.fdref->ino,
        vr->param.file.clearend, 0, 0);
    return OK;
}

// mem_shared.c
static int shared_copy(struct vir_region *vr, struct vir_region *newvr)
{
    struct vmproc *vmp;
    struct vir_region *srcvr;

    if(getsrc(vr, &vmp, &srcvr) != OK)
        panic("copy: original getsrc failed");

    shared_setsource(newvr, vr->param.shared.ep, srcvr);
    return OK;
}
```

**regionid - 区域标识**

返回区域的唯一标识符。

```c
// mem_anon.c
static u32_t anon_regionid(struct vir_region *region)
{
    return region->id;  // 返回区域唯一 ID
}

// mem_shared.c
static u32_t shared_regionid(struct vir_region *vr)
{
    struct vir_region *src_region;
    struct vmproc *src_vmp;

    if(getsrc(vr, &src_vmp, &src_region) != OK)
        return 0;

    return src_region->id;  // 返回源区域的 ID
}
```

**refcount - 引用计数**

返回区域的引用计数。

```c
// mem_anon.c
static int anon_refcount(struct vir_region *vr)
{
    return 1 + vr->remaps;  // 1 + 共享映射数
}

// mem_shared.c
static int shared_refcount(struct vir_region *vr)
{
    return 1 + vr->remaps;  // 源区域 + 共享映射
}
```

**pt_flags - 页表标志**

返回映射时需要的额外页表标志。

```c
// mem_anon.c
static int anon_pt_flags(struct vir_region *vr){
#if defined(__arm__)
    return ARM_VM_PTE_CACHED;  // ARM 需要缓存标志
#else
    return 0;
#endif
}

// 各类型实现类似，ARM 平台返回缓存标志
```

**ev_sanitycheck - 一致性检查**

用于调试时验证数据结构一致性。

```c
// mem_anon.c
static int anon_sanitycheck(struct phys_region *pr, const char *file, int line)
{
    MYASSERT(usedpages_add(pr->ph->phys, VM_PAGE_SIZE) == OK);
    return OK;
}

// region.c - 调用时
if(pr->memtype->ev_sanitycheck)
    pr->memtype->ev_sanitycheck(pr, file, line);
```

**回调函数实现汇总**

| 回调 | 匿名内存 | 直接物理 | 连续匿名 | 缓存 | 文件映射 | 共享内存 |
|------|---------|---------|---------|------|---------|---------|
| `ev_new` | - | - | ✅ | - | - | - |
| `ev_delete` | - | - | - | - | ✅ | ✅ |
| `ev_reference` | - | - | ❌ | ✅ | - | - |
| `ev_unreference` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `ev_pagefault` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `ev_resize` | ✅ | - | ❌ | ❌ | - | - |
| `ev_split` | ✅ | - | ✅ | - | ✅ | - |
| `ev_copy` | - | ✅ | - | - | ✅ | ✅ |
| `ev_lowshrink` | ✅ | - | - | ✅ | ✅ | - |
| `writable` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `ev_sanitycheck` | ✅ | - | ✅ | ✅ | ✅ | ✅ |
| `regionid` | ✅ | - | - | - | - | ✅ |
| `refcount` | ✅ | - | - | - | - | ✅ |
| `pt_flags` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |


---

## 3. Rust 设计决策

### 3.1 从函数指针表到 trait

Ch1§1.2 分析了 Minix3 的 `struct mem_type` 函数指针表。Rust 中对应机制为 trait：

| Minix3 (C) | minix-rs (Rust) | 依据 |
|------------|-----------------|------|
| `struct mem_type` + 函数指针 | `trait MemType` | Ch1§1.2 多态机制 |
| NULL 指针 = 跳过/默认/不支持（见下表） | 默认方法实现 | Ch2 各类型的 NULL 回调 |
| 运行时检查 `if (mt->ev_new)` | 编译时类型检查 | 消除 Ch2 中的 NULL 检查模式 |
| `&mem_type_anon` 静态实例 | `&'static dyn MemType` | Ch1§1.2 全局实例 |

**C 源码中 NULL 回调的三种语义**（region.c 调用方分析）：

| 语义 | NULL 时行为 | 对应回调 | Rust 默认实现 |
|------|-----------|---------|-------------|
| 跳过（无操作） | 不调用，继续执行 | `ev_new`, `ev_delete`, `ev_reference`, `ev_copy` | 空操作 / `Ok(())` |
| 通用默认 | 执行通用逻辑 | `ev_resize`（NULL→通用扩展） | `Ok(())`（框架层处理） |
| 不支持 | 返回 EINVAL | `ev_split`, `ev_lowshrink`（NULL→EINVAL） | `Err(NotSupported)` |

另有"空实现"（函数体只有 `return OK`），如 `phys_unreference`、`anon_split`，语义为"明确无操作但返回成功"，与 NULL 的"跳过"等价。Rust 中由各类型覆盖为 `Ok(())`；未覆盖的类型使用默认 `Err(NotSupported)`，与 C 的 NULL→EINVAL 语义对齐。

### 3.2 多态分发方式

**方案 A：静态分发（泛型）**

```rust
pub struct VirRegion<T: MemType> {
    mem_type: T,
}
```

优点：零运行时开销，编译时单态化优化。
缺点：**无法在运行时更改类型**。

**方案 B：动态分发（trait object）**

```rust
pub struct VirRegion {
    def_memtype: Option<&'static dyn MemType>,
}
```

优点：运行时灵活，支持类型变更。
缺点：虚函数调用开销（约 2-5ns）。

**选择：方案 B**。理由：

1. **CoW 后类型必须变更**：Ch2§2.2.5 分析了 `mappedfile_pagefault` 中写入共享页触发 CoW，物理页类型从 mappedfile 变为 anon。`cow_resolve_core` 中 `region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON)` 将新页的 memtype 设为 anon——这是 per-page 级别的类型变更，静态分发无法支持
2. **页错误不是每秒百万次的热路径**：虚函数开销可忽略
3. **与 Minix3 的 `mem_type_t*` 语义一致**：指向全局静态实例的指针

### 3.3 全局实例策略

**方案 A：`Arc<dyn MemType>` + `LazyLock`**

```rust
pub static MEM_TYPE_ANON: LazyLock<Arc<dyn MemType>> = 
    LazyLock::new(|| Arc::new(AnonymousMemory));
```

优点：支持共享所有权和延迟初始化。
缺点：堆分配、引用计数开销、`no_std` 下 `LazyLock` 不可用。

**方案 B：`&'static dyn MemType` + `const` 静态**

```rust
pub(crate) static MEM_TYPE_ANON: AnonymousMemory = AnonymousMemory::new();
// 使用时: &MEM_TYPE_ANON as &'static dyn MemType
```

优点：零分配，`const` 初始化，`no_std` 兼容。
缺点：不支持运行时注册新类型。

**选择：方案 B**。理由：

1. **所有内存类型实现都是 ZST**（零大小单元结构体），没有内部状态，无需堆分配
2. **VM 是单线程事件循环**，无共享所有权需求，`Arc` 的引用计数是多余开销
3. **ZST 可直接 `const` 初始化**，`LazyLock` 的延迟初始化无必要
4. **`&'static` 与 Minix3 的 `mem_type_t*` 语义完全一致**：指向全局静态实例的指针

**不需要类型注册表**：Minix3 只有 6 种内存类型（Ch1§1.3），编译时已知。运行时动态注册新类型的需求不存在，`HashMap<&str, Arc<dyn MemType>>` 增加了不必要的复杂度。虽然 `no_std` + `extern crate alloc`（见 [09-vm-relocation.md](09-vm-relocation.md)）下 `HashMap` 可用，但类型注册表本身是过度设计。

### 3.4 PFN 索引模型对 trait 签名的影响

Ch2 分析的 Minix3 回调签名基于 `phys_region` 结构体。minix-rs 采用 PFN 索引模型（见 [10-phys-pagestate.md](10-phys-pagestate.md)），物理页状态存储在 `PageFrames` 中，通过 `PageSlot`（PFN 索引）访问。这导致 trait 方法签名发生变化：

| 回调 | Minix3 签名 | Rust 签名 | 变化原因 |
|------|-----------|----------|---------|
| `ev_reference` | `(pr, newpr)` | `(frames, slot)` | PFN 模型下 fork 时 refcount++ 由框架层直接操作 PageFrames |
| `ev_unreference` | `(pr)` | `(frames, pfn)` | 物理页释放由 PfnAllocator 负责，只需告知 pfn |
| `ev_pagefault` | `(vmp, region, ph, write, ...)` | `(proc, region, frames, offset, write)` | 用 offset 查找 PageSlot，而非直接传 PhysRegion |
| `writable` | `(pr)` | `(frames, slot, region)` | 可写判断需要查 PageFrames 中的 refcount，且 AnonymousMemory 需访问 `region.remaps` |
| `ev_sanitycheck` | `(pr, file, line)` | `(frames, slot)` | 同上 |

**关键变化 1：`ev_pagefault` 接收 `offset: VirBytes`**

调用方通过 offset 在 VirRegion 的 `physblocks: Vec<Option<PageSlot>>` 中查找对应 slot，而非直接传入 `PhysRegion`。这避免了热路径（页错误）中频繁创建/销毁 `PhysRegion` 对象。

**关键变化 2：`ev_reference` 简化**

Minix3 的 `ev_reference(pr, newpr)` 接收源和目标两个 phys_region。在 PFN 模型下，fork 时 refcount++ 由框架层（`fork_region`）直接操作 `frames.get_mut(pfn).refcount += 1`，`ev_reference` 只需做类型特化通知（如 cache 更新索引、anon_contig 拒绝 fork）。当前只有 `cache_reference`（空操作）和 `anon_contig_reference`（返回错误）实现了此回调，签名 `(frames, slot)` 暂时够用。

**关键变化 3：`ev_unreference` 接收 `pfn: u32`**

Minix3 的 `ev_unreference` 直接调用 `free_mem` 释放物理页。PFN 模型下物理页释放由 `PfnAllocator::free_pfn()` 负责，`ev_unreference` 只需做类型特化清理（当前所有类型均为空操作）。框架层在 `free_range` 中收集 `(pfn, memtype)` 对，先调用 `ev_unreference`，再调用 `alloc.free_pfn()`。

### 3.5 per-page memtype 与 CoW 类型变更

Minix3 中 `phys_block.mem_type` 是 per-page 的，CoW 后 `ph->ph->memtype = &mem_type_anon`。PFN 模型下，`PageSlot` 也持有 per-page 的 memtype：

```rust
pub(crate) struct PageSlot {
    pub(crate) pfn: u32,
    pub(crate) offset: VirBytes,
    pub(crate) memtype: Option<&'static dyn MemType>,
}
```

**两层 memtype**：

| 层次 | 字段 | 用途 |
|------|------|------|
| VirRegion 级别 | `def_memtype` | 新分配页的默认类型（`map_lazy` 使用） |
| PageSlot 级别 | `memtype` | 该页当前的实际类型（CoW 后可能变更） |

**CoW 类型变更流程**（`cow_resolve_core`）：

```
1. unmap_page(old_pfn) → 返回 (old_pfn, old_memtype)
2. map_page(new_pfn, &MEM_TYPE_ANON) → 新页 memtype = anon
3. old_memtype.ev_unreference(frames, old_pfn) → 类型特化清理
4. alloc.free_pfn(old_pfn) → 释放旧物理页
```

这确保了 CoW 后新页的 memtype 为 anon，旧页通过 `ev_unreference` 通知原类型。

### 3.6 默认实现替代 NULL 回调

Minix3 中未赋值的回调为 NULL，框架层调用前需检查：

```c
if (region->def_memtype->ev_new) {
    region->def_memtype->ev_new(region);
}
```

Rust trait 的默认实现消除了这种运行时检查：

```rust
fn ev_new(&self, _region: &mut VirRegion) -> Result<(), MemTypeError> {
    Ok(())
}
```

**特殊处理**：Minix3 中 `ev_split` 和 `ev_lowshrink` 为 NULL 时框架报错（返回 EINVAL），因此即使无需特化逻辑也必须注册空函数（如 `anon_split`、`anon_lowshrink`）。Rust 中默认实现就是空操作，框架层无需特殊处理——如果某类型确实不支持 split/low_shrink，应在实现中返回 `Err(MemTypeError::NotSupported)`，而非依赖"未注册则报错"的隐式行为。

### 3.7 pt_flags 的架构无关化

Ch2§2.2.2 分析了 Minix3 中 `phys_pt_flags` 通过 `#if defined(__arm__)` 返回架构特定标志。Rust 中 `pt_flags` 返回架构无关的 `PageFlags`，由架构层翻译为具体页表位：

| 内存类型 | `pt_flags` 返回值 | 含义 | 依据 |
|---------|------------------|------|------|
| anon / cache / mappedfile / shared | `PageFlags::empty()` | 默认缓存 | Ch2 各类型的 pt_flags 分析 |
| directphys / anon_contig | `PageFlags::NO_CACHE` | 设备内存，禁用缓存 | Ch2§2.2.2 `ARM_VM_PTE_DEVICE` |

这遵循硬件抽象原则：**描述"做什么"（是否缓存），而非"怎么做"（PTE 位编码）**。

### 3.8 各类型回调实现对比

> ✅ = 有特化实现，默认 = 使用 trait 默认实现，❌ = 返回错误

| 方法 | Anonymous | DirectPhys | Contiguous | Cache | MappedFile | Shared |
|------|-----------|------------|------------|-------|------------|--------|
| `ev_new` | 默认 | 默认 | ✅ | 默认 | 默认 | 默认 |
| `ev_delete` | 默认 | 默认 | 默认 | ✅ | 默认 | 默认 |
| `ev_reference` | 默认 | 默认 | ✅ | 默认 | 默认 | 默认 |
| `ev_unreference` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `ev_pagefault` | ✅ | ✅ | ✅ | ✅ | ✅ | 默认 |
| `ev_resize` | 默认 | 默认 | ❌ | ❌ | 默认 | 默认 |
| `ev_split` | ✅ | ❌ | ✅ | ❌ | ✅ | ❌ |
| `ev_copy` | 默认 | ✅ | ❌ | 默认 | ✅ | ✅ |
| `writable` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `pt_flags` | 默认 | ✅ | ✅ | 默认 | 默认 | 默认 |
| `region_id` | ✅ | 默认 | 默认 | 默认 | 默认 | 默认 |
| `ref_count` | ✅ | 默认 | 默认 | 默认 | 默认 | 默认 |
| `ev_low_shrink` | ✅ | ❌ | ❌ | ✅ | ✅ | ❌ |

> **注意**：MappedFile 的 `ev_pagefault` 已设计为返回 `NeedVfsIo`（对应 Minix3 的 `SUSPEND`），缓存查找和 VFS 异步 I/O 的完整设计见 [23-vfs-interaction.md](23-vfs-interaction.md) §4.6~4.7。CacheMemory、SharedMemory 的 `ev_pagefault` 仍为 stub。

---

## 4. 实现

> 本章代码与 [`memtype.rs`](../../../../os/servers/vm/src/memtype.rs) 一致。

### 4.1 辅助类型

`MemType` 的回调方法使用两个辅助枚举统一错误处理和页错误结果。Minix3 C 源码中，错误用负数 errno（`ENOMEM`、`EINVAL`）表示，页错误结果隐含在控制流中（返回 `OK`/`SUSPEND` 或调用 `mem_cow()`）。Rust 将它们显式化为类型安全的枚举。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemTypeError {
    NoMemory,      // ENOMEM：内存不足
    InvalidParam,  // EINVAL：参数无效（如 DirectPhysical 未设置物理基地址）
    NotSupported,  // EINVAL：操作不支持（如 ContiguousAnonymous 拒绝 resize）
    IoError,       // EIO：I/O 错误（预留，VFS 集成时使用）
    CopyFailed,    // fork 复制失败
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PagefaultResult {
    Handled,          // 页错误已处理，无需进一步操作
    NeedNewPage,      // 需要分配新物理页（框架层执行分配）
    NeedCow,          // 需要写时复制（框架层执行 CoW）
    AccessViolation,  // 访问违规（只读区域写入）
}
```

**与 Minix3 返回值的对应**：

| Minix3 返回值 | Rust 对应 | 说明 |
|-------------|----------|------|
| `OK` | `Ok(PagefaultResult::Handled)` | 页错误已处理 |
| `ENOMEM` | `Err(MemTypeError::NoMemory)` | 内存不足 |
| `EFAULT` | `Err(MemTypeError::InvalidParam)` | 参数无效 |
| `SUSPEND` | 待实现 | VFS 集成时添加 `NeedAsyncIo` |

### 4.2 MemType trait

> 设计决策：§3.1（trait 替代函数指针表）、§3.4（PFN 索引模型签名）、§3.6（默认实现替代 NULL）

Minix3 用 `struct mem_type` + 函数指针实现多态（Ch1§1.2），Rust 用 trait 替代。关键区别在于：C 中 NULL 指针靠运行时 `if (mt->ev_new)` 检查，Rust 中用默认方法实现——每个默认值都对应 C 中 NULL 时的特定语义（§3.1 表格）。

```rust
pub(crate) trait MemType: Send + Sync {
    // ── 必需方法 ──
    // C 中 .name 是 char*，所有类型都必须提供。Rust 同理。
    fn name(&self) -> &'static str;

    // ── 生命周期回调（§3.1：C 中 NULL = 跳过，Rust 默认 = 空操作/Ok(())）──

    // C 中 NULL → 跳过初始化。Rust 默认 Ok(())：框架层自行处理页面分配。
    fn ev_new(&self, _region: &mut VirRegion) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C 中 NULL → 跳过清理。Rust 默认空操作：框架层自行释放资源。
    fn ev_delete(&self, _region: &mut VirRegion) {}

    // C 中 NULL → 跳过引用。PFN 模型下由 PageFrames 管理 refcount，
    // 默认空操作即可。ContiguousAnonymous 覆盖此方法（空操作体），
    // 仅为了在 §3.8 表格中显式标注"有特化"。
    fn ev_reference(&self, _frames: &mut PageFrames, _slot: PageSlot) {}

    // C 中有实现的类型在此回调中释放物理页（如 anon_unreference → free_mem）。
    // PFN 模型下物理页释放由 PfnAllocator::free_pfn() 负责（§3.4），
    // 因此默认空操作。
    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    // ── 页错误处理（§3.4：PFN 签名）──

    // C 中所有类型都实现了 ev_pagefault（无 NULL）。
    // Rust 默认 Handled：已映射页直接返回，适用于 SharedMemory 等简单类型。
    // AnonymousMemory/DirectPhysical 等需要覆盖此方法。
    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::Handled)
    }

    // ── 区域操作回调 ──

    // C 中 NULL → 通用扩展逻辑（region.c:1037-1040）。
    // Rust 默认 Ok(())：框架层处理页面分配。
    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C 中 NULL → EINVAL（region.c:1164），即"不支持 split"。
    // Rust 默认 Err(NotSupported)：与 C 语义对齐。
    // AnonymousMemory/ContiguousAnonymous/MappedFile 覆盖为 Ok(())。
    fn ev_split(
        &self,
        _proc: &ActiveProc<'_>,
        _original: &VirRegion,
        _left: &mut VirRegion,
        _right: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    // C 中 NULL → EINVAL（region.c:1096），即"不支持 low_shrink"。
    // Rust 默认 Err(NotSupported)：与 C 语义对齐。
    // AnonymousMemory/CacheMemory/MappedFile 覆盖为 Ok(())。
    fn ev_low_shrink(
        &self,
        _region: &mut VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Err(MemTypeError::NotSupported)
    }

    // C 中 NULL → 跳过。PFN 模型下不需要运行时检查。
    fn ev_sanitycheck(
        &self,
        _frames: &PageFrames,
        _slot: PageSlot,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    // ── 查询方法（§3.4：PFN 签名）──

    // C 中所有类型都实现了 writable（无 NULL）。
    // Rust 默认 false：MappedFile 等类型不可直接写。
    // AnonymousMemory 覆盖为 remaps>0 或 refcount==1 检查（CoW 语义）。
    fn writable(&self, _frames: &PageFrames, _slot: PageSlot, _region: &VirRegion) -> bool {
        false
    }

    // C 中 NULL → 跳过复制特化。Rust 默认 Ok(())。
    // MappedFile/SharedMemory/DirectPhysical 覆盖以复制 param。
    fn ev_copy(
        &self,
        _src: &VirRegion,
        _dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    // C 中 NULL → 返回 0（隐式）。Rust 默认 0。
    // AnonymousMemory/SharedMemory 覆盖以返回实际 ID。
    fn region_id(&self, _region: &VirRegion) -> u32 {
        0
    }

    // C 中 NULL → 返回 0（隐式）。Rust 默认 0。
    // AnonymousMemory 覆盖为 1+remaps（CoW 引用计数）。
    fn ref_count(&self, _region: &VirRegion) -> i32 {
        0
    }

    // C 中 NULL → 返回 0（默认缓存）。Rust 默认 empty()。
    // DirectPhysical/ContiguousAnonymous 覆盖为 NO_CACHE（§3.7）。
    fn pt_flags(&self, _region: &VirRegion) -> PageFlags {
        PageFlags::empty()
    }
}
```

**默认值的三种语义**（对应 §3.1 表格）：

| 语义 | C 中 NULL 行为 | Rust 默认 | 适用方法 |
|------|-------------|-----------|---------|
| 跳过 | 不调用，继续执行 | 空操作 / `Ok(())` | `ev_new`, `ev_delete`, `ev_reference`, `ev_copy` |
| 通用默认 | 执行通用逻辑 | `Ok(())` | `ev_resize` |
| 不支持 | 返回 EINVAL | `Err(NotSupported)` | `ev_split`, `ev_low_shrink` |

另有 `ev_pagefault`/`writable`/`ev_sanitycheck`/`region_id`/`ref_count`/`pt_flags` 在 C 中无 NULL 情况（所有类型都实现了），Rust 默认值取最保守的语义：`Handled`/`false`/`Ok(())`/`0`/`0`/`empty()`。

### 4.3 AnonymousMemory

> 设计决策：§3.2（动态分发）、§3.3（ZST + const）、§3.4（PFN 签名）、§3.5（per-page memtype）

匿名内存是最核心的内存类型，覆盖了 `malloc`/`mmap(MAP_ANON)` 等常见场景。它有两个关键行为：**CoW（写时复制）**和 **per-page 可写判定**。CoW 使得 fork 后父子进程共享物理页，写入时才复制；per-page 可写判定通过 `refcount == 1` 检查实现——只有独占引用的页才可写，共享页必须先 CoW。

```rust
pub(crate) struct AnonymousMemory;

impl AnonymousMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl MemType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }

    fn writable(&self, frames: &PageFrames, slot: PageSlot, region: &VirRegion) -> bool {
        if !slot.is_mapped() {
            return false;
        }
        if region.remaps > 0 {
            return true;
        }
        frames.get(slot.pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {
        // 物理页释放由调用方通过 PfnAllocator::free_pfn() 负责。
        // Minix3 的 anon_unreference 在此处调用 free_mem，
        // 但 PFN 模型下 PageFrames 只管理 refcount/flags，
        // PfnAllocator 负责分配/释放。
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        let slot = region.get_slot(offset);

        match slot {
            None => {
                return Ok(PagefaultResult::NeedNewPage);
            }
            Some(s) if !s.is_mapped() => {
                return Ok(PagefaultResult::NeedNewPage);
            }
            _ => {}
        }

        let slot = slot.unwrap();
        let refcount = frames.get(slot.pfn)
            .map(|s| s.refcount)
            .unwrap_or(0);

        if refcount < 2 || !write {
            return Ok(PagefaultResult::Handled);
        }

        if !region.is_writable() {
            return Ok(PagefaultResult::AccessViolation);
        }

        Ok(PagefaultResult::NeedCow)
    }

    fn region_id(&self, region: &VirRegion) -> u32 {
        region.id as u32
    }

    fn ref_count(&self, region: &VirRegion) -> i32 {
        1 + region.remaps
    }

    fn ev_split(
        &self,
        _proc: &ActiveProc<'_>,
        _original: &VirRegion,
        _left: &mut VirRegion,
        _right: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 anon_split 为空操作（return）。
        Ok(())
    }

    fn ev_low_shrink(
        &self,
        _region: &mut VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 anon_lowshrink 为空操作（return OK）。
        Ok(())
    }
}
```

**与 Minix3 `anon_pagefault` 的语义对应**（Ch2§2.2.1）：

| Minix3 路径 | Rust 路径 | 说明 |
|------------|----------|------|
| `ph->ph->phys == MAP_NONE` | `slot.is_mapped() == false` | 物理页不存在 → NeedNewPage |
| `ph->ph->refcount < 2 \|\| !write` | `refcount < 2 \|\| !write` | 无需 CoW → Handled |
| `!(region->flags & VR_WRITABLE)` | `!region.is_writable()` | 只读区域写入 → AccessViolation |
| `mem_cow()` | `NeedCow` | 框架层执行 CoW |

**修复了 Minix3 源码缺陷**：Ch2§2.2.1 分析了 `anon_pagefault` 在"无需 CoW"路径中预分配了 `new_page_cl` 但未释放，造成内存泄漏。Rust 版本不在 `ev_pagefault` 中预分配页面，CoW 的页面分配由框架层在收到 `NeedCow` 后执行（`cow_resolve_core`）。

### 4.4 DirectPhysical

> 设计决策：§3.4（PFN 签名）、§3.7（NO_CACHE）

直接物理映射用于设备内存映射（如 MMIO 寄存器）。与匿名内存的关键区别：**物理地址在创建时确定**（`VrParam::Direct { phys }`），页错误时直接计算 PFN 并映射，无需分配新页。设备内存不可缓存，因此 `pt_flags` 返回 `NO_CACHE`。

```rust
pub(crate) struct DirectPhysical;

impl DirectPhysical {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl MemType for DirectPhysical {
    fn name(&self) -> &'static str {
        "physical memory mapping"
    }

    fn writable(&self, frames: &PageFrames, slot: PageSlot, region: &VirRegion) -> bool {
        if !slot.is_mapped() {
            return false;
        }
        if region.remaps > 0 {
            return true;
        }
        frames.get(slot.pfn)
            .map(|s| s.refcount == 1)
            .unwrap_or(false)
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut VirRegion,
        frames: &mut PageFrames,
        offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if let VrParam::Direct { phys: base_phys } = &region.param {
            if base_phys.0 == 0 {
                return Err(MemTypeError::InvalidParam);
            }
            let slot = region.get_slot(offset);
            if let Some(s) = slot {
                if s.is_mapped() {
                    return Ok(PagefaultResult::Handled);
                }
            }
            // Minix3 的 phys_pagefault 直接计算物理地址并设置到 phys_region：
            //   phmem = region->param.phys + ph->offset;
            //   ph->ph->phys = phmem;
            // PFN 模型下，在回调内直接 map_page 设置映射，然后返回 Handled。
            let phys_addr = minix_types::PhysBytes(base_phys.0 + offset.0);
            let pfn = frames.phys_to_pfn(phys_addr);
            let memtype = region.def_memtype
                .ok_or(MemTypeError::InvalidParam)?;
            region.map_page(frames, offset, pfn, memtype);
            Ok(PagefaultResult::Handled)
        } else {
            Err(MemTypeError::InvalidParam)
        }
    }

    fn ev_copy(
        &self,
        src: &VirRegion,
        dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    fn pt_flags(&self, _region: &VirRegion) -> PageFlags {
        PageFlags::NO_CACHE
    }

    fn ev_split(
        &self,
        _proc: &ActiveProc<'_>,
        _original: &VirRegion,
        _left: &mut VirRegion,
        _right: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 mem_type_directphys 未设置 ev_split（NULL → EINVAL）。
        // 直接物理映射不支持 split，使用默认 Err(NotSupported)。
        Err(MemTypeError::NotSupported)
    }
}
```

**与 Minix3 `phys_pagefault` 的语义对应**（Ch2§2.2.2）：

| Minix3 路径 | Rust 路径 | 说明 |
|------------|----------|------|
| `region->param.phys == 0` | `base_phys.0 == 0` | 未设置物理基地址 → InvalidParam |
| `ph->ph->phys == MAP_NONE` | `!slot.is_mapped()` | 未映射 → 在回调内 map_page |
| `phmem = region->param.phys + ph->offset` | `frames.phys_to_pfn(base_phys + offset)` | 计算物理地址对应的 PFN |
| `ph->ph->phys = phmem` | `region.map_page(frames, offset, pfn, memtype)` | 在回调内直接设置映射 |
| `ARM_VM_PTE_DEVICE` | `PageFlags::NO_CACHE` | §3.7 架构无关化 |

**`ev_unreference` 为空操作**：Ch2§2.2.2 分析了 `phys_unreference` 不释放物理页（设备内存不是 VM 管理的），Rust 版本同样为空操作。

**`ev_split` 返回 `NotSupported`**：Minix3 的 `mem_type_directphys` 未设置 `ev_split`（NULL → EINVAL），直接物理映射不支持区域分割。

### 4.5 ContiguousAnonymous

> 设计决策：§3.3（ZST）、§3.7（NO_CACHE）

连续匿名内存用于 DMA 等需要物理连续页的场景。与普通匿名内存的关键区别：**不可 resize、不可 fork、不可 split**。C 源码中 `anon_contig_reference` 返回 ENOMEM 试图拒绝 fork，但其返回值被 region.c 忽略（bug）；Rust 通过 `ev_copy` 返回 `NotSupported` 正确拒绝。

```rust
pub(crate) struct ContiguousAnonymous;

impl ContiguousAnonymous {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl MemType for ContiguousAnonymous {
    fn name(&self) -> &'static str {
        "contiguous anonymous memory"
    }

    fn writable(&self, _frames: &PageFrames, slot: PageSlot) -> bool {
        slot.is_mapped()
    }

    fn ev_reference(&self, _frames: &mut PageFrames, _slot: PageSlot) {
        // Minix3 的 anon_contig_reference 返回 ENOMEM（拒绝 fork），
        // 但其返回值在 region.c 中被忽略（bug）。
        // Rust 版本：ev_reference 为空操作，fork 通过 ev_copy 正确拒绝。
    }

    fn ev_new(&self, region: &mut VirRegion) -> Result<(), MemTypeError> {
        let pages = region.physblocks.len();
        if pages == 0 {
            return Ok(());
        }
        // TODO: 实现连续物理页分配
        // Minix3 的 anon_contig_new 一次性分配所有连续物理页，
        // 等价于 alloc_contig_mem(len, ...)。
        Ok(())
    }

    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 anon_contig_resize 返回 ENOMEM，拒绝 resize。
        Err(MemTypeError::NotSupported)
    }

    fn ev_copy(
        &self,
        _src: &VirRegion,
        _dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 anon_contig_reference 返回 ENOMEM（拒绝 fork），
        // 但其返回值在 region.c 中被忽略（bug）。
        // Rust 版本通过 ev_copy 正确拒绝 fork。
        Err(MemTypeError::NotSupported)
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // Minix3 的 anon_contig_pagefault 直接 panic，
        // 因为 ev_new 已预分配所有页，不应触发页错误。
        // PFN 模型下返回 NeedNewPage 而非 panic，
        // 因为页分配由框架层统一处理。
        Ok(PagefaultResult::NeedNewPage)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {
        // 同 AnonymousMemory，物理页释放由 PfnAllocator 负责。
    }

    fn pt_flags(&self, _region: &VirRegion) -> PageFlags {
        PageFlags::NO_CACHE
    }

    fn ev_split(
        &self,
        _proc: &ActiveProc<'_>,
        _original: &VirRegion,
        _left: &mut VirRegion,
        _right: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 anon_contig_split 为空操作（return）。
        // 连续内存允许 split，split 后两部分仍各自连续。
        Ok(())
    }
}
```

**与 Minix3 `anon_contig_pagefault` 的语义对应**（Ch2§2.2.3）：

| Minix3 行为 | Rust 行为 | 说明 |
|------------|----------|------|
| `ev_new` 预分配连续物理页 | TODO | 等待连续物理页分配器实现 |
| `ev_pagefault` panic | `NeedNewPage` | PFN 模型下不 panic |
| `ev_reference` 返回 ENOMEM（被忽略） | `ev_copy` 返回 `NotSupported` | Rust 正确拒绝 fork |
| `ev_resize` 返回 ENOMEM | `ev_resize` 返回 `NotSupported` | 拒绝 resize |
| `anon_contig_split` 空操作 | `ev_split` → `Ok(())` | 允许 split |
| `ARM_VM_PTE_DEVICE` | `NO_CACHE` | §3.7 |

### 4.6 CacheMemory

> 设计决策：§3.4（PFN 签名）、§3.5（per-page memtype）

缓存内存用于文件系统块缓存。与匿名内存的关键区别：**物理页通过 `pb_link` 从缓存池获取**，而非动态分配；**不可 resize**（缓存块大小固定）。`ev_delete` 清除 `VrParam::PbCache` 中的 pfn，防止悬空引用。

```rust
pub(crate) struct CacheMemory;

impl CacheMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl MemType for CacheMemory {
    fn name(&self) -> &'static str {
        "cache memory"
    }

    fn writable(&self, _frames: &PageFrames, slot: PageSlot) -> bool {
        // Minix3 的 cache_writable 返回 pr->ph->phys != MAP_NONE，
        // 即物理页已分配则可写。PFN 模型下等价于 slot.is_mapped()。
        slot.is_mapped()
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // TODO: 实现缓存索引查找和链接
        // Minix3 的 cache_pagefault 通过 pb_link 将预分配的缓存块
        // 链接到 phys_region。PFN 模型下需要对应的 PageSlot 操作。
        Ok(PagefaultResult::NeedNewPage)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 cache_resize 返回 ENOMEM，拒绝 resize。
        Err(MemTypeError::NotSupported)
    }

    fn ev_delete(&self, region: &mut VirRegion) {
        if let VrParam::PbCache { pfn } = &mut region.param {
            *pfn = 0;
        }
    }

    fn ev_low_shrink(
        &self,
        _region: &mut VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 cache_lowshrink 为空操作（return OK）。
        Ok(())
    }
}
```

**与 Minix3 `cache_pagefault` 的语义对应**（Ch2§2.2.4）：

| Minix3 行为 | Rust 行为 | 说明 |
|------------|----------|------|
| `pb_link` 链接缓存块 | TODO | 等待缓存系统实现 |
| `cache_delete` 清除 pfn | `ev_delete` 清除 `VrParam::PbCache` | Rust 侧增强：C 中 `mem_type_cache` 无 `ev_delete`（NULL），清理逻辑在 `do_forgetcache`/`rmcache` 中；PFN 模型下需防止悬空引用 |
| `cache_reference` 空操作 | 默认实现 | 无需特化 |

### 4.7 SharedMemory

> 设计决策：§3.2（动态分发支持 ev_copy）

共享内存用于 IPC 共享内存段（`shmget`/`shmat`）。与匿名内存的关键区别：**多进程显式共享同一物理页**（非 CoW 隐式共享），因此 `writable` 直接返回 `slot.is_mapped()`，不做 refcount 检查。`ev_copy` 复制 `VrParam::Shared` 参数，使 fork 后的子进程也指向同一共享段。

```rust
pub(crate) struct SharedMemory;

impl SharedMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl MemType for SharedMemory {
    fn name(&self) -> &'static str {
        "shared memory"
    }

    fn writable(&self, _frames: &PageFrames, slot: PageSlot) -> bool {
        slot.is_mapped()
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    fn ev_copy(
        &self,
        src: &VirRegion,
        dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }

    fn ev_split(
        &self,
        _proc: &ActiveProc<'_>,
        _original: &VirRegion,
        _left: &mut VirRegion,
        _right: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 mem_type_shared 未设置 ev_split（NULL → EINVAL）。
        // 共享内存不支持 split，使用默认 Err(NotSupported)。
        Err(MemTypeError::NotSupported)
    }

    fn ev_low_shrink(
        &self,
        _region: &mut VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 mem_type_shared 未设置 ev_lowshrink（NULL → EINVAL）。
        // 共享内存不支持 low_shrink，使用默认 Err(NotSupported)。
        Err(MemTypeError::NotSupported)
    }
}
```

**与 Minix3 `shared_pagefault` 的语义对应**（Ch2§2.2.6）：

| Minix3 行为 | Rust 行为 | 说明 |
|------------|----------|------|
| `getsrc()` 获取源区域 | TODO | 等待共享内存系统实现 |
| `pb_link` 链接源物理块 | TODO | 同上 |
| `shared_copy` 复制 param | `ev_copy` 复制 `VrParam::Shared` | 已实现 |
| `ev_split` NULL → EINVAL | `ev_split` → `Err(NotSupported)` | 不支持 split |
| `ev_lowshrink` NULL → EINVAL | `ev_low_shrink` → `Err(NotSupported)` | 不支持 low_shrink |
| `ev_pagefault` | 使用默认（Handled） | 等待补全 |

### 4.8 MappedFile

> 设计决策：§3.5（per-page memtype + CoW 类型变更）

文件映射内存用于 `mmap` 映射文件到进程地址空间。与匿名内存的关键区别：**页内容来自文件**（通过 VFS 异步读取），**不可直接写**（`writable` 始终返回 `false`），写入触发 CoW 后类型变更为匿名内存（§3.5）。`ev_copy` 复制 `VrParam::File` 参数，fork 后子进程共享同一文件映射。

```rust
pub(crate) struct MappedFile;

impl MappedFile {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl MemType for MappedFile {
    fn name(&self) -> &'static str {
        "mapped file"
    }

    fn writable(&self, _frames: &PageFrames, _slot: PageSlot) -> bool {
        // Minix3 的 mappedfile_writable 始终返回 0（不可直接写）。
        // 文件映射页的写入通过 CoW 机制处理。
        false
    }

    fn ev_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut VirRegion,
        _frames: &mut PageFrames,
        _offset: VirBytes,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // MappedFile::ev_pagefault 完整设计见 23-vfs-interaction.md §4.7
        // 缓存查找 → 命中 → Handled/NeedCow
        // 缓存未命中 → NeedVfsIo（对应 Minix3 的 SUSPEND）
        // 缺页框架将 NeedVfsIo 转换为 PagefaultAction::Suspended，构造 VfsRequest(FdIo)
        Ok(PagefaultResult::NeedVfsIo)
    }

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

    fn ev_copy(
        &self,
        src: &VirRegion,
        dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 mappedfile_copy 调用 mappedfile_setfile 复制文件映射信息，
        // 并返回 OK（fork 文件映射区域是正常行为）。
        // fdref 引用计数由调用方负责（fdref_ref），见 23-vfs-interaction.md §4.5
        dst.param = src.param.clone();
        Ok(())
    }

    fn ev_split(
        &self,
        _proc: &ActiveProc<'_>,
        _original: &VirRegion,
        _left: &mut VirRegion,
        _right: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 mappedfile_split 复制 vm_region_param。
        // TODO: 待 VrParam::File 补全后实现参数复制。
        Ok(())
    }

    fn ev_low_shrink(
        &self,
        _region: &mut VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        // Minix3 的 mappedfile_lowshrink 调整 vm_region_param offset。
        // TODO: 待 VrParam::File 补全后实现偏移调整。
        Ok(())
    }
}
```

**与 Minix3 `mappedfile_pagefault` 的语义对应**（Ch2§2.2.5）：

| Minix3 行为 | Rust 行为 | 说明 |
|------------|----------|------|
| 缓存查找（`pb_cache`） | `PageCache::find_by_inode/find_by_device` | 见 23-vfs-interaction.md §4.9 |
| VFS 异步读取（`SUSPEND`） | `PagefaultResult::NeedVfsIo` → `PagefaultAction::Suspended` | 见 23-vfs-interaction.md §4.6 |
| `mappedfile_writable` 返回 0 | `writable` 返回 `false` | 文件映射不可直接写 |
| `mappedfile_copy` 返回 OK | `ev_copy` 复制 `VrParam::File` | fork 文件映射是正常行为 |
| `mappedfile_split` 复制 param | `ev_split` 设置子区域 `fdref_id` | 见 23-vfs-interaction.md §4.7 |
| `mappedfile_lowshrink` 调整 offset | `ev_low_shrink` 调整 `offset` | 见 23-vfs-interaction.md §4.7 |

> `VrParam::File` 已设计 `fdref_id: Option<u32>` 字段，通过 `FdRefTable` 管理引用计数。详见 [23-vfs-interaction.md](23-vfs-interaction.md) §3.2、§4.5、§4.8。

### 4.9 全局实例

> 设计决策：§3.3（ZST + const static）

所有内存类型都是零大小单元结构体（ZST），可以在编译时用 `const fn` 构造，然后声明为 `static` 全局实例。这对应 Minix3 中的 `&mem_type_anon` 等静态全局指针，但 Rust 的 `&'static dyn MemType` 提供了类型安全的动态分发。

```rust
pub(crate) static MEM_TYPE_ANON: AnonymousMemory = AnonymousMemory::new();
pub(crate) static MEM_TYPE_DIRECT: DirectPhysical = DirectPhysical::new();
pub(crate) static MEM_TYPE_SHARED: SharedMemory = SharedMemory::new();
pub(crate) static MEM_TYPE_CONTIG_ANON: ContiguousAnonymous = ContiguousAnonymous::new();
pub(crate) static MEM_TYPE_CACHE: CacheMemory = CacheMemory::new();
pub(crate) static MEM_TYPE_MAPPED_FILE: MappedFile = MappedFile::new();
```

**使用方式**：`VirRegion` 的 `def_memtype` 字段类型为 `Option<&'static dyn MemType>`，通过 `with_memtype()` 构造：

```rust
let region = VirRegion::with_memtype(vaddr, length, flags, &MEM_TYPE_ANON);
```

**懒映射**：`map_lazy` 从 `def_memtype` 复制到 PageSlot：

```rust
pub(crate) fn map_lazy(&mut self, offset: VirBytes) {
    let slot = PageSlot::new(PFN_NONE, offset, self.def_memtype);
    self.physblocks[page_idx] = Some(slot);
}
```

**CoW 后类型变更**：`cow_resolve_core` 中新页的 memtype 设为 anon（§3.5）：

```rust
region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON);
```

---

## 5. 参见

- [10-phys-pagestate.md](10-phys-pagestate.md) — 全局物理页状态（PageState/PageFrames，替代原 phys_block）
- [11-region-mapping.md](11-region-mapping.md) — 页映射（PageSlot + VirRegion，替代原 vir_region + phys_region）
- [14-cow-mechanism.md](14-cow-mechanism.md) — CoW 机制与 cow_resolve_core 实现
- [15-pagefault.md](15-pagefault.md) — 页错误处理流程与 memtype 回调的调用时机
- [16-vm-fork.md](16-vm-fork.md) — fork 中的 ev_copy 与内存类型继承
- [18-vm-mmap.md](18-vm-mmap.md) — mmap 使用 mappedfile/shared 内存类型
- [25-page-cache.md](25-page-cache.md) — CacheMemory/SharedMemory/ContiguousAnonymous/MappedFile 补全

---

*分类: VM私有*
