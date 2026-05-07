# 12-vir-region: 虚拟区域 (vir_region)

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/region.h`, `region.c`  
> **说明**: 管理进程的虚拟地址空间布局，使用 AVL 树组织

---

## 1. 概述

### 1.1 虚拟区域的作用

`vir_region`（虚拟区域）是 VM 管理进程虚拟地址空间的基本单元。每个区域代表一段连续的虚拟内存，具有相同的属性（可读、可写、可执行等）。

**典型区域类型**:
- **代码段**: 程序指令，通常只读
- **数据段**: 已初始化的全局变量
- **BSS 段**: 未初始化的全局变量
- **堆**: 动态分配的内存，向上增长
- **栈**: 函数调用栈，向下增长
- **内存映射**: 通过 `mmap()` 映射的文件或匿名内存

### 1.2 地址空间布局

进程的虚拟地址空间由多个不连续的区域组成。以下是典型的区域布局（具体地址范围由 Minix3 的 `VM_MMAPBASE` 等常量决定）：

```
低地址
┌─────────────────┐
│     代码段       │  ← 只读，可执行
├─────────────────┤
│     数据段       │  ← 读写
├─────────────────┤
│      堆         │  ← 向上增长（vm_region_top 跟踪顶部）
│                 │
├─────────────────┤
│    未使用        │
│                 │
├─────────────────┤
│   内存映射区      │  ← mmap 分配（从 VM_MMAPBASE 开始）
│                 │
├─────────────────┤
│    未使用        │
│                 │
├─────────────────┤
│      栈         │  ← 向下增长，带 guard page
└─────────────────┘
高地址
```

### 1.3 vm_region_top 的作用

`vm_region_top` 是 `vmproc` 结构中的一个字段，记录已分配区域中最高的结束地址。

**使用场景**:
- **堆增长**: `brk()` 系统调用增加堆大小时，通常从 `vm_region_top` 向上扩展
- **内存映射**: `mmap()` 没有指定地址时，默认从 `vm_region_top` 开始分配

**示例**:
```c
// 当前 vm_region_top = 0x10000
// brk(0x12000) 会创建新区域 [0x10000, 0x12000)
// 成功后 vm_region_top 更新为 0x12000
```

**fork 时的处理**: 直接复制父进程的 `vm_region_top` 值，子进程的地址空间布局与父进程一致。

> **注意**: `vm_region_top` 只是一个性能优化字段，用于快速找到分配新区域的位置。实际的区域边界需要通过 AVL 树查询确认。

---

## 2. C 源码分析

### 2.1 vir_region 结构体

**文件**: `minix3/minix/servers/vm/region.h`

```c
typedef struct vir_region {
    vir_bytes   vaddr;          /* 虚拟地址，页表偏移 */
    vir_bytes   length;         /* 长度（字节） */
    struct phys_region **physblocks;  /* 物理块指针数组 */
    u16_t       flags;          /* 区域标志 */
    struct vmproc *parent;      /* 拥有此区域的进程 */
    mem_type_t  *def_memtype;   /* 默认内存类型 */
    int         remaps;         /* 重映射计数 */
    int         id;             /* 唯一 ID */

    union {
        phys_bytes phys;        /* VR_DIRECT: 直接物理映射 */
        struct {
            endpoint_t ep;
            vir_bytes vaddr;
            int id;
        } shared;               /* VR_SHARED: 共享内存 */
        struct phys_block *pb_cache;  /* 物理块缓存 */
        struct {
            int     inited;
            struct fdref *fdref;
            u64_t   offset;
            u16_t   clearend;
        } file;                 /* 文件映射（def_memtype==mem_type_mappedfile 时使用） */
    } param;

    /* AVL 树字段 */
    struct vir_region *lower, *higher;
    int         factor;
} region_t;
```

**字段分类**:

| 分类 | 字段 | 说明 |
|------|------|------|
| **地址管理** | `vaddr`, `length` | 定义虚拟地址范围 |
| **物理映射** | `physblocks` | 指向 phys_region 指针数组 |
| **属性标志** | `flags` | 区域类型和权限 |
| **进程关联** | `parent` | 所属进程 |
| **内存类型** | `def_memtype` | 默认内存类型处理器 |
| **AVL 树** | `lower`, `higher`, `factor` | 用于快速查找 |

**与 phys_region 的关系**:

```
vir_region (虚拟区域)
    │
    │  vaddr=0x1000, length=0x4000 (16KB = 4 页)
    │
    ├── physblocks[0] ──→ phys_region ──→ phys_block (物理页)
    │       offset=0                          phys=0x8000
    │
    ├── physblocks[1] ──→ phys_region ──→ phys_block (物理页)
    │       offset=4096                       phys=0x9000
    │
    ├── physblocks[2] ──→ NULL (未分配物理页)
    │
    └── physblocks[3] ──→ phys_region ──→ phys_block (共享)
            offset=12288                      phys=0xA000
                                            refcount=2
