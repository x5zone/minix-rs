# 10-phys-block: 物理块 (phys_block)

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/pb.c`, `region.h`  
> **说明**: 管理物理内存块，实现引用计数和生命周期管理

---

## 1. 概述

**物理块的核心作用**

`phys_block` 是 VM 中管理物理内存页的核心数据结构。每个 `phys_block` 代表一个物理内存页（4KB），并通过引用计数机制支持多个虚拟区域共享同一物理页。

```
┌─────────────────────────────────────────────────────────────┐
│                    phys_block 在 VM 中的位置                  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   进程虚拟地址空间                                           │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ vir_region (虚拟区域)                                │   │
│   │   vaddr: 0x400000                                    │   │
│   │   length: 0x3000                                     │   │
│   │   physblocks[0] ──────────────────┐                 │   │
│   │   physblocks[1] ──────────────────┼──────────┐      │   │
│   │   physblocks[2] ──────────────────┼──────────┼───┐  │   │
│   └───────────────────────────────────┼──────────┼───┼──┘   │
│                                       │          │   │      │
│   phys_region (物理区域)              │          │   │      │
│   ┌───────────────────────────────────┼──────────┼───┼──┐   │
│   │ offset: 0x0000                    │          │   │  │   │
│   │ ph ───────────────────────────────┘          │   │  │   │
│   │ memtype: anon                                 │   │  │   │
│   └───────────────────────────────────────────────┘   │  │   │
│                                                       │  │   │
│   phys_block (物理块)                                  │  │   │
│   ┌───────────────────────────────────────────────┐   │  │   │
│   │ phys: 0x1234000                                │   │  │   │
│   │ refcount: 1                                    │   │  │   │
│   │ firstregion ───────────────────────────────────┘  │  │   │
│   └───────────────────────────────────────────────────┘  │   │
│                                                           │   │
│   CoW 场景: fork 后父子进程共享物理页                       │   │
│   ┌───────────────────────────────────────────────────┐   │   │
│   │ 父进程 vir_region                                  │   │   │
│   │   physblocks[0] ──┐                               │   │   │
│   └────────────────────┼──────────────────────────────┘   │   │
│                        │                                   │   │
│   ┌────────────────────┼──────────────────────────────┐   │   │
│   │ 子进程 vir_region  │                              │   │   │
│   │   physblocks[0] ───┼──┐                           │   │   │
│   └────────────────────┼──┼──────────────────────────┘   │   │
│                        │  │                               │   │
│                        ▼  ▼                               │   │
│   ┌───────────────────────────────────────────────────┐   │   │
│   │ phys_block                                         │   │   │
│   │ phys: 0x1234000                                    │   │   │
│   │ refcount: 2  ◄─── 两个进程共享                      │   │   │
│   │ firstregion ──► phys_region 链表                   │   │   │
│   └───────────────────────────────────────────────────┘   │   │
│                                                           │   │
└─────────────────────────────────────────────────────────────┘
```

**引用计数机制**

引用计数是 `phys_block` 的核心特性，实现了物理内存的安全共享：

```
┌─────────────────────────────────────────────────────────────┐
│                    引用计数生命周期                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   创建 phys_block:                                          │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ pb_new() → phys_block { refcount: 0 }              │   │
│   │                    │                                │   │
│   │ pb_link() ─────────┼──► refcount: 1                │   │
│   │                    │                                │   │
│   │ 第一个 phys_region 引用此块                          │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   增加引用 (fork, 共享内存):                                 │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ pb_link() ────► refcount++                          │   │
│   │                                                     │   │
│   │ refcount: 1 → 2 → 3 ...                            │   │
│   │                                                     │   │
│   │ 每增加一个 phys_region 引用，refcount +1            │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   减少引用 (munmap, exit, CoW):                             │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ pb_unreferenced() ────► refcount--                  │   │
│   │                                                     │   │
│   │ refcount: 3 → 2 → 1 ...                            │   │
│   │                                                     │   │
│   │ 当 refcount == 0 时:                                │   │
│   │   - 调用 memtype->ev_unreference()                  │   │
│   │   - 释放物理内存                                     │   │
│   │   - SLABFREE(pb)                                    │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**与 Minix3 的对应关系**

| Minix3 结构 | 作用 | Rust 对应 |
|------------|------|----------|
| `phys_block` | 物理内存块，含引用计数 | `PhysBlock` |
| `phys_region` | 虚拟区域到物理块的映射 | `PhysRegion` |
| `pb_new()` | 创建物理块 | `PhysBlock::new()` |
| `pb_free()` | 释放物理块 | `Drop` trait |
| `pb_link()` | 增加引用 | `PhysBlock::link()` |
| `pb_unreferenced()` | 减少引用 | `PhysBlock::unlink()` |
| `refcount` | 引用计数 | `AtomicU8` 或 `Cell<u8>` |
| `firstregion` | 引用链表头 | `Option<NonNull<PhysRegion>>` |

**关键设计要点**

```
1. 引用计数存储位置:
   - Minix3: 在 phys_block 中存储 refcount
   - 优点: 集中管理，易于验证
   - 缺点: 需要额外的链表遍历

2. 物理块与虚拟区域的关系:
   - 一个 phys_block 可被多个 phys_region 引用
   - 一个 phys_region 只能引用一个 phys_block
   - 通过 firstregion 链表维护所有引用

3. 生命周期管理:
   - 创建: 分配物理页 + 初始化 refcount=0
   - 链接: pb_link() 增加 refcount
   - 解链: pb_unreferenced() 减少 refcount
   - 释放: refcount==0 时释放物理页和 phys_block

4. CoW 支持:
   - 写操作时检查 refcount
   - refcount > 1 时触发写时复制
   - 复制后原块 refcount--，新块 refcount=1
```

---

## 2. C 源码分析

### 2.1 phys_block 结构体

**结构体定义**

Minix3 中 `phys_block` 的定义位于 `region.h`：

```c
struct phys_block {
#if SANITYCHECKS
    u32_t       seencount;      /* 调试用：遍历检查 */
#endif
    phys_bytes  phys;           /* 物理内存地址 */
    
    /* 引用此块的 phys_region 链表头 */
    struct phys_region *firstregion;    
    u8_t        refcount;       /* 引用计数 */
    u8_t        flags;          /* 标志位 */
};

/* phys_block 标志位 */
#define PBF_INCACHE     0x01    /* 此块在页面缓存中 */
```

**字段详解**

```
┌─────────────────────────────────────────────────────────────┐
│                    phys_block 字段布局                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   字段            类型         大小    说明                   │
│   ────────────────────────────────────────────────────────  │
│   seencount      u32_t        4      调试用（条件编译）       │
│   phys           phys_bytes   8      物理内存地址            │
│   firstregion    *phys_region 8      引用链表头              │
│   refcount       u8_t         1      引用计数                │
│   flags          u8_t         1      标志位                  │
│   ────────────────────────────────────────────────────────  │
│   总大小（不含调试字段）: 约 24 字节（64位，含对齐填充）     │
│   SLAB 分配对齐后: 约 24-32 字节                             │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**phys 字段**

```c
phys_bytes phys;  /* 物理内存地址 */
```

- 存储物理页的起始地址
- 必须是页对齐（4KB 对齐）
- 值为 `MAP_NONE` 表示未分配物理内存（延迟分配）

```
示例:
phys = 0x1234000  → 物理页 0x1234000-0x1234FFF
phys = MAP_NONE   → 无物理内存（延迟分配）
```

**firstregion 字段**

```c
struct phys_region *firstregion;  /* 引用链表头 */
```

- 指向第一个引用此块的 `phys_region`
- 通过 `phys_region.next_ph_list` 形成链表
- 用于遍历所有引用此块的虚拟区域

```
┌─────────────────────────────────────────────────────────────┐
│                    firstregion 链表结构                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   phys_block                                                 │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ phys: 0x1234000                                     │   │
│   │ refcount: 3                                         │   │
│   │ firstregion ────────────────────────────────┐       │   │
│   └───────────────────────────────────────────────┼───────┘   │
│                                                     │         │
│   phys_region 链表                                  ▼         │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ phys_region 1 (进程A, offset=0x0000)                │   │
│   │ next_ph_list ───────────────────────────────┐       │   │
│   └───────────────────────────────────────────────┼───────┘   │
│                                                     │         │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ phys_region 2 (进程B, offset=0x1000)                │   │
│   │ next_ph_list ───────────────────────────────┐       │   │
│   └───────────────────────────────────────────────┼───────┘   │
│                                                     │         │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ phys_region 3 (进程C, offset=0x2000)                │   │
│   │ next_ph_list = NULL                                 │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   refcount = 链表长度 = 3                                   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**refcount 字段**

```c
u8_t refcount;  /* 引用计数 */
```

- 记录有多少个 `phys_region` 引用此块
- 范围: 0-255（实际很少超过 10）
- 关键操作:
  - `pb_link()`: refcount++
  - `pb_unreferenced()`: refcount--

```
refcount 状态:

0: 新创建，尚未被引用
   - pb_new() 返回时
   - 等待第一次 pb_link()

1: 单一引用
   - 正常的私有内存
   - 可以直接写入

>1: 多重引用
   - fork 后的共享内存
   - 写入需要 CoW
```

**flags 字段**

```c
u8_t flags;
#define PBF_INCACHE  0x01  /* 此块在页面缓存中 */
```

- `PBF_INCACHE`: 表示此物理块被页面缓存管理
  - 由文件映射或磁盘缓存使用
  - 释放时需要从缓存中移除
  - 避免重复缓存同一物理页

```
flags 使用场景:

1. 匿名内存:
   flags = 0x00
   - 普通进程堆/栈内存
   - 释放时直接归还空闲列表

2. 文件映射:
   flags = PBF_INCACHE
   - mmap 文件内容
   - 释放时更新缓存状态

3. 磁盘缓存:
   flags = PBF_INCACHE
   - 页面缓存中的块
   - 可能被多个文件共享
```

**seencount 字段（调试用）**

```c
#if SANITYCHECKS
u32_t seencount;  /* 调试用 */
#endif
```

- 仅在 `SANITYCHECKS` 宏定义时存在
- 用于遍历检查，防止重复访问
- 生产环境不包含此字段

**内存布局示意**

```
┌─────────────────────────────────────────────────────────────┐
│                    phys_block 内存布局                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   生产环境 (SANITYCHECKS 未定义):                            │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ phys (8 bytes)                                      │   │
│   ├─────────────────────────────────────────────────────┤   │
│   │ firstregion (8 bytes)                               │   │
│   ├─────────────────────────────────────────────────────┤   │
│   │ refcount (1 byte) │ flags (1 byte) │ padding (6)    │   │
│   └─────────────────────────────────────────────────────┘   │
│   总计: 24 bytes                                             │
│                                                             │
│   调试环境 (SANITYCHECKS 定义):                              │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ seencount (4 bytes)                                 │   │
│   ├─────────────────────────────────────────────────────┤   │
│   │ padding (4 bytes)                                   │   │
│   ├─────────────────────────────────────────────────────┤   │
│   │ phys (8 bytes)                                      │   │
│   ├─────────────────────────────────────────────────────┤   │
│   │ firstregion (8 bytes)                               │   │
│   ├─────────────────────────────────────────────────────┤   │
│   │ refcount (1 byte) │ flags (1 byte) │ padding (6)    │   │
│   └─────────────────────────────────────────────────────┘   │
│   总计: 32 bytes                                             │
│                                                             │
│   SLAB 分配: 使用 pb_slab (见 slab.c)                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---### 2.2 物理块生命周期

#### 2.2.1 pb_new - 创建物理块

**函数签名**

```c
struct phys_block *pb_new(phys_bytes phys);
```

**Minix3 源码实现**

```c
struct phys_block *pb_new(phys_bytes phys)
{
    struct phys_block *newpb;

    /* 从 SLAB 分配 phys_block 结构体 */
    if(!SLABALLOC(newpb)) {
        printf("vm: pb_new: couldn't allocate phys block\n");
        return NULL;
    }

    /* 验证物理地址页对齐 */
    if(phys != MAP_NONE)
        assert(!(phys % VM_PAGE_SIZE));
    
    /* 初始化字段 */
    newpb->phys = phys;           /* 物理地址 */
    newpb->refcount = 0;          /* 初始引用计数为 0 */
    newpb->firstregion = NULL;    /* 无引用 */
    newpb->flags = 0;             /* 无标志 */

