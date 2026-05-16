# 10-phys-block: 物理块 (phys_block)

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/pb.c`, `region.h`  
> **说明**: 管理物理内存块，实现引用计数和生命周期管理

---

## 1. 概述

**物理块的核心作用**

`phys_block` 是 VM 中管理物理内存页的核心数据结构。每个 `phys_block` 代表一个物理内存页（4KB），并通过引用计数机制支持多个虚拟区域共享同一物理页。

**phys_block 在 VM 中的位置**

三层关系：
1. `vir_region`（虚拟区域）：进程的虚拟地址空间段，通过 `physblocks[]` 数组间接引用物理块
2. `phys_region`（物理区域）：连接虚拟区域与物理块的桥梁，记录 offset、memtype 等映射信息
3. `phys_block`（物理块）：代表一个物理页，维护引用计数和引用者链表

CoW 场景下，fork 后父子进程的 `phys_region` 通过各自的 `vir_region.physblocks[]` 引用同一个 `phys_block`，此时 `refcount` 增为 2，页表标记为只读。写入时触发缺页异常，执行写时复制。

**引用计数机制**

引用计数是 `phys_block` 的核心特性，实现了物理内存的安全共享：

- **创建**：`pb_new()` 返回 `refcount=0` 的物理块，尚未被任何 `phys_region` 引用
- **增加引用**（fork、共享内存）：`pb_link()` 使 `refcount++`，每增加一个 `phys_region` 引用，refcount +1
- **减少引用**（munmap、exit、CoW）：`pb_unreferenced()` 使 `refcount--`，当 `refcount==0` 时调用 `memtype->ev_unreference()` 释放物理内存，然后 `SLABFREE(pb)` 释放结构体

**与 Minix3 的对应关系**

| Minix3 结构 | 作用 |
|------------|------|
| `phys_block` | 物理内存块，含引用计数 |
| `phys_region` | 虚拟区域到物理块的映射 |
| `pb_new()` | 创建物理块 |
| `pb_free()` | 释放物理块 |
| `pb_link()` | 增加引用 |
| `pb_unreferenced()` | 减少引用 |
| `refcount` | 引用计数（u8_t） |
| `firstregion` | 引用链表头 |

**关键设计要点**

1. **引用计数存储位置**：Minix3 在 `phys_block` 中存储 `refcount`，集中管理，易于验证，但需要额外的链表遍历
2. **物理块与虚拟区域的关系**：一个 `phys_block` 可被多个 `phys_region` 引用；一个 `phys_region` 只能引用一个 `phys_block`；通过 `firstregion` 链表维护所有引用
3. **生命周期管理**：创建（分配物理页 + 初始化 `refcount=0`）→ 链接（`pb_link()` 增加 refcount）→ 解链（`pb_unreferenced()` 减少 refcount）→ 释放（`refcount==0` 时释放物理页和 `phys_block`）
4. **CoW 支持**：写操作时检查 refcount；`refcount > 1` 时触发写时复制；复制后原块 `refcount--`，新块 `refcount=1`

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

| 字段 | 类型 | 说明 |
|------|------|------|
| `seencount` | `u32_t` | 调试用（仅 `SANITYCHECKS` 构建） |
| `phys` | `phys_bytes` | 物理内存地址 |
| `firstregion` | `*phys_region` | 引用链表头 |
| `refcount` | `u8_t` | 引用计数 |
| `flags` | `u8_t` | 标志位 |

> **32位 vs 64位差异**：Minix3 原始代码运行在 x86-32 上，`phys_bytes` 为 `u32_t`（4字节），指针为4字节；minix-rs 目标为 x86-64，`phys_bytes` 为 `u64_t`（8字节），指针为8字节。因此结构体大小不同：
> - 32位（不含 seencount）：phys(4) + firstregion(4) + refcount(1) + flags(1) + padding(2) = 12字节
> - 64位（不含 seencount）：phys(8) + firstregion(8) + refcount(1) + flags(1) + padding(6) = 24字节

**phys 字段**

```c
phys_bytes phys;  /* 物理内存地址 */
```

- 存储物理页的起始地址
- 必须是页对齐（4KB 对齐）
- 值为 `MAP_NONE`（`0xFFFFFFFE`）表示未分配物理内存（延迟分配）

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
- 链表长度始终等于 `refcount`

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

**内存布局**

生产环境（`SANITYCHECKS` 未定义）：phys(8) + firstregion(8) + refcount(1) + flags(1) + padding(6) = 24字节（64位）

调试环境（`SANITYCHECKS` 定义）：seencount(4) + padding(4) + phys(8) + firstregion(8) + refcount(1) + flags(1) + padding(6) = 32字节（64位）

SLAB 分配：通过通用 slaballoc() 按大小分配


---

### 2.2 物理块生命周期

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
    
    /* 初始化字段（USE 宏在 SANITYCHECKS 构建中执行 slabunlock/slablock） */
    USE(newpb,
    newpb->phys = phys;           /* 物理地址 */
    newpb->refcount = 0;          /* 初始引用计数为 0 */
    newpb->firstregion = NULL;    /* 无引用 */
    newpb->flags = 0;             /* 无标志 */
    );

    return newpb;
}
```

**参数说明**

- `phys = MAP_NONE`（`0xFFFFFFFE`）：延迟分配，不立即分配物理页，后续通过缺页处理分配
- `phys = 有效地址`：已分配的物理页地址，必须页对齐（4KB），例如 `0x1234000`

**创建流程**

1. `SLABALLOC(newpb)`：从通用 SLAB 分配器分配 `phys_block` 结构体，失败返回 NULL
2. 验证物理地址：`if (phys != MAP_NONE) assert(phys % VM_PAGE_SIZE == 0)`，确保物理地址页对齐
3. 初始化字段：`USE(newpb, ...)` 宏包裹字段赋值（SANITYCHECKS 构建中执行 slabunlock/slablock），设置 `phys`、`refcount=0`、`firstregion=NULL`、`flags=0`
4. 返回 `newpb`

**关键点：refcount 初始化为 0**

refcount 初始化为 0 而不是 1 的原因：

1. **分离创建和引用**：`pb_new()` 仅创建结构体，`pb_link()` 建立引用关系并 `refcount++`，允许创建后不立即使用
2. **灵活性**：可以预分配 `phys_block`，延迟到需要时再链接，支持批量操作优化
3. **一致性**：`refcount` 始终等于链表长度，初始状态无链表则 `refcount=0`，易于验证正确性

典型使用模式：`pb = pb_new(phys);` → `refcount=0`，然后 `pb_link(pr, pb, ...);` → `refcount=1`

**使用场景**