```

**关键设计**:

1. **physblocks 数组**: 
   - 大小 = `length / VM_PAGE_SIZE`
   - 每个元素对应一页虚拟内存
   - 元素为 `NULL` 表示该页未分配物理内存（按需分配）

2. **param 联合体**:
   - 根据 `flags` 中的类型标志和 `def_memtype` 使用不同字段
   - `VR_DIRECT`: 直接物理映射（设备内存）
   - `VR_SHARED`: 共享内存映射
   - 文件映射: 由 `def_memtype == &mem_type_mappedfile` 决定，使用 `param.file` 字段

3. **AVL 树组织**:
   - 所有 vir_region 按 `vaddr` 排序
   - `lower` 指向低地址区域
   - `higher` 指向高地址区域
   - O(log n) 查找复杂度

### 2.2 区域标志 VR_*

**文件**: `minix3/minix/servers/vm/region.h`

Minix3 的区域标志分为两类：**映射类型标志**（高 8 位）和 **映射属性标志**（低 8 位）。

#### 2.2.1 映射类型标志

类型标志决定区域的内存管理方式，位于 `flags` 的高 8 位：

| 标志 | 值 | 说明 |
|------|-----|------|
| `VR_ANON` | `0x100` | 匿名内存，按需分配物理页，分配时清零 |
| `VR_DIRECT` | `0x200` | 直接映射，不由 VM 管理（如设备内存） |
| `VR_PREALLOC_MAP` | `0x400` | 预分配映射，用于 RS（重启服务）保留区域 |

```c
/* Mapping type: */
#define VR_ANON         0x100   /* Memory to be cleared and allocated */
#define VR_DIRECT       0x200   /* Mapped, but not managed by VM */
#define VR_PREALLOC_MAP 0x400   /* Preallocated map. */
```

**类型说明**:

- **VR_ANON**: 最常见的类型，用于堆、栈、匿名 mmap。物理页按需分配，首次访问时清零。
- **VR_DIRECT**: 直接物理映射，VM 不管理其物理内存。用于映射设备寄存器或预分配的物理内存。
- **VR_PREALLOC_MAP**: RS 服务专用，用于保存进程镜像，支持快速重启。

#### 2.2.2 映射属性标志

属性标志控制区域的访问权限和物理内存约束，位于 `flags` 的低 8 位：

| 标志 | 值 | 说明 |
|------|-----|------|
| `VR_WRITABLE` | `0x001` | 进程可写入此区域 |
| `VR_PHYS64K` | `0x004` | 物理内存必须 64KB 对齐 |
| `VR_LOWER16MB` | `0x008` | 物理内存必须在低 16MB（DMA 兼容） |
| `VR_LOWER1MB` | `0x010` | 物理内存必须在低 1MB（BIOS 兼容） |
| `VR_SHARED` | `0x040` | 共享内存区域 |
| `VR_UNINITIALIZED` | `0x080` | 分配后不清零（性能优化） |

```c
/* Mapping flags: */
#define VR_WRITABLE     0x001   /* Process may write here. */
#define VR_PHYS64K      0x004   /* Physical memory must be 64k aligned. */
#define VR_LOWER16MB    0x008
#define VR_LOWER1MB     0x010
#define VR_SHARED       0x040
#define VR_UNINITIALIZED 0x080  /* Do not clear after allocation  */
```

**属性说明**:

- **VR_WRITABLE**: 控制写权限。fork 时 CoW 会临时清除此标志，写时复制后恢复。
- **VR_PHYS64K/VR_LOWER16MB/VR_LOWER1MB**: DMA 约束，用于需要特定物理地址范围的设备。
- **VR_SHARED**: 标记共享内存区域，影响 fork 时的复制行为（共享而非 CoW）。
- **VR_UNINITIALIZED**: 跳过清零，用于性能敏感场景（如大型数据缓冲区）。

**标志组合示例**:

```c
// 普通堆区域
flags = VR_ANON | VR_WRITABLE;

// DMA 缓冲区（低 16MB，64KB 对齐）
flags = VR_ANON | VR_WRITABLE | VR_LOWER16MB | VR_PHYS64K;

// 设备寄存器映射
flags = VR_DIRECT | VR_WRITABLE;