    return newpb;
}
```

**参数说明**

```
┌─────────────────────────────────────────────────────────────┐
│                    pb_new 参数说明                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   phys: 物理内存地址                                         │
│   ────────────────────────────────────────────────────────  │
│   MAP_NONE (0):  延迟分配，不立即分配物理页                  │
│                  后续通过缺页处理分配                        │
│                                                             │
│   有效地址:      已分配的物理页地址                          │
│                  必须页对齐 (4KB)                            │
│                  例如: 0x1234000                            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**创建流程**

```
┌─────────────────────────────────────────────────────────────┐
│                    pb_new 创建流程                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. SLAB 分配                                              │
│      ┌─────────────────────────────────────────────────┐   │
│      │ SLABALLOC(newpb)                                │   │
│      │                                                 │   │
│      │ 从 pb_slab 分配 phys_block 结构体               │   │
│      │ 失败返回 NULL                                   │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   2. 验证物理地址                                           │
│      ┌─────────────────────────────────────────────────┐   │
│      │ if (phys != MAP_NONE)                           │   │
│      │     assert(phys % VM_PAGE_SIZE == 0)            │   │
│      │                                                 │   │
│      │ 确保物理地址是页对齐的                           │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   3. 初始化字段                                             │
│      ┌─────────────────────────────────────────────────┐   │
│      │ newpb->phys = phys                              │   │
│      │ newpb->refcount = 0    ◄─── 尚未被引用          │   │
│      │ newpb->firstregion = NULL                       │   │
│      │ newpb->flags = 0                                │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   4. 返回                                                   │
│      ┌─────────────────────────────────────────────────┐   │
│      │ return newpb                                    │   │
│      └─────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**关键点：refcount 初始化为 0**

```
为什么 refcount 初始化为 0 而不是 1？

┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   设计原因:                                                  │
│                                                             │
│   1. 分离创建和引用                                          │
│      - pb_new(): 仅创建结构体                                │
│      - pb_link(): 建立引用关系，refcount++                  │
│      - 允许创建后不立即使用                                  │
│                                                             │
│   2. 灵活性                                                  │
│      - 可以预分配 phys_block                                 │
│      - 延迟到需要时再链接                                    │
│      - 支持批量操作优化                                      │
│                                                             │
│   3. 一致性                                                  │
│      - refcount 始终等于链表长度                             │
│      - 初始状态: 无链表，refcount = 0                        │
│      - 易于验证正确性                                        │
│                                                             │
│   典型使用模式:                                              │
│   pb = pb_new(phys);        // refcount = 0                │
│   pb_link(pr, pb, ...);     // refcount = 1                │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**使用场景**

```c
/* 场景 1: 已分配物理页 */
phys_bytes phys = alloc_mem(1, flags);
struct phys_block *pb = pb_new(CLICK2ABS(phys));
/* pb->phys = 物理地址, refcount = 0 */

/* 场景 2: 延迟分配 */
struct phys_block *pb = pb_new(MAP_NONE);
/* pb->phys = 0, refcount = 0 */
/* 后续缺页时再分配物理页 */

/* 场景 3: CoW 复制 */
phys_bytes new_page = alloc_mem(1, flags);
sys_abscopy(old_page, new_page, VM_PAGE_SIZE);
struct phys_block *pb = pb_new(new_page);
/* 新块独立，refcount = 0，等待链接 */
```

**内存分配细节**

```
SLABALLOC 宏展开:

#define SLABALLOC(var) \
    ((var) = allocate(&pb_slab, sizeof(*(var))))

特点:
1. 从预分配的 SLAB 池中分配
2. 避免频繁调用 malloc/free
3. 提高分配效率
4. 减少内存碎片

pb_slab 定义 (sanitycheck.h):
SLAB_DECLARE(pb_slab);
```

#### 2.2.2 pb_free - 释放物理块

**函数签名**

```c
void pb_free(struct phys_block *pb);
```

**Minix3 源码实现**

```c
void pb_free(struct phys_block *pb)
{
    /* 释放物理内存页（如果有） */
    if(pb->phys != MAP_NONE)
        free_mem(ABS2CLICK(pb->phys), 1);
    
    /* 释放 phys_block 结构体到 SLAB */
    SLABFREE(pb);
}
```

**释放流程**

```
┌─────────────────────────────────────────────────────────────┐
│                    pb_free 释放流程                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   输入: pb (要释放的 phys_block 指针)                        │
│                                                             │
│   1. 检查物理地址                                            │
│      ┌─────────────────────────────────────────────────┐   │
│      │ if (pb->phys != MAP_NONE)                       │   │
│      │     free_mem(ABS2CLICK(pb->phys), 1);           │   │
│      │                                                 │   │
│      │ 如果有物理页，归还给内存分配器                     │   │
│      │ MAP_NONE 表示延迟分配，无物理页需要释放           │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   2. 释放结构体                                              │
│      ┌─────────────────────────────────────────────────┐   │
│      │ SLABFREE(pb);                                   │   │
│      │                                                 │   │
│      │ 将 phys_block 归还给 SLAB 分配器                 │   │
│      └─────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**重要前提条件**

```
┌─────────────────────────────────────────────────────────────┐
│                    pb_free 调用前提                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   ⚠️ pb_free 只能在 refcount == 0 时调用！                   │
│                                                             │
│   正确的释放路径:                                            │
│                                                             │
│   1. 通过 pb_unreferenced() 减少 refcount                   │
│   2. 当 refcount 变为 0 时                                   │
│   3. pb_unreferenced() 内部调用 SLABFREE(pb)                │
│                                                             │
│   pb_free() 的实际用途:                                      │
│   - 清理创建后未使用的 phys_block                            │
│   - 错误处理路径                                             │
│   - 特殊的内存类型释放                                       │
│                                                             │
│   正常流程中，pb_free 由 pb_unreferenced 间接调用:           │
│                                                             │
│   pb_unreferenced() {                                       │
│       pb->refcount--;                                       │
│       if (pb->refcount == 0) {                              │
│           pr->memtype->ev_unreference(pr);  // 释放物理页   │
│           SLABFREE(pb);                     // 释放结构体   │
│       }                                                     │
│   }                                                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**物理内存释放细节**

```c
free_mem(ABS2CLICK(pb->phys), 1);
```

```
ABS2CLICK 宏:
- 将字节地址转换为 click (4KB 块号)
- #define ABS2ABS(a) ((a) >> CLICK_SHIFT)
- 例如: 0x1234000 → 0x1234

free_mem 参数:
- 第一个参数: 起始 click 号
- 第二个参数: 释放的 click 数量 (1 = 4KB)
```

**使用场景**

```c
/* 场景 1: 错误处理 - 创建后未使用 */
struct phys_block *pb = pb_new(phys);
if (!pb) {
    return ENOMEM;
}

if (some_error_condition) {
    pb_free(pb);  /* 清理未使用的块 */
    return ERROR;
}

/* 正常使用 */
pb_link(pr, pb, offset, region);

/* 场景 2: 内存类型释放回调 */
/* mem_type_anon 的 ev_unreference 可能调用 pb_free */
static int anon_unreference(struct phys_region *pr) {
    /* 匿名内存直接释放 */
    pb_free(pr->ph);
    return OK;
}

/* 场景 3: 延迟分配的块 */
struct phys_block *pb = pb_new(MAP_NONE);
/* pb->phys = 0，无物理页 */
pb_free(pb);  /* 只释放结构体，无物理页释放 */
```

**与 pb_unreferenced 的关系**

```
┌─────────────────────────────────────────────────────────────┐
│                    释放路径对比                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   直接调用 pb_free():                                        │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 适用场景:                                            │   │
│   │ - 创建后未使用的块                                   │   │
│   │ - 错误处理路径                                       │   │
│   │ - refcount == 0 的块                                 │   │
│   │                                                     │   │
│   │ 前提: 确保无 phys_region 引用此块                    │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   通过 pb_unreferenced():                                    │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 适用场景:                                            │   │
│   │ - 正常的引用释放                                     │   │
│   │ - munmap, exit, CoW                                 │   │
│   │                                                     │   │
│   │ 流程:                                               │   │
│   │ 1. 从链表中移除 phys_region                         │   │
│   │ 2. refcount--                                       │   │
│   │ 3. 如果 refcount == 0，调用 memtype 回调            │   │
│   │ 4. SLABFREE(pb)                                     │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---### 2.3 引用计数管理

#### 2.3.1 pb_reference - 增加引用

**函数签名**

```c
struct phys_region *pb_reference(
    struct phys_block *newpb,
    vir_bytes offset,
    struct vir_region *region,
    mem_type_t *memtype
);
```

**Minix3 源码实现**

```c
struct phys_region *pb_reference(
    struct phys_block *newpb,
    vir_bytes offset,
    struct vir_region *region,
    mem_type_t *memtype
)
{
    struct phys_region *newphysr;

    /* 分配 phys_region 结构体 */
    if(!SLABALLOC(newphysr)) {
        printf("vm: pb_reference: couldn't allocate phys region\n");
        return NULL;
    }

    /* 设置内存类型 */
    newphysr->memtype = memtype;

    /* 建立引用关系 */
    pb_link(newphysr, newpb, offset, region);

    /* 更新 vir_region 的 physblocks 数组 */
    physblock_set(region, offset, newphysr);

    return newphysr;
}
```

**pb_link 辅助函数**

```c
void pb_link(
    struct phys_region *newphysr,
    struct phys_block *newpb,
    vir_bytes offset,
    struct vir_region *parent
)
{
    /* 设置 phys_region 字段 */
    newphysr->offset = offset;
    newphysr->ph = newpb;
    newphysr->parent = parent;

    /* 将 phys_region 加入 phys_block 的引用链表 */
    newphysr->next_ph_list = newpb->firstregion;
    newpb->firstregion = newphysr;

    /* 增加引用计数 */
    newpb->refcount++;
}
```

**参数说明**

```
┌─────────────────────────────────────────────────────────────┐
│                    pb_reference 参数说明                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   newpb:    要引用的 phys_block                              │
│   offset:   在 vir_region 中的偏移量（页对齐）               │
│   region:   所属的 vir_region                                │
│   memtype:  内存类型（anon, file, cache 等）                 │
│                                                             │
│   返回值:   新创建的 phys_region，或 NULL（失败）            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**引用建立流程**

```
┌─────────────────────────────────────────────────────────────┐
│                    pb_reference 流程                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 分配 phys_region                                        │
│      ┌─────────────────────────────────────────────────┐   │
│      │ SLABALLOC(newphysr)                             │   │
│      │                                                 │   │
│      │ 从 physr_slab 分配 phys_region 结构体           │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   2. 设置内存类型                                           │
│      ┌─────────────────────────────────────────────────┐   │
│      │ newphysr->memtype = memtype                     │   │
│      │                                                 │   │
│      │ 决定此物理区域的内存管理策略                      │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   3. 调用 pb_link 建立引用                                   │
│      ┌─────────────────────────────────────────────────┐   │
│      │ pb_link(newphysr, newpb, offset, region)        │   │
│      │                                                 │   │
│      │ - 设置 phys_region 字段                         │   │
│      │ - 加入 phys_block 的引用链表                    │   │
│      │ - refcount++                                    │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   4. 更新 vir_region 的 physblocks 数组                     │
│      ┌─────────────────────────────────────────────────┐   │
│      │ physblock_set(region, offset, newphysr)         │   │
│      │                                                 │   │
│      │ 将 phys_region 记录在 vir_region 中             │   │
│      └─────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**pb_link 链表操作详解**

```
链表插入操作（头插法）:

插入前:
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   phys_block                                                │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ firstregion ──► pr1 ──► pr2 ──► NULL               │   │
│   │ refcount = 2                                        │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   新 phys_region: newphysr                                  │
│                                                             │
└─────────────────────────────────────────────────────────────┘

插入后:
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   phys_block                                                │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ firstregion ──► newphysr ──► pr1 ──► pr2 ──► NULL  │   │
│   │ refcount = 3  ◄─── refcount++                       │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   代码:                                                      │
│   newphysr->next_ph_list = newpb->firstregion;  // 指向旧头 │
│   newpb->firstregion = newphysr;                // 成为新头 │
│   newpb->refcount++;                            // 计数+1   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**使用场景**

```c
/* 场景 1: 创建新的物理区域 */
struct phys_block *pb = pb_new(phys_addr);
struct phys_region *pr = pb_reference(pb, 0x1000, region, &mem_type_anon);
/* pb->refcount = 1 */

/* 场景 2: fork 共享物理页 */
/* 父进程已有 phys_block */
struct phys_region *child_pr = pb_reference(
    parent_pb,           /* 共享父进程的物理块 */
    offset,
    child_region,
    &mem_type_anon
);
/* parent_pb->refcount++ */

/* 场景 3: 文件映射 */
struct phys_region *pr = pb_reference(
    cache_pb,            /* 缓存中的物理块 */
    offset,
    region,
    &mem_type_cache
);
```

