# 11-memtype: 内存类型系统

> **分类**: VM库  
> **源码**: `minix3/minix/servers/vm/memtype.h`, `mem_anon.c`  
> **说明**: 多态内存类型系统，支持匿名内存、文件映射、物理内存等不同类型

---

## 1. 概述

### 1.1 内存类型系统的作用

**为什么需要内存类型系统？**

在操作系统中，不同来源和用途的内存有不同的行为特征：

| 内存类型 | 来源 | 分配时机 | 释放时机 | 特殊处理 |
|---------|------|---------|---------|---------|
| **匿名内存** | 按需分配 | 页错误时 | 进程退出/CoW | 支持写时复制 |
| **文件映射** | 文件系统 | mmap 时 | munmap 时 | 需同步到磁盘 |
| **共享内存** | IPC 创建 | shmget 时 | 进程分离时 | 多进程可见 |
| **物理映射** | 设备寄存器 | 驱动初始化 | 驱动卸载 | 不由 VM 管理 |

**核心问题**：如何用统一的接口管理这些行为各异的内存？

**Minix3 的解决方案**：使用**函数指针表**实现多态，每种内存类型实现一组回调函数。

### 1.2 多态设计

**C 语言的多态实现**

Minix3 使用函数指针表实现多态：

```c
// memtype.h
typedef struct mem_type {
    const char *name;                          // 类型名称
    int (*ev_new)(struct vir_region *region);  // 创建区域
    void (*ev_delete)(struct vir_region *region); // 删除区域
    int (*ev_reference)(struct phys_region *pr, struct phys_region *newpr); // 引用
    int (*ev_unreference)(struct phys_region *pr); // 取消引用
    int (*ev_pagefault)(struct vmproc *vmp, struct vir_region *region,
     struct phys_region *ph, int write, vfs_callback_t cb, void *state,
     int len, int *io);                       // 页错误处理
    int (*ev_resize)(struct vmproc *vmp, struct vir_region *vr, vir_bytes len); // 大小调整
    void (*ev_split)(struct vmproc *vmp, struct vir_region *vr,
            struct vir_region *r1, struct vir_region *r2); // 区域分割
    int (*writable)(struct phys_region *pr);   // 是否可写
    int (*ev_sanitycheck)(struct phys_region *pr, const char *file, int line); // 完整性检查
    int (*ev_copy)(struct vir_region *vr, struct vir_region *newvr); // 复制区域
    int (*ev_lowshrink)(struct vir_region *vr, vir_bytes len); // 收缩低端地址
    u32_t (*regionid)(struct vir_region *vr);  // 区域 ID
    int (*refcount)(struct vir_region *vr);    // 引用计数
    int (*pt_flags)(struct vir_region *vr);    // 页表标志
} mem_type_t;
```

**调用方式**：

```c
// 统一接口，不同行为
int result = pr->memtype->ev_pagefault(vmp, region, ph, write, ...);
```

### 1.3 与 Minix3 的对应关系

**全局内存类型实例**

| Minix3 变量 | 用途 |
|-------------|------|
| `mem_type_anon` | 普通堆内存、栈 |
| `mem_type_directphys` | 设备内存映射 |
| `mem_type_anon_contig` | DMA 缓冲区（物理连续） |
| `mem_type_cache` | 文件系统缓存 |
| `mem_type_mappedfile` | mmap 文件映射 |
| `mem_type_shared` | 进程间共享内存 |

**源码位置**：

| 类型 | Minix3 源文件 |
|------|--------------|
| `mem_type_anon` | `mem_anon.c` |
| `mem_type_directphys` | `mem_directphys.c` |
| `mem_type_shared` | `mem_shared.c` |
| `mem_type_mappedfile` | `mem_mapped.c` |

### 1.4 内存类型与区域的关系

**两层 memtype**：

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

#### 2.1.2 回调函数详解

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

**回调函数实现**