// 共享内存
flags = VR_SHARED | VR_WRITABLE;
```

### 2.3 区域操作

**文件**: `minix3/minix/servers/vm/region.c`

Minix3 提供了一组区域操作函数，用于分配、释放和查找虚拟区域。

#### 2.3.1 map_page_region - 分配区域

分配新的虚拟区域并插入进程的 AVL 树（`region.c:463`）：

```c
struct vir_region *map_page_region(struct vmproc *vmp, vir_bytes minv,
    vir_bytes maxv, vir_bytes length, u32_t flags, int mapflags,
    mem_type_t *memtype)
{
    struct vir_region *newregion;
    vir_bytes startv;

    assert(!(length % VM_PAGE_SIZE));

    // 1. 在地址空间中找到合适的空闲槽位
    startv = region_find_slot(vmp, minv, maxv, length);
    if (startv == SLOT_FAIL)
        return NULL;

    // 2. 使用 Slab 分配器分配 vir_region 结构体
    if(!(newregion = region_new(vmp, startv, length, flags, memtype))) {
        printf("VM: map_page_region: allocating region failed\n");
        return NULL;
    }

    // 3. 调用内存类型的 ev_new 回调（如需要）
    if(newregion->def_memtype->ev_new) {
        if(newregion->def_memtype->ev_new(newregion) != OK) {
            /* ev_new will have freed and removed the region */
            return NULL;
        }
    }

    // 4. MF_PREALLOC: 预分配物理页（可选）
    if(mapflags & MF_PREALLOC) {
        if(map_handle_memory(vmp, newregion, 0, length, 1,
            NULL, 0, 0) != OK) {
            printf("VM: map_page_region: prealloc failed\n");
            map_free(newregion);
            return NULL;
        }
    }
    newregion->flags &= ~VR_UNINITIALIZED;  // 预分配后取消 UNINITIALIZED

    // 5. 插入 AVL 树
    region_insert(&vmp->vm_regions_avl, newregion);

    return newregion;
}
```

**内部函数 `region_new`**:

```c
static struct vir_region *region_new(struct vmproc *vmp, vir_bytes startv,
    vir_bytes length, int flags, mem_type_t *memtype)
{
    struct vir_region *newregion;
    struct phys_region **newphysregions;
    static u32_t id;
    int slots = phys_slot(length);  // length / VM_PAGE_SIZE

    // 使用 Slab 分配器分配结构体
    if(!(SLABALLOC(newregion))) {
        return NULL;
    }

    // 初始化字段
    memset(newregion, 0, sizeof(*newregion));
    newregion->vaddr = startv;
    newregion->length = length;
    newregion->flags = flags;
    newregion->def_memtype = memtype;
    newregion->remaps = 0;
    newregion->id = id++;
    newregion->lower = newregion->higher = NULL;
    newregion->parent = vmp;

    // 分配 physblocks 数组
    if(!(newphysregions = calloc(slots, sizeof(struct phys_region *)))) {
        SLABFREE(newregion);
        return NULL;
    }
    newregion->physblocks = newphysregions;

    return newregion;
}
```

**关键点**:
- `SLABALLOC` 是 Slab 分配器宏，用于分配固定大小的内核对象
- `physblocks` 数组初始化为全 NULL，表示没有物理页映射
- 区域插入 AVL 树后按 `vaddr` 排序

#### 2.3.2 map_free - 释放区域

释放虚拟区域及其所有物理页映射（`region.c:568`）：

```c
int map_free(struct vir_region *region)
{
    int r;

    // 1. 释放所有 phys_region 和减少 phys_block 引用计数
    if((r=map_subfree(region, 0, region->length)) != OK) {
        printf("%d\n", __LINE__);
        return r;
    }

    // 2. 调用内存类型的 ev_delete 回调
    if(region->def_memtype->ev_delete)
        region->def_memtype->ev_delete(region);

    // 3. 释放 physblocks 数组
    free(region->physblocks);
    region->physblocks = NULL;

    // 4. 使用 Slab 分配器释放结构体
    SLABFREE(region);

    return OK;
}