**与 pb_link 的关系**

```
┌─────────────────────────────────────────────────────────────┐
│                    pb_reference vs pb_link                   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   pb_reference():                                           │
│   - 高层接口                                                 │
│   - 分配 phys_region                                        │
│   - 调用 pb_link                                            │
│   - 更新 vir_region                                         │
│   - 用于创建新引用                                          │
│                                                             │
│   pb_link():                                                │
│   - 低层接口                                                 │
│   - 不分配 phys_region（需要已分配）                         │
│   - 只建立链接关系                                          │
│   - 用于内部操作（如 CoW 复制后重新链接）                    │
│                                                             │
│   典型使用:                                                  │
│   - 外部代码: 调用 pb_reference()                           │
│   - 内部代码: 直接调用 pb_link()                            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---######## 2.3.2 pb_unreferenced - 减少引用

**函数签名**

```c
void pb_unreferenced(
    struct vir_region *region,
    struct phys_region *pr,
    int rm
);
```

**Minix3 源码实现**

```c
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb;

    pb = pr->ph;
    assert(pb->refcount > 0);
    
    /* 减少引用计数 */
    pb->refcount--;

    /* 从链表中移除 phys_region */
    if(pb->firstregion == pr) {
        /* pr 是链表头 */
        pb->firstregion = pr->next_ph_list;
    } else {
        /* pr 在链表中间，需要遍历查找 */
        struct phys_region *others;

        for(others = pb->firstregion; others;
            others = others->next_ph_list) {
            assert(others->ph == pb);
            if(others->next_ph_list == pr) {
                others->next_ph_list = pr->next_ph_list;
                break;
            }
        }

        assert(others); /* 否则说明 pr 不在链表中 */
    }

    /* 如果引用计数为 0，释放 phys_block */
    if(pb->refcount == 0) {
        assert(!pb->firstregion);
        int r;
        if((r = pr->memtype->ev_unreference(pr)) != OK)
            panic("unref failed, %d", r);

        SLABFREE(pb);
    }

    pr->ph = NULL;

    /* 如果 rm 为真，从 vir_region 中移除 */
    if(rm) physblock_set(region, pr->offset, NULL);
}
```

**参数说明**

```
┌─────────────────────────────────────────────────────────────┐
│                    pb_unreferenced 参数说明                  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   region:   phys_region 所属的 vir_region                   │
│   pr:       要取消引用的 phys_region                         │
│   rm:       是否从 vir_region 中移除                         │
│             0 = 不移除（用于 CoW 重新链接）                  │
│             1 = 移除（用于 munmap, exit）                    │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**引用释放流程**

```
┌─────────────────────────────────────────────────────────────┐
│                    pb_unreferenced 流程                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 减少引用计数                                            │
│      ┌─────────────────────────────────────────────────┐   │
│      │ pb = pr->ph;                                     │   │
│      │ assert(pb->refcount > 0);                        │   │
│      │ pb->refcount--;                                  │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   2. 从链表中移除 phys_region                               │
│      ┌─────────────────────────────────────────────────┐   │
│      │ if (pb->firstregion == pr)                       │   │
│      │     pb->firstregion = pr->next_ph_list;  // 头部 │   │
│      │ else                                             │   │
│      │     遍历链表找到 pr 并移除                // 中间 │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   3. 检查是否需要释放                                        │
│      ┌─────────────────────────────────────────────────┐   │
│      │ if (pb->refcount == 0) {                         │   │
│      │     pr->memtype->ev_unreference(pr);             │   │
│      │     SLABFREE(pb);                                │   │
│      │ }                                                │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   4. 清理 phys_region                                        │
│      ┌─────────────────────────────────────────────────┐   │
│      │ pr->ph = NULL;                                   │   │
│      │ if (rm) physblock_set(region, offset, NULL);     │   │
│      └─────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**链表移除操作详解**

```
情况 1: pr 是链表头

移除前:
┌─────────────────────────────────────────────────────────────┐
│   pb->firstregion ──► pr ──► pr2 ──► pr3 ──► NULL          │
└─────────────────────────────────────────────────────────────┘

移除后:
┌─────────────────────────────────────────────────────────────┐
│   pb->firstregion ──► pr2 ──► pr3 ──► NULL                 │
│   pr->next_ph_list = ? (不再使用)                           │
└─────────────────────────────────────────────────────────────┘

代码:
pb->firstregion = pr->next_ph_list;


情况 2: pr 在链表中间

移除前:
┌─────────────────────────────────────────────────────────────┐
│   pb->firstregion ──► pr1 ──► pr ──► pr3 ──► NULL          │
└─────────────────────────────────────────────────────────────┘

移除后:
┌─────────────────────────────────────────────────────────────┐
│   pb->firstregion ──► pr1 ──► pr3 ──► NULL                 │
│   pr->next_ph_list = ? (不再使用)                           │
└─────────────────────────────────────────────────────────────┘

代码:
for (others = pb->firstregion; others; others = others->next_ph_list) {
    if (others->next_ph_list == pr) {
        others->next_ph_list = pr->next_ph_list;
        break;
    }
}
```

**memtype->ev_unreference 回调**

> 各内存类型的 `ev_unreference` 实现详见 [11-memtype.md](11-memtype.md) §2。
> 此处仅列出接口签名和语义概要：

```c
/* ev_unreference 回调接口 */
int (*ev_unreference)(struct phys_region *pr);

/* 匿名内存：释放物理页 */
static int anon_unreference(struct phys_region *pr);

/* 文件映射：可能写回磁盘，由缓存管理 */
static int file_unreference(struct phys_region *pr);

/* 共享内存：仅取消映射，不释放物理页 */
static int shared_unreference(struct phys_region *pr);
```

**rm 参数的作用**

```
┌─────────────────────────────────────────────────────────────┐
│                    rm 参数使用场景                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   rm = 1 (移除):                                            │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ - munmap: 完全移除映射                               │   │
│   │ - exit: 进程退出，清理所有映射                       │   │
│   │ - physblock_set(region, offset, NULL) 被调用        │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   rm = 0 (不移除):                                          │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ - CoW: 取消旧引用，但保留 phys_region 结构           │   │
│   │ - 后续会 pb_link 到新的 phys_block                   │   │
│   │ - physblock_set 不被调用                            │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   CoW 示例:                                                  │
│   mem_cow() {                                               │
│       pb_unreferenced(region, ph, 0);  // rm=0, 不移除     │
│       pb_link(ph, new_pb, ...);       // 链接到新块        │
│   }                                                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**使用场景**

```c
/* 场景 1: munmap */
void region_free(struct vir_region *region) {
    for (each phys_region pr in region) {
        pb_unreferenced(region, pr, 1);  /* rm=1, 移除 */
        SLABFREE(pr);
    }
}

/* 场景 2: 进程退出 */
void vm_proc_cleanup(struct vmproc *vmp) {
    for (each region in vmp) {
        for (each phys_region pr in region) {
            pb_unreferenced(region, pr, 1);  /* rm=1 */
        }
    }
}

/* 场景 3: CoW 写时复制 */
int mem_cow(struct vir_region *region, struct phys_region *ph, ...) {
    /* 分配新物理页 */
    new_pb = pb_new(new_page);
    
    /* 取消旧引用，但不移除 phys_region */
    pb_unreferenced(region, ph, 0);  /* rm=0 */
    
    /* 链接到新块 */
    pb_link(ph, new_pb, ph->offset, region);
    
    return OK;
}
```

**引用计数状态变化**

```
┌─────────────────────────────────────────────────────────────┐
│                    refcount 状态变化示例                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   初始状态:                                                  │
│   pb->refcount = 3                                          │
│   pb->firstregion ──► pr1 ──► pr2 ──► pr3 ──► NULL         │
│                                                             │
│   调用 pb_unreferenced(region, pr2, 1):                     │
│   pb->refcount = 2                                          │
│   pb->firstregion ──► pr1 ──► pr3 ──► NULL                  │
│                                                             │
│   调用 pb_unreferenced(region, pr1, 1):                     │
│   pb->refcount = 1                                          │
│   pb->firstregion ──► pr3 ──► NULL                          │
│                                                             │
│   调用 pb_unreferenced(region, pr3, 1):                     │
│   pb->refcount = 0                                          │
│   pb->firstregion = NULL                                    │
│   → 调用 memtype->ev_unreference(pr3)                       │
│   → SLABFREE(pb)                                            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---### 2.4 物理块链表

**链表结构概述**

`phys_block` 通过 `firstregion` 指针维护一个 `phys_region` 链表，记录所有引用此物理块的虚拟区域。

```
┌─────────────────────────────────────────────────────────────┐
│                    phys_block 链表结构                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   phys_block                                                │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ phys: 0x1234000                                     │   │
│   │ refcount: 3                                         │   │
│   │ firstregion ────────────────────────────────┐       │   │
│   └───────────────────────────────────────────────┼───────┘   │
│                                                     │         │
│                     phys_region 链表               ▼         │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ phys_region A                                       │   │
│   │ ┌─────────────────────────────────────────────────┐ │   │
│   │ │ ph: ───────────────────────────────────┐        │ │   │
│   │ │ offset: 0x0000                          │        │ │   │
│   │ │ parent: vir_region (进程A)              │        │ │   │
│   │ │ memtype: mem_type_anon                  │        │ │   │
│   │ │ next_ph_list ───────────────────────────┼───┐    │ │   │
│   │ └─────────────────────────────────────────┘   │    │ │   │
│   └───────────────────────────────────────────────┼────┘   │
│                                                     │        │
│   ┌───────────────────────────────────────────────┼────┐   │
│   │ phys_region B                                  │    │   │
│   │ ┌─────────────────────────────────────────────┼──┐ │   │
│   │ │ ph: ───────────────────────────────────┐    │  │ │   │
│   │ │ offset: 0x1000                          │    │  │ │   │
│   │ │ parent: vir_region (进程B)              │    │  │ │   │
│   │ │ memtype: mem_type_anon                  │    │  │ │   │
│   │ │ next_ph_list ───────────────────────────┼────┼┐ │   │
│   │ └─────────────────────────────────────────┘    ││  │ │   │
│   └─────────────────────────────────────────────────┘│  │   │
│                                                       │  │   │
│   ┌───────────────────────────────────────────────────┼┐ │   │
│   │ phys_region C                                     ││ │   │
│   │ ┌─────────────────────────────────────────────────┼┐│ │   │
│   │ │ ph: ───────────────────────────────────┐        │││ │   │
│   │ │ offset: 0x2000                          │        │││ │   │
│   │ │ parent: vir_region (进程C)              │        │││ │   │
│   │ │ memtype: mem_type_anon                  │        │││ │   │
│   │ │ next_ph_list: NULL                      │        │││ │   │
│   │ └─────────────────────────────────────────┘        │││ │   │
│   └─────────────────────────────────────────────────────┘││ │   │
│                                                           ││ │   │
│   所有 ph 指针都指向同一个 phys_block ◄────────────────────┘│ │   │
│                                                             │ │   │
└─────────────────────────────────────────────────────────────┘ │   │
```

**链表的作用**

```
┌─────────────────────────────────────────────────────────────┐
│                    phys_region 链表的作用                     │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 引用计数验证                                            │
│      - refcount 应该等于链表长度                             │
│      - 可用于调试和一致性检查                                │
│                                                             │
│   2. 遍历所有引用者                                          │
│      - 找出哪些进程/区域引用此物理页                          │
│      - 用于调试和诊断                                        │
│                                                             │
│   3. 批量操作                                                │
│      - 页面换出时通知所有引用者                               │
│      - 更新所有相关的页表映射                                 │
│                                                             │
│   4. CoW 判断                                                │
│      - 检查 refcount > 1 判断是否需要 CoW                    │
│      - 快速判断是否为共享页                                  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**链表操作时间复杂度**