```c
/* 场景 1: 已分配物理页 */
phys_bytes phys = alloc_mem(1, flags);
struct phys_block *pb = pb_new(CLICK2ABS(phys));
/* pb->phys = 物理地址, refcount = 0 */

/* 场景 2: 延迟分配 */
struct phys_block *pb = pb_new(MAP_NONE);
/* pb->phys = MAP_NONE (0xFFFFFFFE), refcount = 0 */
/* 后续缺页时再分配物理页 */

/* 场景 3: CoW 复制 */
phys_bytes new_page = alloc_mem(1, flags);
sys_abscopy(old_page, new_page, VM_PAGE_SIZE);
struct phys_block *pb = pb_new(new_page);
/* 新块独立，refcount = 0，等待链接 */
```

#### 方案四视角：CoW 复制的简化

> **方案四标注**：Direct Map 方案下，`sys_abscopy` 被 `vm_phys_to_virt() + copy_nonoverlapping()` 替代。

```rust
/* 场景 3: CoW 复制（Direct Map 方案） */
let new_phys = alloc_phys(1, flags)?;
let new_phys_mt = PhysBytes::new(new_phys.as_u64());
unsafe {
    core::ptr::copy_nonoverlapping(
        vm_phys_to_virt(old_phys) as *const u8,
        vm_phys_to_virt(new_phys) as *mut u8,
        4096,
    );
}
let pb = PhysBlock::new(new_phys_mt);
```

`sys_abscopy` 的存在意味着 VM 不信任自己能直接操作物理内存——需要内核作为中介。Direct map 使 VM 成为物理内存的直接操作者，不再需要"委托内核复制"。读者应理解：这是 VM 从"受信任的请求者"到"物理内存的主人"的角色转变。详见 [15-cow-mechanism.md](15-cow-mechanism.md) §2.2.1。

**内存分配细节**

```
SLABALLOC 宏展开:

#define SLABALLOC(var) (var = slaballoc(sizeof(*var)))

特点:
1. slaballoc() 是通用分配器，根据对象大小自动选择合适的 slab 池
2. 避免频繁调用 malloc/free
3. 提高分配效率
4. 减少内存碎片

注意：Minix3 的 slab 分配器是通用的，不为 phys_block 单独声明专用 slab。
     SLABALLOC 根据sizeof(*var)自动路由到对应大小的 slab 池。
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

1. 检查物理地址：`if (pb->phys != MAP_NONE) free_mem(ABS2CLICK(pb->phys), 1);`，如果有物理页则归还给内存分配器，`MAP_NONE` 表示延迟分配无需释放
2. 释放结构体：`SLABFREE(pb);`，将 `phys_block` 归还给 SLAB 分配器

**重要前提条件**

⚠️ `pb_free` 只能在 `refcount == 0` 时调用！

**两条释放路径**：

1. **直接调用 `pb_free()`**：适用于创建后未使用的块、错误处理路径、`refcount == 0` 的块。前提是确保无 `phys_region` 引用此块。`pb_free` 释放物理页（`free_mem`）+ 释放结构体（`SLABFREE`）。

2. **通过 `pb_unreferenced()`**：适用于正常的引用释放（munmap、exit、CoW）。流程：从链表中移除 `phys_region` → `refcount--` → 如果 `refcount == 0`，调用 `memtype->ev_unreference(pr)` 释放物理页 → `SLABFREE(pb)` 释放结构体。

> 注意：`pb_unreferenced` 并不调用 `pb_free`。两者是独立的释放路径。`pb_unreferenced` 在 `refcount==0` 时直接调用 `pr->memtype->ev_unreference(pr)`（由 memtype 回调负责释放物理页），然后 `SLABFREE(pb)`。而 `pb_free` 是直接释放物理页和结构体的便捷函数。

**物理内存释放细节**

```c
free_mem(ABS2CLICK(pb->phys), 1);
```

```
ABS2CLICK 宏:
- 将字节地址转换为 click (4KB 块号)
- #define ABS2CLICK(a) ((a) >> CLICK_SHIFT)   /* minix/include/minix/const.h */
- 例如: 0x1234000 → 0x1234

CLICK2ABS 宏:
- 将 click 号转换为字节地址
- #define CLICK2ABS(v) ((v) << CLICK_SHIFT)
- 例如: 0x1234 → 0x1234000

free_mem 参数:
- 第一个参数: 起始 click 号
- 第二个参数: 释放的 click 数量 (1 = 4KB)
```

**使用场景**

```c
/* 场景 1: 错误处理 - 创建后未使用（简化示例） */
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
/* anon_unreference: 匿名内存的 ev_unreference 回调 */
static int anon_unreference(struct phys_region *pr) {
    /* 断言 refcount 已经为 0 */
    assert(pr->ph->refcount == 0);
    /* 匿名内存直接释放物理页（不是调用 pb_free） */
    if(pr->ph->phys != MAP_NONE)
        free_mem(ABS2CLICK(pr->ph->phys), 1);
    return OK;
}

/* 场景 3: 延迟分配的块 */
struct phys_block *pb = pb_new(MAP_NONE);
/* pb->phys = MAP_NONE (0xFFFFFFFE)，无物理页 */
pb_free(pb);  /* 只释放结构体，无物理页释放 */
```

**与 pb_unreferenced 的关系**

| 方面 | 直接调用 `pb_free()` | 通过 `pb_unreferenced()` |
|------|----------------------|--------------------------|
| 适用场景 | 创建后未使用的块、错误处理路径 | 正常的引用释放（munmap、exit、CoW） |
| 前提 | 确保无 `phys_region` 引用此块 | `pr` 必须已链接到 `pb` |
| 物理页释放 | `free_mem(ABS2CLICK(pb->phys), 1)` | 由 `memtype->ev_unreference(pr)` 回调负责 |
| 结构体释放 | `SLABFREE(pb)` | `SLABFREE(pb)`（在 `refcount==0` 时） |
| refcount 操作 | 无（调用者保证 `refcount==0`） | `refcount--`，检查是否为 0 |


---

### 2.3 引用计数管理

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
    /* 设置 phys_region 字段并加入链表（USE 宏在 SANITYCHECKS 构建中执行 slabunlock/slablock） */
    USE(newphysr,
    newphysr->offset = offset;
    newphysr->ph = newpb;
    newphysr->parent = parent;

    /* 将 phys_region 加入 phys_block 的引用链表（头插法） */
    newphysr->next_ph_list = newpb->firstregion;
    newpb->firstregion = newphysr;);

    /* 增加引用计数 */
    newpb->refcount++;
}
```

**参数说明**

- `newpb`：要引用的 `phys_block`
- `offset`：在 `vir_region` 中的偏移量（页对齐）
- `region`：所属的 `vir_region`
- `memtype`：内存类型（anon、file、cache 等）
- 返回值：新创建的 `phys_region`，或 NULL（失败）