static int map_subfree(struct vir_region *region, 
    vir_bytes start, vir_bytes len)
{
    struct phys_region *pr;
    vir_bytes end = start + len;
    vir_bytes voffset;

#if SANITYCHECKS
    // 调试模式下先验证链表完整性
    SLABSANE(region);
    for(voffset = 0; voffset < phys_slot(region->length);
        voffset += VM_PAGE_SIZE) {
        struct phys_region *others;
        struct phys_block *pb;
        if(!(pr = physblock_get(region, voffset)))
            continue;
        pb = pr->ph;
        for(others = pb->firstregion; others;
            others = others->next_ph_list) {
            assert(others->ph == pb);
        }
    }
#endif

    for(voffset = start; voffset < end; voffset += VM_PAGE_SIZE) {
        if(!(pr = physblock_get(region, voffset)))
            continue;
        assert(pr->offset >= start);
        assert(pr->offset < end);
        // 减少引用计数，可能释放物理页
        pb_unreferenced(region, pr, 1);
        SLABFREE(pr);
    }

    return OK;
}
```

**释放流程**:
1. 遍历 `physblocks` 数组
2. 对每个非空 `phys_region`，调用 `pb_unreferenced` 减少引用计数
3. 如果引用计数降为 0，释放物理页
4. 释放 `physblocks` 数组和 `vir_region` 结构体

#### 2.3.3 map_lookup - 查找区域

通过 AVL 树查找包含指定地址的区域（`region.c:616`）：

```c
struct vir_region *map_lookup(struct vmproc *vmp,
    vir_bytes offset, struct phys_region **physr)
{
    struct vir_region *r;

#if SANITYCHECKS
    if(!region_search_root(&vmp->vm_regions_avl))
        panic("process has no regions: %d", vmp->vm_endpoint);
#endif

    // 在 AVL 树中搜索（查找 <= offset 的最大 vaddr）
    if((r = region_search(&vmp->vm_regions_avl, offset, AVL_LESS_EQUAL))) {
        // 检查地址是否在区域内
        if(offset >= r->vaddr && offset < r->vaddr + r->length) {
            // 计算页内偏移并获取 phys_region
            vir_bytes ph = offset - r->vaddr;
            if(physr) {
                *physr = physblock_get(r, ph);
                if(*physr) assert((*physr)->offset == ph);
            }
            return r;
        }
    }

    return NULL;  // 未找到
}
```

**AVL 树搜索类型**:

| 类型 | 值 | 说明 |
|------|-----|------|
| `AVL_EQUAL` | 1 | 精确匹配 |
| `AVL_LESS` | 2 | 小于 |
| `AVL_GREATER` | 4 | 大于 |
| `AVL_LESS_EQUAL` | 3 | 小于等于 |
| `AVL_GREATER_EQUAL` | 5 | 大于等于 |

**查找复杂度**: O(log n)，其中 n 为区域数量

### 2.4 物理块引用链表

**文件**: `minix3/minix/servers/vm/pb.c`, `phys_region.h`

当一个物理块被多个虚拟区域共享时（如 fork 后的 CoW），需要跟踪所有引用者。Minix3 使用单向链表组织这些引用。

#### 2.4.1 链表结构

```
phys_block (物理块)
    │
    ├── firstregion ──→ phys_region P1 (父进程)
    │                       │
    │                       └── next_ph_list ──→ phys_region P2 (子进程)
    │                                                │
    │                                                └── next_ph_list ──→ NULL
    │
    └── refcount = 2
```

**相关字段**:

```c
// region.h
struct phys_block {
#if SANITYCHECKS
    u32_t            seencount;     // 调试：遍历检查计数
#endif
    phys_bytes       phys;          // 物理内存地址
    struct phys_region *firstregion; // 引用链表头
    u8_t             refcount;      // 引用计数
    u8_t             flags;
};