```
┌─────────────────────────────────────────────────────────────┐
│                    链表操作复杂度                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   操作              时间复杂度    说明                       │
│   ────────────────────────────────────────────────────────  │
│   头部插入          O(1)         pb_link()                  │
│   头部删除          O(1)         pb_unreferenced()          │
│   中间删除          O(n)         pb_unreferenced()          │
│   遍历链表          O(n)         调试/诊断                   │
│   查找特定 pr       O(n)         删除时需要                  │
│                                                             │
│   n = refcount，通常很小 (1-5)，所以 O(n) 可接受            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**链表遍历示例**

```c
/* 遍历 phys_block 的所有引用者 */
void print_pb_references(struct phys_block *pb)
{
    struct phys_region *pr;
    int count = 0;

    printf("phys_block %p (phys=0x%lx, refcount=%d):\n",
           pb, pb->phys, pb->refcount);

    for (pr = pb->firstregion; pr; pr = pr->next_ph_list) {
        printf("  [%d] phys_region %p, offset=0x%lx, parent=%p\n",
               count++, pr, pr->offset, pr->parent);
    }

    /* 验证 refcount 与链表长度一致 */
    assert(count == pb->refcount);
}

/* 检查指定 vir_region 是否引用此 phys_block */
int is_region_referencing_pb(struct phys_block *pb, 
                              struct vir_region *vr)
{
    struct phys_region *pr;

    for (pr = pb->firstregion; pr; pr = pr->next_ph_list) {
        if (pr->parent == vr)
            return 1;
    }
    return 0;
}
```

**链表与 CoW 的关系**

```
┌─────────────────────────────────────────────────────────────┐
│                    链表在 CoW 中的作用                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   fork 后的共享状态:                                         │
│                                                             │
│   phys_block                                                │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ phys: 0x1234000                                     │   │
│   │ refcount: 2  ◄─── 共享页                             │   │
│   │ firstregion ──► parent_pr ──► child_pr ──► NULL    │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   写操作触发 CoW:                                            │
│   1. 检查 pb->refcount > 1 → 需要复制                       │
│   2. 分配新物理页                                            │
│   3. 复制内容                                                │
│   4. 调用 pb_unreferenced() 减少原块引用                    │
│   5. 调用 pb_link() 链接到新块                              │
│                                                             │
│   写入后:                                                    │
│   原块: refcount = 1 (另一个进程)                            │
│   新块: refcount = 1 (当前进程)                              │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**链表一致性验证**

```c
#if SANITYCHECKS
/* 验证 phys_block 链表一致性 */
void verify_pb_chain(struct phys_block *pb)
{
    struct phys_region *pr;
    int count = 0;

    /* 遍历链表 */
    for (pr = pb->firstregion; pr; pr = pr->next_ph_list) {
        /* 验证每个 pr 都指向此 pb */
        assert(pr->ph == pb);
        count++;
    }

    /* 验证 refcount 与链表长度一致 */
    assert(count == pb->refcount);
}

/* 全局验证 */
void verify_all_pb_chains(void)
{
    /* 遍历所有 phys_block，验证每个链表 */
    for (each phys_block pb in system) {
        verify_pb_chain(pb);
    }
}
#endif
```

**链表在页面换出中的应用**

```c
/* 换出物理页时，需要通知所有引用者 */
int page_out(struct phys_block *pb)
{
    struct phys_region *pr;

    /* 检查是否可以换出 */
    if (pb->refcount > 1) {
        /* 共享页，可能不应该换出 */
        return EBUSY;
    }

    /* 遍历所有引用者，更新页表 */
    for (pr = pb->firstregion; pr; pr = pr->next_ph_list) {
        /* 从页表中移除映射 */
        pt_clearmap(pr->parent, pr->offset);
    }

    /* 写入交换区 */
    write_to_swap(pb->phys, ...);

    /* 释放物理页 */
    free_mem(ABS2CLICK(pb->phys), 1);
    pb->phys = MAP_NONE;

    return OK;
}
```

**链表设计总结**

```
┌─────────────────────────────────────────────────────────────┐
│                    phys_region 链表设计总结                   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   优点:                                                      │
│   ✓ 简单高效的头插法                                         │
│   ✓ O(1) 的引用增加操作                                      │
│   ✓ 直接访问所有引用者                                       │
│   ✓ 易于调试和验证                                           │
│                                                             │
│   缺点:                                                      │
│   ✗ 中间删除需要 O(n) 遍历                                   │
│   ✗ 链表指针增加内存开销                                     │
│                                                             │
│   适用场景:                                                  │
│   • refcount 通常很小 (1-5)                                 │
│   • 删除操作相对较少                                         │
│   • 需要遍历引用者的场景                                     │
│                                                             │
│   替代方案:                                                  │
│   • 使用双向链表: 删除 O(1)，但增加内存开销                  │
│   • 使用引用计数数组: 不需要链表，但失去遍历能力             │
│   • Minix3 选择单向链表是合理的权衡                          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Rust 设计决策

### 3.1 PhysBlock 结构

**设计目标**

将 Minix3 的 `phys_block` 封装为安全的 Rust 结构体，需要解决以下问题：

```
┌─────────────────────────────────────────────────────────────┐
│                    Rust 设计挑战                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 引用计数安全                                            │
│      - C 代码手动管理 refcount，容易出错                     │
│      - Rust 需要保证引用计数的正确性                          │
│                                                             │
│   2. 链表安全                                                │
│      - firstregion 链表涉及裸指针                            │
│      - 需要在 unsafe 块中操作，但提供安全接口                 │
│                                                             │
│   3. 生命周期管理                                            │
│      - phys_block 生命周期由引用计数决定                     │
│      - 不能简单使用 Rust 的所有权模型                        │
│                                                             │
│   4. 与 phys_region 的双向引用                               │
│      - phys_block → phys_region (firstregion 链表)          │
│      - phys_region → phys_block (ph 指针)                   │
│      - 需要使用弱引用或其他机制避免循环                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**结构体定义**

```rust
use minix_types::{PhysBytes, VirBytes};
use core::cell::Cell;
use core::ptr::NonNull;

/// 物理块标志位
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PbFlags(u8);

impl PbFlags {
    /// 此块在页面缓存中
    pub const IN_CACHE: Self = Self(0x01);
    
    pub fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
    
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
    
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

/// 物理块结构体
///
/// 对应 Minix3: `struct phys_block`
///
/// # Safety
///
/// - `firstregion` 链表操作需要通过安全接口进行
/// - `refcount` 由内部方法维护，不应直接修改
pub struct PhysBlock {
    /// 物理内存地址（页对齐）
    pub phys: PhysBytes,
    
    /// 引用此块的 phys_region 链表头
    firstregion: Option<NonNull<PhysRegion>>,
    
    /// 引用计数
    refcount: Cell<u8>,
    
    /// 标志位
    pub flags: PbFlags,
}

impl PhysBlock {
    /// 创建新的物理块
    ///
    /// 对应 Minix3: `pb_new()`
    ///
    /// # 参数
    ///
    /// - `phys`: 物理内存地址，`PhysBytes(0)` 表示延迟分配
    ///
    /// # 返回
    ///
    /// 返回新的 PhysBlock，初始 refcount = 0
    pub fn new(phys: PhysBytes) -> Self {
        Self {
            phys,
            firstregion: None,
            refcount: Cell::new(0),
            flags: PbFlags::default(),
        }
    }
    
    /// 获取引用计数
    pub fn refcount(&self) -> u8 {
        self.refcount.get()
    }
    
    /// 检查是否为共享页（refcount > 1）
    pub fn is_shared(&self) -> bool {
        self.refcount.get() > 1
    }
    
    /// 检查是否被引用（refcount > 0）
    pub fn is_referenced(&self) -> bool {
        self.refcount.get() > 0
    }
    
    /// 检查是否有物理内存
    pub fn has_phys(&self) -> bool {
        self.phys.0 != 0
    }
    
    /// 获取链表头（仅用于遍历）
    pub fn first_region(&self) -> Option<&PhysRegion> {
        self.firstregion.map(|ptr| unsafe { ptr.as_ref() })
    }
}
```

**封装决策说明**

```
┌─────────────────────────────────────────────────────────────┐
│                    封装决策                                   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. refcount 使用 Cell<u8>                                  │
│      - 允许通过 &self 修改                                    │
│      - 避免使用 &mut self 导致借用冲突                       │
│      - 单线程环境足够，VM 是单线程的                          │
│                                                             │
│   2. firstregion 使用 Option<NonNull<PhysRegion>>            │
│      - NonNull 表示非空指针，优化 Option 大小                │
│      - 访问需要 unsafe，但提供安全封装                        │
│      - 链表操作封装在方法中                                   │
│                                                             │
│   3. phys 和 flags 公开                                      │
│      - 这些字段需要外部访问                                   │
│      - 不涉及内存安全                                        │
│                                                             │
│   4. 不实现 Drop                                             │
│      - PhysBlock 由 Slab 分配，不自动释放                    │
│      - 释放通过显式调用 pb_free() 完成                       │
│      - 避免 Drop 与引用计数的冲突                            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**与 Minix3 的对应**

| Minix3 字段 | Rust 字段 | 说明 |
|------------|----------|------|
| `phys_bytes phys` | `phys: PhysBytes` | 物理地址，使用 newtype |
| `struct phys_region *firstregion` | `firstregion: Option<NonNull<PhysRegion>>` | 链表头，使用 Option |
| `u8_t refcount` | `refcount: Cell<u8>` | 内部可变性 |
| `u8_t flags` | `flags: PbFlags` | 标志位，使用 newtype |
| `u32_t seencount` | 不实现 | 仅调试用 |

**安全性考虑**

```rust
impl PhysBlock {
    /// 增加引用计数（内部方法）
    ///
    /// # Safety
    ///
    /// 调用者必须确保：
    /// - phys_region 已正确初始化
    /// - 不会导致 refcount 溢出
    unsafe fn inc_refcount(&self) {
        let count = self.refcount.get();
        debug_assert!(count < u8::MAX, "refcount overflow");
        self.refcount.set(count + 1);
    }
    
    /// 减少引用计数（内部方法）
    ///
    /// # Safety
    ///
    /// 调用者必须确保：
    /// - refcount > 0
    /// - 如果 refcount 变为 0，需要处理释放逻辑
    unsafe fn dec_refcount(&self) -> u8 {
        let count = self.refcount.get();
        debug_assert!(count > 0, "refcount underflow");
        self.refcount.set(count - 1);
        count - 1
    }
}
```

---###### 3.2 引用计数模式

**手动管理 vs Arc**

在 Rust 中实现引用计数有两种主要方式：

```
┌─────────────────────────────────────────────────────────────┐
│                    引用计数方案对比                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   方案 1: Arc<T> (标准库)                                    │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 优点:                                               │   │
│   │ - 自动管理，无需手动增减                             │   │
│   │ - 线程安全（Arc）或单线程（Rc）                       │   │
│   │ - 与 Rust 所有权模型一致                             │   │
│   │                                                     │   │
│   │ 缺点:                                               │   │
│   │ - 不支持弱引用链表遍历                               │   │
│   │ - 无法实现 Minix3 的 firstregion 链表               │   │
│   │ - Drop 会自动释放，与 VM 的生命周期管理冲突          │   │
│   │ - 每个 Arc 有额外的内存开销（strong/weak count）     │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   方案 2: 手动管理 (Cell<u8>)                                │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 优点:                                               │   │
│   │ - 完全控制生命周期                                   │   │
│   │ - 支持 firstregion 链表                             │   │
│   │ - 与 Minix3 设计一致                                │   │
│   │ - 无额外内存开销                                     │   │
│   │                                                     │   │
│   │ 缺点:                                               │   │
│   │ - 需要手动调用 link/unlink                          │   │
│   │ - 容易出错（但可通过封装减少）                       │   │
│   │ - 需要 unsafe 代码                                   │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   结论: 选择方案 2（手动管理）                                │
│   - Minix3 的设计需要链表遍历能力                            │
│   - VM 是单线程环境，不需要 Arc 的线程安全                   │
│   - 需要与 Slab 分配器集成                                   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**为什么不用 Arc**

```rust
// Arc 方案的问题示例

use std::sync::Arc;

// 问题 1: 无法实现链表遍历
struct PhysBlockArc {
    phys: PhysBytes,
    // Arc 无法提供遍历所有引用者的能力
    // firstregion: ??? 
}

// 问题 2: Drop 自动释放
fn example_arc() {
    let pb = Arc::new(PhysBlockArc { phys: PhysBytes(0x1000) });
    let pb2 = Arc::clone(&pb);
    
    // 当 pb 和 pb2 都 drop 时，PhysBlockArc 自动释放
    // 但 VM 需要控制释放时机（如调用 memtype 回调）
    // Arc::drop 无法调用自定义的释放逻辑
}

// 问题 3: 无法与 Slab 集成
// Arc 使用堆分配，而 PhysBlock 应该从 Slab 分配
```