**引用建立流程**

1. `SLABALLOC(newphysr)`：分配 `phys_region` 结构体
2. `newphysr->memtype = memtype`：设置内存类型，决定此物理区域的管理策略
3. `pb_link(newphysr, newpb, offset, region)`：建立引用关系——设置 `phys_region` 字段、头插法加入链表、`refcount++`
4. `physblock_set(region, offset, newphysr)`：将 `phys_region` 记录在 `vir_region` 的 `physblocks[]` 数组中

**pb_link 链表操作详解**

```
链表插入操作（头插法）：

插入前：`firstregion → pr1 → pr2 → NULL`，`refcount = 2`

插入后：`firstregion → newphysr → pr1 → pr2 → NULL`，`refcount = 3`

代码：
```c
newphysr->next_ph_list = newpb->firstregion;  // 指向旧头
newpb->firstregion = newphysr;                // 成为新头
newpb->refcount++;                            // 计数+1
```
```

**使用场景**

```c
/* 场景 1: 创建新的物理区域（简化示例） */
struct phys_block *pb = pb_new(phys_addr);
struct phys_region *pr = pb_reference(pb, 0x1000, region, &mem_type_anon);
/* pb->refcount = 1 */

/* 场景 2: fork 共享物理页（简化示例） */
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

| 方面 | `pb_reference()` | `pb_link()` |
|------|-------------------|-------------|
| 层级 | 高层接口 | 低层接口 |
| 分配 | 分配 `phys_region` | 不分配（需要已分配） |
| 操作 | 调用 `pb_link` + 更新 `vir_region` | 只建立链接关系 |
| 用途 | 创建新引用 | 内部操作（如 CoW 复制后重新链接） |


---

######## 2.3.2 pb_unreferenced - 减少引用

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
    
    /* 减少引用计数（USE 宏在 SANITYCHECKS 构建中执行 slabunlock/slablock） */
    USE(pb, pb->refcount--;);

    /* 从链表中移除 phys_region */
    if(pb->firstregion == pr) {
        /* pr 是链表头 */
        USE(pb, pb->firstregion = pr->next_ph_list;);
    } else {
        /* pr 在链表中间，需要遍历查找 */
        struct phys_region *others;

        for(others = pb->firstregion; others;
            others = others->next_ph_list) {
            assert(others->ph == pb);
            if(others->next_ph_list == pr) {
                USE(others, others->next_ph_list = pr->next_ph_list;);
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

- `region`：`phys_region` 所属的 `vir_region`
- `pr`：要取消引用的 `phys_region`
- `rm`：是否从 `vir_region` 中移除（0 = 不移除，用于 CoW 重新链接；1 = 移除，用于 munmap/exit）

**引用释放流程**

1. 减少引用计数：`pb = pr->ph; assert(pb->refcount > 0); USE(pb, pb->refcount--;);`
2. 从链表中移除 `phys_region`：若 `pr` 是链表头则 `USE(pb, pb->firstregion = pr->next_ph_list;)`，否则遍历链表查找并移除
3. 检查是否需要释放：若 `pb->refcount == 0`，调用 `pr->memtype->ev_unreference(pr)` 释放物理页，然后 `SLABFREE(pb)` 释放结构体
4. 清理 `phys_region`：`pr->ph = NULL;` 若 `rm` 为真则 `physblock_set(region, pr->offset, NULL)`

**链表移除操作详解**

情况 1：pr 是链表头

移除前：`firstregion → pr → pr2 → pr3 → NULL`
移除后：`firstregion → pr2 → pr3 → NULL`

代码：`pb->firstregion = pr->next_ph_list;`

情况 2：pr 在链表中间

移除前：`firstregion → pr1 → pr → pr3 → NULL`
移除后：`firstregion → pr1 → pr3 → NULL`

代码：
```c
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

- `rm = 1`（移除）：用于 munmap（完全移除映射）、exit（进程退出清理所有映射），会调用 `physblock_set(region, offset, NULL)` 从 `vir_region` 中移除
- `rm = 0`（不移除）：用于 CoW（取消旧引用但保留 `phys_region` 结构），后续会 `pb_link` 到新的 `phys_block`，`physblock_set` 不被调用

CoW 示例：`mem_cow()` 中 `pb_unreferenced(region, ph, 0)` → `pb_link(ph, new_pb, ...)`

**使用场景**