// phys_region.h
struct phys_region {
    struct phys_block  *ph;         // 指向物理块
    struct vir_region  *parent;     // 所属虚拟区域
    vir_bytes          offset;      // 区域内偏移
#if SANITYCHECKS
    int                written;     // 已写入页表标记
#endif
    mem_type_t        *memtype;     // 内存类型回调
    struct phys_region *next_ph_list; // 链表下一个节点
};
```

#### 2.4.2 pb_link - 插入链表

使用**头插法**将新的 `phys_region` 插入链表（`pb.c:61`）：

```c
void pb_link(struct phys_region *newphysr, struct phys_block *newpb,
    vir_bytes offset, struct vir_region *parent)
{
    // 设置 phys_region 字段（USE 宏在 SANITYCHECKS 下有额外检查）
    USE(newphysr,
    newphysr->offset = offset;
    newphysr->ph = newpb;
    newphysr->parent = parent;
    newphysr->next_ph_list = newpb->firstregion;
    newpb->firstregion = newphysr;);
    newpb->refcount++;
}
```

**头插法优势**: O(1) 时间复杂度，无需遍历链表。

#### 2.4.3 pb_unreferenced - 从链表移除

从链表中移除 `phys_region` 并减少引用计数（`pb.c:96`）：

```c
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb = pr->ph;

    assert(pb->refcount > 0);
    pb->refcount--;

    // 从链表中移除
    if (pb->firstregion == pr) {
        // 要移除的是链表头
        pb->firstregion = pr->next_ph_list;
    } else {
        // 遍历链表找到前驱节点
        struct phys_region *others;
        for (others = pb->firstregion; others; others = others->next_ph_list) {
            assert(others->ph == pb);  // 验证链表完整性
            if (others->next_ph_list == pr) {
                others->next_ph_list = pr->next_ph_list;
                break;
            }
        }
        assert(others);  // 确保找到了节点（否则不在链表中）
    }

    // 引用计数为 0，释放物理块
    if (pb->refcount == 0) {
        assert(!pb->firstregion);  // 链表应为空
        int r;
        if((r = pr->memtype->ev_unreference(pr)) != OK)
            panic("unref failed, %d", r);
        SLABFREE(pb);
    }

    pr->ph = NULL;
    if (rm) physblock_set(region, pr->offset, NULL);
}
```

#### 2.4.4 遍历链表 - CoW 场景

在 CoW 写时复制时，需要遍历链表设置所有引用者为只读：

```c
// region.c - 健全性检查中的遍历示例
for (others = pb->firstregion; others; others = others->next_ph_list) {
    assert(others->ph == pb);  // 验证链表完整性
    // 可以在这里设置页表权限
}
```

**CoW 流程中的使用**:

1. fork 时：`pb_reference()` 创建新 `phys_region`，插入链表，`refcount++`
2. 写入时：遍历链表，设置所有引用者的页表为只读
3. 写缺页时：`pb_unreferenced()` 移除引用，`refcount--`，可能释放物理页

---

## 3. Rust 设计决策

### 3.1 VirRegion 结构

**C 结构体** (`region.h`):

```c
typedef struct vir_region {
    vir_bytes vaddr;           // 虚拟地址
    vir_bytes length;          // 长度
    struct phys_region **physblocks;  // 物理区域指针数组
    u16_t flags;               // 标志位
    struct vmproc *parent;     // 所属进程
    mem_type_t *def_memtype;   // 默认内存类型
    int remaps;                // 重映射计数
    int id;                    // 唯一 ID
    union { ... } param;       // 参数联合体
    struct vir_region *lower, *higher;  // AVL 子节点
    int factor;                // 平衡因子
} region_t;
```

**Rust 结构体** (`vir_region.rs`):

```rust
pub struct VirRegion {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub physblocks: Vec<Option<Box<PhysRegion>>>,
    pub flags: VrFlags,
    pub parent: Option<NonNull<VmProc>>,
    pub remaps: i32,
    pub id: i32,
    pub param: VrParam,
    pub lower: Option<Box<VirRegion>>,
    pub higher: Option<Box<VirRegion>>,
    pub factor: i8,
}
```

**设计差异**:

| 字段 | C 类型 | Rust 类型 | 说明 |
|------|--------|-----------|------|
| `physblocks` | `struct phys_region**` | `Vec<Option<Box<PhysRegion>>>` | Rust 使用 Vec 替代裸指针数组，内存安全 |
| `parent` | `struct vmproc*` | `Option<NonNull<VmProc>>` | 显式表达可能为空，避免空指针 |
| `flags` | `u16_t` | `VrFlags(u16)` | 封装为类型安全的结构体 |
| `param` | `union` | `enum VrParam` | Rust enum 提供类型安全的联合体 |
| AVL 字段 | 内嵌指针 | `Option<Box<VirRegion>>` | 所有权明确，自动内存管理 |

**32位 vs 64位架构差异**:

| 方面 | Minix3 (32位) | minix-rs (64位) | 说明 |
|------|---------------|-----------------|------|
| `vaddr` / `length` | `vir_bytes` = u32 | `VirBytes` = u64 | 64位地址空间 |
| `physblocks` 指针 | 4字节指针 | 8字节指针 | 指针大小翻倍 |
| `param.phys` | `phys_bytes` = u32 | u64 | 支持更大物理地址 |
| 结构体大小 | ~40字节 | ~72字节 | 指针和地址字段增大 |

### 3.2 标志位设计

**C 宏定义**:

```c
#define VR_WRITABLE     0x001
#define VR_PHYS64K      0x004
#define VR_LOWER16MB    0x008
#define VR_LOWER1MB     0x010
#define VR_SHARED       0x040
#define VR_UNINITIALIZED 0x080
#define VR_ANON         0x100
#define VR_DIRECT       0x200
#define VR_PREALLOC_MAP 0x400
```

**Rust 实现**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VrFlags(pub u16);

impl VrFlags {
    pub const WRITABLE: u16 = 0x001;
    pub const PHYS64K: u16 = 0x004;
    pub const LOWER16MB: u16 = 0x008;
    pub const LOWER1MB: u16 = 0x010;
    pub const SHARED: u16 = 0x040;
    pub const UNINITIALIZED: u16 = 0x080;
    pub const ANON: u16 = 0x100;
    pub const DIRECT: u16 = 0x200;
    pub const PREALLOC_MAP: u16 = 0x400;

    pub const fn contains(&self, flag: u16) -> bool {
        (self.0 & flag) != 0
    }
}
```