| 回调 | 实现函数 | 功能 |
|------|---------|------|
| `ev_unreference` | `anon_unreference` | 引用为 0 时释放物理页 |
| `ev_pagefault` | `anon_pagefault` | 按需分配或 CoW |
| `ev_resize` | `anon_resize` | 允许扩展（收缩忽略） |
| `ev_split` | `anon_split` | 空实现 |
| `ev_lowshrink` | `anon_lowshrink` | 空实现 |
| `writable` | `anon_writable` | 判断是否可写 |
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

    // 预分配一页（可能用于 CoW）
    if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) {
        printf("anon_pagefault: out of memory\n");
        return ENOMEM;
    }
    new_page = CLICK2ABS(new_page_cl);

    // 情况 1: 全新页面，从未分配
    if(ph->ph->phys == MAP_NONE) {
        ph->ph->phys = new_page;
        return OK;
    }

    // 情况 2: 只有一个引用，或非写操作
    if(ph->ph->refcount < 2 || !write) {
        // 预分配的页面不需要，释放
        free_mem(new_page_cl, 1);
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

**回调函数实现**

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

**回调函数实现**

| 回调 | 实现函数 | 功能 |
|------|---------|------|
| `ev_new` | `anon_contig_new` | 一次性分配所有物理页 |
| `ev_reference` | `anon_contig_reference` | 返回错误（不支持 fork） |
| `ev_unreference` | `anon_contig_unreference` | 复用匿名内存实现 |
| `ev_pagefault` | `anon_contig_pagefault` | panic（不应发生） |
| `ev_resize` | `anon_contig_resize` | 返回错误 |
| `ev_split` | `anon_contig_split` | 空实现 |
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

**回调函数实现**

| 回调 | 实现函数 | 功能 |
|------|---------|------|
| `ev_reference` | `cache_reference` | 空操作（OK） |
| `ev_unreference` | `cache_unreference` | 复用匿名内存实现 |
| `ev_pagefault` | `cache_pagefault` | 链接预分配的缓存块 |
| `ev_resize` | `cache_resize` | 返回错误 |
| `ev_lowshrink` | `cache_lowshrink` | 空操作（OK） |
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
        hb = find_cached_page_bydev(dev, dev_off + offset, ...);
        
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
        
        // 检查类型必须是匿名内存
        if(phys_region->memtype != &mem_type_anon &&
           phys_region->memtype != &mem_type_anon_contig) {
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

**回调函数实现**

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
   - 缓存未命中且有回调：向 VFS 请求（`vfs_request`），返回 `SUSPEND`
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

**回调函数实现**

| 回调 | 实现函数 | 功能 |
|------|---------|------|
| `ev_unreference` | `shared_unreference` | 复用匿名内存实现 |
| `ev_pagefault` | `shared_pagefault` | 链接到源区域的物理页 |
| `ev_sanitycheck` | `shared_sanitycheck` | 返回 OK |
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
┌─────────────────────────────────────────────────────────────┐
│                    共享内存架构                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   进程 A (源)                                                │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ vir_region (匿名内存)                                │   │
│   │   - vaddr: 0x400000                                 │   │
│   │   - def_memtype: &mem_type_anon                     │   │
│   │   - remaps: 2  ←─────────────────┐                  │   │
│   │   ┌─────────────────────────┐    │                  │   │
│   │   │ phys_region             │    │                  │   │
│   │   │   - ph->phys: 0x12345000│────┼─────────────┐    │   │
│   │   └─────────────────────────┘    │             │    │   │
│   └─────────────────────────────────────────────────────┘   │
│                                        │             │       │
│   进程 B (共享者 1)                      │             │       │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ vir_region (共享内存)              │             │   │
│   │   - vaddr: 0x500000               │             │   │
│   │   - def_memtype: &mem_type_shared │             │   │
│   │   - param.shared:                 │             │   │
│   │       ep: 进程A端点               │             │   │
│   │       vaddr: 0x400000 ────────────┘             │   │
│   │   ┌─────────────────────────┐                   │   │
│   │   │ phys_region             │                   │   │
│   │   │   - ph->phys: 0x12345000│←──────────────────┘   │
│   │   └─────────────────────────┘                       │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   进程 C (共享者 2)                                          │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ vir_region (共享内存)                                │   │
│   │   - vaddr: 0x600000                                 │   │
│   │   - def_memtype: &mem_type_shared                   │   │
│   │   - param.shared:                                   │   │
│   │       ep: 进程A端点                                 │   │
│   │       vaddr: 0x400000                               │   │
│   │   ┌─────────────────────────┐                       │   │
│   │   │ phys_region             │                       │   │
│   │   │   - ph->phys: 0x12345000│← 同一物理页           │   │
│   │   └─────────────────────────┘                       │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   物理内存: 0x12345000 被三个进程共享                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
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

> **方案四标注**：CoW 中的 `sys_abscopy` 在方案四下被 `vm_phys_to_virt() + copy_nonoverlapping()` 替代。这不是性能优化，而是概念消除——`sys_abscopy` 的存在前提是 VM 无法直接访问物理内存，direct map 消除了这个前提，`sys_abscopy` 就没有存在的理由了。
>
> 关键理解：Minix3 内核不是独立线程。VM 发起 `sys_abscopy` 时，执行流程是 VM(ring 3) → syscall → 内核(ring 0，仍在 VM 进程上下文中) → memcpy → 返回 VM(ring 3)。内核的"执行"就是 VM 自己在 ring 0 的执行，VM 仍然被阻塞，没有并行性优势。因此 `sys_abscopy` 相比 VM 直接 memcpy 多了 ring 切换开销（~100-200 cycles），没有任何补偿收益。
>
> 这与 15-cow-mechanism.md §2.2.1 的 `mem_cow()` 简化是同一个范式转变——VM 从"委托内核复制"变为"自己直接复制"。

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
        // 释放预分配的页面
        free_mem(new_page_cl, 1);
        return OK;
    }

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
| `ev_sanitycheck` | ✅ | ✅ | ✅ | ✅ | ✅ | - |
| `regionid` | ✅ | - | - | - | - | ✅ |
| `refcount` | ✅ | - | - | - | - | ✅ |
| `pt_flags` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

---

## 3. Rust 设计决策

### 3.1 MemoryType trait

**设计目标**

将 Minix3 的函数指针表转换为 Rust 的 trait，实现类型安全的多态。

**C 与 Rust 对比**

| 特性 | Minix3 (C) | Rust |
|------|-----------|------|
| 多态机制 | 函数指针表 | trait |
| 类型安全 | 运行时检查 | 编译时检查 |
| 空指针风险 | 存在 | 不存在 |
| 默认行为 | 需手动检查 NULL | 提供默认实现 |
| 扩展性 | 添加函数指针 | 实现 trait |

**trait 定义**

```rust
/// 内存类型 trait
///
/// 定义内存类型的核心操作接口。
/// 对应 Minix3: `struct mem_type`
pub(crate) trait MemType: Send + Sync {
    /// 获取类型名称
    fn name(&self) -> &'static str;

    /// 创建新区域时的回调
    ///
    /// 对应 Minix3: `ev_new`
    fn on_new(&self, _region: &mut VirRegion) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// 删除区域时的回调
    ///
    /// 对应 Minix3: `ev_delete`
    fn on_delete(&self, _region: &mut VirRegion) {}

    /// 引用物理区域时的回调
    ///
    /// 对应 Minix3: `ev_reference`
    fn on_reference(
        &self,
        _src: &PhysRegion,
        _dst: &mut PhysRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// 取消引用物理区域时的回调
    ///
    /// 对应 Minix3: `ev_unreference`
    fn on_unreference(&self, _pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
    }

    /// 页错误处理回调
    ///
    /// 对应 Minix3: `ev_pagefault`
    fn on_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        _region: &mut VirRegion,
        _pr: &mut PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::Handled)
    }

    /// 区域大小调整回调
    ///
    /// 对应 Minix3: `ev_resize`
    fn on_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// 区域分割回调
    ///
    /// 对应 Minix3: `ev_split`
    fn on_split(
        &self,
        _proc: &ActiveProc<'_>,
        _original: &VirRegion,
        _left: &mut VirRegion,
        _right: &mut VirRegion,
    ) {
    }

    /// 检查是否可写
    ///
    /// 对应 Minix3: `writable`
    fn is_writable(&self, _pr: &PhysRegion) -> bool {
        false
    }

    /// 复制区域时的回调
    ///
    /// 对应 Minix3: `ev_copy`
    fn on_copy(
        &self,
        _src: &VirRegion,
        _dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// 低地址收缩回调
    ///
    /// 对应 Minix3: `ev_lowshrink`
    fn on_low_shrink(
        &self,
        _region: &mut VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// 一致性检查
    ///
    /// 对应 Minix3: `ev_sanitycheck`
    fn on_sanitycheck(&self, _pr: &PhysRegion) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// 获取区域 ID
    ///
    /// 对应 Minix3: `regionid`
    fn region_id(&self, _region: &VirRegion) -> u32 {
        0
    }

    /// 获取引用计数
    ///
    /// 对应 Minix3: `refcount`
    fn ref_count(&self, _region: &VirRegion) -> i32 {
        0
    }

    /// 获取页表标志
    ///
    /// 对应 Minix3: `pt_flags`
    fn pt_flags(&self, _region: &VirRegion) -> i32 {
        0
    }
}
```

**辅助类型定义**

```rust
/// 内存类型错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemTypeError {
    /// 内存不足
    NoMemory,
    /// 无效参数
    InvalidParam,
    /// 不支持的操作
    NotSupported,
    /// IO 错误
    IoError,
    /// 复制失败
    CopyFailed,
}

/// 页错误处理结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PagefaultResult {
    /// 已处理
    Handled,
    /// 需要分配新页
    NeedNewPage,
    /// 需要 CoW
    NeedCow,
    /// 访问违规
    AccessViolation,
}
```

**多态实现方式**

**方式 1: 静态分发 (泛型)**

```rust
// 编译时确定具体类型，零开销抽象
pub struct VirRegion<T: MemType> {
    vaddr: usize,
    length: usize,
    mem_type: T,
    // ...
}

// 调用时
fn handle_pagefault<T: MemType>(region: &mut VirRegion<T>) {
    region.mem_type.on_pagefault(...);
}
```

优点：
- 零运行时开销
- 编译时优化

缺点：
- 无法在运行时更改类型
- 需要为每种类型生成代码

**方式 2: 动态分发 (trait object)**

```rust
// 运行时确定具体类型，类似 C 的函数指针
pub struct VirRegion {
    vaddr: usize,
    length: usize,
    mem_type: &'static dyn MemType,  // 或 Arc<dyn MemType>
    // ...
}

// 调用时
fn handle_pagefault(region: &mut VirRegion) {
    region.mem_type.on_pagefault(...);
}
```

优点：
- 运行时灵活
- 类似 C 的使用方式

缺点：
- 轻微的虚函数调用开销
- 需要对象安全

**推荐方案**

考虑到 Minix3 的设计模式和实际使用场景，推荐使用 **trait object**：

```rust
use std::sync::Arc;

pub struct VirRegion {
    vaddr: usize,
    length: usize,
    mem_type: Arc<dyn MemType>,  // 共享所有权
    // ...
}
```

理由：
1. 与 Minix3 的 `mem_type_t*` 指针语义一致
2. 支持运行时类型变更（如 CoW 后类型改变）
3. 全局类型实例可以安全共享

**全局类型实例**

```rust
use std::sync::LazyLock;

/// 匿名内存类型
pub static MEM_TYPE_ANON: LazyLock<Arc<AnonymousMemory>> = 
    LazyLock::new(|| Arc::new(AnonymousMemory::new()));

/// 直接物理映射类型
pub static MEM_TYPE_DIRECT: LazyLock<Arc<DirectPhysical>> = 
    LazyLock::new(|| Arc::new(DirectPhysical::new()));

/// 连续匿名内存类型
pub static MEM_TYPE_ANON_CONTIG: LazyLock<Arc<ContiguousAnonymous>> = 
    LazyLock::new(|| Arc::new(ContiguousAnonymous::new()));

/// 磁盘缓存类型
pub static MEM_TYPE_CACHE: LazyLock<Arc<CacheMemory>> = 
    LazyLock::new(|| Arc::new(CacheMemory::new()));

/// 文件映射类型
pub static MEM_TYPE_MAPPEDFILE: LazyLock<Arc<MappedFile>> = 
    LazyLock::new(|| Arc::new(MappedFile::new()));

/// 共享内存类型
pub static MEM_TYPE_SHARED: LazyLock<Arc<SharedMemory>> = 
    LazyLock::new(|| Arc::new(SharedMemory::new()));
```

**类型变更示例**

CoW 后类型从文件映射变为匿名内存：

```rust
// 在 cow_block 中
fn cow_block(region: &mut VirRegion, pr: &mut PhysRegion) -> Result<()> {
    // ... 分配新页面，复制数据 ...
    
    // 类型改为匿名内存
    pr.mem_type = MEM_TYPE_ANON.clone();
    
    Ok(())
}
```

**与 C 的兼容性考虑**

如果需要与 C 代码交互，可以提供转换函数：

```rust
impl MemType {
    /// 从 C 的 mem_type_t 指针创建 Rust 包装
    pub unsafe fn from_c(ptr: *const mem_type_t) -> Arc<dyn MemType> {
        // 根据 name 字段判断类型
        let name = CStr::from_ptr((*ptr).name);
        match name.to_str() {
            Ok("anonymous memory") => MEM_TYPE_ANON.clone(),
            Ok("physical memory mapping") => MEM_TYPE_DIRECT.clone(),
            // ...
            _ => panic!("Unknown memory type"),
        }
    }
}

### 3.2 具体类型实现

**实现概览**

| 类型 | 结构体 | 关键特性 |
|------|--------|---------|
| AnonymousMemory | 单元结构体 | 支持 CoW、按需分配 |
| DirectPhysical | 单元结构体 | 映射固定物理地址 |
| ContiguousAnonymous | 单元结构体 | DMA 缓冲区、物理连续 |
| CacheMemory | 单元结构体 | 文件系统缓存 |
| MappedFile | 包含 fdref | 文件映射、延迟加载 |
| SharedMemory | 包含源引用 | 进程间共享 |

**AnonymousMemory - 匿名内存**

```rust
/// 匿名内存类型
///
/// 普通堆内存，支持 CoW（写时复制）。
/// 对应 Minix3: `mem_type_anon`
pub(crate) struct AnonymousMemory;

impl AnonymousMemory {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for AnonymousMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }

    fn is_writable(&self, pr: &PhysRegion) -> bool {
        if pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) == PhysBlock::MAP_NONE {
            return false;
        }
        if let Some(parent) = pr.parent {
            unsafe {
                if (*parent.as_ptr()).remaps > 0 {
                    return true;
                }
            }
        }
        if let Some(refcount) = pr.get_refcount() {
            refcount == 1
        } else {
            false
        }
    }

    fn on_unreference(&self, pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        let refcount = pr.get_refcount().unwrap_or(0);
        if refcount == 0 && pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn on_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut VirRegion,
        pr: &mut PhysRegion,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) == PhysBlock::MAP_NONE {
            return Ok(PagefaultResult::NeedNewPage);
        }

        let refcount = pr.get_refcount().unwrap_or(0);

        if refcount < 2 || !write {
            return Ok(PagefaultResult::Handled);
        }

        if !region.is_writable() {
            return Ok(PagefaultResult::AccessViolation);
        }

        Ok(PagefaultResult::NeedCow)
    }

    fn region_id(&self, _region: &VirRegion) -> u32 {
        1
    }

    fn ref_count(&self, region: &VirRegion) -> i32 {
        let mut mapped = 0i32;
        for pb in &region.physblocks {
            if pb.is_some() {
                mapped += 1;
            }
        }
        mapped
    }
}
```

**DirectPhysical - 直接物理映射**

```rust
/// 直接物理映射类型
///
/// 设备内存映射，不由 VM 管理分配。
/// 对应 Minix3: `mem_type_directphys`
pub(crate) struct DirectPhysical;

impl DirectPhysical {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for DirectPhysical {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for DirectPhysical {
    fn name(&self) -> &'static str {
        "physical memory mapping"
    }

    fn is_writable(&self, pr: &PhysRegion) -> bool {
        pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE
    }

    fn on_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut VirRegion,
        pr: &mut PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        if let crate::region::VrParam::Direct { phys: base_phys } = &region.param {
            if *base_phys == PhysBlock::MAP_NONE {
                return Err(MemTypeError::InvalidParam);
            }
            if pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE {
                return Ok(PagefaultResult::Handled);
            }
            return Ok(PagefaultResult::NeedNewPage);
        }
        Err(MemTypeError::InvalidParam)
    }

    fn on_copy(
        &self,
        src: &VirRegion,
        dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }

    fn on_unreference(&self, _pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
    }
}
```

**ContiguousAnonymous - 连续匿名内存**（尚未实现）

> 以下为设计代码，实际 Rust 实现中尚未包含此类型。

```rust
/// 连续匿名内存类型
///
/// 物理连续的匿名内存，用于 DMA 缓冲区。
/// 对应 Minix3: `mem_type_anon_contig`
pub struct ContiguousAnonymous;