```c
/* 场景 1: munmap（简化示例） */
void region_free(struct vir_region *region) {
    for (each phys_region pr in region) {
        pb_unreferenced(region, pr, 1);  /* rm=1, 移除 */
        SLABFREE(pr);
    }
}

/* 场景 2: 进程退出（简化示例） */
void vm_proc_cleanup(struct vmproc *vmp) {
    for (each region in vmp) {
        for (each phys_region pr in region) {
            pb_unreferenced(region, pr, 1);  /* rm=1 */
        }
    }
}

/* 场景 3: CoW 写时复制（简化示例，非源码引用） */
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

**引用计数状态变化示例**

初始状态：`pb->refcount = 3`，链表 `firstregion → pr1 → pr2 → pr3 → NULL`

1. `pb_unreferenced(region, pr2, 1)`：`refcount = 2`，链表 `firstregion → pr1 → pr3 → NULL`
2. `pb_unreferenced(region, pr1, 1)`：`refcount = 1`，链表 `firstregion → pr3 → NULL`
3. `pb_unreferenced(region, pr3, 1)`：`refcount = 0`，`firstregion = NULL` → 调用 `memtype->ev_unreference(pr3)` → `SLABFREE(pb)`


---

### 2.4 物理块链表

**链表结构概述**

`phys_block` 通过 `firstregion` 指针维护一个 `phys_region` 链表，记录所有引用此物理块的虚拟区域。链表使用头插法，`pb_link()` 将新 `phys_region` 插入链表头部，`pb_unreferenced()` 从链表中移除指定节点。

链表核心不变量：**`refcount` 始终等于链表长度**。

**链表的作用**

1. **引用计数验证**：`refcount` 应该等于链表长度，可用于调试和一致性检查
2. **遍历所有引用者**：找出哪些进程/区域引用此物理页，用于调试和诊断
3. **CoW 判断**：检查 `refcount > 1` 判断是否需要 CoW，快速判断是否为共享页

**链表操作时间复杂度**

| 操作 | 时间复杂度 | 说明 |
|------|-----------|------|
| 头部插入 | O(1) | `pb_link()` |
| 头部删除 | O(1) | `pb_unreferenced()`（pr 是链表头时） |
| 中间删除 | O(n) | `pb_unreferenced()`（需遍历查找） |
| 遍历链表 | O(n) | 调试/诊断 |

n = refcount，通常很小（1-5），所以 O(n) 可接受。

**链表与 CoW 的关系**

fork 后共享状态：`phys_block.refcount = 2`，`firstregion → parent_pr → child_pr → NULL`

写操作触发 CoW：检查 `pb->refcount > 1` → 分配新物理页 → 复制内容 → `pb_unreferenced()` 减少原块引用 → `pb_link()` 链接到新块。写入后原块 `refcount = 1`，新块 `refcount = 1`。

---

## 3. Rust 设计决策

### 3.1 PhysBlock 结构

**设计目标**

将 Minix3 的 `phys_block` 封装为安全的 Rust 结构体，需要解决以下问题：

1. **引用计数安全**：C 代码手动管理 refcount，容易出错；Rust 需要保证引用计数的正确性
2. **链表安全**：`firstregion` 链表涉及裸指针，需要在 unsafe 块中操作，但提供安全接口
3. **生命周期管理**：`phys_block` 生命周期由引用计数决定，不能简单使用 Rust 的所有权模型
4. **与 phys_region 的双向引用**：`phys_block → phys_region`（firstregion 链表）、`phys_region → phys_block`（ph 指针），需要使用裸指针避免循环

**结构体定义**

```rust
pub(crate) struct PhysBlock {
    phys: PhysBytes,
    refcount: u16,
    flags: PhysBlockFlags,
    first_region: Option<NonNull<PhysRegion>>,
}
```

**为什么 phys 使用 PhysBytes 而非裸 u64**

Minix3 的 `phys_bytes` 在 32 位系统上是 `u32_t`，在 64 位系统上应为 `u64`。Rust 版本使用 `PhysBytes` newtype（`PhysBytes(pub u64)`）而非裸 `u64`，原因：

1. **类型安全**：`PhysBytes` 与 `VirBytes` 是不同类型，编译期防止物理地址和虚拟地址混用
2. **可读性**：函数签名中 `PhysBytes` 比 `u64` 更清晰地表达"这是一个物理地址"
3. **与 minix_types 一致**：`PhysBytes` 和 `VirBytes` 定义在 `minix_types` crate 中，全项目统一使用

`PhysBlock` 内部定义了 `MAP_NONE` 常量：

```rust
pub(crate) const MAP_NONE: PhysBytes = PhysBytes(0xFFFF_FFFF_FFFF_FFFE);
```

注意：Minix3 C 代码中 `MAP_NONE` 为 `0xFFFFFFFE`（32 位），Rust 版本扩展为 64 位值 `0xFFFF_FFFF_FFFF_FFFE`，语义相同——全 1 末位 0，表示无效物理地址。

**为什么 refcount 使用 u16 而非 u8**

Minix3 使用 `u8_t`（0-255），但实际场景中 refcount 很少超过 10。Rust 版本使用 `u16`（0-65535），原因：

1. **更大的安全裕量**：`u8` 的 255 上限在某些极端共享场景下可能不足（如大量进程通过 shm 共享同一页）
2. **对齐友好**：`u16` 在结构体中对齐更自然，避免 padding
3. **saturating_add 仍适用**：溢出保护逻辑不变，只是上限从 255 提升到 65535

**为什么 first_region 使用 Option\<NonNull\<PhysRegion\>\> 而非 Option\<*mut PhysRegion\>**

`NonNull<T>` 保证指针非空（当 `Some` 时），比 `*mut T` 提供更强的语义保证：

1. **非空保证**：`NonNull::new()` 返回 `Option<NonNull<T>>`，强制检查 null
2. **协变**：`NonNull<T>` 是协变的，`*mut T` 是不变的，协变更灵活
3. **与 Rust 惯用法一致**：`Option<NonNull<T>>` 是 Rust 中表示"可空的非空指针"的标准模式

**为什么 flags 使用 bitflags 宏而非手动 newtype**

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(crate) struct PhysBlockFlags: u8 {
        const IN_CACHE = 0x01;
    }
}
```

`bitflags!` 宏自动生成 `contains`、`insert`、`remove` 等方法，以及 `Debug`、`PartialEq` 等 trait 实现，比手动 newtype 更简洁、更规范。Minix3 中 `PBF_INCACHE = 0x01`，Rust 版本保持相同值。

**为什么操作方法放在 PhysRegion 而非 PhysBlock 上**

Minix3 中 `pb_link()` 和 `pb_unreferenced()` 是独立函数，操作涉及 `PhysBlock` 和 `PhysRegion` 双方。Rust 实现将这些操作放在 `PhysRegion` 上（`link_to_block`、`unlink_from_block`），原因：

1. **所有权语义**：`PhysRegion` 是链表节点，"链接到块"和"从块解链"是节点的行为，不是块的行为
2. **借用一致性**：`PhysRegion` 持有 `&mut self` 时可以安全修改自身字段，同时通过 unsafe 修改 `PhysBlock` 的链表头和 refcount
3. **封装性**：`PhysBlock` 的 `add_ref`/`release_ref` 方法是内部实现细节，由 `PhysRegion` 的方法调用

**与 Minix3 的对应**

| Minix3 字段 | Rust 字段 | 说明 |
|------------|----------|------|
| `phys_bytes phys` | `phys: PhysBytes` | 物理地址，newtype 包装 |
| `struct phys_region *firstregion` | `first_region: Option<NonNull<PhysRegion>>` | 链表头，NonNull 保证非空 |
| `u8_t refcount` | `refcount: u16` | u16 提供更大安全裕量 |
| `u8_t flags` | `flags: PhysBlockFlags` | bitflags 宏生成 |
| `u32_t seencount` | 不实现 | 仅调试用 |

> **Direct Map 标注**：PhysBlock 的 `phys: PhysBytes` 字段存储物理地址。在方案三中，访问该物理页的内容需要通过 PtRegion 分配 VA 并建立映射；在 Direct Map 方案中，`vm_phys_to_virt(phys)` 一行加法即可获得 VA。PhysBlock 本身不持有 VA——VA 总是通过 `vm_phys_to_virt()` 按需计算，与 PhysBlock 的生命周期无关。这是 direct map 统一性的又一个例证：物理地址是唯一的"锚点"，VA 是物理地址的派生值。

**PhysBlock 的核心方法**