**为何不用 `bitflags!` 宏**:

当前实现使用手动封装而非 `bitflags!` 宏，原因：
1. 标志位分为两组：**映射属性**（低 8 位）和**映射类型**（高 8 位）
2. 需要支持运行时动态组合，宏生成的常量函数有限
3. 保持与 C 代码的直接对应，便于对照理解

**未来改进**: 可迁移至 `bitflags!` 宏，获得更好的类型安全和格式化支持：

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct VrFlags: u16 {
        const WRITABLE = 0x001;
        const PHYS64K = 0x004;
        // ...
    }
}
```

### 3.3 physblocks 数组设计

**C 实现**: 指针数组

```c
struct phys_region **physblocks;  // 指针数组，每个元素是 phys_region*
```

**Rust 实现**: `Vec<Option<Box<PhysRegion>>>`

```rust
pub physblocks: Vec<Option<Box<PhysRegion>>>,
```

**设计理由**:

1. **索引访问 O(1)**: 根据虚拟地址偏移直接计算页号，无需遍历
2. **内存连续**: 数组在内存中连续，缓存友好
3. **可选映射**: `Option` 表示该页尚未分配物理内存（按需分配）

**数组大小计算**:

```rust
let pages = ((length.get() + 4095) / 4096) as usize;
```

**与链表对比**:

| 特性 | 数组 | 链表 |
|------|------|------|
| 索引访问 | O(1) | O(n) |
| 插入/删除 | O(n) | O(1) |
| 内存布局 | 连续 | 分散 |
| 适用场景 | 固定大小、频繁索引 | 动态大小、频繁插入 |

**选择数组的原因**: `physblocks` 的大小在区域创建时确定（`length / PAGE_SIZE`），之后不会改变，且需要频繁按页号索引，数组是最佳选择。

---

## 4. 实现详解

### 4.1 区域创建

**核心函数**: `region_new()` + `map_page_region()`

**流程图**:

```
map_page_region()
    │
    ├── 1. region_find_slot()     // 查找空闲槽位
    │
    ├── 2. region_new()           // 分配结构体
    │       ├── SLABALLOC(vir_region)
    │       ├── 初始化字段
    │       └── calloc(physblocks 数组)
    │
    ├── 3. ev_new() 回调          // 内存类型初始化
    │
    ├── 4. MF_PREALLOC 处理       // 预分配物理页（可选）
    │
    └── 5. region_insert()        // 插入 AVL 树
```

**region_new() 实现** (`region.c:424`):

```c
static struct vir_region *region_new(struct vmproc *vmp, 
    vir_bytes startv, vir_bytes length, int flags, mem_type_t *memtype)
{
    struct vir_region *newregion;
    struct phys_region **newphysregions;
    static u32_t id;
    int slots = phys_slot(length);  // length / VM_PAGE_SIZE

    // 使用 Slab 分配器分配结构体
    if(!(SLABALLOC(newregion))) {
        return NULL;
    }

    // 初始化字段
    memset(newregion, 0, sizeof(*newregion));
    newregion->vaddr = startv;
    newregion->length = length;
    newregion->flags = flags;
    newregion->def_memtype = memtype;
    newregion->remaps = 0;
    newregion->id = id++;
    newregion->lower = newregion->higher = NULL;
    newregion->parent = vmp;

    // 分配 physblocks 数组
    if(!(newphysregions = calloc(slots, sizeof(struct phys_region *)))) {
        SLABFREE(newregion);
        return NULL;
    }
    newregion->physblocks = newphysregions;

    return newregion;
}
```

**关键点**:
- `physblocks` 数组大小 = `length / PAGE_SIZE`，初始化为 NULL
- 使用 `SLABALLOC` 而非 `malloc`，便于内核内存管理
- 区域创建时不分配物理页，按需分配（懒分配）

### 4.2 区域查找

**核心函数**: `map_lookup()`

**实现** (`region.c:616`):

```c
struct vir_region *map_lookup(struct vmproc *vmp,
    vir_bytes offset, struct phys_region **physr)
{
    struct vir_region *r;

    // AVL 树搜索：查找 <= offset 的最大 vaddr
    if((r = region_search(&vmp->vm_regions_avl, offset, AVL_LESS_EQUAL))) {
        vir_bytes ph;
        // 检查地址是否在区域内
        if(offset >= r->vaddr && offset < r->vaddr + r->length) {
            ph = offset - r->vaddr;  // 区域内偏移
            if(physr) {
                *physr = physblock_get(r, ph);  // 获取 phys_region
            }
            return r;
        }
    }

    return NULL;  // 未找到
}
```

**查找逻辑**:

```
输入: offset = 0x5000