impl MemType for ContiguousAnonymous {
    fn name(&self) -> &'static str {
        "anonymous memory (physically contiguous)"
    }

    fn on_new(&self, region: &mut VirRegion) -> Result<(), MemTypeError> {
        let pages = region.length / PAGE_SIZE;
        
        // 一次性分配所有连续物理页
        let phys = alloc_contiguous(pages)?;
        
        // 为每个页面创建 phys_block
        for offset in (0..region.length).step_by(PAGE_SIZE) {
            let pb = PhysBlock::new(phys + offset);
            region.set_phys_block(offset, pb)?;
        }
        
        Ok(())
    }

    fn on_reference(
        &self,
        _src: &PhysRegion,
        _dst: &mut PhysRegion,
    ) -> Result<(), MemTypeError> {
        // 不支持 fork
        Err(MemTypeError::NotSupported)
    }

    fn on_pagefault(
        &self,
        _vmp: &VmProc,
        _region: &mut VirRegion,
        pr: &mut PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // 创建时已分配，不应发生页错误
        if pr.get_phys_addr().is_some() {
            Ok(PagefaultResult::Handled)
        } else {
            panic!("contiguous memory pagefault: already allocated");
        }
    }

    fn on_resize(
        &self,
        _vmp: &mut VmProc,
        _region: &mut VirRegion,
        _new_len: usize,
    ) -> Result<(), MemTypeError> {
        // 不支持调整大小
        Err(MemTypeError::NotSupported)
    }
}
```

**CacheMemory - 磁盘缓存**（尚未实现）

> 以下为设计代码，实际 Rust 实现中尚未包含此类型。

```rust
/// 磁盘缓存类型
///
/// 文件系统块缓存。
/// 对应 Minix3: `mem_type_cache`
pub struct CacheMemory;

impl MemType for CacheMemory {
    fn name(&self) -> &'static str {
        "cache memory"
    }

    fn on_reference(
        &self,
        _src: &PhysRegion,
        _dst: &mut PhysRegion,
    ) -> Result<(), MemTypeError> {
        // 空操作
        Ok(())
    }

    fn on_unreference(&self, pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        // 复用匿名内存的实现
        AnonymousMemory.on_unreference(pr)
    }

    fn on_pagefault(
        &self,
        _vmp: &VmProc,
        region: &mut VirRegion,
        pr: &mut PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // 链接预分配的缓存块
        if let Some(cache_block) = region.take_cache_block() {
            pr.link_to_block(cache_block);
            Ok(PagefaultResult::Handled)
        } else {
            Err(MemTypeError::InvalidParam)
        }
    }

    fn on_resize(
        &self,
        _vmp: &mut VmProc,
        _region: &mut VirRegion,
        _new_len: usize,
    ) -> Result<(), MemTypeError> {
        // 不支持调整大小
        Err(MemTypeError::NotSupported)
    }

    fn is_writable(&self, pr: &PhysRegion) -> bool {
        // 缓存块总是可写
        pr.get_phys_addr().is_some()
    }
}
```

**MappedFile - 文件映射**（尚未实现）

> 以下为设计代码，实际 Rust 实现中尚未包含此类型。

```rust
/// 文件映射类型
///
/// mmap 文件，延迟加载，写入时 CoW。
/// 对应 Minix3: `mem_type_mappedfile`
pub struct MappedFile;

impl MemType for MappedFile {
    fn name(&self) -> &'static str {
        "file-mapped memory"
    }

    fn on_unreference(&self, pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        // 引用计数为 0 时释放物理页
        if pr.get_refcount() == Some(0) {
            if let Some(phys) = pr.get_phys_addr() {
                // free_mem(ABS2CLICK(phys), 1)
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn on_pagefault(
        &self,
        vmp: &VmProc,
        region: &mut VirRegion,
        pr: &mut PhysRegion,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // 情况 1: 物理页不存在
        if pr.get_phys_addr().is_none() {
            // 尝试从缓存查找
            if let Some(cache_block) = find_cached_page(region)? {
                pr.link_to_block(cache_block);
                
                // 需要写入或末尾清理时执行 CoW
                if write || needs_clearend(region) {
                    return Ok(PagefaultResult::NeedCow);
                }
                return Ok(PagefaultResult::Handled);
            }
            
            // 缓存未命中，需要从 VFS 加载
            return Ok(PagefaultResult::NeedAsyncIo);
        }
        
        // 情况 2: 物理页存在，需要写入
        if write {
            return Ok(PagefaultResult::NeedCow);
        }
        
        Ok(PagefaultResult::Handled)
    }

    fn on_delete(&self, region: &mut VirRegion) {
        // 释放 fdref 引用
        if let Some(fdref) = region.take_fdref() {
            fdref.dec_ref();
        }
    }

    fn on_copy(
        &self,
        src: &VirRegion,
        dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // 复制文件映射信息
        dst.set_file_info(
            src.get_fd(),
            src.get_file_offset(),
            src.get_dev(),
            src.get_ino(),
            src.get_clearend(),
        );
        Ok(())
    }

    fn on_split(
        &self,
        _vmp: &VmProc,
        original: &VirRegion,
        left: &mut VirRegion,
        right: &mut VirRegion,
    ) {
        // 复制文件信息
        left.set_file_info_from(original);
        right.set_file_info_from(original);
        
        // 增加引用计数
        if let Some(fdref) = original.get_fdref() {
            fdref.inc_ref();
            fdref.inc_ref();
        }
        
        // 调整偏移
        left.set_clearend(0);
        right.set_file_offset(original.get_file_offset() + left.length);
    }

    fn is_writable(&self, _pr: &PhysRegion) -> bool {
        // 文件映射不可直接写入，必须通过 CoW
        false
    }
}
```

**SharedMemory - 共享内存**

```rust
/// 共享内存类型
///
/// 进程间共享内存，多个进程映射同一物理页。
/// 对应 Minix3: `mem_type_shared`
pub(crate) struct SharedMemory;

impl SharedMemory {
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl Default for SharedMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for SharedMemory {
    fn name(&self) -> &'static str {
        "shared memory"
    }

    fn is_writable(&self, pr: &PhysRegion) -> bool {
        pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE
    }

    fn on_unreference(&self, _pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
    }

    fn on_copy(
        &self,
        src: &VirRegion,
        dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        dst.param = src.param.clone();
        Ok(())
    }
}
```

**类型实现对比**

| 方法 | Anonymous | DirectPhys | Contiguous | Cache | MappedFile | Shared |
|------|-----------|------------|------------|-------|------------|--------|
| `on_new` | 默认 | 默认 | ✅ 分配连续页 | 默认 | 默认 | 默认 |
| `on_delete` | 默认 | 默认 | 默认 | 默认 | ✅ 释放 fdref | ✅ 减少 remaps |
| `on_reference` | 默认 | 默认 | ❌ 不支持 | ✅ 空操作 | 默认 | 默认 |
| `on_unreference` | ✅ 释放页 | 默认 | ✅ 复用匿名 | ✅ 复用匿名 | ✅ 释放页 | ✅ 复用匿名 |
| `on_pagefault` | ✅ CoW 判断 | ✅ 已映射 | ✅ panic | ✅ 链接缓存 | ✅ 缓存/IO | ✅ 链接源页 |
| `on_resize` | ✅ 扩展 | 默认 | ❌ 不支持 | ❌ 不支持 | 默认 | 默认 |
| `on_split` | 默认 | 默认 | ✅ | 默认 | ✅ 调整偏移 | 默认 |
| `on_copy` | 默认 | ✅ 复制物理 | 默认 | 默认 | ✅ 复制信息 | ✅ 复制引用 |
| `is_writable` | ✅ refcount=1 | ✅ 已分配 | ✅ 已分配 | ✅ 已分配 | ❌ false | ✅ 已分配 |

### 3.3 与 C 的兼容性

**函数指针 vs trait object 对比**

| 特性 | C 函数指针 | Rust trait object |
|------|-----------|-------------------|
| 类型安全 | 运行时检查 | 编译时 + 运行时 |
| 空指针 | 可能存在 | 不可能 |
| 默认行为 | 需手动检查 NULL | 提供默认实现 |
| 大小 | 固定（指针数组） | 固定（胖指针） |
| 调用开销 | 间接调用 | 虚函数调用 |
| 热路径优化 | 难以内联 | 难以内联 |

**C 的函数指针表**

```c
struct mem_type {
    const char *name;
    int (*ev_new)(struct vir_region *region);
    void (*ev_delete)(struct vir_region *region);
    int (*ev_pagefault)(struct vmproc *vmp, struct vir_region *region,
        struct phys_region *ph, int write, ...);
    // ... 更多函数指针
};

// 全局实例
struct mem_type mem_type_anon = {
    .name = "anonymous memory",
    .ev_new = NULL,  // 不需要
    .ev_delete = NULL,
    .ev_pagefault = anon_pagefault,
    // ...
};

// 调用方式
if (region->def_memtype->ev_pagefault) {
    result = region->def_memtype->ev_pagefault(vmp, region, ph, write, ...);
}
```

**Rust 的 trait object**

```rust
// trait object 是胖指针： (data_ptr, vtable_ptr)
// vtable 包含所有方法的函数指针

pub struct VirRegion {
    vaddr: usize,
    length: usize,
    mem_type: Arc<dyn MemType>,  // 胖指针
}

// 调用方式（无需检查 NULL）
let result = region.mem_type.on_pagefault(vmp, region, pr, write);
```

**内存布局对比**

| 方面 | C 函数指针表 | Rust trait object |
|------|-------------|-------------------|
| 存储方式 | 静态结构体，包含所有函数指针 | 单元结构体（无数据）+ 编译器生成 vtable |
| 引用方式 | `vir_region.def_memtype` 指向静态实例 | `Arc<dyn MemType>` 胖指针（data_ptr + vtable_ptr） |
| NULL 处理 | 需运行时检查每个函数指针 | 不需要，默认实现替代 NULL |
| vtable 共享 | 每个实例自带函数指针数组 | 同类型共享 vtable |

**性能对比**

| 操作 | C 函数指针 | Rust trait object |
|------|-----------|-------------------|
| 调用开销 | 1 次间接跳转 | 1 次虚函数调用 |
| NULL 检查 | 需要（分支） | 不需要 |
| 默认行为 | 需显式检查 | 自动使用默认 |
| 内存占用 | 指针数组 | vtable（共享） |

**FFI 兼容层**

如果需要与 C 代码交互，可以提供桥接：

```rust
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

/// C 兼容的 mem_type 结构体
#[repr(C)]
pub struct c_mem_type {
    name: *const c_char,
    ev_new: Option<extern "C" fn(*mut c_void) -> c_int>,
    ev_delete: Option<extern "C" fn(*mut c_void)>,
    ev_pagefault: Option<extern "C" fn(*mut c_void, *mut c_void, *mut c_void, c_int, ...) -> c_int>,
    // ...
}

/// 将 Rust MemType 转换为 C 结构体
pub fn mem_type_to_c(mem_type: &dyn MemType) -> c_mem_type {
    c_mem_type {
        name: CString::new(mem_type.name()).unwrap().into_raw(),
        ev_new: Some(c_ev_new_wrapper),
        ev_delete: Some(c_ev_delete_wrapper),
        ev_pagefault: Some(c_ev_pagefault_wrapper),
        // ...
    }
}

// 包装函数
extern "C" fn c_ev_new_wrapper(region: *mut c_void) -> c_int {
    let region = unsafe { &mut *(region as *mut VirRegion) };
    match region.mem_type.on_new(region) {
        Ok(()) => 0,
        Err(e) => e.to_errno(),
    }
}
```

**迁移策略**

从 C 代码迁移到 Rust 的建议：

1. **阶段 1**: 保留 C 实现，Rust 通过 FFI 调用
2. **阶段 2**: 用 Rust 实现新类型，C 通过 FFI 调用
3. **阶段 3**: 完全迁移到 Rust trait

**混合使用示例**

```rust
// 同时支持 C 和 Rust 调用
pub struct HybridMemType {
    rust_impl: Arc<dyn MemType>,
    c_vtable: c_mem_type,
}

impl HybridMemType {
    pub fn new<T: MemType + 'static>(rust_impl: T) -> Self {
        let arc = Arc::new(rust_impl);
        let c_vtable = mem_type_to_c(arc.as_ref());
        Self {
            rust_impl: arc,
            c_vtable,
        }
    }
}

// C 代码可以使用 c_vtable
// Rust 代码可以使用 rust_impl
```

---

## 4. 实现详解

### 4.1 trait 定义

**必需方法 vs 默认方法**

| 方法 | 类型 | 说明 |
|------|------|------|
| `name()` | 必需 | 每个类型必须有唯一名称 |
| `on_pagefault()` | 默认 | 大部分类型需要重写 |
| `is_writable()` | 默认 | 大部分类型需要重写 |
| 其他 | 默认 | 按需重写 |

**核心 trait 定义**

```rust
/// 内存类型 trait
///
/// # Safety
///
/// 实现者必须确保：
/// - `on_pagefault` 返回正确的结果
/// - `is_writable` 与页表权限一致
/// - 引用计数管理正确
pub trait MemType: Send + Sync + 'static {
    /// 返回类型名称（必需）
    fn name(&self) -> &'static str;

    /// 页错误处理（核心方法）
    ///
    /// # Arguments
    /// - `vmp`: 触发页错误的进程
    /// - `region`: 虚拟区域
    /// - `pr`: 物理区域
    /// - `write`: 是否为写入操作
    ///
    /// # Returns
    /// - `Ok(PagefaultResult::Handled)`: 已处理
    /// - `Ok(PagefaultResult::NeedNewPage)`: 需要分配新页
    /// - `Ok(PagefaultResult::NeedCow)`: 需要 CoW
    /// - `Ok(PagefaultResult::NeedAsyncIo)`: 需要异步 I/O
    /// - `Err(...)`: 错误
    fn on_pagefault(
        &self,
        _vmp: &VmProc,
        _region: &mut VirRegion,
        _pr: &mut PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::Handled)
    }

    /// 判断是否可写（核心方法）
    ///
    /// 必须与页表权限一致，否则会导致页错误循环
    fn is_writable(&self, _pr: &PhysRegion) -> bool {
        false
    }

    // ... 其他方法使用默认实现
}
```

**辅助 trait**

```rust
/// 引用计数管理
pub trait RefCounted {
    fn inc_ref(&self);
    fn dec_ref(&self) -> bool;  // 返回是否应该释放
    fn ref_count(&self) -> u32;
}