```rust
impl PhysBlock {
    pub(crate) const MAP_NONE: PhysBytes = PhysBytes(0xFFFF_FFFF_FFFF_FFFE);

    pub(crate) fn new(phys: PhysBytes) -> Self {
        debug_assert!(phys.0 != 0, "PhysBlock::new(0) is likely a bug; use PhysBlock::new(MAP_NONE) for unmapped blocks");
        Self {
            phys,
            refcount: 0,
            flags: PhysBlockFlags::empty(),
            first_region: None,
        }
    }

    pub(crate) fn phys(&self) -> PhysBytes {
        self.phys
    }

    pub(crate) fn set_phys(&mut self, phys: PhysBytes) {
        self.phys = phys;
    }

    pub(crate) fn is_mapped(&self) -> bool {
        self.phys != Self::MAP_NONE
    }

    pub(crate) fn refcount(&self) -> u16 {
        self.refcount
    }

    pub(crate) fn add_ref(&mut self) {
        self.refcount = self.refcount.saturating_add(1);
    }

    pub(crate) fn release_ref(&mut self) -> bool {
        if self.refcount > 0 {
            self.refcount -= 1;
        }
        self.refcount > 0
    }

    pub(crate) fn is_in_cache(&self) -> bool {
        self.flags.contains(PhysBlockFlags::IN_CACHE)
    }
}
```

**为什么 add_ref 使用 saturating_add 而非 panic**

`saturating_add` 在溢出时饱和到 65535 而非 panic，与 Minix3 的 `u8_t` 自溢出行为一致。Minix3 的 `pb->refcount++` 在 u8 溢出时也是静默回绕（实际场景 refcount 极少超过 10）。`debug_assert` 可在调试构建中检测溢出，但生产构建不应 panic。

**为什么 release_ref 返回 bool 而非新 refcount**

`release_ref` 返回 `bool` 表示"块是否仍有引用"（`refcount > 0`），而非返回新的 refcount 值。调用者只需知道"是否可以释放"，不需要具体数值。这与 Minix3 中 `pb_unreferenced` 检查 `refcount == 0` 的逻辑对应。


---

###### 3.2 引用计数模式

**手动管理 vs Arc**

在 Rust 中实现引用计数有两种主要方式：

| 方面 | `Arc<T>`（标准库） | 手动管理（`u16`） |
|------|---------------------|------------------------|
| 自动管理 | 自动增减，无需手动 | 需要手动调用 add_ref/release_ref |
| 链表遍历 | 不支持 | 支持 first_region 链表 |
| Drop | 自动释放，与 VM 生命周期管理冲突 | 完全控制释放时机 |
| 内存开销 | 额外 strong/weak count | 无额外开销 |
| 线程安全 | Arc 线程安全 | 单线程，无需原子 |

选择手动管理（方案2）的原因：Minix3 的设计需要链表遍历能力；VM 是单线程环境，不需要 Arc 的线程安全；需要与 PhysRegion 的链表操作集成。

**为什么不用 Arc**

1. **无法实现链表遍历**：Arc 无法提供遍历所有引用者的能力，而 Minix3 的 `firstregion` 链表是核心功能
2. **Drop 自动释放**：当所有 Arc clone 都 drop 时，PhysBlock 自动释放，但 VM 需要控制释放时机（如调用 memtype 回调）
3. **无法与链表集成**：Arc 的引用计数是内部的，无法与 `first_region` 链表保持同步

**引用计数不变量**

1. **refcount >= 0**：`release_ref` 在 refcount 为 0 时不减少，避免下溢
2. **refcount == 链表长度**：每次 `link_to_block` 增加 refcount，每次 `unlink_from_block` 减少 refcount，可通过 `iterate_block_refs` 验证
3. **refcount == 0 时可以释放**：此时 `first_region` 必须为 None，物理页可以归还
4. **refcount > 1 时需要 CoW**：`PhysRegion::needs_cow()` 检查 `refcount > 1`

**与 Minix3 的对比**

| 方面 | Minix3 (C) | Rust |
|------|-----------|------|
| 引用计数类型 | `u8_t` | `u16` |
| 增加 | `pb->refcount++` | `self.refcount.saturating_add(1)` |
| 减少 | `pb->refcount--` | `if self.refcount > 0 { self.refcount -= 1 }` |
| 溢出检查 | 无 | saturating_add 饱和 |
| 下溢检查 | assert(refcount > 0) | 条件判断避免下溢 |
| 一致性验证 | SANITYCHECKS 宏 | debug_assert + iterate_block_refs |


---

### 3.3 内存分配策略

**Minix3 的 Slab 分配器**

Minix3 使用通用 Slab 分配器管理 `phys_block` 和 `phys_region` 结构体：

```c
#define SLABALLOC(var) (var = slaballoc(sizeof(*var)))
#define SLABFREE(ptr) do { slabfree(ptr, sizeof(*(ptr))); (ptr) = NULL; } while(0)
```

Slab 分配器优势：固定大小分配 O(1)、减少内存碎片、缓存友好。Minix3 使用通用 `slaballoc()` 按 `sizeof(*var)` 自动路由到对应大小的 slab 池，不为 `phys_block` 单独声明专用 slab。

**为什么 Rust 实现不使用 Slab**

Rust 实现当前使用 `Box<PhysBlock>` 和 `Vec<Option<Box<PhysRegion>>>` 管理物理块和物理区域，而非 Slab 分配器，原因：

1. **Rust 所有权模型**：`Box<T>` 提供堆分配和自动 Drop，与 Rust 所有权系统天然集成；Slab 分配器需要手动管理生命周期，与 Rust 安全模型冲突
2. **开发阶段优先正确性**：当前阶段优先保证逻辑正确性，Slab 优化可后续引入；`Box` 分配在功能上等价，性能差异在开发阶段可接受
3. **VirRegion 的 physblocks 字段**：使用 `Vec<Option<Box<PhysRegion>>>` 存储，Vec 自动管理内存，无需手动 Slab
4. **PhysBlock 的存储**：PhysBlock 通过 `Box::new(PhysBlock::new(phys))` 分配，指针存储在 PhysRegion 的 `ph` 字段中

**未来可能的 Slab 集成**

如果性能分析表明 `Box` 分配成为瓶颈，可以引入 Slab 分配器：

1. 实现 `PhysBlockSlab` 类型，内部维护预分配的 PhysBlock 池
2. 使用 `unsafe` 实现 `alloc`/`dealloc` 方法，返回 `&mut PhysBlock` 引用
3. 替换 `Box::new()` 为 `slab.alloc()`，替换 `drop(Box)` 为 `slab.dealloc()`

但这需要谨慎处理：Slab 分配的 PhysBlock 不能实现 Drop（否则 double free）；需要确保所有引用在 dealloc 前清除；需要处理 Slab 耗尽的情况。

**与 Minix3 的对比**

| 方面 | Minix3 | Rust |
|------|--------|------|
| 分配方式 | 通用 `slaballoc()` | `Box::new()` |
| 释放方式 | `SLABFREE(ptr)` | 自动 Drop 或手动 |
| PhysBlock 存储 | SLAB 分配的裸指针 | `Box<PhysBlock>` 的裸指针 |
| PhysRegion 存储 | SLAB 分配的裸指针 | `Vec<Option<Box<PhysRegion>>>` |
| 碎片管理 | Slab 自动管理 | 依赖 Rust 全局分配器 |