**手动管理的实现**

```rust
use core::cell::Cell;
use core::ptr::NonNull;

/// PhysBlock 的引用计数管理
impl PhysBlock {
    /// 将 phys_region 链接到此物理块
    ///
    /// 对应 Minix3: `pb_link()`
    ///
    /// # Safety
    ///
    /// - `pr` 必须是有效且未链接到其他 phys_block 的
    /// - 调用后 `pr.ph` 将指向此块
    pub unsafe fn link(
        &mut self,
        pr: &mut PhysRegion,
        offset: VirBytes,
        parent: *mut VirRegion,
    ) {
        pr.offset = offset;
        pr.ph = NonNull::new(self as *mut _);
        pr.parent = parent;
        
        // 头插法加入链表
        pr.next_ph_list = self.firstregion;
        self.firstregion = NonNull::new(pr as *mut _);
        
        // 增加引用计数
        let count = self.refcount.get();
        self.refcount.set(count + 1);
    }
    
    /// 从此物理块取消链接 phys_region
    ///
    /// 对应 Minix3: `pb_unreferenced()` 的链表移除部分
    ///
    /// # Safety
    ///
    /// - `pr` 必须已链接到此块
    /// - 返回新的引用计数
    pub unsafe fn unlink(&mut self, pr: &PhysRegion) -> u8 {
        // 从链表中移除
        if let Some(head) = self.firstregion {
            if head.as_ptr() == pr as *const _ as *mut _ {
                // pr 是链表头
                self.firstregion = pr.next_ph_list;
            } else {
                // 遍历链表查找 pr
                let mut current = head;
                loop {
                    let current_ref = current.as_ref();
                    if let Some(next) = current_ref.next_ph_list {
                        if next.as_ptr() == pr as *const _ as *mut _ {
                            current.as_mut().next_ph_list = pr.next_ph_list;
                            break;
                        }
                        current = next;
                    } else {
                        panic!("phys_region not found in chain");
                    }
                }
            }
        }
        
        // 减少引用计数
        let count = self.refcount.get();
        self.refcount.set(count - 1);
        count - 1
    }
}
```

**引用计数不变量**

```
┌─────────────────────────────────────────────────────────────┐
│                    引用计数不变量                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. refcount >= 0                                          │
│      - 不会出现负数                                          │
│      - dec_refcount 前检查 count > 0                        │
│                                                             │
│   2. refcount == 链表长度                                    │
│      - 每次 link 增加 refcount                              │
│      - 每次 unlink 减少 refcount                            │
│      - 可通过遍历链表验证                                    │
│                                                             │
│   3. refcount == 0 时可以释放                                │
│      - 此时 firstregion 必须为 None                         │
│      - 物理页可以归还                                        │
│      - PhysBlock 可以归还给 Slab                            │
│                                                             │
│   4. refcount > 1 时需要 CoW                                 │
│      - 写操作前检查 is_shared()                             │
│      - 如果共享，先复制再写入                                │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**验证函数**

```rust
impl PhysBlock {
    /// 验证引用计数一致性（调试用）
    ///
    /// 遍历链表，确认 refcount 与链表长度一致
    #[cfg(debug_assertions)]
    pub fn verify_refcount(&self) -> bool {
        let mut count = 0u8;
        let mut current = self.firstregion;
        
        while let Some(ptr) = current {
            let pr = unsafe { ptr.as_ref() };
            // 验证 pr.ph 指向此块
            if pr.ph.map(|p| p.as_ptr() as *const _ != self as *const _).unwrap_or(true) {
                return false;
            }
            count = count.saturating_add(1);
            current = pr.next_ph_list;
        }
        
        count == self.refcount.get()
    }
}
```

**与 Minix3 的对比**

| 方面 | Minix3 (C) | Rust |
|------|-----------|------|
| 引用计数类型 | `u8_t` | `Cell<u8>` |
| 增加 | `pb->refcount++` | `self.refcount.set(count + 1)` |
| 减少 | `pb->refcount--` | `self.refcount.set(count - 1)` |
| 溢出检查 | 无 | debug_assert! |
| 下溢检查 | assert(refcount > 0) | debug_assert!(count > 0) |
| 一致性验证 | SANITYCHECKS 宏 | debug_assertions + verify_refcount() |

---### 3.3 与 Slab 的关系

**Slab 分配器的作用**

Minix3 使用 Slab 分配器管理 `phys_block` 和 `phys_region` 结构体，而非通用的 malloc/free。

```
┌─────────────────────────────────────────────────────────────┐
│                    Slab 分配器优势                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 固定大小分配                                            │
│      - phys_block 大小固定（约 24-32 字节）                  │
│      - 无需每次计算大小                                      │
│      - 分配/释放 O(1)                                        │
│                                                             │
│   2. 减少内存碎片                                            │
│      - 相同大小的对象在同一 Slab 中                          │
│      - 不会产生外部碎片                                      │
│                                                             │
│   3. 缓存友好                                                │
│      - 连续内存分配                                          │
│      - 提高缓存命中率                                        │
│                                                             │
│   4. 批量管理                                                │
│      - 可以预分配一批对象                                    │
│      - 统计使用情况                                          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Minix3 的 Slab 定义**

```c
/* sanitycheck.h */
SLAB_DECLARE(pb_slab);      /* phys_block 的 Slab */
SLAB_DECLARE(physr_slab);   /* phys_region 的 Slab */

/* slab.c */
SLAB_DEFINE(pb_slab, sizeof(struct phys_block));
SLAB_DEFINE(physr_slab, sizeof(struct phys_region));
```

**Rust Slab 实现**

```rust
use crate::slab::Slab;

/// phys_block 的 Slab 分配器
///
/// 对应 Minix3: `pb_slab`
pub struct PhysBlockSlab {
    inner: Slab<PhysBlock>,
}

impl PhysBlockSlab {
    /// 创建新的 Slab
    pub fn new() -> Self {
        Self {
            inner: Slab::new(),
        }
    }
    
    /// 分配一个 PhysBlock
    ///
    /// 对应 Minix3: `SLABALLOC(pb)`
    pub fn alloc(&mut self, phys: PhysBytes) -> Option<&mut PhysBlock> {
        let idx = self.inner.alloc(PhysBlock::new(phys))?;
        Some(&mut self.inner[idx])
    }
    
    /// 释放一个 PhysBlock
    ///
    /// 对应 Minix3: `SLABFREE(pb)`
    ///
    /// # Safety
    ///
    /// - pb 必须是从此 Slab 分配的
    /// - pb 的 refcount 必须为 0
    pub unsafe fn dealloc(&mut self, pb: &PhysBlock) {
        debug_assert_eq!(pb.refcount(), 0, "cannot free PhysBlock with refcount > 0");
        // 找到索引并释放
        let idx = self.inner.find(pb as *const _);
        if let Some(i) = idx {
            self.inner.dealloc(i);
        }
    }
    
    /// 获取 Slab 统计信息
    pub fn stats(&self) -> SlabStats {
        self.inner.stats()
    }
}
```

**全局 Slab 管理**

```rust
use spin::Mutex;

/// 全局 phys_block Slab
pub static PB_SLAB: Mutex<PhysBlockSlab> = Mutex::new(PhysBlockSlab::new());

/// 全局 phys_region Slab
pub static PHYSR_SLAB: Mutex<PhysRegionSlab> = Mutex::new(PhysRegionSlab::new());

/// 分配新的 PhysBlock
///
/// 对应 Minix3: `pb_new()`
pub fn pb_alloc(phys: PhysBytes) -> Option<*mut PhysBlock> {
    let mut slab = PB_SLAB.lock();
    slab.alloc(phys).map(|pb| pb as *mut _)
}

/// 释放 PhysBlock
///
/// 对应 Minix3: `pb_free()`
///
/// # Safety
///
/// - pb 必须是从 pb_alloc() 获得的
/// - pb 的 refcount 必须为 0
pub unsafe fn pb_free(pb: *mut PhysBlock) {
    if pb.is_null() {
        return;
    }
    
    let pb_ref = &*pb;
    
    // 释放物理内存页
    if pb_ref.has_phys() {
        free_phys_page(pb_ref.phys);
    }
    
    // 归还给 Slab
    let mut slab = PB_SLAB.lock();
    slab.dealloc(pb_ref);
}
```

**Slab 与生命周期的关系**

```
┌─────────────────────────────────────────────────────────────┐
│                    PhysBlock 生命周期                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 分配                                                    │
│      ┌─────────────────────────────────────────────────┐   │
│      │ pb_alloc(phys) → PhysBlock                      │   │
│      │                                                 │   │
│      │ - 从 Slab 获取内存                               │   │
│      │ - 初始化字段                                     │   │
│      │ - refcount = 0                                   │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   2. 使用                                                    │
│      ┌─────────────────────────────────────────────────┐   │
│      │ pb_link() → refcount++                          │   │
│      │ pb_unlink() → refcount--                        │   │
│      │                                                 │   │
│      │ 可能多次增减                                      │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   3. 释放                                                    │
│      ┌─────────────────────────────────────────────────┐   │
│      │ refcount == 0 时                                 │   │
│      │                                                 │   │
│      │ - 调用 memtype->ev_unreference()                 │   │
│      │ - 释放物理页（如果有）                            │   │
│      │ - 归还给 Slab                                    │   │
│      └─────────────────────────────────────────────────┘   │
│                                                             │
│   注意: PhysBlock 不实现 Drop trait                         │
│   - 释放必须显式调用 pb_free()                              │
│   - 避免 Drop 与引用计数的冲突                              │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Slab 统计与调试**

```rust
impl PhysBlockSlab {
    /// 验证所有 PhysBlock 的一致性
    #[cfg(debug_assertions)]
    pub fn verify_all(&self) -> bool {
        for (_, pb) in self.inner.iter() {
            if !pb.verify_refcount() {
                return false;
            }
        }
        true
    }
    
    /// 获取使用中的 PhysBlock 数量
    pub fn used_count(&self) -> usize {
        self.inner.used_count()
    }
    
    /// 获取空闲的 PhysBlock 数量
    pub fn free_count(&self) -> usize {
        self.inner.free_count()
    }
}

/// 打印 Slab 统计信息（调试用）
#[cfg(debug_assertions)]
pub fn print_slab_stats() {
    let pb_slab = PB_SLAB.lock();
    let physr_slab = PHYSR_SLAB.lock();
    
    println!("PhysBlock Slab: used={}, free={}", 
             pb_slab.used_count(), pb_slab.free_count());
    println!("PhysRegion Slab: used={}, free={}", 
             physr_slab.used_count(), physr_slab.free_count());
}
```

**与 Minix3 的对比**

| 方面 | Minix3 | Rust |
|------|--------|------|
| Slab 定义 | `SLAB_DECLARE` / `SLAB_DEFINE` 宏 | `struct PhysBlockSlab` |
| 分配 | `SLABALLOC(var)` | `slab.alloc(phys)` |
| 释放 | `SLABFREE(ptr)` | `slab.dealloc(pb)` |
| 全局访问 | 直接使用 `pb_slab` | `PB_SLAB` Mutex |
| 线程安全 | 单线程，无需保护 | Mutex 保护 |

---

## 4. 实现详解

### 4.1 创建物理块

**创建流程**

```rust
impl PhysBlock {
    /// 创建新的物理块
    ///
    /// 对应 Minix3: `pb_new()`
    pub fn new(phys: PhysBytes) -> Self {
        Self {
            phys,
            firstregion: None,
            refcount: Cell::new(0),
            flags: PbFlags::default(),
        }
    }
}

/// 分配物理块并分配物理页
///
/// 完整的物理块创建流程
pub fn pb_alloc_with_page() -> Option<*mut PhysBlock> {
    // 1. 分配物理页
    let phys_click = alloc_mem(1, AllocFlags::empty())?;
    let phys = PhysBytes(click2abs(phys_click));
    
    // 2. 从 Slab 分配 PhysBlock
    let mut slab = PB_SLAB.lock();
    slab.alloc(phys).map(|pb| pb as *mut _)
}

/// 分配延迟分配的物理块
///
/// 物理页在首次访问时分配
pub fn pb_alloc_delayed() -> Option<*mut PhysBlock> {
    let mut slab = PB_SLAB.lock();
    slab.alloc(PhysBytes(0)).map(|pb| pb as *mut _)
}
```

**物理页分配**

```rust
/// 分配物理页
///
/// 对应 Minix3: `alloc_mem()`
fn alloc_phys_page(flags: AllocFlags) -> Option<PhysBytes> {
    // 调用内存分配器
    let click = alloc_mem(1, flags)?;
    Some(PhysBytes(click2abs(click)))
}