AVL 树:
         [0x3000, 0x2000]  (vaddr=0x3000, length=0x2000)
              │
              ├── [0x1000, 0x1000]  (vaddr=0x1000, length=0x1000)
              │
              └── [0x6000, 0x1000]  (vaddr=0x6000, length=0x1000)

步骤 1: region_search(AVL_LESS_EQUAL, 0x5000)
        → 返回 [0x3000, 0x2000] (vaddr=0x3000 <= 0x5000)

步骤 2: 检查 0x5000 是否在 [0x3000, 0x5000) 范围内
        0x3000 <= 0x5000 < 0x5000 → 是

步骤 3: 计算页内偏移 ph = 0x5000 - 0x3000 = 0x2000
        获取 physblocks[0x2000 / 4096] = physblocks[2]
```

**复杂度**: O(log n)，n 为区域数量

### 4.3 区域合并

**Minix3 不实现区域合并**。

**原因分析**:

1. **复杂度高**: 需要检查相邻区域的标志、内存类型、权限是否一致
2. **收益有限**: 合并主要优化查找，但 AVL 树查找已是 O(log n)
3. **munmap 稀疏**: 实际场景中，munmap 往往释放整个区域，很少产生碎片

**Rust 实现建议**: 同样不实现合并，保持简单。若未来需要，可在 `munmap` 后检查相邻区域是否可合并。

### 4.4 区域分割

**核心函数**: `split_region()`

**使用场景**: `munmap` 部分区域时，需要将一个区域分割成两个。

**流程图**:

```
原始区域: [0x1000, 0x4000]  (vaddr=0x1000, length=0x4000)
munmap(0x2000, 0x1000)  // 释放中间部分

步骤 1: 分割成三个区域
        [0x1000, 0x1000]  ← 保留
        [0x2000, 0x1000]  ← 释放
        [0x3000, 0x2000]  ← 保留

步骤 2: 释放中间区域
```

**实现** (`region.c:1150`):

```c
static int split_region(struct vmproc *vmp, struct vir_region *vr,
    struct vir_region **vr1, struct vir_region **vr2, vir_bytes split_len)
{
    struct vir_region *r1 = NULL, *r2 = NULL;
    vir_bytes rem_len = vr->length - split_len;
    int slots1, slots2;
    vir_bytes voffset;
    int n1 = 0, n2 = 0;

    assert(!(split_len % VM_PAGE_SIZE));
    assert(!(rem_len % VM_PAGE_SIZE));

    // 检查内存类型是否支持分割
    if(!vr->def_memtype->ev_split) {
        printf("VM: split region not implemented for %s\n",
            vr->def_memtype->name);
        return EINVAL;
    }

    slots1 = phys_slot(split_len);
    slots2 = phys_slot(rem_len);

    // 创建两个新区域
    if(!(r1 = region_new(vmp, vr->vaddr, split_len, vr->flags,
        vr->def_memtype))) {
        goto bail;
    }
    if(!(r2 = region_new(vmp, vr->vaddr+split_len, rem_len, vr->flags,
        vr->def_memtype))) {
        map_free(r1);
        goto bail;
    }

    // 迁移 r1 的 phys_region 引用
    for(voffset = 0; voffset < r1->length; voffset += VM_PAGE_SIZE) {
        struct phys_region *ph, *phn;
        if(!(ph = physblock_get(vr, voffset))) continue;
        if(!(phn = pb_reference(ph->ph, voffset, r1, ph->memtype)))
            goto bail;
        n1++;
    }

    // 迁移 r2 的 phys_region 引用
    for(voffset = 0; voffset < r2->length; voffset += VM_PAGE_SIZE) {
        struct phys_region *ph, *phn;
        if(!(ph = physblock_get(vr, split_len + voffset))) continue;
        if(!(phn = pb_reference(ph->ph, voffset, r2, ph->memtype)))
            goto bail;
        n2++;
    }

    // 调用内存类型的 ev_split 回调
    vr->def_memtype->ev_split(vmp, vr, r1, r2);

    // 替换原区域
    region_remove(&vmp->vm_regions_avl, vr->vaddr);
    map_free(vr);
    region_insert(&vmp->vm_regions_avl, r1);
    region_insert(&vmp->vm_regions_avl, r2);

    *vr1 = r1;
    *vr2 = r2;
    return OK;