---

## 4. 实现详解

### 4.1 创建物理块

**创建流程**

```rust
impl PhysBlock {
    pub(crate) fn new(phys: PhysBytes) -> Self {
        debug_assert!(phys.0 != 0, "PhysBlock::new(0) is likely a bug; use PhysBlock::new(MAP_NONE) for unmapped blocks");
        Self {
            phys,
            refcount: 0,
            flags: PhysBlockFlags::empty(),
            first_region: None,
        }
    }
}
```

`PhysBlock::new()` 对应 Minix3 的 `pb_new()`，初始化 `refcount = 0`、`first_region = None`、`flags = empty()`。`debug_assert` 检查 `phys.0 != 0`，因为 0 既不是有效物理地址也不是 `MAP_NONE`，很可能是编程错误。

**PhysRegion 创建**

```rust
impl PhysRegion {
    pub(crate) fn new(offset: VirBytes) -> Self {
        Self {
            ph: None,
            parent: None,
            offset,
            memtype: None,
            next_ph_list: None,
        }
    }

    pub(crate) fn with_memtype(offset: VirBytes, memtype: &'static dyn MemType) -> Self {
        Self {
            ph: None,
            parent: None,
            offset,
            memtype: Some(memtype),
            next_ph_list: None,
        }
    }
}
```

`PhysRegion::new()` 创建未链接的物理区域，`with_memtype()` 同时设置内存类型。对应 Minix3 中 `SLABALLOC(newphysr)` + `newphysr->memtype = memtype` 的组合。

**在 VirRegion 中使用**

```rust
impl VirRegion {
    pub(crate) fn set_phys_region(&mut self, offset: VirBytes, region: PhysRegion) {
        let page = (offset.get() / 4096) as usize;
        if page < self.physblocks.len() {
            self.physblocks[page] = Some(Box::new(region));
        }
    }
}
```

VirRegion 使用 `Vec<Option<Box<PhysRegion>>>` 存储物理区域，对应 Minix3 的 `physblocks[]` 数组和 `physblock_set()` 函数。


---

###### 4.2 引用管理

**链接操作：link_to_block**

```rust
impl PhysRegion {
    pub(crate) unsafe fn link_to_block(
        &mut self, block: NonNull<PhysBlock>, parent: NonNull<VirRegion>, offset: VirBytes
    ) {
        debug_assert!(self.ph.is_none(), "PhysRegion must not already be in a list");

        self.offset = offset;
        self.ph = Some(block);
        self.parent = Some(parent);

        let block_ptr = block.as_ptr();
        self.next_ph_list = unsafe { (*block_ptr).first_region };
        unsafe {
            (*block_ptr).first_region = NonNull::new(self as *mut PhysRegion);
            (*block_ptr).refcount = (*block_ptr).refcount.saturating_add(1);
        }

        debug_assert!(unsafe { (*block_ptr).refcount } > 0, "refcount must be positive after link");
        debug_assert!(self.ph.is_some(), "ph must be set after link");
    }
}
```

对应 Minix3 的 `pb_link()`，实现头插法链表插入和 refcount 递增。关键步骤：

1. 设置 `self.offset`、`self.ph`、`self.parent` 字段
2. 头插法：`self.next_ph_list = block.first_region`，`block.first_region = NonNull::new(self)`
3. `block.refcount` 使用 `saturating_add(1)` 递增
4. `debug_assert` 验证链接后状态一致

**简化链接：bind_block**

```rust
impl PhysRegion {
    pub(crate) unsafe fn bind_block(&mut self, block: NonNull<PhysBlock>) {
        self.ph = Some(block);
        unsafe {
            (*block.as_ptr()).add_ref();
        }
    }
}
```

`bind_block` 是 `link_to_block` 的简化版本，只设置 `ph` 指针和递增 refcount，不操作链表。用于不需要链表遍历的场景（如 fork 时的引用共享）。

**解链操作：unlink_from_block**

```rust
impl PhysRegion {
    pub(crate) fn unlink_from_block(&mut self) -> bool {
        if let Some(block) = self.ph {
            unsafe {
                let block_ptr = block.as_ptr();
                debug_assert!((*block_ptr).refcount > 0, "refcount must be positive before unlink");
                debug_assert!(self.is_in_list(block), "PhysRegion must be in the list");

                (*block_ptr).refcount = (*block_ptr).refcount.saturating_sub(1);

                let self_ptr = NonNull::new(self as *mut PhysRegion);
                if (*block_ptr).first_region == self_ptr {
                    (*block_ptr).first_region = self.next_ph_list;
                } else if let Some(first) = (*block_ptr).first_region {
                    let mut current = first;
                    loop {
                        let next = (*current.as_ptr()).next_ph_list;
                        if next == self_ptr {
                            (*current.as_ptr()).next_ph_list = self.next_ph_list;
                            break;
                        }
                        match next {
                            Some(n) => current = n,
                            None => {
                                debug_assert!(false, "PhysRegion not found in list");
                                break;
                            }
                        }
                    }
                }

                self.ph = None;
                self.next_ph_list = None;

                debug_assert!(self.ph.is_none(), "ph must be None after unlink");
                debug_assert!(self.next_ph_list.is_none(), "next_ph_list must be None after unlink");

                (*block_ptr).refcount == 0
            }
        } else {
            false
        }
    }
}
```

对应 Minix3 的 `pb_unreferenced()` 中链表移除和 refcount 递减部分。返回 `bool` 表示 PhysBlock 的 refcount 是否为 0（即是否应该释放）。

关键步骤：
1. `refcount` 使用 `saturating_sub(1)` 递减
2. 从链表中移除 self：如果是头节点则直接替换头，否则遍历查找
3. 清除 `self.ph` 和 `self.next_ph_list`
4. 返回 `block.refcount == 0`

**简化解链：unbind_block**

```rust
impl PhysRegion {
    pub(crate) fn unbind_block(&mut self) -> bool {
        if let Some(block) = self.ph {
            unsafe {
                let has_more_refs = (*block.as_ptr()).release_ref();
                self.ph = None;
                has_more_refs
            }
        } else {
            false
        }
    }
}
```

`unbind_block` 是 `unlink_from_block` 的简化版本，只递减 refcount 和清除 `ph`，不操作链表。返回 `bool` 表示 PhysBlock 是否仍有引用。

**链表遍历：iterate_block_refs**

```rust
impl PhysRegion {
    pub(crate) fn iterate_block_refs<F>(block: &PhysBlock, mut f: F)
    where
        F: FnMut(&PhysRegion),
    {
        let mut current = block.first_region;
        while let Some(ptr) = current {
            unsafe {
                let region = &*ptr.as_ptr();
                f(region);
                current = region.next_ph_list;
            }
        }
    }
}
```