/// 释放物理页
///
/// 对应 Minix3: `free_mem()`
fn free_phys_page(phys: PhysBytes) {
    if phys.0 != 0 {
        free_mem(abs2click(phys.0), 1);
    }
}
```

**创建示例**

```rust
#[test]
fn test_pb_create() {
    // 创建带物理页的块
    let pb = pb_alloc_with_page().expect("allocation failed");
    unsafe {
        assert!((*pb).has_phys());
        assert_eq!((*pb).refcount(), 0);
        assert!(!(*pb).is_referenced());
        
        // 清理
        pb_free(pb);
    }
}

#[test]
fn test_pb_create_delayed() {
    // 创建延迟分配的块
    let pb = pb_alloc_delayed().expect("allocation failed");
    unsafe {
        assert!(!(*pb).has_phys());
        assert_eq!((*pb).refcount(), 0);
        
        // 清理
        pb_free(pb);
    }
}
```

---###### 4.2 引用管理

**安全的引用操作**

```rust
impl PhysBlock {
    /// 增加引用计数
    ///
    /// # Panics
    ///
    /// 如果 refcount 会溢出，触发 panic
    pub fn inc_ref(&self) {
        let count = self.refcount.get();
        assert!(count < u8::MAX, "PhysBlock refcount overflow");
        self.refcount.set(count + 1);
    }
    
    /// 减少引用计数
    ///
    /// # Returns
    ///
    /// 返回新的引用计数
    ///
    /// # Panics
    ///
    /// 如果 refcount 为 0，触发 panic
    pub fn dec_ref(&self) -> u8 {
        let count = self.refcount.get();
        assert!(count > 0, "PhysBlock refcount underflow");
        let new_count = count - 1;
        self.refcount.set(new_count);
        new_count
    }
}
```

**链接操作**

```rust
/// 将 phys_region 链接到 phys_block
///
/// 对应 Minix3: `pb_link()`
///
/// # Safety
///
/// - `pr` 必须是有效的 phys_region
/// - `pr` 不能已经链接到其他 phys_block
pub unsafe fn pb_link(
    pr: &mut PhysRegion,
    pb: &mut PhysBlock,
    offset: VirBytes,
    parent: *mut VirRegion,
) {
    pr.offset = offset;
    pr.ph = NonNull::new(pb as *mut _);
    pr.parent = parent;
    
    // 头插法加入链表
    pr.next_ph_list = pb.firstregion;
    pb.firstregion = NonNull::new(pr as *mut _);
    
    // 增加引用计数
    pb.inc_ref();
}

/// 创建新的 phys_region 并链接到 phys_block
///
/// 对应 Minix3: `pb_reference()`
pub fn pb_reference(
    pb: *mut PhysBlock,
    offset: VirBytes,
    parent: *mut VirRegion,
    memtype: &'static MemoryType,
) -> Option<*mut PhysRegion> {
    // 分配 phys_region
    let pr = physr_alloc()?;
    
    unsafe {
        (*pr).memtype = memtype;
        pb_link(&mut *pr, &mut *pb, offset, parent);
    }
    
    Some(pr)
}
```

**取消链接操作**

```rust
/// 从 phys_block 取消链接 phys_region
///
/// 对应 Minix3: `pb_unreferenced()`
///
/// # Returns
///
/// 返回新的引用计数
///
/// # Safety
///
/// - `pr` 必须已链接到 `pb`
pub unsafe fn pb_unlink(
    pb: &mut PhysBlock,
    pr: &PhysRegion,
) -> u8 {
    // 从链表中移除
    if let Some(head) = pb.firstregion {
        if head.as_ptr() == pr as *const _ as *mut _ {
            // pr 是链表头
            pb.firstregion = pr.next_ph_list;
        } else {
            // 遍历链表查找 pr
            let mut current = head;
            loop {
                let current_ref = current.as_ref();
                if let Some(next) = current_ref.next_ph_list {
                    if next.as_ptr() == pr as *const _ as *mut _ {
                        current.as_mut().next_ph_list = pr.next_ph_list;
                        break;
                    }
                    current = next;
                } else {
                    panic!("phys_region not found in chain");
                }
            }
        }
    }
    
    // 减少引用计数
    pb.dec_ref()
}
```

**引用管理示例**

```rust
#[test]
fn test_refcount_management() {
    let mut pb = PhysBlock::new(PhysBytes(0x1000));
    
    // 初始状态
    assert_eq!(pb.refcount(), 0);
    assert!(!pb.is_referenced());
    assert!(!pb.is_shared());
    
    // 增加引用
    pb.inc_ref();
    assert_eq!(pb.refcount(), 1);
    assert!(pb.is_referenced());
    assert!(!pb.is_shared());
    
    // 再次增加
    pb.inc_ref();
    assert_eq!(pb.refcount(), 2);
    assert!(pb.is_shared());  // refcount > 1
    
    // 减少引用
    let new_count = pb.dec_ref();
    assert_eq!(new_count, 1);
    assert!(!pb.is_shared());
    
    // 减少到 0
    let new_count = pb.dec_ref();
    assert_eq!(new_count, 0);
    assert!(!pb.is_referenced());
}

#[test]
#[should_panic(expected = "underflow")]
fn test_refcount_underflow() {
    let pb = PhysBlock::new(PhysBytes(0x1000));
    pb.dec_ref();  // 应该 panic
}

#[test]
#[should_panic(expected = "overflow")]
fn test_refcount_overflow() {
    let mut pb = PhysBlock::new(PhysBytes(0x1000));
    pb.refcount.set(255);
    pb.inc_ref();  // 应该 panic
}
```

### 4.3 释放策略

**释放条件**

PhysBlock 只能在 refcount == 0 时释放：

```
┌─────────────────────────────────────────────────────────────┐
│                    释放条件检查                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   释放前必须满足:                                            │
│   1. refcount == 0                                          │
│   2. firstregion == None                                    │
│   3. 所有引用都已取消                                        │
│                                                             │
│   释放步骤:                                                  │
│   1. 调用 memtype->ev_unreference()                         │
│   2. 释放物理页（如果有）                                    │
│   3. 归还 PhysBlock 给 Slab                                 │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**释放实现**

```rust
/// 释放 PhysBlock
///
/// 对应 Minix3: `pb_free()`
///
/// # Safety
///
/// - pb 必须是从 pb_alloc() 获得的
/// - pb 的 refcount 必须为 0
pub unsafe fn pb_free(pb: *mut PhysBlock) {
    if pb.is_null() {
        return;
    }
    
    let pb_ref = &*pb;
    
    // 验证可以释放
    debug_assert_eq!(pb_ref.refcount(), 0, "cannot free PhysBlock with refcount > 0");
    debug_assert!(pb_ref.firstregion.is_none(), "cannot free PhysBlock with active references");
    
    // 释放物理页
    if pb_ref.has_phys() {
        free_phys_page(pb_ref.phys);
    }
    
    // 归还给 Slab
    let mut slab = PB_SLAB.lock();
    slab.dealloc(pb_ref);
}

/// 取消引用并可能释放
///
/// 对应 Minix3: `pb_unreferenced()`
///
/// # Safety
///
/// - pr 必须是有效的 phys_region
/// - pr 必须已链接到某个 phys_block
pub unsafe fn pb_unreferenced(
    region: *mut VirRegion,
    pr: *mut PhysRegion,
    remove: bool,
) {
    let pr_ref = &mut *pr;
    let pb = pr_ref.ph.expect("phys_region not linked").as_ptr();
    
    // 从链表中移除
    let new_count = pb_unlink(&mut *pb, pr_ref);
    
    // 清除 pr 的 ph 指针
    pr_ref.ph = None;
    
    // 如果需要，从 vir_region 中移除
    if remove {
        physblock_set(region, pr_ref.offset, None);
    }
    
    // 如果引用计数为 0，释放
    if new_count == 0 {
        // 调用 memtype 回调
        let memtype = pr_ref.memtype;
        if let Err(e) = memtype.ev_unreference(pr_ref) {
            panic!("memtype unreference failed: {:?}", e);
        }
        
        // 释放 PhysBlock
        pb_free(pb);
    }
}
```

**memtype 回调**

```rust
/// 内存类型 trait
///
/// 定义不同内存类型的释放行为
pub trait MemoryType {
    /// 取消引用时的回调
    ///
    /// 返回 Err 表示释放失败
    fn ev_unreference(&self, pr: &PhysRegion) -> Result<(), i32>;
}

/// 匿名内存类型
pub struct AnonMemoryType;

impl MemoryType for AnonMemoryType {
    fn ev_unreference(&self, pr: &PhysRegion) -> Result<(), i32> {
        // 匿名内存直接释放物理页
        let pb = unsafe { &*pr.ph.unwrap().as_ptr() };
        if pb.has_phys() {
            free_phys_page(pb.phys);
        }
        Ok(())
    }
}

/// 文件映射内存类型
pub struct FileMemoryType;

impl MemoryType for FileMemoryType {
    fn ev_unreference(&self, pr: &PhysRegion) -> Result<(), i32> {
        // 文件映射可能需要写回磁盘
        let pb = unsafe { &*pr.ph.unwrap().as_ptr() };
        if pb.flags.contains(PbFlags::DIRTY) {
            // 写回文件
            write_back_to_file(pb)?;
        }
        // 物理页保留在缓存中
        Ok(())
    }
}
```

**释放示例**

```rust
#[test]
fn test_pb_free() {
    // 创建并立即释放
    let pb = pb_alloc_with_page().expect("allocation failed");
    unsafe {
        assert_eq!((*pb).refcount(), 0);
        pb_free(pb);
    }
    
    // 验证 Slab 统计
    let slab = PB_SLAB.lock();
    assert_eq!(slab.used_count(), 0);
}

#[test]
fn test_pb_unreferenced() {
    // 创建 PhysBlock
    let pb = pb_alloc_with_page().expect("allocation failed");
    
    unsafe {
        // 创建引用
        let mut pr = PhysRegion::new();
        pb_link(&mut pr, &mut *pb, VirBytes(0), std::ptr::null_mut());
        
        assert_eq!((*pb).refcount(), 1);
        
        // 取消引用
        pb_unreferenced(std::ptr::null_mut(), &mut pr, false);
        
        // PhysBlock 应该被释放
        // 注意：此时 pb 指针已无效
    }
}

#[test]
#[should_panic(expected = "refcount > 0")]
fn test_pb_free_with_refcount() {
    let pb = pb_alloc_with_page().expect("allocation failed");
    
    unsafe {
        // 增加引用计数
        (*pb).inc_ref();
        
        // 尝试释放应该 panic
        pb_free(pb);
    }
}
```

---### 4.4 线程安全

**单线程假设**

Minix3 的 VM 是单线程的，不需要原子操作。但在 Rust 实现中，我们可能需要考虑线程安全：

```
┌─────────────────────────────────────────────────────────────┐
│                    线程安全分析                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Minix3 VM:                                                │
│   - 单线程事件循环                                          │
│   - 无并发访问                                              │
│   - refcount 使用普通 u8_t                                  │
│                                                             │
│   Rust 实现:                                                │
│   - 可能在多核环境运行                                      │
│   - 需要考虑中断处理                                        │
│   - 使用 Cell<u8> 或 AtomicU8                               │
│                                                             │
│   选择:                                                      │
│   - 如果确定单线程: Cell<u8>                                │
│   - 如果可能多线程: AtomicU8                                │
│   - 当前选择 Cell<u8>，与 Minix3 保持一致                   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**原子操作版本（可选）**

```rust
use core::sync::atomic::{AtomicU8, Ordering};

/// 线程安全的 PhysBlock（可选实现）
pub struct AtomicPhysBlock {
    pub phys: PhysBytes,
    firstregion: AtomicPtr<PhysRegion>,
    refcount: AtomicU8,
    pub flags: PbFlags,
}

impl AtomicPhysBlock {
    /// 增加引用计数（原子操作）
    pub fn inc_ref(&self) {
        let old = self.refcount.fetch_add(1, Ordering::Relaxed);
        if old == u8::MAX {
            // 溢出，回滚并 panic
            self.refcount.fetch_sub(1, Ordering::Relaxed);
            panic!("PhysBlock refcount overflow");
        }
    }
    