  bail:
    if(r1) map_free(r1);
    if(r2) map_free(r2);
    printf("split_region: failed\n");
    return ENOMEM;
}
```

**关键点**:
- 分割后物理页引用计数不变（`pb_reference` 增加引用）
- 需要内存类型支持 `ev_split` 回调
- 原区域被释放，新区域插入 AVL 树

---

## 5. fork 相关操作

> 本章仅概述 vir_region 在 fork 中的角色，详细分析参见 [17-vm-fork.md](17-vm-fork.md)。

### 5.1 区域复制

fork 时通过 `map_proc_copy()` → `map_copy_region()` 复制父进程的所有虚拟区域。核心逻辑：

1. `map_copy_region()` (`region.c:802`) 为每个 `vir_region` 创建新实例
2. 新区域共享原物理页（`pb_reference` 增加引用计数），不复制数据
3. `ev_reference` 回调设置 CoW 标记

**关键设计**: `map_copy_region` 创建的新区域处于"limbo"状态——不增加 `phys_block.refcount`，由调用者（`map_proc_copy_range`）在链接到子进程后负责增加。

### 5.2 CoW 设置

fork 后父子进程共享物理页，写入时才复制：

1. **页表只读**: `map_ph_writept()` 根据区域和内存类型的可写性设置页表标志
2. **写入触发缺页**: 缺页处理调用 `mem_cow()` (`pb.c:136`) 分配新物理页并复制数据
3. **CoW 后变匿名**: `mem_cow()` 将 `ph->memtype` 设为 `mem_type_anon`

> CoW 机制的完整分析参见 [15-cow-mechanism.md](15-cow-mechanism.md)。

---

## 6. 测试与验证

### 6.1 区域操作测试

**测试文件**: `os/servers/vm/src/region/vir_region.rs`

**已实现测试**:

| 测试名称 | 测试内容 |
|----------|----------|
| `test_vir_region_creation` | 区域创建，验证 vaddr、length、end_addr |
| `test_vir_region_contains` | 地址包含判断，边界条件 |
| `test_vir_region_flags` | 标志位操作，insert/remove |
| `test_vir_region_split` | 区域分割，验证左右区域属性 |
| `test_vir_region_split_invalid` | 无效分割参数处理 |
| `test_vir_region_free_range` | 范围释放，验证页释放计数 |

**示例测试代码**:

```rust
#[test]
fn test_vir_region_split() {
    let region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());

    let (left, right) = region.split(VirBytes(0x2000)).unwrap();

    assert_eq!(left.vaddr, VirBytes(0x1000));
    assert_eq!(left.length, VirBytes(0x2000));
    assert_eq!(right.vaddr, VirBytes(0x3000));
    assert_eq!(right.length, VirBytes(0x2000));
}
```

### 6.2 AVL 树集成测试

**测试文件**: `os/servers/vm/src/region/avl.rs`

**已实现测试**:

| 测试名称 | 测试内容 |
|----------|----------|
| `test_avl_insert_and_find` | 插入和查找，验证 O(log n) 查找 |
| `test_avl_remove` | 删除节点，验证树结构完整性 |
| `test_avl_find_overlap` | 重叠区域查找 |
| `test_avl_traverse` | 中序遍历，验证按地址排序 |

**示例测试代码**:

```rust
#[test]
fn test_avl_insert_and_find() {
    let mut avl = RegionAvl::new();

    avl.insert(VirRegion::new(VirBytes(0x1000), VirBytes(0x1000), VrFlags::empty()));
    avl.insert(VirRegion::new(VirBytes(0x3000), VirBytes(0x1000), VrFlags::empty()));
    avl.insert(VirRegion::new(VirBytes(0x2000), VirBytes(0x1000), VrFlags::empty()));

    assert_eq!(avl.len(), 3);

    // 查找存在的区域
    let found = avl.find(VirBytes(0x1500));
    assert!(found.is_some());
    assert_eq!(found.unwrap().vaddr, VirBytes(0x1000));
}
```

**运行测试**:

```bash
cd os/servers/vm
cargo test region
```

---

## 7. 参见

- [00-vm-overview.md](00-vm-overview.md) - VM 模块总览
- [08-slab-allocator.md](08-slab-allocator.md) - 区域分配使用 Slab
- [10-phys-block.md](10-phys-block.md) - 物理块（phys_block 结构体）
- [14-phys-region.md](14-phys-region.md) - 物理区域（phys_region 结构体、pb_link、pb_unreferenced）
- [11-memtype.md](11-memtype.md) - 内存类型（mem_type_t 及回调机制）
- [13-region-avl.md](13-region-avl.md) - AVL 树实现
- [15-cow-mechanism.md](15-cow-mechanism.md) - CoW 机制详解（mem_cow、CoW 触发流程）
- [16-pagefault.md](16-pagefault.md) - 缺页处理（CoW 触发入口）
- [17-vm-fork.md](17-vm-fork.md) - fork 时的区域复制（map_proc_copy、map_copy_region 详解）

---

*分类: VM私有*