/// 物理内存管理
pub trait PhysMem {
    fn alloc_page() -> Result<PhysAddr, MemTypeError>;
    fn free_page(addr: PhysAddr);
    fn alloc_contiguous(pages: usize) -> Result<PhysAddr, MemTypeError>;
}

/// 虚拟内存管理
pub trait VirtMem {
    fn map_page(vaddr: VirtAddr, paddr: PhysAddr, flags: u32) -> Result<(), MemTypeError>;
    fn unmap_page(vaddr: VirtAddr);
    fn protect_page(vaddr: VirtAddr, flags: u32) -> Result<(), MemTypeError>;
}
```

**错误处理**

```rust
/// 内存类型错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemTypeError {
    /// 内存不足
    NoMemory,
    /// 无效参数
    InvalidParam,
    /// 不支持的操作
    NotSupported,
    /// IO 错误
    IoError,
    /// 复制失败
    CopyFailed,
    /// 访问违规
    AccessViolation,
    /// 区域不存在
    RegionNotFound,
    /// 进程不存在
    ProcessNotFound,
}

impl MemTypeError {
    /// 转换为 errno
    pub fn to_errno(self) -> i32 {
        match self {
            MemTypeError::NoMemory => 12,        // ENOMEM
            MemTypeError::InvalidParam => 22,    // EINVAL
            MemTypeError::NotSupported => 95,    // EOPNOTSUPP
            MemTypeError::IoError => 5,          // EIO
            MemTypeError::CopyFailed => 14,      // EFAULT
            MemTypeError::AccessViolation => 13, // EACCES
            MemTypeError::RegionNotFound => 14,  // EFAULT
            MemTypeError::ProcessNotFound => 3,  // ESRCH
        }
    }
}
```

**页错误结果**

```rust
/// 页错误处理结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagefaultResult {
    /// 已处理，无需进一步操作
    Handled,
    /// 需要分配新的物理页
    NeedNewPage,
    /// 需要 CoW（写时复制）
    NeedCow,
    /// 需要异步 I/O（从文件系统加载）
    NeedAsyncIo,
    /// 访问违规
    AccessViolation,
    /// 挂起等待
    Suspended,
}
```

### 4.2 匿名内存实现

**完整实现**

```rust
use crate::arch::{PhysAddr, VirtAddr, PAGE_SIZE};
use crate::vm::{PhysBlock, PhysRegion, VirRegion, VmProc};
use crate::vm::memtype::{MemType, MemTypeError, PagefaultResult};
use crate::vm::physmem::PhysMem;

/// 匿名内存类型
///
/// 普通堆内存，支持 CoW（写时复制）。
/// 对应 Minix3: `mem_type_anon`
pub struct AnonymousMemory;

impl AnonymousMemory {
    /// 创建新的匿名内存类型实例
    pub const fn new() -> Self {
        Self
    }
}