遍历 PhysBlock 的 `first_region` 链表，对每个 PhysRegion 执行闭包。用于调试和一致性验证。

### 4.3 释放策略

**释放条件**

PhysBlock 只能在 refcount == 0 时释放。释放前必须满足：1. refcount == 0；2. first_region == None；3. 所有引用都已取消。

释放步骤：1. 调用 `memtype.on_unreference()`（由回调决定是否释放物理页）；2. 释放 PhysBlock（Drop 或手动）

**MemType trait 的 on_unreference 回调**

```rust
pub(crate) trait MemType: Send + Sync {
    fn on_unreference(&self, _pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
    }
}
```

`on_unreference` 返回 `Result<bool, MemTypeError>`：
- `Ok(true)`：物理页需要释放（refcount 为 0 且有物理页）
- `Ok(false)`：不需要释放物理页
- `Err(...)`：释放失败

对应 Minix3 的 `memtype->ev_unreference(pr)` 回调，但语义不同：Minix3 回调直接释放物理页，Rust 版本返回是否需要释放，由调用者执行释放。

**AnonymousMemory 的 on_unreference**

```rust
impl MemType for AnonymousMemory {
    fn on_unreference(&self, pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        let refcount = pr.get_refcount().unwrap_or(0);
        if refcount == 0 && pr.get_phys_addr().unwrap_or(PhysBlock::MAP_NONE) != PhysBlock::MAP_NONE {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
```

匿名内存在 refcount 为 0 且有物理页时返回 `Ok(true)`，表示需要释放物理页。对应 Minix3 的 `anon_unreference()`。

**SharedMemory 的 on_unreference**

```rust
impl MemType for SharedMemory {
    fn on_unreference(&self, _pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
    }
}
```

共享内存永不释放物理页（由 shm 系统调用管理），始终返回 `Ok(false)`。对应 Minix3 的 `shared_unreference()`。

**DirectPhysical 的 on_unreference**

```rust
impl MemType for DirectPhysical {
    fn on_unreference(&self, _pr: &mut PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
    }
}
```

直接物理映射不释放物理页，始终返回 `Ok(false)`。

**释放流程**

Minix3 的 `pb_unreferenced()` 在 refcount == 0 时调用 `memtype->ev_unreference(pr)` 释放物理页，然后 `SLABFREE(pb)` 释放结构体。Rust 实现中，释放逻辑由调用者协调：

1. 调用 `phys_region.unlink_from_block()` 或 `unbind_block()` 获取 refcount 状态
2. 如果 refcount == 0，调用 `memtype.on_unreference(&mut pr)` 判断是否需要释放物理页
3. 如果需要释放，执行物理页释放
4. PhysBlock 随 Box Drop 自动释放（或由调用者手动处理）

**与 Minix3 的对比**

| 方面 | Minix3 | Rust |
|------|--------|------|
| 释放回调 | `memtype->ev_unreference(pr)` 直接释放 | `memtype.on_unreference(&mut pr)` 返回是否需要释放 |
| 回调返回值 | `int`（OK/错误） | `Result<bool, MemTypeError>` |
| 物理页释放 | 回调内部 `free_mem()` | 调用者根据返回值决定 |
| 结构体释放 | `SLABFREE(pb)` | Box Drop 或手动 |


---

### 4.4 线程安全

**单线程假设**

Minix3 的 VM 是单线程的，不需要原子操作。Rust 实现同样遵循单线程假设，使用普通 `u16` 而非 `AtomicU16`。

- Minix3 VM：单线程事件循环，无并发访问，refcount 使用普通 `u8_t`
- Rust 实现：遵循单线程假设，使用普通 `u16`，与 Minix3 保持一致

> ⚠️ 如果未来扩展为多线程，需要将 `u16` 替换为 `AtomicU16`，并重新评估所有 unsafe 代码的安全性。`MemType` trait 已要求 `Send + Sync`，为多线程扩展预留了基础。

**MemType 的 Send + Sync 约束**

```rust
pub(crate) trait MemType: Send + Sync {
    // ...
}
```

`MemType` trait 要求实现者满足 `Send + Sync`，这意味着：
- `Send`：可以安全地跨线程转移所有权
- `Sync`：可以安全地在线程间共享引用

当前 VM 是单线程的，这两个约束不会带来实际开销，但为未来多线程扩展预留了安全性保证。`AnonymousMemory`、`DirectPhysical`、`SharedMemory` 都是零大小类型（ZST），天然满足 `Send + Sync`。

**PhysRegion 的裸指针与线程安全**

PhysRegion 中使用 `Option<NonNull<PhysBlock>>` 等指针字段，`NonNull` 本身不满足 `Send` 和 `Sync`（因为裸指针是 `!Send + !Sync`），所以包含 `NonNull` 的结构体自动成为 `!Send + !Sync`。在单线程环境中这不是问题，但如果扩展为多线程：

1. `NonNull<PhysBlock>` 需要替换为 `AtomicPtr<PhysBlock>` 或用 Mutex 保护
2. `refcount: u16` 需要替换为 `AtomicU16`
3. `first_region: Option<NonNull<PhysRegion>>` 需要替换为 `AtomicPtr<PhysRegion>`
4. 所有 unsafe 块需要重新评估数据竞争风险

**线程安全总结**

1. **当前实现**：与 Minix3 保持一致，适用于单线程环境，更好的性能
2. **MemType trait**：已要求 `Send + Sync`，为多线程扩展预留
3. **裸指针**：PhysRegion/PhysBlock 中的裸指针在单线程中安全，多线程需要原子化
4. **建议**：保持与 Minix3 一致的单线程假设；如需多线程，先原子化 refcount 和 first_region

---

## 5. 与 CoW 的关系

### 5.1 共享物理页

**fork 后的共享状态**

fork 系统调用创建子进程时，父子进程共享物理页：

- fork 前：父进程 `vir_region.physblocks[0]` → `phys_block`（`refcount=1`，`first_region → parent_pr`）
- fork 后：父子进程的 `phys_region` 都引用同一个 `phys_block`（`refcount=2`，`first_region → child_pr → parent_pr`），页表项标记为只读

**共享的实现**

fork 时共享物理页通过 `clone_region_for_fork()` 和 `link_phys_blocks()` 实现：