    /// 减少引用计数（原子操作）
    /// 
    /// 返回新的引用计数
    pub fn dec_ref(&self) -> u8 {
        let old = self.refcount.fetch_sub(1, Ordering::Release);
        if old == 0 {
            // 下溢，回滚并 panic
            self.refcount.fetch_add(1, Ordering::Release);
            panic!("PhysBlock refcount underflow");
        }
        old - 1
    }
    
    /// 获取引用计数
    pub fn refcount(&self) -> u8 {
        self.refcount.load(Ordering::Acquire)
    }
}
```

**内存顺序说明**

```
┌─────────────────────────────────────────────────────────────┐
│                    内存顺序选择                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Ordering::Relaxed:                                        │
│   - 仅保证原子性                                            │
│   - 不保证顺序                                              │
│   - 适用于简单的计数器                                      │
│                                                             │
│   Ordering::Release / Acquire:                              │
│   - Release: 写操作前的所有写操作对其他线程可见             │
│   - Acquire: 读操作后的所有读操作看到最新值                 │
│   - 适用于有数据依赖的场景                                  │
│                                                             │
│   对于 refcount:                                            │
│   - inc_ref: Relaxed 足够（无数据依赖）                     │
│   - dec_ref: Release（释放前确保所有写操作完成）            │
│   - refcount(): Acquire（读取最新值）                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Mutex 保护**

当前实现使用 Mutex 保护 Slab：

```rust
use spin::Mutex;

/// 全局 Slab 使用 Mutex 保护
pub static PB_SLAB: Mutex<PhysBlockSlab> = Mutex::new(PhysBlockSlab::new());

/// 分配 PhysBlock
pub fn pb_alloc(phys: PhysBytes) -> Option<*mut PhysBlock> {
    let mut slab = PB_SLAB.lock();
    slab.alloc(phys).map(|pb| pb as *mut _)
}

/// 释放 PhysBlock
pub unsafe fn pb_free(pb: *mut PhysBlock) {
    let mut slab = PB_SLAB.lock();
    slab.dealloc(&*pb);
}
```

**中断安全**

```rust
/// 禁用中断的保护
/// 
/// 如果在中断上下文中可能访问 PhysBlock，需要禁用中断
pub fn pb_alloc_irqsafe(phys: PhysBytes) -> Option<*mut PhysBlock> {
    // 禁用中断
    let irq_flags = irq_save();
    
    let result = {
        let mut slab = PB_SLAB.lock();
        slab.alloc(phys).map(|pb| pb as *mut _)
    };
    
    // 恢复中断
    irq_restore(irq_flags);
    
    result
}
```

**线程安全测试**

```rust
#[cfg(test)]
mod thread_safety_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    
    #[test]
    fn test_concurrent_refcount() {
        // 测试原子版本的并发安全性
        let pb = Arc::new(AtomicPhysBlock::new(PhysBytes(0x1000)));
        
        let mut handles = vec![];
        
        // 多线程增加引用
        for _ in 0..10 {
            let pb_clone = Arc::clone(&pb);
            handles.push(thread::spawn(move || {
                pb_clone.inc_ref();
            }));
        }
        
        for h in handles {
            h.join().unwrap();
        }
        
        assert_eq!(pb.refcount(), 10);
        
        // 多线程减少引用
        let mut handles = vec![];
        for _ in 0..10 {
            let pb_clone = Arc::clone(&pb);
            handles.push(thread::spawn(move || {
                pb_clone.dec_ref();
            }));
        }
        
        for h in handles {
            h.join().unwrap();
        }
        
        assert_eq!(pb.refcount(), 0);
    }
}
```

**线程安全总结**

```
┌─────────────────────────────────────────────────────────────┐
│                    线程安全策略总结                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 默认实现 (Cell<u8>)                                     │
│      - 与 Minix3 保持一致                                    │
│      - 适用于单线程环境                                      │
│      - 更好的性能                                            │
│                                                             │
│   2. 可选原子实现 (AtomicU8)                                 │
│      - 适用于多线程环境                                      │
│      - 使用适当的内存顺序                                    │
│      - 可通过 feature flag 切换                             │
│                                                             │
│   3. Slab 保护                                               │
│      - 使用 Mutex 保护全局 Slab                              │
│      - 分配/释放操作需要获取锁                               │
│      - 考虑中断上下文的特殊处理                              │
│                                                             │
│   4. 建议                                                    │
│      - 保持与 Minix3 一致的单线程假设                        │
│      - 如需多线程，使用 AtomicU8 版本                        │
│      - 通过编译时 feature 选择实现方式                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 5. 与 CoW 的关系

### 5.1 共享物理页

**fork 后的共享状态**

fork 系统调用创建子进程时，父子进程共享物理页：

```
┌─────────────────────────────────────────────────────────────┐
│                    fork 后的共享状态                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   fork 前:                                                  │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 父进程 vir_region                                    │   │
│   │   vaddr: 0x400000                                    │   │
│   │   physblocks[0] ──► phys_block                       │   │
│   │                     ├─ phys: 0x1234000               │   │
│   │                     ├─ refcount: 1                   │   │
│   │                     └─ firstregion ──► parent_pr     │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   fork 后:                                                  │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 父进程 vir_region                                    │   │
│   │   physblocks[0] ──┐                                  │   │
│   └────────────────────┼──────────────────────────────────┘   │
│                        │                                    │
│   ┌────────────────────┼──────────────────────────────────┐   │
│   │ 子进程 vir_region  │                                  │   │
│   │   physblocks[0] ───┼──┐                               │   │
│   └────────────────────┼──┼──────────────────────────────┘   │
│                        │  │                                  │
│                        ▼  ▼                                  │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 共享的 phys_block                                    │   │
│   │   phys: 0x1234000                                    │   │
│   │   refcount: 2  ◄─── 共享                              │   │
│   │   firstregion ──► parent_pr ──► child_pr ──► NULL    │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   关键点:                                                    │
│   - 同一个物理页被两个进程映射                              │
│   - refcount = 2 表示共享                                   │
│   - 页表项标记为只读，触发 CoW                              │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**共享的实现**

```rust
/// fork 时共享物理页
///
/// 创建新的 phys_region 引用同一个 phys_block
pub fn fork_share_phys(
    parent_region: &VirRegion,
    child_region: &mut VirRegion,
    offset: VirBytes,
) -> Result<(), i32> {
    // 获取父进程的 phys_region
    let parent_pr = parent_region.physblock_get(offset)
        .ok_or(ENOMEM)?;
    
    let pb = unsafe { &mut *parent_pr.ph.unwrap().as_ptr() };
    
    // 创建子进程的 phys_region
    let child_pr = pb_reference(
        pb as *mut _,
        offset,
        child_region as *mut _ as *mut VirRegion,
        parent_pr.memtype,
    ).ok_or(ENOMEM)?;
    
    // 设置页表为只读
    unsafe {
        pt_makereadonly(child_region, offset);
    }
    
    Ok(())
}
```

**共享检测**

```rust
impl PhysBlock {
    /// 检查是否需要 CoW
    ///
    /// refcount > 1 表示共享，写入需要 CoW
    pub fn needs_cow(&self) -> bool {
        self.refcount.get() > 1
    }
    
    /// 检查是否为私有页
    ///
    /// refcount == 1 表示私有，可以直接写入
    pub fn is_private(&self) -> bool {
        self.refcount.get() == 1
    }
}
```

---### 5.2 写时复制触发

**CoW 触发流程**

当进程尝试写入共享页时，触发写时复制：

```
┌─────────────────────────────────────────────────────────────┐
│                    CoW 触发流程                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   1. 写操作尝试                                              │
│      ┌─────────────────────────────────────────────────┐   │
│      │ 进程写入共享页                                    │   │
│      │ 页表项为只读                                      │   │
│      │ 触发页面保护异常                                  │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   2. 异常处理                                                │
│      ┌─────────────────────────────────────────────────┐   │
│      │ VM 接收异常                                       │   │
│      │ 查找对应的 vir_region 和 phys_region             │   │
│      │ 检查 pb->refcount > 1                            │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   3. CoW 复制                                                │
│      ┌─────────────────────────────────────────────────┐   │
│      │ 分配新物理页                                      │   │
│      │ 复制原页内容                                      │   │
│      │ 更新页表映射                                      │   │
│      │ 减少 refcount                                     │   │
│      │ 链接到新 phys_block                               │   │
│      └─────────────────────────────────────────────────┘   │
│                         │                                   │
│                         ▼                                   │
│   4. 继续写入                                                │
│      ┌─────────────────────────────────────────────────┐   │
│      │ 进程拥有私有页                                    │   │
│      │ refcount = 1                                      │   │
│      │ 可以正常写入                                      │   │
│      └─────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**CoW 实现**

```rust
/// 写时复制
///
/// 对应 Minix3: `mem_cow()`
///
/// # 参数
///
/// - `region`: 发生写入的虚拟区域
/// - `pr`: 对应的 phys_region
/// - `new_page_cl`: 预分配的新物理页（可选）
///
/// # 返回
///
/// 成功返回 OK，失败返回错误码
pub fn mem_cow(
    region: &mut VirRegion,
    pr: &mut PhysRegion,
    new_page_cl: Option<PhysClick>,
) -> Result<(), i32> {
    // 1. 分配新物理页
    let (new_page_cl, new_page) = if let Some(cl) = new_page_cl {
        (cl, click2abs(cl))
    } else {
        let cl = alloc_mem(1, vrallocflags(region.flags))?;
        (cl, click2abs(cl))
    };
    
    let old_pb = unsafe { &*pr.ph.unwrap().as_ptr() };
    
    // 2. 复制内容
    if old_pb.has_phys() {
        unsafe {
            sys_abscopy(old_pb.phys.0, new_page, VM_PAGE_SIZE)?;
        }
    }
    
    // 3. 创建新的 phys_block
    let new_pb = pb_alloc(PhysBytes(new_page)).ok_or(ENOMEM)?;
    
    // 4. 取消旧引用
    unsafe {
        pb_unreferenced(region as *mut _ as *mut VirRegion, pr, false);
    }
    
    // 5. 链接到新块
    unsafe {
        pb_link(pr, &mut *new_pb, pr.offset, region as *mut _ as *mut VirRegion);
    }
    
    // 6. 更新 memtype
    pr.memtype = &MEM_TYPE_ANON;
    
    Ok(())
}
```

**CoW 前后对比**

```
┌─────────────────────────────────────────────────────────────┐
│                    CoW 前后状态对比                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   CoW 前:                                                   │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 父进程 vir_region                                    │   │
│   │   physblocks[0] ──┐                                  │   │
│   └────────────────────┼──────────────────────────────────┘   │
│                        │                                    │
│   ┌────────────────────┼──────────────────────────────────┐   │
│   │ 子进程 vir_region  │   (尝试写入)                     │   │
│   │   physblocks[0] ───┼──┐                               │   │
│   └────────────────────┼──┼──────────────────────────────┘   │
│                        │  │                                  │
│                        ▼  ▼                                  │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 共享的 phys_block                                    │   │
│   │   phys: 0x1234000                                    │   │
│   │   refcount: 2                                        │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   CoW 后:                                                   │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 父进程 vir_region                                    │   │
│   │   physblocks[0] ──► phys_block_A                     │   │
│   │                     ├─ phys: 0x1234000               │   │
│   │                     └─ refcount: 1                   │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ 子进程 vir_region   (写入完成)                       │   │
│   │   physblocks[0] ──► phys_block_B                     │   │
│   │                     ├─ phys: 0x5678000 (新页)        │   │
│   │                     └─ refcount: 1                   │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
│   关键变化:                                                  │
│   - 子进程拥有独立的物理页                                  │
│   - 父子进程的 refcount 都变为 1                            │
│   - 各自可以独立写入                                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**CoW 测试**

```rust
#[test]
fn test_cow_basic() {
    // 创建父进程的 vir_region
    let mut parent_region = VirRegion::new(VirBytes(0x400000), VirBytes(0x1000), VrFlags::WRITABLE);
    
    // 分配物理页
    let pb = pb_alloc_with_page().expect("allocation failed");
    unsafe {
        // 链接到父进程
        let mut pr = PhysRegion::new();
        pb_link(&mut pr, &mut *pb, VirBytes(0), &mut parent_region as *mut _);
        
        assert_eq!((*pb).refcount(), 1);
        assert!(!(*pb).needs_cow());
    }
    
    // 模拟 fork
    let mut child_region = VirRegion::new(VirBytes(0x400000), VirBytes(0x1000), VrFlags::WRITABLE);
    unsafe {
        fork_share_phys(&parent_region, &mut child_region, VirBytes(0)).unwrap();
        
        // 现在是共享状态
        assert_eq!((*pb).refcount(), 2);
        assert!((*pb).needs_cow());
    }
    
    // 模拟子进程写入，触发 CoW
    let pr = child_region.physblock_get(VirBytes(0)).unwrap();
    mem_cow(&mut child_region, pr, None).unwrap();
    
    unsafe {
        // 父进程的 refcount 恢复为 1
        assert_eq!((*pb).refcount(), 1);
        assert!(!(*pb).needs_cow());
        
        // 子进程有新的 phys_block
        let child_pb = pr.ph.unwrap().as_ptr();
        assert_ne!(child_pb, pb);
        assert_eq!((*child_pb).refcount(), 1);
    }
}
```