impl MemType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }

    fn on_unreference(&self, pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        // 减少引用计数
        if pr.phys_block.dec_ref() {
            // 引用计数为 0，释放物理页
            if let Some(phys) = pr.phys_block.phys_addr() {
                PhysMem::free_page(phys);
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn on_pagefault(
        &self,
        _vmp: &VmProc,
        region: &mut VirRegion,
        pr: &mut PhysRegion,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        let refcount = pr.phys_block.ref_count();

        // 情况 1: 物理页不存在，需要分配
        if pr.phys_block.phys_addr().is_none() {
            return Ok(PagefaultResult::NeedNewPage);
        }

        // 情况 2: 不需要 CoW
        // - refcount < 2: 只有一个引用
        // - !write: 只读访问
        if refcount < 2 || !write {
            return Ok(PagefaultResult::Handled);
        }

        // 情况 3: 需要 CoW
        if !region.is_writable() {
            return Err(MemTypeError::AccessViolation);
        }

        Ok(PagefaultResult::NeedCow)
    }

    fn on_resize(
        &self,
        _vmp: &mut VmProc,
        region: &mut VirRegion,
        new_len: usize,
    ) -> Result<(), MemTypeError> {
        // 只支持扩展，不支持收缩
        if new_len > region.length {
            region.length = new_len;
        }
        Ok(())
    }

    fn on_split(
        &self,
        _vmp: &VmProc,
        _original: &VirRegion,
        _left: &mut VirRegion,
        _right: &mut VirRegion,
    ) {
        // 匿名内存不需要特殊处理
    }

    fn is_writable(&self, pr: &PhysRegion) -> bool {
        // 有共享映射或只有一个引用时可写
        if pr.parent_remaps() > 0 {
            return true;
        }
        pr.phys_block.ref_count() == 1
    }

    fn on_sanitycheck(&self, pr: &PhysRegion) -> Result<(), MemTypeError> {
        // 验证物理页存在
        if pr.phys_block.phys_addr().is_none() {
            return Err(MemTypeError::InvalidParam);
        }
        Ok(())
    }

    fn region_id(&self, region: &VirRegion) -> Option<u32> {
        Some(region.id)
    }

    fn ref_count(&self, region: &VirRegion) -> Option<u32> {
        Some(1 + region.remaps as u32)
    }

    fn pt_flags(&self, _region: &VirRegion) -> u32 {
        // ARM 平台需要缓存标志
        #[cfg(target_arch = "arm")]
        {
            arch::ARM_VM_PTE_CACHED
        }
        #[cfg(not(target_arch = "arm"))]
        {
            0
        }
    }
}
```

**页错误处理流程**

```rust
/// 处理匿名内存页错误
pub fn handle_anon_pagefault(
    vmp: &mut VmProc,
    region: &mut VirRegion,
    pr: &mut PhysRegion,
    write: bool,
) -> Result<(), MemTypeError> {
    // 预分配一个页面（可能用于 CoW 或首次分配）
    let new_page = PhysMem::alloc_page()?;

    // 检查物理页是否存在
    if pr.phys_block.phys_addr().is_none() {
        // 首次分配
        pr.phys_block.set_phys_addr(new_page);
        return Ok(());
    }

    // 检查是否需要 CoW
    let refcount = pr.phys_block.ref_count();
    if refcount < 2 || !write {
        // 不需要 CoW，释放预分配的页面
        PhysMem::free_page(new_page);
        return Ok(());
    }

    // 执行 CoW
    mem_cow(region, pr, new_page)
}

/// 执行写时复制
pub fn mem_cow(
    region: &mut VirRegion,
    pr: &mut PhysRegion,
    new_page: PhysAddr,
) -> Result<(), MemTypeError> {
    // 原页面必须存在
    let old_phys = pr.phys_block.phys_addr()
        .ok_or(MemTypeError::InvalidParam)?;

    // 复制原页面内容到新页面
    arch::memcpy_phys(old_phys, new_page, PAGE_SIZE)?;

    // 创建新的物理块
    let new_block = PhysBlock::new(new_page);

    // 解除对原物理块的引用
    let old_block = core::mem::replace(&mut pr.phys_block, new_block);
    if old_block.dec_ref() {
        // 引用计数为 0，释放原页面
        if let Some(phys) = old_block.phys_addr() {
            PhysMem::free_page(phys);
        }
    }

    // 类型转为匿名内存
    pr.mem_type = MEM_TYPE_ANON.clone();

    Ok(())
}
```

**测试用例**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anon_pagefault_first_alloc() {
        let anon = AnonymousMemory::new();
        let mut region = VirRegion::new(0x1000, 4096);
        let mut pr = PhysRegion::new(0);

        // 首次访问，需要分配
        let result = anon.on_pagefault(
            &VmProc::dummy(),
            &mut region,
            &mut pr,
            false,
        ).unwrap();

        assert_eq!(result, PagefaultResult::NeedNewPage);
    }

    #[test]
    fn test_anon_pagefault_no_cow() {
        let anon = AnonymousMemory::new();
        let mut region = VirRegion::new(0x1000, 4096);
        let mut pr = PhysRegion::new(0);
        
        // 设置物理页，引用计数为 1
        pr.phys_block = PhysBlock::new(PhysAddr::new(0x10000));

        // 只读访问，不需要 CoW
        let result = anon.on_pagefault(
            &VmProc::dummy(),
            &mut region,
            &mut pr,
            false,
        ).unwrap();

        assert_eq!(result, PagefaultResult::Handled);
    }

    #[test]
    fn test_anon_pagefault_need_cow() {
        let anon = AnonymousMemory::new();
        let mut region = VirRegion::new_writable(0x1000, 4096);
        let mut pr = PhysRegion::new(0);
        
        // 设置物理页，引用计数为 2（模拟 fork 后）
        let mut block = PhysBlock::new(PhysAddr::new(0x10000));
        block.inc_ref();  // refcount = 2
        pr.phys_block = block;

        // 写入访问，需要 CoW
        let result = anon.on_pagefault(
            &VmProc::dummy(),
            &mut region,
            &mut pr,
            true,
        ).unwrap();

        assert_eq!(result, PagefaultResult::NeedCow);
    }

    #[test]
    fn test_anon_writable() {
        let anon = AnonymousMemory::new();
        let pr = PhysRegion::new(0);
        
        // 引用计数为 1，可写
        assert!(anon.is_writable(&pr));
        
        // 引用计数 > 1，不可写（需要 CoW）
        let mut pr2 = PhysRegion::new(0);
        pr2.phys_block.inc_ref();
        assert!(!anon.is_writable(&pr2));
    }

    #[test]
    fn test_anon_resize() {
        let anon = AnonymousMemory::new();
        let mut region = VirRegion::new(0x1000, 4096);
        let mut vmp = VmProc::dummy();

        // 扩展
        assert!(anon.on_resize(&mut vmp, &mut region, 8192).is_ok());
        assert_eq!(region.length, 8192);

        // 收缩（不支持，但不会失败）
        assert!(anon.on_resize(&mut vmp, &mut region, 2048).is_ok());
        assert_eq!(region.length, 8192);  // 长度不变
    }
}
```

### 4.3 文件映射实现

**完整实现**

```rust
use crate::arch::{PhysAddr, VirtAddr, PAGE_SIZE};
use crate::vm::{PhysBlock, PhysRegion, VirRegion, VmProc, FdRef};
use crate::vm::memtype::{MemType, MemTypeError, PagefaultResult};
use crate::vm::physmem::PhysMem;
use crate::vm::cache::{find_cached_page, CachedPage};

/// 文件映射类型
///
/// mmap 文件，延迟加载，写入时 CoW。
/// 对应 Minix3: `mem_type_mappedfile`
pub struct MappedFile;

impl MappedFile {
    pub const fn new() -> Self {
        Self
    }
}

impl MemType for MappedFile {
    fn name(&self) -> &'static str {
        "file-mapped memory"
    }

    fn on_unreference(&self, pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        // 引用计数为 0 时释放物理页
        if pr.phys_block.dec_ref() {
            if let Some(phys) = pr.phys_block.phys_addr() {
                PhysMem::free_page(phys);
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn on_pagefault(
        &self,
        _vmp: &VmProc,
        region: &mut VirRegion,
        pr: &mut PhysRegion,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // 情况 1: 物理页不存在
        if pr.phys_block.phys_addr().is_none() {
            // 获取文件信息
            let file_info = region.get_file_info()
                .ok_or(MemTypeError::InvalidParam)?;

            // 尝试从缓存查找
            if let Some(cache_block) = find_cached_page(
                file_info.dev,
                file_info.ino,
                file_info.offset + pr.offset,
            )? {
                // 缓存命中，链接缓存块
                pr.link_to_block(cache_block);

                // 需要写入或末尾清理时执行 CoW
                if write || file_info.clearend > 0 {
                    return Ok(PagefaultResult::NeedCow);
                }
                return Ok(PagefaultResult::Handled);
            }

            // 缓存未命中，需要从 VFS 加载
            return Ok(PagefaultResult::NeedAsyncIo);
        }

        // 情况 2: 物理页存在，需要写入
        if write {
            return Ok(PagefaultResult::NeedCow);
        }

        Ok(PagefaultResult::Handled)
    }

    fn on_delete(&self, region: &mut VirRegion) {
        // 释放 fdref 引用
        if let Some(fdref) = region.take_fdref() {
            fdref.dec_ref();
        }
    }

    fn on_copy(
        &self,
        src: &VirRegion,
        dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> {
        // 复制文件映射信息
        let file_info = src.get_file_info()
            .ok_or(MemTypeError::InvalidParam)?;

        dst.set_file_info(
            file_info.fd,
            file_info.offset,
            file_info.dev,
            file_info.ino,
            file_info.clearend,
        );

        // 增加 fdref 引用计数
        if let Some(fdref) = src.get_fdref() {
            fdref.inc_ref();
        }

        Ok(())
    }

    fn on_split(
        &self,
        _vmp: &VmProc,
        original: &VirRegion,
        left: &mut VirRegion,
        right: &mut VirRegion,
    ) {
        // 复制文件信息
        if let Some(file_info) = original.get_file_info() {
            left.set_file_info(
                file_info.fd,
                file_info.offset,
                file_info.dev,
                file_info.ino,
                0,  // 前半部分无末尾清理
            );

            right.set_file_info(
                file_info.fd,
                file_info.offset + left.length,
                file_info.dev,
                file_info.ino,
                file_info.clearend,
            );

            // 增加引用计数
            if let Some(fdref) = original.get_fdref() {
                fdref.inc_ref();
                fdref.inc_ref();
            }
        }
    }

    fn on_lowshrink(
        &self,
        region: &mut VirRegion,
        len: usize,
    ) -> Result<(), MemTypeError> {
        // 调整文件偏移
        if let Some(ref mut file_info) = region.file_info {
            file_info.offset += len;
            Ok(())
        } else {
            Err(MemTypeError::InvalidParam)
        }
    }

    fn is_writable(&self, _pr: &PhysRegion) -> bool {
        // 文件映射不可直接写入，必须通过 CoW
        false
    }

    fn on_sanitycheck(&self, pr: &PhysRegion) -> Result<(), MemTypeError> {
        // 验证物理页存在
        if pr.phys_block.phys_addr().is_none() {
            return Err(MemTypeError::InvalidParam);
        }
        Ok(())
    }

    fn pt_flags(&self, _region: &VirRegion) -> u32 {
        // ARM 平台需要缓存标志
        #[cfg(target_arch = "arm")]
        {
            arch::ARM_VM_PTE_CACHED
        }
        #[cfg(not(target_arch = "arm"))]
        {
            0
        }
    }
}
```

**文件信息结构**

```rust
/// 文件映射信息
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// 文件描述符引用
    pub fdref: Arc<FdRef>,
    /// 文件偏移
    pub offset: u64,
    /// 末尾清理字节数
    pub clearend: u16,
}

/// 文件描述符引用
#[derive(Debug)]
pub struct FdRef {
    /// 文件描述符
    pub fd: i32,
    /// 设备号
    pub dev: u64,
    /// inode 号
    pub ino: u64,
    /// 引用计数
    refcount: AtomicU32,
}

impl FdRef {
    pub fn new(fd: i32, dev: u64, ino: u64) -> Self {
        Self {
            fd,
            dev,
            ino,
            refcount: AtomicU32::new(1),
        }
    }

    pub fn inc_ref(&self) {
        self.refcount.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_ref(&self) -> bool {
        self.refcount.fetch_sub(1, Ordering::Release) == 1
    }
}
```

**CoW 处理**

```rust
/// 文件映射的 CoW 处理
pub fn mappedfile_cow(
    region: &mut VirRegion,
    pr: &mut PhysRegion,
) -> Result<(), MemTypeError> {
    // 分配新物理页
    let new_page = PhysMem::alloc_page()?;

    // 原页面必须存在
    let old_phys = pr.phys_block.phys_addr()
        .ok_or(MemTypeError::InvalidParam)?;

    // 复制原页面内容到新页面
    arch::memcpy_phys(old_phys, new_page, PAGE_SIZE)?;

    // 创建新的物理块
    let new_block = PhysBlock::new(new_page);

    // 解除对原物理块的引用
    let old_block = core::mem::replace(&mut pr.phys_block, new_block);
    if old_block.dec_ref() {
        // 引用计数为 0，释放原页面
        if let Some(phys) = old_block.phys_addr() {
            PhysMem::free_page(phys);
        }
    }

    // 类型转为匿名内存
    pr.mem_type = MEM_TYPE_ANON.clone();

    // 清理末尾字节
    if let Some(file_info) = &region.file_info {
        if file_info.clearend > 0 {
            let offset = PAGE_SIZE - file_info.clearend as usize;
            arch::memset_phys(new_page + offset, 0, file_info.clearend as usize)?;
        }
    }

    Ok(())
}
```

**测试用例**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mappedfile_pagefault_cache_hit() {
        let mapped = MappedFile::new();
        let fdref = Arc::new(FdRef::new(3, 0x100, 0x200));
        let mut region = VirRegion::new(0x1000, 4096);
        region.set_file_info(3, 0, 0x100, 0x200, 0);
        region.set_fdref(fdref);
        
        let mut pr = PhysRegion::new(0);

        // 缓存命中
        let result = mapped.on_pagefault(
            &VmProc::dummy(),
            &mut region,
            &mut pr,
            false,
        ).unwrap();

        // 应该返回 Handled 或 NeedAsyncIo（取决于缓存状态）
        assert!(matches!(result, 
            PagefaultResult::Handled | PagefaultResult::NeedAsyncIo));
    }

    #[test]
    fn test_mappedfile_pagefault_need_cow() {
        let mapped = MappedFile::new();
        let mut region = VirRegion::new(0x1000, 4096);
        region.set_file_info(3, 0, 0x100, 0x200, 0);
        
        let mut pr = PhysRegion::new(0);
        pr.phys_block = PhysBlock::new(PhysAddr::new(0x10000));

        // 写入访问，需要 CoW
        let result = mapped.on_pagefault(
            &VmProc::dummy(),
            &mut region,
            &mut pr,
            true,
        ).unwrap();

        assert_eq!(result, PagefaultResult::NeedCow);
    }

    #[test]
    fn test_mappedfile_split() {
        let mapped = MappedFile::new();
        let fdref = Arc::new(FdRef::new(3, 0x100, 0x200));
        let original = VirRegion::new(0x1000, 8192);
        original.set_file_info(3, 0, 0x100, 0x200, 100);
        original.set_fdref(fdref.clone());

        let mut left = VirRegion::new(0x1000, 4096);
        let mut right = VirRegion::new(0x2000, 4096);

        mapped.on_split(&VmProc::dummy(), &original, &mut left, &mut right);

        // 验证偏移调整
        assert_eq!(left.get_file_info().unwrap().offset, 0);
        assert_eq!(left.get_file_info().unwrap().clearend, 0);
        assert_eq!(right.get_file_info().unwrap().offset, 4096);
        assert_eq!(right.get_file_info().unwrap().clearend, 100);

        // 验证引用计数增加
        assert_eq!(fdref.refcount.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_mappedfile_not_writable() {
        let mapped = MappedFile::new();
        let pr = PhysRegion::new(0);
        
        // 文件映射不可直接写入
        assert!(!mapped.is_writable(&pr));
    }
}
```

### 4.4 类型注册

**全局类型实例**

使用 `LazyLock` 创建全局单例：

```rust
use std::sync::{Arc, LazyLock};

/// 匿名内存类型
pub static MEM_TYPE_ANON: LazyLock<Arc<AnonymousMemory>> = 
    LazyLock::new(|| Arc::new(AnonymousMemory::new()));

/// 直接物理映射类型
pub static MEM_TYPE_DIRECT: LazyLock<Arc<DirectPhysical>> = 
    LazyLock::new(|| Arc::new(DirectPhysical::new()));

/// 连续匿名内存类型
pub static MEM_TYPE_ANON_CONTIG: LazyLock<Arc<ContiguousAnonymous>> = 
    LazyLock::new(|| Arc::new(ContiguousAnonymous::new()));

/// 磁盘缓存类型
pub static MEM_TYPE_CACHE: LazyLock<Arc<CacheMemory>> = 
    LazyLock::new(|| Arc::new(CacheMemory::new()));

/// 文件映射类型
pub static MEM_TYPE_MAPPEDFILE: LazyLock<Arc<MappedFile>> = 
    LazyLock::new(|| Arc::new(MappedFile::new()));

/// 共享内存类型
pub static MEM_TYPE_SHARED: LazyLock<Arc<SharedMemory>> = 
    LazyLock::new(|| Arc::new(SharedMemory::new()));
```

**类型注册表**

```rust
use std::collections::HashMap;
use std::sync::RwLock;

/// 类型注册表
static MEM_TYPE_REGISTRY: LazyLock<RwLock<HashMap<&'static str, Arc<dyn MemType>>>> = 
    LazyLock::new(|| {
        let mut registry = HashMap::new();
        
        // 注册内置类型
        registry.insert("anonymous memory", MEM_TYPE_ANON.clone() as Arc<dyn MemType>);
        registry.insert("physical memory mapping", MEM_TYPE_DIRECT.clone() as Arc<dyn MemType>);
        registry.insert("anonymous memory (physically contiguous)", 
            MEM_TYPE_ANON_CONTIG.clone() as Arc<dyn MemType>);
        registry.insert("cache memory", MEM_TYPE_CACHE.clone() as Arc<dyn MemType>);
        registry.insert("file-mapped memory", MEM_TYPE_MAPPEDFILE.clone() as Arc<dyn MemType>);
        registry.insert("shared memory", MEM_TYPE_SHARED.clone() as Arc<dyn MemType>);
        
        registry
    });

/// 注册新类型
pub fn register_mem_type(mem_type: Arc<dyn MemType>) -> Result<(), MemTypeError> {
    let mut registry = MEM_TYPE_REGISTRY.write().unwrap();
    let name = mem_type.name();
    
    if registry.contains_key(name) {
        return Err(MemTypeError::InvalidParam);
    }
    
    registry.insert(name, mem_type);
    Ok(())
}

/// 查找类型
pub fn find_mem_type(name: &str) -> Option<Arc<dyn MemType>> {
    let registry = MEM_TYPE_REGISTRY.read().unwrap();
    registry.get(name).cloned()
}
```

**类型查找函数**

```rust
/// 根据标志获取默认类型
pub fn get_default_mem_type(flags: u32) -> Arc<dyn MemType> {
    if flags & VR_DIRECT != 0 {
        MEM_TYPE_DIRECT.clone()
    } else if flags & VR_CONTIG != 0 {
        MEM_TYPE_ANON_CONTIG.clone()
    } else if flags & VR_SHARED != 0 {
        MEM_TYPE_SHARED.clone()
    } else {
        MEM_TYPE_ANON.clone()
    }
}

/// 根据名称获取类型
pub fn get_mem_type_by_name(name: &str) -> Option<Arc<dyn MemType>> {
    find_mem_type(name)
}
```

**VirRegion 使用示例**

```rust
impl VirRegion {
    /// 创建新的虚拟区域
    pub fn new(
        vaddr: VirtAddr,
        length: usize,
        flags: u32,
    ) -> Self {
        let mem_type = get_default_mem_type(flags);
        
        Self {
            vaddr,
            length,
            flags,
            mem_type,
            id: allocate_region_id(),
            remaps: 0,
            phys_regions: Vec::new(),
            file_info: None,
        }
    }

    /// 设置内存类型
    pub fn set_mem_type(&mut self, mem_type: Arc<dyn MemType>) {
        self.mem_type = mem_type;
    }

    /// 获取内存类型
    pub fn get_mem_type(&self) -> &Arc<dyn MemType> {
        &self.mem_type
    }
}
```

**类型变更示例**

```rust
/// CoW 后类型变更
pub fn change_type_to_anon(pr: &mut PhysRegion) {
    pr.mem_type = MEM_TYPE_ANON.clone();
}

/// 文件映射设置
pub fn set_file_mapping(region: &mut VirRegion, fd: i32, offset: u64) {
    region.mem_type = MEM_TYPE_MAPPEDFILE.clone();
    region.file_info = Some(FileInfo {
        fd,
        offset,
        clearend: 0,
    });
}

/// 共享内存设置
pub fn set_shared_mapping(
    region: &mut VirRegion,
    src_ep: Endpoint,
    src_vaddr: VirtAddr,
    src_region: &VirRegion,
) {
    region.mem_type = MEM_TYPE_SHARED.clone();
    region.shared_info = Some(SharedInfo {
        src_ep,
        src_vaddr,
        src_id: src_region.id,
    });
    src_region.remaps += 1;
}
```

**测试用例**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_types() {
        // 验证全局类型实例已创建
        assert_eq!(MEM_TYPE_ANON.name(), "anonymous memory");
        assert_eq!(MEM_TYPE_DIRECT.name(), "physical memory mapping");
        assert_eq!(MEM_TYPE_ANON_CONTIG.name(), 
            "anonymous memory (physically contiguous)");
        assert_eq!(MEM_TYPE_CACHE.name(), "cache memory");
        assert_eq!(MEM_TYPE_MAPPEDFILE.name(), "file-mapped memory");
        assert_eq!(MEM_TYPE_SHARED.name(), "shared memory");
    }

    #[test]
    fn test_type_registry() {
        // 验证注册表包含所有类型
        let registry = MEM_TYPE_REGISTRY.read().unwrap();
        assert_eq!(registry.len(), 6);
        
        // 验证查找功能
        assert!(find_mem_type("anonymous memory").is_some());
        assert!(find_mem_type("nonexistent").is_none());
    }

    #[test]
    fn test_default_mem_type() {
        // 验证默认类型选择
        let anon = get_default_mem_type(0);
        assert_eq!(anon.name(), "anonymous memory");
        
        let direct = get_default_mem_type(VR_DIRECT);
        assert_eq!(direct.name(), "physical memory mapping");
        
        let contig = get_default_mem_type(VR_CONTIG);
        assert_eq!(contig.name(), "anonymous memory (physically contiguous)");
    }

    #[test]
    fn test_register_custom_type() {
        // 自定义类型
        struct CustomMemory;
        impl MemType for CustomMemory {
            fn name(&self) -> &'static str {
                "custom memory"
            }
        }
        
        // 注册
        let custom = Arc::new(CustomMemory);
        assert!(register_mem_type(custom.clone()).is_ok());
        
        // 查找
        let found = find_mem_type("custom memory");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "custom memory");
        
        // 重复注册应失败
        assert!(register_mem_type(custom).is_err());
    }
}
```

---

## 5. 扩展机制

### 5.1 添加新内存类型

**实现步骤**

添加新的内存类型需要：

1. 实现 `MemType` trait
2. 注册到全局注册表
3. 定义创建标志

**示例：GPU 内存类型**

```rust
use crate::arch::{PhysAddr, VirtAddr, PAGE_SIZE};
use crate::vm::{PhysBlock, PhysRegion, VirRegion, VmProc};
use crate::vm::memtype::{MemType, MemTypeError, PagefaultResult};
use crate::vm::physmem::PhysMem;

/// GPU 内存类型
///
/// GPU 设备内存，可能需要特殊处理（如缓存策略）。
pub struct GpuMemory {
    /// GPU 设备 ID
    device_id: u32,
    /// 是否支持缓存
    cacheable: bool,
}

impl GpuMemory {
    pub fn new(device_id: u32, cacheable: bool) -> Self {
        Self {
            device_id,
            cacheable,
        }
    }
}

impl MemType for GpuMemory {
    fn name(&self) -> &'static str {
        "gpu memory"
    }

    fn on_pagefault(
        &self,
        _vmp: &VmProc,
        _region: &mut VirRegion,
        pr: &mut PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // GPU 内存通常预分配，页错误不应发生
        if pr.phys_block.phys_addr().is_some() {
            Ok(PagefaultResult::Handled)
        } else {
            Err(MemTypeError::InvalidParam)
        }
    }

    fn is_writable(&self, _pr: &PhysRegion) -> bool {
        // GPU 内存可写
        true
    }

    fn pt_flags(&self, _region: &VirRegion) -> u32 {
        // GPU 内存可能需要特殊的页表标志
        let mut flags = 0;
        
        #[cfg(target_arch = "arm")]
        {
            // GPU 内存可能需要设备内存属性
            if !self.cacheable {
                flags |= arch::ARM_VM_PTE_DEVICE;
            }
        }
        
        flags
    }
}
```

**注册新类型**

```rust
use std::sync::Arc;

/// 初始化 GPU 内存类型
pub fn init_gpu_memory() -> Result<(), MemTypeError> {
    // 创建 GPU 内存类型实例
    let gpu_mem = Arc::new(GpuMemory::new(0, false));
    
    // 注册到全局注册表
    register_mem_type(gpu_mem)?;
    
    Ok(())
}

// 在系统初始化时调用
fn vm_init() {
    // ... 其他初始化 ...
    
    // 初始化 GPU 内存类型
    init_gpu_memory().expect("Failed to init GPU memory");
}
```

**使用新类型**

```rust
/// 创建 GPU 内存区域
pub fn map_gpu_memory(
    vmp: &mut VmProc,
    vaddr: VirtAddr,
    length: usize,
    device_id: u32,
) -> Result<Arc<dyn MemType>, MemTypeError> {
    // 查找 GPU 内存类型
    let gpu_mem = find_mem_type("gpu memory")
        .ok_or(MemTypeError::InvalidParam)?;
    
    // 创建虚拟区域
    let mut region = VirRegion::new(vaddr, length, VR_WRITABLE);
    region.set_mem_type(gpu_mem.clone());
    
    // 分配 GPU 物理内存
    let phys = alloc_gpu_memory(device_id, length)?;
    
    // 映射到进程地址空间
    for offset in (0..length).step_by(PAGE_SIZE) {
        let paddr = phys + offset;
        let mut pr = PhysRegion::new(offset);
        pr.phys_block = PhysBlock::new(paddr);
        region.add_phys_region(pr);
        
        // 映射页表
        arch::map_page(vaddr + offset, paddr, gpu_mem.pt_flags(&region))?;
    }
    
    // 添加到进程
    vmp.add_region(region);
    
    Ok(gpu_mem)
}
```

**完整示例：网络缓冲区类型**

```rust
/// 网络缓冲区内存类型
///
/// 用于网络设备 DMA 的缓冲区，需要物理连续。
pub struct NetworkBuffer {
    /// 网络设备 ID
    device_id: u32,
    /// 缓冲区类型（RX/TX）
    buffer_type: BufferType,
}

#[derive(Debug, Clone, Copy)]
pub enum BufferType {
    Rx,  // 接收缓冲区
    Tx,  // 发送缓冲区
}

impl MemType for NetworkBuffer {
    fn name(&self) -> &'static str {
        "network buffer"
    }

    fn on_new(&self, region: &mut VirRegion) -> Result<(), MemTypeError> {
        let pages = region.length / PAGE_SIZE;
        
        // 分配物理连续的 DMA 缓冲区
        let phys = alloc_dma_buffer(self.device_id, pages)?;
        
        // 为每个页面创建 phys_block
        for offset in (0..region.length).step_by(PAGE_SIZE) {
            let pb = PhysBlock::new(phys + offset);
            region.set_phys_block(offset, pb)?;
        }
        
        Ok(())
    }

    fn on_reference(
        &self,
        _src: &PhysRegion,
        _dst: &mut PhysRegion,
    ) -> Result<(), MemTypeError> {
        // 网络缓冲区不支持 fork
        Err(MemTypeError::NotSupported)
    }

    fn on_pagefault(
        &self,
        _vmp: &VmProc,
        _region: &mut VirRegion,
        pr: &mut PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        // 创建时已分配，不应发生页错误
        if pr.phys_block.phys_addr().is_some() {
            Ok(PagefaultResult::Handled)
        } else {
            panic!("network buffer pagefault: already allocated");
        }
    }

    fn on_resize(
        &self,
        _vmp: &mut VmProc,
        _region: &mut VirRegion,
        _new_len: usize,
    ) -> Result<(), MemTypeError> {
        // 不支持调整大小
        Err(MemTypeError::NotSupported)
    }

    fn is_writable(&self, pr: &PhysRegion) -> bool {
        // TX 缓冲区可写，RX 缓冲区只读
        match self.buffer_type {
            BufferType::Tx => pr.phys_block.phys_addr().is_some(),
            BufferType::Rx => false,
        }
    }
}

/// 注册网络缓冲区类型
pub fn register_network_buffer(device_id: u32, buffer_type: BufferType) -> Result<Arc<dyn MemType>, MemTypeError> {
    let net_buf = Arc::new(NetworkBuffer {
        device_id,
        buffer_type,
    });
    register_mem_type(net_buf.clone())?;
    Ok(net_buf)
}
```

**扩展点总结**

| 扩展点 | 用途 | 示例 |
|--------|------|------|
| `on_new` | 自定义初始化 | GPU 内存预分配 |
| `on_pagefault` | 自定义页错误处理 | 设备内存映射 |
| `is_writable` | 自定义写权限 | RX 缓冲区只读 |
| `pt_flags` | 自定义页表标志 | 设备内存属性 |
| `on_resize` | 自定义大小调整 | 固定大小缓冲区 |

**最佳实践**

1. **命名规范**：使用描述性名称，如 "gpu memory"、"network buffer"
2. **错误处理**：返回明确的错误类型
3. **文档注释**：说明类型的用途和限制
4. **测试覆盖**：为新类型编写测试用例
5. **性能考虑**：避免在热路径中进行复杂操作

### 5.2 驱动使用

**设备驱动内存映射**

设备驱动通常需要映射设备内存或分配 DMA 缓冲区。

**映射设备寄存器**

```rust
use crate::arch::{PhysAddr, VirtAddr, PAGE_SIZE};
use crate::vm::{VirRegion, VmProc};
use crate::vm::memtype::{find_mem_type, MemTypeError};

/// 映射设备寄存器到用户空间
pub fn map_device_registers(
    vmp: &mut VmProc,
    vaddr: VirtAddr,
    phys: PhysAddr,
    length: usize,
) -> Result<(), MemTypeError> {
    let mem_type = find_mem_type("physical memory mapping")
        .ok_or(MemTypeError::InvalidParam)?;

    let mut region = VirRegion::new(vaddr, length, VR_WRITABLE);
    region.set_mem_type(mem_type.clone());
    region.set_direct_phys(phys);

    for offset in (0..length).step_by(PAGE_SIZE) {
        let paddr = phys + offset;
        arch::map_page(vaddr + offset, paddr, arch::PT_FLAG_DEVICE)?;
    }

    vmp.add_region(region);
    Ok(())
}
```

**分配 DMA 缓冲区**

```rust
/// 分配 DMA 缓冲区
pub fn alloc_dma_buffer(
    vmp: &mut VmProc,
    length: usize,
) -> Result<(VirtAddr, PhysAddr), MemTypeError> {
    let mem_type = find_mem_type("anonymous memory (physically contiguous)")
        .ok_or(MemTypeError::InvalidParam)?;

    let vaddr = vmp.alloc_virtual_space(length)?;

    let mut region = VirRegion::new(vaddr, length, VR_WRITABLE | VR_CONTIG);
    region.set_mem_type(mem_type.clone());

    mem_type.on_new(&mut region)?;

    let phys = region.get_phys_block(0)
        .and_then(|pb| pb.phys_addr())
        .ok_or(MemTypeError::NoMemory)?;

    for offset in (0..length).step_by(PAGE_SIZE) {
        let paddr = phys + offset;
        arch::map_page(vaddr + offset, paddr, arch::PT_FLAG_WRITABLE)?;
    }

    vmp.add_region(region);
    Ok((vaddr, phys))
}
```

> **方案四标注**：`alloc_virtual_space` 是为进程分配虚拟地址空间（mmap 等），属于"进程地址空间管理"层次，不受 direct map 影响。但 `arch::map_page()` 内部操作页表时，在 direct map 下通过 `vm_phys_to_virt()` 直接写入页表项，无需临时映射窗口。
>
> 读者应区分两个不同层次：
> - **进程地址空间管理**（`alloc_virtual_space`）：为进程分配 VA，建立 VA→PA 映射。这是 VM 的核心职责，direct map 不改变这个层次。
> - **VM 物理页访问**（`vm_phys_to_virt`）：VM 操作页表页、物理页等。这是 VM 的内部实现，direct map 简化了这个层次。
>
> 两个层次的区分是理解 direct map 影响范围的关键：direct map 简化的是 VM 的内部实现（如何访问物理页），而不是 VM 的外部接口（如何管理进程地址空间）。

**驱动 API 总结**

| API | 用途 | 内存类型 |
|-----|------|---------|
| `map_device_registers` | 映射设备寄存器 | DirectPhysical |
| `alloc_dma_buffer` | 分配 DMA 缓冲区 | ContiguousAnonymous |
| `create_shared_memory` | 创建共享内存 | AnonymousMemory |
| `map_shared_memory` | 映射共享内存 | SharedMemory |

**注意事项**

1. **同步问题**：共享内存需要用户自行处理同步
2. **权限控制**：设备内存映射需要检查权限
3. **缓存一致性**：DMA 缓冲区可能需要刷新缓存
4. **地址对齐**：确保地址和长度按页对齐

---

## 6. 测试与验证

### 6.1 多态调用测试

**测试目标**

验证不同内存类型可以通过统一的 trait 接口进行多态调用。

**测试框架**

```rust
#[cfg(test)]
mod polymorphism_tests {
    use super::*;
    use std::sync::Arc;

    /// 测试所有内存类型的名称
    #[test]
    fn test_all_mem_type_names() {
        let types: Vec<Arc<dyn MemType>> = vec![
            MEM_TYPE_ANON.clone(),
            MEM_TYPE_DIRECT.clone(),
            MEM_TYPE_ANON_CONTIG.clone(),
            MEM_TYPE_CACHE.clone(),
            MEM_TYPE_MAPPEDFILE.clone(),
            MEM_TYPE_SHARED.clone(),
        ];

        let names: Vec<&str> = types.iter().map(|t| t.name()).collect();
        
        assert!(names.contains(&"anonymous memory"));
        assert!(names.contains(&"physical memory mapping"));
        assert!(names.contains(&"anonymous memory (physically contiguous)"));
        assert!(names.contains(&"cache memory"));
        assert!(names.contains(&"file-mapped memory"));
        assert!(names.contains(&"shared memory"));
    }

    /// 测试多态页错误处理
    #[test]
    fn test_polymorphic_pagefault() {
        let test_cases: Vec<(Arc<dyn MemType>, PagefaultResult)> = vec![
            // 匿名内存：物理页不存在，需要分配
            (MEM_TYPE_ANON.clone(), PagefaultResult::NeedNewPage),
            // 直接物理映射：物理页已存在
            (MEM_TYPE_DIRECT.clone(), PagefaultResult::Handled),
        ];

        for (mem_type, expected) in test_cases {
            let mut region = VirRegion::new(0x1000, 4096);
            let mut pr = PhysRegion::new(0);
            
            if mem_type.name() == "physical memory mapping" {
                pr.phys_block = PhysBlock::new(PhysAddr::new(0x10000));
            }

            let result = mem_type.on_pagefault(
                &VmProc::dummy(),
                &mut region,
                &mut pr,
                false,
            ).unwrap();

            assert_eq!(result, expected, "Failed for {}", mem_type.name());
        }
    }

    /// 测试多态可写判断
    #[test]
    fn test_polymorphic_writable() {
        let types: Vec<Arc<dyn MemType>> = vec![
            MEM_TYPE_ANON.clone(),
            MEM_TYPE_DIRECT.clone(),
            MEM_TYPE_SHARED.clone(),
        ];

        for mem_type in types {
            let pr = PhysRegion::new(0);
            let writable = mem_type.is_writable(&pr);
            
            match mem_type.name() {
                "anonymous memory" => assert!(writable),
                "physical memory mapping" => assert!(writable),
                "shared memory" => assert!(!writable),  // 无物理页时不可写
                _ => {}
            }
        }
    }

    /// 测试类型注册和查找
    #[test]
    fn test_type_registration() {
        // 验证所有内置类型已注册
        assert!(find_mem_type("anonymous memory").is_some());
        assert!(find_mem_type("physical memory mapping").is_some());
        assert!(find_mem_type("cache memory").is_some());
        
        // 验证未注册的类型返回 None
        assert!(find_mem_type("nonexistent type").is_none());
    }

    /// 测试动态类型变更
    #[test]
    fn test_dynamic_type_change() {
        let mut pr = PhysRegion::new(0);
        pr.phys_block = PhysBlock::new(PhysAddr::new(0x10000));
        
        // 初始为匿名内存
        pr.mem_type = MEM_TYPE_ANON.clone();
        assert_eq!(pr.mem_type.name(), "anonymous memory");
        
        // 变更为共享内存
        pr.mem_type = MEM_TYPE_SHARED.clone();
        assert_eq!(pr.mem_type.name(), "shared memory");
    }
}
```

**性能测试**

```rust
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    /// 测试多态调用开销
    #[test]
    fn test_polymorphic_call_overhead() {
        let mem_type = MEM_TYPE_ANON.clone();
        let mut region = VirRegion::new(0x1000, 4096);
        let mut pr = PhysRegion::new(0);
        let vmp = VmProc::dummy();

        // 预热
        for _ in 0..1000 {
            let _ = mem_type.is_writable(&pr);
        }

        // 测试
        let start = Instant::now();
        let iterations = 100_000;
        
        for _ in 0..iterations {
            let _ = mem_type.is_writable(&pr);
        }
        
        let duration = start.elapsed();
        let ns_per_call = duration.as_nanos() / iterations;
        
        println!("Polymorphic call overhead: {} ns/call", ns_per_call);
        
        // 虚函数调用应该在 10ns 以内
        assert!(ns_per_call < 10, "Too slow: {} ns/call", ns_per_call);
    }

    /// 测试类型查找性能
    #[test]
    fn test_type_lookup_performance() {
        // 预热
        for _ in 0..1000 {
            let _ = find_mem_type("anonymous memory");
        }

        // 测试
        let start = Instant::now();
        let iterations = 10_000;
        
        for _ in 0..iterations {
            let _ = find_mem_type("anonymous memory");
        }
        
        let duration = start.elapsed();
        let ns_per_lookup = duration.as_nanos() / iterations;
        
        println!("Type lookup: {} ns/lookup", ns_per_lookup);
        
        // 哈希表查找应该在 100ns 以内
        assert!(ns_per_lookup < 100, "Too slow: {} ns/lookup", ns_per_lookup);
    }
}
```

**集成测试**

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    /// 测试 fork 场景
    #[test]
    fn test_fork_scenario() {
        // 父进程创建匿名内存区域
        let mut parent_region = VirRegion::new(0x1000, 4096, VR_WRITABLE);
        parent_region.mem_type = MEM_TYPE_ANON.clone();
        
        // 分配物理页
        let mut pr = PhysRegion::new(0);
        pr.phys_block = PhysBlock::new(PhysAddr::new(0x10000));
        parent_region.add_phys_region(pr);

        // fork：创建子进程区域
        let mut child_region = VirRegion::new(0x1000, 4096, VR_WRITABLE);
        child_region.mem_type = MEM_TYPE_ANON.clone();
        
        // 复制物理区域引用
        let child_pr = parent_region.phys_regions[0].clone();
        child_region.add_phys_region(child_pr);

        // 验证引用计数
        assert_eq!(parent_region.phys_regions[0].phys_block.ref_count(), 2);

        // 子进程写入触发 CoW
        let result = child_region.mem_type.on_pagefault(
            &VmProc::dummy(),
            &mut child_region,
            &mut child_region.phys_regions[0],
            true,
        ).unwrap();
        
        assert_eq!(result, PagefaultResult::NeedCow);
    }

    /// 测试 mmap 文件场景
    #[test]
    fn test_mmap_file_scenario() {
        // 创建文件映射区域
        let mut region = VirRegion::new(0x1000, 4096, VR_WRITABLE);
        region.mem_type = MEM_TYPE_MAPPEDFILE.clone();
        region.file_info = Some(FileInfo {
            fd: 3,
            offset: 0,
            clearend: 0,
        });

        // 首次访问，从缓存或文件加载
        let mut pr = PhysRegion::new(0);
        let result = region.mem_type.on_pagefault(
            &VmProc::dummy(),
            &mut region,
            &mut pr,
            false,
        ).unwrap();

        // 应该需要异步 I/O 或已处理（如果有缓存）
        assert!(matches!(result, 
            PagefaultResult::NeedAsyncIo | PagefaultResult::Handled));

        // 写入触发 CoW
        pr.phys_block = PhysBlock::new(PhysAddr::new(0x10000));
        let result = region.mem_type.on_pagefault(
            &VmProc::dummy(),
            &mut region,
            &mut pr,
            true,
        ).unwrap();
        
        assert_eq!(result, PagefaultResult::NeedCow);
    }

    /// 测试共享内存场景
    #[test]
    fn test_shared_memory_scenario() {
        // 进程 A 创建共享内存
        let mut region_a = VirRegion::new(0x1000, 4096, VR_WRITABLE | VR_SHARED);
        region_a.mem_type = MEM_TYPE_SHARED.clone();
        
        // 分配物理页
        let mut pr = PhysRegion::new(0);
        pr.phys_block = PhysBlock::new(PhysAddr::new(0x10000));
        region_a.add_phys_region(pr);

        // 进程 B 映射共享内存
        let mut region_b = VirRegion::new(0x2000, 4096, VR_WRITABLE | VR_SHARED);
        region_b.mem_type = MEM_TYPE_SHARED.clone();
        region_b.shared_info = Some(SharedInfo {
            src_ep: Endpoint::new(1),
            src_vaddr: 0x1000,
            src_id: region_a.id,
        });

        // 进程 B 访问触发页错误
        let mut pr_b = PhysRegion::new(0);
        let result = region_b.mem_type.on_pagefault(
            &VmProc::dummy(),
            &mut region_b,
            &mut pr_b,
            true,
        ).unwrap();

        // 应该链接到进程 A 的物理页
        assert_eq!(result, PagefaultResult::Handled);
    }
}
```

### 6.2 CoW 集成测试

> ⚠️ 以下测试代码为**设计示意伪代码**，使用了尚未实现的 API（如 `PhysMem::alloc_page()`、`mem_cow()`、`arch::write_phys()` 等），不可直接编译。待对应模块实现后更新为可编译测试。

**测试目标**

验证 CoW 机制在不同内存类型中的正确性，与 `15-cow-mechanism.md` 的集成。

**集成测试清单**

- [ ] 匿名内存 CoW 基本流程
- [ ] 文件映射 CoW
- [ ] CoW 后写入隔离
- [ ] 多次 fork 的 CoW
- [ ] 共享内存不触发 CoW（共享内存直接写入）
- [ ] 连续匿名内存不支持 fork（不支持 CoW）

**与 15-cow-mechanism.md 的关系**

| 方面 | 本文档 (11-memtype.md) | 15-cow-mechanism.md |
|------|----------------------|---------------------|
| 侧重点 | 内存类型多态性 | CoW 机制细节 |
| 测试范围 | 各类型的 CoW 行为 | CoW 算法本身 |
| 实现层次 | trait 接口 | 底层实现 |
| 集成点 | `mem_cow()` 函数 | 页错误处理流程 |

---

## 7. 参见

- [10-phys-block.md](10-phys-block.md) - phys_block 结构与引用计数（pb_reference/pb_unreferenced）
- [12-vir-region.md](12-vir-region.md) - 区域的 def_memtype 字段与内存类型绑定
- [14-phys-region.md](14-phys-region.md) - phys_region 的 memtype 字段与类型覆盖
- [15-cow-mechanism.md](15-cow-mechanism.md) - 匿名内存的 CoW 与 cow_block 实现
- [16-pagefault.md](16-pagefault.md) - 页错误处理流程与 memtype 回调的调用时机
- [17-vm-fork.md](17-vm-fork.md) - fork 中的 ev_reference 与内存类型继承
- [19-vm-map.md](19-vm-map.md) - mmap 使用 mappedfile/shared 内存类型

---

*分类: VM库 | 可被其他服务使用*