```rust
fn clone_region_for_fork(original: &VirRegion) -> VirRegion {
    let mut new_region = VirRegion::new(original.vaddr, original.length, original.flags);
    new_region.def_memtype = original.def_memtype;
    new_region.remaps = original.remaps;
    new_region.id = original.id;
    new_region.param = original.param.clone();
    
    for (i, phys_opt) in original.physblocks.iter().enumerate() {
        if let Some(phys) = phys_opt {
            let offset = VirBytes((i as u64) * 4096);
            let mut new_phys = PhysRegion::new(offset);
            new_phys.ph = phys.ph;
            new_phys.memtype = phys.memtype;
            new_region.physblocks[i] = Some(Box::new(new_phys));
        }
    }
    
    new_region
}

unsafe fn link_phys_blocks(region: &mut VirRegion) {
    let parent_ptr = NonNull::from(&*region);
    for phys_opt in region.physblocks.iter_mut() {
        if let Some(phys) = phys_opt.as_mut() {
            if let Some(block_ptr) = phys.ph {
                phys.link_to_block(block_ptr, parent_ptr, phys.offset);
            }
        }
    }
}
```

`clone_region_for_fork` 复制 VirRegion 的元数据，并将子进程的 PhysRegion 的 `ph` 指向父进程的同一个 PhysBlock。`link_phys_blocks` 遍历子进程的所有 PhysRegion，对每个调用 `link_to_block()` 完整链接（设置 parent、offset、头插法插入链表、递增 refcount）。

注意：`link_phys_blocks` 使用 `link_to_block()` 而非简单的 `add_ref()`，因为 fork 需要完整的链表管理——子进程的 PhysRegion 需要被插入到 PhysBlock 的 `first_region` 链表中，以便后续 CoW 时能遍历所有引用者。

**共享检测**

```rust
impl PhysRegion {
    pub(crate) fn needs_cow(&self) -> bool {
        match self.get_refcount() {
            Some(count) if count > 1 => true,
            _ => false,
        }
    }
}
```

`needs_cow()` 放在 PhysRegion 上而非 PhysBlock 上，因为 CoW 判断需要结合 PhysRegion 的上下文（如 memtype）。对应 Minix3 中检查 `pb->refcount > 1` 的逻辑。


---

### 5.2 写时复制触发

**CoW 触发流程**

1. **写操作尝试**：进程写入共享页，页表项为只读，触发页面保护异常
2. **异常处理**：VM 接收异常，查找对应的 `vir_region` 和 `phys_region`，检查 `needs_cow()`
3. **CoW 复制**：分配新物理页，复制原页内容，更新页表映射，`unbind_block()` 减少原块 refcount，`bind_block()` 链接到新 `phys_block`
4. **继续写入**：进程拥有私有页（`refcount=1`），可以正常写入

**AnonymousMemory 的 pagefault 处理**

```rust
impl MemType for AnonymousMemory {
    fn on_pagefault(
        &self,
        _proc: &ActiveProc<'_>,
        region: &mut crate::region::VirRegion,
        pr: &mut crate::region::PhysRegion,
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
}
```

`on_pagefault` 返回 `PagefaultResult` 枚举，由上层调度器决定如何处理：
- `NeedNewPage`：延迟分配，需要分配新物理页
- `Handled`：页已就绪，无需操作
- `NeedCow`：需要写时复制
- `AccessViolation`：访问违规（SIGSEGV）

对应 Minix3 的 `anon_pagefault()` 函数，但使用枚举返回值而非直接执行 CoW。

**CoW 前后状态对比**

CoW 前（共享状态）：`phys_block_A`（`phys=0x1234000, refcount=2`），`first_region → parent_pr → child_pr`

CoW 后（私有状态）：
- `phys_block_A`（`phys=0x1234000, refcount=1`），`first_region → parent_pr`（父进程保留原页）
- `phys_block_B`（`phys=0x5678000, refcount=1`），`first_region → child_pr`（子进程拥有新页）

> 注意：Minix3 的 `mem_cow()` 中 `pb_unreferenced(region, ph, 0)` 使用 `rm=0`，不移除 `phys_region`，而是后续 `pb_link(ph, pb, ...)` 重新链接到新块。Rust 实现中，CoW 由上层根据 `PagefaultResult::NeedCow` 执行，先 `unbind_block()` 再 `bind_block()` 到新块。

---

## 6. 测试要点

### 6.1 引用计数测试

- **初始状态**：`PhysBlock::new()` 返回 `refcount=0`，`is_mapped()=true`（给定有效地址）
- **增减操作**：`add_ref` 后 `refcount` 递增，`release_ref` 后递减，`release_ref` 返回 `bool` 表示是否仍有引用
- **CoW 判断**：`PhysRegion::needs_cow()` 在 `refcount > 1` 时返回 true
- **饱和行为**：`add_ref` 使用 `saturating_add`，溢出时饱和到 65535；`release_ref` 在 `refcount==0` 时不递减

### 6.2 链表一致性测试

- **refcount 与链表长度一致**：每次 `link_to_block` 后 `refcount` 应等于链表长度，每次 `unlink_from_block` 后也应一致
- **头插法顺序**：链表顺序为后插入的在前（头插法），可通过 `iterate_block_refs` 验证
- **中间节点删除**：从链表中间移除 `PhysRegion` 后，链表仍完整，`refcount` 正确递减

### 6.3 生命周期测试

- **创建和释放**：`PhysBlock::new()` → Box Drop，Vec 自动管理 PhysRegion
- **延迟分配**：`PhysBlock::new(PhysBlock::MAP_NONE)` 创建无物理页的块，`is_mapped()=false`
- **引用生命周期**：`bind_block` → `unbind_block`，完整流程
- **多引用释放**：多个 `PhysRegion` 引用同一 `PhysBlock`，逐个 `unbind_block`，最后一个触发 refcount 归零
- **链表操作**：`link_to_block` → `unlink_from_block`，完整链表管理流程

### 6.4 CoW 测试

- **fork 共享**：`clone_region_for_fork` + `link_phys_blocks` 后 `refcount` 增为 2，`needs_cow()=true`
- **CoW 判定**：`AnonymousMemory::on_pagefault` 在 `refcount >= 2 && write` 时返回 `NeedCow`
- **无泄漏**：fork + CoW 循环后，PhysBlock 和 PhysRegion 数量应与初始一致

### 6.5 MemType 测试

- **AnonymousMemory**：`on_unreference` 在 refcount==0 且有物理页时返回 `Ok(true)`
- **SharedMemory**：`on_unreference` 始终返回 `Ok(false)`
- **DirectPhysical**：`on_unreference` 始终返回 `Ok(false)`

---

## 7. 参见

- [08-slab-allocator.md](08-slab-allocator.md) - 使用 Slab 分配 phys_block
- [09-vir-region.md](09-vir-region.md) - vir_region 通过 physblocks[] 引用 phys_block
- [12-page-fault.md](12-page-fault.md) - 缺页处理与延迟分配（phys=MAP_NONE）
- [14-phys-region.md](14-phys-region.md) - phys_region 连接 vir_region 与 phys_block
- [15-cow-mechanism.md](15-cow-mechanism.md) - 基于 phys_block 的 CoW

---

*分类: VM私有*