---

## 6. 测试与验证

### 6.1 引用计数测试

**基本测试**

```rust
#[cfg(test)]
mod refcount_tests {
    use super::*;
    
    #[test]
    fn test_refcount_initial() {
        let pb = PhysBlock::new(PhysBytes(0x1000));
        assert_eq!(pb.refcount(), 0);
        assert!(!pb.is_referenced());
        assert!(!pb.is_shared());
    }
    
    #[test]
    fn test_refcount_inc_dec() {
        let pb = PhysBlock::new(PhysBytes(0x1000));
        
        pb.inc_ref();
        assert_eq!(pb.refcount(), 1);
        assert!(pb.is_referenced());
        assert!(!pb.is_shared());
        
        pb.inc_ref();
        assert_eq!(pb.refcount(), 2);
        assert!(pb.is_shared());
        
        let new_count = pb.dec_ref();
        assert_eq!(new_count, 1);
        assert!(!pb.is_shared());
        
        let new_count = pb.dec_ref();
        assert_eq!(new_count, 0);
        assert!(!pb.is_referenced());
    }
    
    #[test]
    #[should_panic(expected = "underflow")]
    fn test_refcount_underflow() {
        let pb = PhysBlock::new(PhysBytes(0x1000));
        pb.dec_ref();  // 应该 panic
    }
    
    #[test]
    #[should_panic(expected = "overflow")]
    fn test_refcount_overflow() {
        let mut pb = PhysBlock::new(PhysBytes(0x1000));
        pb.refcount.set(255);
        pb.inc_ref();  // 应该 panic
    }
    
    #[test]
    fn test_needs_cow() {
        let pb = PhysBlock::new(PhysBytes(0x1000));
        
        assert!(!pb.needs_cow());  // refcount = 0
        
        pb.inc_ref();
        assert!(!pb.needs_cow());  // refcount = 1
        
        pb.inc_ref();
        assert!(pb.needs_cow());   // refcount = 2
    }
}
```

**链表一致性测试**

```rust
#[cfg(test)]
mod chain_tests {
    use super::*;
    
    #[test]
    fn test_chain_consistency() {
        let mut pb = PhysBlock::new(PhysBytes(0x1000));
        let mut pr1 = PhysRegion::new();
        let mut pr2 = PhysRegion::new();
        let mut pr3 = PhysRegion::new();
        
        unsafe {
            // 链接三个 phys_region
            pb_link(&mut pr1, &mut pb, VirBytes(0x0000), std::ptr::null_mut());
            pb_link(&mut pr2, &mut pb, VirBytes(0x1000), std::ptr::null_mut());
            pb_link(&mut pr3, &mut pb, VirBytes(0x2000), std::ptr::null_mut());
        }
        
        // 验证 refcount
        assert_eq!(pb.refcount(), 3);
        
        // 验证链表一致性
        #[cfg(debug_assertions)]
        assert!(pb.verify_refcount());
        
        // 移除中间节点
        unsafe {
            let new_count = pb_unlink(&mut pb, &pr2);
            assert_eq!(new_count, 2);
        }
        
        assert_eq!(pb.refcount(), 2);
        
        #[cfg(debug_assertions)]
        assert!(pb.verify_refcount());
    }
    
    #[test]
    fn test_chain_order() {
        let mut pb = PhysBlock::new(PhysBytes(0x1000));
        let mut pr1 = PhysRegion::new();
        let mut pr2 = PhysRegion::new();
        let mut pr3 = PhysRegion::new();
        
        unsafe {
            pb_link(&mut pr1, &mut pb, VirBytes(0x0000), std::ptr::null_mut());
            pb_link(&mut pr2, &mut pb, VirBytes(0x1000), std::ptr::null_mut());
            pb_link(&mut pr3, &mut pb, VirBytes(0x2000), std::ptr::null_mut());
        }
        
        // 验证链表顺序（头插法，应该是 3 -> 2 -> 1）
        let first = pb.first_region().unwrap();
        assert_eq!(first.offset, VirBytes(0x2000));
        
        let second = first.next_ph_list.as_ref().unwrap();
        assert_eq!(second.offset, VirBytes(0x1000));
        
        let third = second.next_ph_list.as_ref().unwrap();
        assert_eq!(third.offset, VirBytes(0x0000));
        
        assert!(third.next_ph_list.is_none());
    }
}
```

---### 6.2 生命周期测试

**创建和释放测试**

```rust
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    
    #[test]
    fn test_create_free_basic() {
        // 创建 PhysBlock
        let pb = pb_alloc_with_page().expect("allocation failed");
        
        unsafe {
            assert!((*pb).has_phys());
            assert_eq!((*pb).refcount(), 0);
            
            // 立即释放
            pb_free(pb);
        }
        
        // 验证 Slab 统计
        let slab = PB_SLAB.lock();
        assert_eq!(slab.used_count(), 0);
    }
    
    #[test]
    fn test_delayed_allocation() {
        // 创建延迟分配的 PhysBlock
        let pb = pb_alloc_delayed().expect("allocation failed");
        
        unsafe {
            assert!(!(*pb).has_phys());
            assert_eq!((*pb).phys, PhysBytes(0));
            
            // 后续分配物理页
            let phys = alloc_phys_page(AllocFlags::empty()).expect("no memory");
            (*pb).phys = phys;
            assert!((*pb).has_phys());
            
            pb_free(pb);
        }
    }
    
    #[test]
    fn test_reference_lifecycle() {
        let pb = pb_alloc_with_page().expect("allocation failed");
        
        unsafe {
            // 创建引用
            let mut pr = PhysRegion::new();
            pb_link(&mut pr, &mut *pb, VirBytes(0), std::ptr::null_mut());
            
            assert_eq!((*pb).refcount(), 1);
            
            // 取消引用
            let new_count = pb_unlink(&mut *pb, &pr);
            assert_eq!(new_count, 0);
            
            // 现在可以释放
            pb_free(pb);
        }
    }
    
    #[test]
    fn test_multiple_references() {
        let pb = pb_alloc_with_page().expect("allocation failed");
        
        unsafe {
            let mut pr1 = PhysRegion::new();
            let mut pr2 = PhysRegion::new();
            let mut pr3 = PhysRegion::new();
            
            // 创建多个引用
            pb_link(&mut pr1, &mut *pb, VirBytes(0x0000), std::ptr::null_mut());
            pb_link(&mut pr2, &mut *pb, VirBytes(0x1000), std::ptr::null_mut());
            pb_link(&mut pr3, &mut *pb, VirBytes(0x2000), std::ptr::null_mut());
            
            assert_eq!((*pb).refcount(), 3);
            
            // 逐个取消引用
            pb_unlink(&mut *pb, &pr1);
            assert_eq!((*pb).refcount(), 2);
            
            pb_unlink(&mut *pb, &pr2);
            assert_eq!((*pb).refcount(), 1);
            
            pb_unlink(&mut *pb, &pr3);
            assert_eq!((*pb).refcount(), 0);
            
            // 所有引用取消后可以释放
            pb_free(pb);
        }
    }
    
    #[test]
    #[should_panic(expected = "refcount > 0")]
    fn test_free_with_references() {
        let pb = pb_alloc_with_page().expect("allocation failed");
        
        unsafe {
            let mut pr = PhysRegion::new();
            pb_link(&mut pr, &mut *pb, VirBytes(0), std::ptr::null_mut());
            
            // 尝试释放有引用的块，应该 panic
            pb_free(pb);
        }
    }
}
```

**Slab 统计测试**

```rust
#[cfg(test)]
mod slab_tests {
    use super::*;
    
    #[test]
    fn test_slab_allocation() {
        let initial_free = {
            let slab = PB_SLAB.lock();
            slab.free_count()
        };
        
        // 分配多个 PhysBlock
        let mut blocks = Vec::new();
        for _ in 0..10 {
            blocks.push(pb_alloc_delayed().expect("allocation failed"));
        }
        
        {
            let slab = PB_SLAB.lock();
            assert_eq!(slab.used_count(), 10);
            assert_eq!(slab.free_count(), initial_free.saturating_sub(10));
        }
        
        // 释放所有块
        unsafe {
            for pb in blocks {
                pb_free(pb);
            }
        }
        
        {
            let slab = PB_SLAB.lock();
            assert_eq!(slab.used_count(), 0);
            assert_eq!(slab.free_count(), initial_free);
        }
    }
    
    #[test]
    fn test_slab_exhaustion() {
        // 分配直到耗尽
        let mut blocks = Vec::new();
        while let Some(pb) = pb_alloc_delayed() {
            blocks.push(pb);
        }
        
        let used = blocks.len();
        assert!(used > 0, "should have allocated at least one block");
        
        // 下一次分配应该失败
        assert!(pb_alloc_delayed().is_none());
        
        // 释放一个
        unsafe {
            pb_free(blocks.pop().unwrap());
        }
        
        // 现在应该可以再分配一个
        assert!(pb_alloc_delayed().is_some());
        
        // 清理
        unsafe {
            for pb in blocks {
                pb_free(pb);
            }
        }
    }
}
```

**内存泄漏检测**

```rust
#[cfg(test)]
mod leak_tests {
    use super::*;
    
    #[test]
    fn test_no_leak_basic() {
        let initial_stats = {
            let slab = PB_SLAB.lock();
            slab.stats()
        };
        
        // 执行一些操作
        for _ in 0..100 {
            let pb = pb_alloc_with_page().expect("allocation failed");
            unsafe {
                let mut pr = PhysRegion::new();
                pb_link(&mut pr, &mut *pb, VirBytes(0), std::ptr::null_mut());
                pb_unlink(&mut *pb, &pr);
                pb_free(pb);
            }
        }
        
        // 验证没有泄漏
        let final_stats = {
            let slab = PB_SLAB.lock();
            slab.stats()
        };
        
        assert_eq!(initial_stats.used, final_stats.used);
        assert_eq!(initial_stats.free, final_stats.free);
    }
    
    #[test]
    fn test_no_leak_cow() {
        let initial_stats = {
            let slab = PB_SLAB.lock();
            slab.stats()
        };
        
        // 模拟 fork 和 CoW
        for _ in 0..50 {
            // 父进程创建区域
            let pb = pb_alloc_with_page().expect("allocation failed");
            
            unsafe {
                let mut parent_pr = PhysRegion::new();
                pb_link(&mut parent_pr, &mut *pb, VirBytes(0), std::ptr::null_mut());
                
                // fork 共享
                let mut child_pr = PhysRegion::new();
                pb_link(&mut child_pr, &mut *pb, VirBytes(0), std::ptr::null_mut());
                
                assert_eq!((*pb).refcount(), 2);
                
                // 子进程 CoW
                let new_pb = pb_alloc_with_page().expect("allocation failed");
                pb_unlink(&mut *pb, &child_pr);
                pb_link(&mut child_pr, &mut *new_pb, VirBytes(0), std::ptr::null_mut());
                
                assert_eq!((*pb).refcount(), 1);
                assert_eq!((*new_pb).refcount(), 1);
                
                // 清理
                pb_unlink(&mut *pb, &parent_pr);
                pb_free(pb);
                
                pb_unlink(&mut *new_pb, &child_pr);
                pb_free(new_pb);
            }
        }
        
        // 验证没有泄漏
        let final_stats = {
            let slab = PB_SLAB.lock();
            slab.stats()
        };
        
        assert_eq!(initial_stats.used, final_stats.used);
    }
}
```

---

## 7. 参见

- [08-slab-allocator.md](08-slab-allocator.md) - 使用 Slab 分配 phys_block
- [14-phys-region.md](14-phys-region.md) - 引用 phys_block
- [15-cow-mechanism.md](15-cow-mechanism.md) - 基于 phys_block 的 CoW

---

*分类: VM私有*
