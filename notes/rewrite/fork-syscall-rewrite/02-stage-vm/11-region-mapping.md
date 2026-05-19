# 11-region-mapping: 区域映射 (PageSlot + VirRegion)

> **分类**: VM私有
> **源码**: `minix3/minix/servers/vm/region.h`, `region.c`, `phys_region.h`, `pb.c`
> **前置**: [10-phys-pagestate.md](10-phys-pagestate.md)（PageFrames API）
> **说明**: 虚拟区域到物理页的映射管理，包含 PageSlot 映射记录和 VirRegion 虚拟区域

---

## 1. 概述

### 1.1 区域映射的作用：虚拟地址范围 → 物理页集合

区域映射回答的核心问题是：**虚拟地址范围 [A, B) 由哪些物理页支撑？每页的映射属性是什么？**

进程的虚拟地址空间不是一整块连续内存，而是由多个**区域**（region）组成——代码段、数据段、堆、栈、mmap 区域各是一个区域。每个区域覆盖一段连续虚拟地址，内部按页粒度映射到物理页。区域映射需要支持：

- **按需分配**：区域内的虚拟页可以不立即映射物理页，首次访问时再分配
- **共享**：不同进程的区域可以映射到同一组物理页（fork、共享内存）
- **CoW**：写入共享页时自动复制，保证进程间隔离
- **多种内存类型**：匿名、文件映射、设备映射、缓存等，各有不同行为
- **稀疏映射**：大区域内只有部分页面实际使用，未用页不消耗物理内存

### 1.2 Minix3 的三层结构

Minix3 使用三个独立结构体实现页映射：

```
vir_region (虚拟地址空间区域)
    │
    └── physblocks[] ──→ phys_region* ──→ phys_block
                           (映射记录)      (物理页)
```

| 结构体 | 回答的问题 | 管理粒度 |
|--------|-----------|---------|
| `phys_block` | 物理页 X 现在是什么状态？谁在引用它？ | 每个物理页一个 |
| `phys_region` | 虚拟页 Y 映射到哪个物理页？以什么类型？ | 每个虚拟页一个 |
| `vir_region` | 虚拟地址范围 [A, B) 有什么属性？ | 每个区域一个 |

三层缺一不可——vir_region 定位区域，phys_region 定位映射，phys_block 判断是否需要 CoW。三层在 fork/CoW 中的协作示例、以及整体改造思路见 [10-phys-pagestate.md §1.3-1.4](10-phys-pagestate.md#13-统一的三层系统phys_block--phys_region--vir_region)。本文档聚焦于**映射层**（phys_region + vir_region）。

### 1.3 映射层的子问题

三层结构共同解决的核心问题可分解为五个子问题（[10.md §1.4](10-phys-pagestate.md#14-五个子问题)）。其中映射层负责三个：

| # | 子问题 | Minix3 解决方式 | 涉及结构 |
|---|--------|----------------|---------|
| P1 | 虚拟地址空间分段 | `vir_region` + BTreeMap | `vir_region` |
| P2 | 虚拟页→物理页映射 | `physblocks[]` + `phys_region` | `vir_region`, `phys_region` |
| P4 | 多种内存类型的行为差异 | `mem_type_t` 函数指针表 | `phys_region.memtype` |

另外两个子问题（P3 物理页共享与引用计数、P5 延迟分配与按需填充）由 `phys_block` 承担，见 [10-phys-pagestate.md](10-phys-pagestate.md)。P4 的 memtype 回调设计详见 [12-memtype.md](12-memtype.md)。

**P1：虚拟地址空间分段**

进程的虚拟地址空间不是一整块连续内存，而是由多个 `vir_region` 组成——代码段、数据段、堆、栈、mmap 区域各是一个 `vir_region`。每个 `vir_region` 描述一段连续虚拟地址范围 `[vaddr, vaddr+length)`，拥有独立的属性（可写、匿名、共享等）和内存类型。

Minix3 使用 **AVL 树**按虚拟地址组织同一进程的所有 `vir_region`，支持 O(log n) 的查找、插入和删除。Rust 实现中改用 `BTreeMap<VirBytes, VirRegion>` 替代自定义 AVL 树，提供相同的 O(log n) 语义，同时利用标准库的质量保证，无需在 `VirRegion` 中内嵌树节点字段。详细分析见 [13-region-avl.md](13-region-avl.md)。

**P2：虚拟页→物理页映射**

每个 `vir_region` 内部维护一个 `physblocks[]` 指针数组，每个槽位对应一页虚拟内存：

- 槽位非 NULL → 指向 `phys_region`，该页已映射物理页
- 槽位为 NULL → 该页未分配物理页，首次访问时触发缺页中断

这种设计天然支持**稀疏映射**：一个 1MB 的区域可能只有 100KB 被实际使用，未使用的页保持 NULL，不消耗物理内存。

**P4：多种内存类型的行为差异**

同一 `vir_region` 内的不同页面可以有不同的 `memtype`（虽然大多数情况下使用区域的 `def_memtype`）。memtype 决定了页面在关键事件中的行为：引用时是否设 CoW、取消引用时是否释放、缺页时如何填充等。

### 1.4 vir_region 的宏观角色

`vir_region` 在三层结构中扮演**区域管理者**的角色：

```
进程的虚拟地址空间（vmproc.vm_regions_avl）
  │
  ├── vir_region: 代码段 (vaddr=0x400000, length=0x1000, VR_ANON)
  │     └── physblocks[0] → phys_region → phys_block
  │
  ├── vir_region: 堆 (vaddr=0x600000, length=0x2000, VR_ANON|VR_WRITABLE)
  │     ├── physblocks[0] → phys_region → phys_block
  │     └── physblocks[1] → NULL (按需分配)
  │
  ├── vir_region: mmap 文件 (vaddr=0x7f000, length=0x3000, VR_WRITABLE, def_memtype=mappedfile)
  │     ├── physblocks[0] → phys_region (memtype=mappedfile) → phys_block
  │     ├── physblocks[1] → NULL
  │     └── physblocks[2] → phys_region (memtype=mappedfile) → phys_block
  │
  └── vir_region: 栈 (vaddr=0x7fff000, length=0x1000, VR_ANON|VR_WRITABLE)
        └── physblocks[0] → phys_region → phys_block
```

**区域类型与 param 联合体**

`vir_region` 的 `flags` 字段标记区域类型（匿名、直接映射、共享等），`param` 联合体存储各类型特有参数。两者的对应关系见 §2.5。

**flags vs def_memtype**：`flags` 描述区域的**权限和类型标记**（可写、匿名、共享等），是静态属性；`def_memtype` 描述区域的**行为策略**（引用/取消引用/缺页时怎么做），是动态回调。两者正交：`VR_ANON` 区域的 `def_memtype` 通常是 `mem_type_anon`，但文件映射区域没有专用 flag——它的类型由 `def_memtype == mem_type_mappedfile` 隐式决定。这种"类型标记和行为策略分离"的设计在 Rust 重写中由 `VrFlags` + `VrParam` enum + `MemType` trait 三者分别承担，消除了 C union 的变体歧义。

### 1.5 phys_region 中间层的必要性

直觉上似乎可以让 `vir_region` 直接指向 `phys_block`，省掉 `phys_region` 这个中间层。但中间层解决四个问题：

| 场景 | 没有 phys_region 的问题 | phys_region 的解决方式 |
|------|------------------------|----------------------|
| **共享/CoW** | 一个物理页被多个进程映射，无法区分各映射的属性 | 每个映射有独立 `phys_region`，共享同一 `phys_block` |
| **稀疏映射** | 大区域只有部分页面有物理页，无法表达 NULL | 每页一个槽位，未映射为 NULL |
| **内存类型差异** | 同一区域不同页面可能有不同 memtype | `phys_region.memtype` 可独立于区域的 `def_memtype` |
| **偏移记录** | 需要知道物理页在区域中的偏移 | `phys_region.offset` 记录偏移 |

最典型的场景是 **fork**：父进程和子进程的同一虚拟页各有一个 `phys_region`，但它们指向同一个 `phys_block`（refcount=2）。写入时 CoW 只替换写入方的 `phys_region.ph`，不影响另一方。

> **设计讨论**：页映射的重新建模见第 3 章。

---

## 2. C 源码分析

### 2.1 phys_region 结构体

**定位**：三层映射结构的中间层，连接 `vir_region`（虚拟地址空间）和 `phys_block`（物理页状态）。每个 `phys_region` 代表一个虚拟页到物理页的映射关系，记录该映射的偏移、内存类型回调，并通过侵入式链表与共享同一物理页的其他映射串联。

**源码定义** (`phys_region.h:8`):

```c
typedef struct phys_region {
    struct phys_block   *ph;           // 指向物理块
    struct vir_region   *parent;       // 所属虚拟区域 (或 NULL)
    vir_bytes            offset;       // 在 vir_region 中的偏移
#if SANITYCHECKS
    int                  written;      // 调试: 是否已写入页表
#endif
    mem_type_t          *memtype;      // 内存类型回调
    struct phys_region  *next_ph_list; // 同一 phys_block 的链表
} phys_region_t;
```

**字段详解**:

| 字段 | 类型 | 条件编译 | 说明 |
|------|------|---------|------|
| `ph` | `phys_block*` | - | 指向物理块，可能为 NULL（未分配物理页） |
| `parent` | `vir_region*` | - | 反向指针：所属虚拟区域。sanity check 中验证 `pr->parent == vr`；运行时用于 `mem_anon.c` 检查 `pr->parent->remaps` 判断区域是否正在重映射。源码注释称"yielded 时为 NULL"，但当前代码中无任何路径将 parent 置 NULL |
| `offset` | `vir_bytes` | - | phys_region 在 vir_region 内的页对齐偏移，值 = `physblocks[]` 下标 × `VM_PAGE_SIZE`。`physblocks[]` 是密集指针数组（大小 = 区域长度 / 页大小），`physblock_get` 用 `offset/VM_PAGE_SIZE` 直接下标定位，并断言 `foundregion->offset == offset` 做一致性检查。offset 本质是冗余的（可从数组位置推算），但作为 sanity check 锚点。区域收缩时 offset 随 `memmove` 调整 |
| `written` | `int` | `SANITYCHECKS` | 检测页表条目被意外覆盖：首次 `pt_writemap` 时不带 `WMF_OVERWRITE`（要求 PTE 为空），写入成功后置 1；后续覆盖带 `WMF_OVERWRITE`。fork 时新区域清零。用于发现 VM 不知情的外部页表修改 |
| `memtype` | `mem_type_t*` | - | 内存类型回调（anon、file 等） |
| `next_ph_list` | `phys_region*` | - | 链向同一 phys_block 的其他引用 |

**32位 vs 64位大小差异**：

- 32位（不含 SANITYCHECKS）：ph(4) + parent(4) + offset(4) + memtype(4) + next_ph_list(4) = **20字节**
- 64位（不含 SANITYCHECKS）：ph(8) + parent(8) + offset(8) + memtype(8) + next_ph_list(8) = **40字节**

**关键约束**:

1. **一对一映射**: 每个 `phys_region` 只属于一个 `vir_region`
2. **多对一引用**: 多个 `phys_region` 可引用同一 `phys_block`（共享/COW）
3. **偏移对齐**: `offset` 必须是页大小的倍数
4. **生命周期**: `phys_region` 生命周期由 `vir_region` 管理

### 2.2 vir_region 结构体

**定位**：虚拟地址空间中的一段连续区域，是进程内存映射的管理单元。每个 `vir_region` 拥有一个 `physblocks[]` 密集指针数组，数组中每个槽位对应一页虚拟内存，指向该页的 `phys_region`（或 NULL 表示未映射）。Minix3 中区域通过 AVL 树组织，Rust 实现改用 `BTreeMap<VirBytes, VirRegion>` 管理，支持 O(log n) 的地址查找。

**文件**: `minix3/minix/servers/vm/region.h`

```c
typedef struct vir_region {
    vir_bytes   vaddr;          /* 虚拟地址，页表偏移 */
    vir_bytes   length;         /* 长度（字节） */
    struct phys_region **physblocks;  /* 物理块指针数组 */
    u16_t       flags;          /* 权限和类型标记（VR_WRITABLE|VR_ANON 等），静态属性 */
    struct vmproc *parent;      /* 拥有此区域的进程 */
    mem_type_t  *def_memtype;   /* 行为策略回调（缺页/引用/取消引用），动态属性 */
    int         remaps;         /* 共享映射引用计数：被其他区域引用的次数，refcount = 1 + remaps */
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
        } file;                 /* 文件映射 */
    } param;

    /* AVL 树字段（Rust 实现中不需要，改用 BTreeMap 管理） */
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
| **参数联合体** | `param` | 各类型特有参数（含 `param.file.fdref` 文件引用指针） |
| **AVL 树** | `lower`, `higher`, `factor` | Minix3 自平衡二叉树节点字段，Rust 中由 BTreeMap 外部管理，无需内嵌 |

**与 phys_region 和 phys_block 的关系**:

```
vir_region (vaddr=0x1000, length=0x4000)
    │
    ├── physblocks[0] ──→ phys_region ──→ phys_block (phys=0x8000, refcount=1)
    │                         │
    │                         └─ offset=0, memtype=anon
    │
    ├── physblocks[1] ──→ phys_region ──→ phys_block (phys=0x9000, refcount=2)
    │                         │                  │
    │                         └─ offset=0x1000   └─ firstregion → ...
    │
    ├── physblocks[2] ──→ NULL (未分配物理页)
    │
    └── physblocks[3] ──→ phys_region ──→ phys_block (共享)
            offset=0x3000                      refcount=2
```

**phys_block 的 flags 字段和 PBF_INCACHE**：

`phys_block` 除了 `phys`、`firstregion`、`refcount` 外，还有一个 `flags` 字段（`u8_t`），目前只定义了一个标志位：

```c
#define PBF_INCACHE  0x01
```

`PBF_INCACHE` 表示该物理页正在 mappedfile 的缓存中。**对 refcount 语义的影响**：缓存页的 refcount = 链表长度 + 1（缓存本身算一个引用），非缓存页的 refcount = 链表长度。因此 `pb_unreferenced` 在 refcount 降为 0 时才释放物理页，而 `unmap_page` 在 Rust 重写中需要检查 `IN_CACHE` 标志来决定是否触发 `ev_unreference`（见 §3.3.4）。

**physblocks 数组**:
- 大小 = `length / VM_PAGE_SIZE`
- 每个元素对应一页虚拟内存
- 元素为 `NULL` 表示该页未分配物理内存（按需分配）

### 2.3 两者的交互：pb_reference / physblock_set / map_copy_region

**本节内容**：`phys_region` 和 `vir_region` 不是孤立的数据结构，它们通过三个关键函数协作——`pb_reference` 创建映射引用（分配 phys_region + 链入 phys_block + 设置数组槽位），`physblock_set/get` 操作数组槽位，`map_copy_region` 在 fork 时批量复制整个区域的映射关系。

#### pb_reference — 创建新映射引用

**调用场景**：`map_copy_region`（fork 复制区域）、`split_region`（区域分割）、`map_pf`（缺页创建新映射）等需要为虚拟页建立到 `phys_block` 的映射时调用。

**核心作用**：分配新的 `phys_region`，将其链入 `phys_block` 的引用链表（refcount++），并设置 `vir_region.physblocks[]` 数组的对应槽位。是建立虚拟页→物理页映射关系的核心操作。

**源码位置**: [pb.c:73](minix3/minix/servers/vm/pb.c#L73)

```c
struct phys_region *pb_reference(struct phys_block *pb, vir_bytes offset,
    struct vir_region *parent, mem_type_t *memtype)
{
    struct phys_region *pr;

    if(!(SLABALLOC(pr)))
        return NULL;

    pr->memtype = memtype;
    pb_link(pr, pb, offset, parent);
    physblock_set(parent, offset, pr);

    return pr;
}
```

`pb_reference` = `SLABALLOC`（分配 phys_region）+ 设置 memtype + `pb_link`（插入链表 + refcount++）+ `physblock_set`（设置 physblocks 数组槽位）。这是 fork 和区域分割时创建新映射的核心操作。

#### physblock_set — 设置/清除数组槽位

**调用场景**：`pb_reference` 创建映射时设置槽位、`pb_unreferenced` 移除映射时清除槽位（`rm=1`）。

**核心作用**：`vir_region.physblocks[]` 数组的直接索引访问器。`set` 写入/清除指针，`get` 读取指针。索引 = `offset / VM_PAGE_SIZE`。`set` 时还会更新进程的 `vm_total` 内存统计。

**源码位置**: [region.c:48](minix3/minix/servers/vm/region.c#L48)

```c
void physblock_set(struct vir_region *region, vir_bytes offset,
    struct phys_region *newphysr)
{
    int i;
    struct vmproc *proc;
    assert(!(offset % VM_PAGE_SIZE));
    assert(offset < region->length);
    i = offset / VM_PAGE_SIZE;
    proc = region->parent;
    assert(proc);
    if(newphysr) {
        /* 设置槽位：增加进程内存统计 */
        assert(!region->physblocks[i]);
        assert(newphysr->offset == offset);
        proc->vm_total += VM_PAGE_SIZE;
        if (proc->vm_total > proc->vm_total_max)
            proc->vm_total_max = proc->vm_total;
    } else {
        /* 清除槽位：减少进程内存统计 */
        assert(region->physblocks[i]);
        proc->vm_total -= VM_PAGE_SIZE;
    }
    region->physblocks[i] = newphysr;
}

struct phys_region *physblock_get(struct vir_region *region, vir_bytes offset)
{
    int i = offset / VM_PAGE_SIZE;
    struct phys_region *foundregion = region->physblocks[i];
    if(foundregion)
        assert(foundregion->offset == offset);
    return foundregion;
}
```

`physblock_set` 不仅是数组索引访问器，还维护进程的 `vm_total` 内存统计：设置槽位时 `vm_total += VM_PAGE_SIZE`，清除槽位时 `vm_total -= VM_PAGE_SIZE`。`physblock_get` 则是纯读取，附带 offset 一致性断言。

#### map_copy_region — fork 时复制区域

**调用场景**：`map_proc_copy`（fork 系统调用复制进程地址空间）中，为源进程的每个 `vir_region` 调用，创建子进程的对应区域。

**核心作用**：创建新的 `vir_region`，遍历源区域的所有已映射 `phys_region`，通过 `pb_reference` 为每个虚拟页创建新引用（共享同一 `phys_block`，refcount++），然后可选调用 `ev_reference` 回调（仅 cache 和 anon_contig 实现，anon 无此回调）。CoW 只读保护由 `map_proc_copy_range` 末尾的 `map_writept(src); map_writept(dst);` 实现——共享页（refcount > 1）通过 `pr_writable` → `anon_writable(refcount==1)` 自然变为只读。失败时 `map_free` 整体释放新区域。

**源码位置**: [region.c:802](minix3/minix/servers/vm/region.c#L802)

```c
struct vir_region *map_copy_region(struct vmproc *vmp, struct vir_region *vr)
{
    struct vir_region *newvr;
    struct phys_region *ph;
    vir_bytes p;

    /* 创建新 vir_region，继承源区域的 vaddr/length/flags/def_memtype
     * 注意：region_new 的第一个参数是 vr->parent（源区域的所属进程），
     * 下一行再用 USE 宏修正为 vmp（目标进程） */
    if(!(newvr = region_new(vr->parent, vr->vaddr, vr->length,
                            vr->flags, vr->def_memtype)))
        return NULL;

    /* 修正 parent 为目标进程（绕过 sanity check 限制） */
    USE(newvr, newvr->parent = vmp;);

    /* 调用 memtype 的 ev_copy 回调，处理类型特定的复制逻辑
     * （如共享内存复制 shared_copy、文件映射复制 mappedfile_copy） */
    if(vr->def_memtype->ev_copy &&
       vr->def_memtype->ev_copy(vr, newvr) != OK) {
        map_free(newvr);
        return NULL;
    }

    /* 遍历源区域的所有虚拟页槽位 */
    for(p = 0; p < phys_slot(vr->length); p++) {
        struct phys_region *newph;

        /* 跳过未分配物理页的槽位（稀疏映射） */
        if(!(ph = physblock_get(vr, p*VM_PAGE_SIZE))) continue;

        /* pb_reference 做三件事：
         * 1. SLABALLOC 分配新 phys_region
         * 2. pb_link 将其链入 phys_block 的引用链表（refcount++）
         * 3. physblock_set 设置 newvr->physblocks[p]
         * 注意：新 phys_region 使用 def_memtype 而非源页的 memtype，
         * 由下面的 ev_reference 回调修正 */
        newph = pb_reference(ph->ph, ph->offset, newvr,
                             vr->def_memtype);
        if(!newph) { map_free(newvr); return NULL; }

        /* ev_reference 回调：memtype 特定的引用通知
         * 仅 cache 和 anon_contig 实现（cache_reference 返回 OK，
         * anon_contig_reference 返回 ENOMEM 阻止 fork）。
         * mem_type_anon 无此回调（NULL），CoW 只读保护由
         * map_proc_copy_range 末尾的 map_writept 实现 */
        if(ph->memtype->ev_reference)
            ph->memtype->ev_reference(ph, newph);

#if SANITYCHECKS
        USE(newph, newph->written = 0;);
#endif
    }

    return newvr;
}
```

`map_copy_region` 为源区域的每个已映射 `phys_region` 调用 `pb_reference` 创建新引用（refcount++），并可选调用 `ev_reference` 回调（仅 cache/contig 实现，anon 为 NULL）。CoW 只读保护由 `map_proc_copy_range` 末尾的 `map_writept(src); map_writept(dst);` 实现。注意：

- 函数内部创建目标区域（`region_new`），不接受外部传入的 dst
- `pb_reference` 内部已调用 `physblock_set`，无需额外设置数组槽位
- 新 phys_region 使用区域的 `def_memtype`（而非源页的 `ph->memtype`），`ev_reference` 回调可修正
- 失败时直接 `map_free(newvr)` 整体释放，无需逐个回滚

### 2.4 vir_region 标志位 VR_*

**本节内容**：`vir_region.flags` 是 u16_t 位域，分为高 8 位的**映射类型标志**（决定区域的物理映射方式：匿名/直接/共享/预分配）和低 8 位的**映射属性标志**（决定物理页的分配约束：可写/对齐/低地址/未初始化）。类型标志决定 `param` 联合体使用哪个变体，属性标志影响 `vrallocflags` 生成的物理页分配参数。

**文件**: `minix3/minix/servers/vm/region.h`

Minix3 的区域标志分为两类：**映射类型标志**（高 8 位）和**映射属性标志**（低 8 位）。

#### 2.4.1 映射类型标志

| 标志 | 值 | 说明 |
|------|-----|------|
| `VR_ANON` | `0x100` | 匿名内存，按需分配物理页，分配时清零 |
| `VR_DIRECT` | `0x200` | 直接映射，不由 VM 管理（如设备内存） |
| `VR_PREALLOC_MAP` | `0x400` | 预分配映射，用于 RS（重启服务）保留区域 |

```c
#define VR_ANON         0x100
#define VR_DIRECT       0x200
#define VR_PREALLOC_MAP 0x400
```

- **VR_ANON**: 最常见的类型，用于堆、栈、匿名 mmap。物理页按需分配，首次访问时清零。
- **VR_DIRECT**: 直接物理映射，VM 不管理其物理内存。用于映射设备寄存器或预分配的物理内存。
- **VR_PREALLOC_MAP**: RS 服务专用，用于保存进程镜像，支持快速重启。

#### 2.4.2 映射属性标志

| 标志 | 值 | 说明 |
|------|-----|------|
| `VR_WRITABLE` | `0x001` | 进程可写入此区域 |
| `VR_PHYS64K` | `0x004` | 物理内存必须 64KB 对齐 |
| `VR_LOWER16MB` | `0x008` | 物理内存必须在低 16MB（DMA 兼容） |
| `VR_LOWER1MB` | `0x010` | 物理内存必须在低 1MB（BIOS 兼容） |
| `VR_SHARED` | `0x040` | 共享内存区域 |
| `VR_UNINITIALIZED` | `0x080` | 分配后不清零（性能优化） |

**vrallocflags — VR_* 标志到分配标志的转换**：

`vrallocflags`（[region.c:645](minix3/minix/servers/vm/region.c#L645)）将 `vir_region.flags` 中的映射属性标志转换为物理页分配标志（`PAF_*`），影响 `alloc_mem` 的行为：

```c
u32_t vrallocflags(u32_t flags)
{
    u32_t allocflags = 0;

    if(flags & VR_PHYS64K)
        allocflags |= PAF_ALIGN64K;
    if(flags & VR_LOWER16MB)
        allocflags |= PAF_LOWER16MB;
    if(flags & VR_LOWER1MB)
        allocflags |= PAF_LOWER1MB;
    if(!(flags & VR_UNINITIALIZED))
        allocflags |= PAF_CLEAR;

    return allocflags;
}
```

关键语义：默认情况下（未设 `VR_UNINITIALIZED`），`PAF_CLEAR` 被设置，新分配的物理页会被**清零**。只有显式设置 `VR_UNINITIALIZED` 的区域（如预分配映射）才跳过清零。

Rust 重写中，`VrFlags` 到分配标志的转换封装为 `VrFlags::to_alloc_flags()` 方法，返回 `PageAllocFlags` 类型。

**标志组合示例**:

```c
flags = VR_ANON | VR_WRITABLE;                              // 普通堆区域
flags = VR_ANON | VR_WRITABLE | VR_LOWER16MB | VR_PHYS64K;  // DMA 缓冲区
flags = VR_DIRECT | VR_WRITABLE;                             // 设备寄存器映射
flags = VR_SHARED | VR_WRITABLE;                             // 共享内存
```

### 2.5 vir_region param 联合体

**本节内容**：`vir_region.param` 是 C union，根据区域的映射类型存储不同的附加参数——`VR_DIRECT` 存物理地址，`VR_SHARED` 存源进程端点和虚拟地址，文件映射存 fdref 和偏移量，`mem_type_cache` 存缓存页指针。union 的变体选择由 `flags` 和 `def_memtype` 共同决定，运行时无类型保护。

**文件**: `minix3/minix/servers/vm/region.h`

```c
union {
    phys_bytes phys;                /* VR_DIRECT: 直接物理映射 */
    struct {
        endpoint_t ep;
        vir_bytes vaddr;
        int id;
    } shared;                       /* VR_SHARED: 共享内存 */
    struct phys_block *pb_cache;    /* 物理块缓存 */
    struct {
        int     inited;
        struct fdref *fdref;
        u64_t   offset;
        u16_t   clearend;
    } file;                         /* 文件映射 */
} param;
```

| 变体 | 使用条件 | 字段说明 |
|------|---------|---------|
| `phys` | `VR_DIRECT` | 直接映射的物理地址 |
| `shared` | `VR_SHARED` | 共享内存的源进程端点和虚拟地址 |
| `pb_cache` | `mem_type_cache` | 缺页时暂存缓存页的 phys_block 指针。`cache_pagefault` 中从 `pb_cache` 取出 phys_block 并 `pb_link` 到 phys_region，然后清零 `pb_cache` |
| `file` | `def_memtype == mem_type_mappedfile` | 文件映射参数：fdref、偏移量、clearend |

**注意**：`param` 的变体选择不完全由 `flags` 决定——`file` 变体由 `def_memtype` 决定。这是 C union 的典型风险：运行时才能发现使用了错误的变体。

### 2.6 区域操作

**文件**: `minix3/minix/servers/vm/region.c`

#### 2.6.1 map_page_region — 分配区域

**调用场景**：`mmap`、`brk` 扩展、`fork` 复制地址空间等需要为进程新增虚拟内存区域时调用。是创建 `vir_region` 的唯一入口。

**核心作用**：在进程虚拟地址空间的 `[minv, maxv)` 范围内寻找空闲槽位，创建新的 `vir_region` 并插入区域映射表。物理页按需分配（懒分配），创建时不映射任何物理页。

**源码位置**: `region.c:463`

```c
struct vir_region *map_page_region(struct vmproc *vmp, vir_bytes minv,
    vir_bytes maxv, vir_bytes length, u32_t flags, int mapflags,
    mem_type_t *memtype)
{
    struct vir_region *newregion;
    vir_bytes startv;

    assert(!(length % VM_PAGE_SIZE));

    /* 在进程的虚拟地址空间 [minv, maxv) 范围内寻找空闲槽位
     * region_find_slot 遍历进程的区域映射表，在已有区域之间的空隙中
     * 找到一段长度 >= length 的空闲虚拟地址。不同调用场景：
     *   mmap（无 MAP_FIXED）: minv=VM_MMAPBASE, maxv=VM_MMAPTOP
     *   mmap（MAP_FIXED）:    minv=addr, maxv=0（=addr+len，精确映射）
     *   brk 扩展:            minv=当前堆顶, maxv=0（紧接堆顶，不找洞）
     * Rust 实现对应 RegionMap::find_slot()，详见 13-region-avl.md */
    startv = region_find_slot(vmp, minv, maxv, length);
    if (startv == SLOT_FAIL)
        return NULL;

    /* 创建新 vir_region，分配 physblocks 数组（全 NULL） */
    if(!(newregion = region_new(vmp, startv, length, flags, memtype))) {
        printf("VM: map_page_region: allocating region failed\n");
        return NULL;
    }

    /* 调用 memtype 的 ev_new 回调（如 shared_new 初始化共享内存参数） */
    if(newregion->def_memtype->ev_new) {
        if(newregion->def_memtype->ev_new(newregion) != OK) {
            /* ev_new 负责释放和移除区域 */
            return NULL;
        }
    }

    /* MF_PREALLOC: 预分配所有物理页（如 RS 服务的预分配映射） */
    if(mapflags & MF_PREALLOC) {
        if(map_handle_memory(vmp, newregion, 0, length, 1,
            NULL, 0, 0) != OK) {
            map_free(newregion);
            return NULL;
        }
    }

    /* 预分配后清除 VR_UNINITIALIZED 标志（后续按需分配的页需要清零） */
    USE(newregion, newregion->flags &= ~VR_UNINITIALIZED;);

    /* 插入进程的区域映射表（Minix3: AVL 树 region_insert; Rust: BTreeMap.insert） */
    region_insert(&vmp->vm_regions_avl, newregion);
    return newregion;
}
```

**内部函数 `region_new`** (`region.c:424`):

```c
static struct vir_region *region_new(struct vmproc *vmp, vir_bytes startv,
    vir_bytes length, int flags, mem_type_t *memtype)
{
    struct vir_region *newregion;
    struct phys_region **newphysregions;
    static u32_t id;
    int slots = phys_slot(length);

    /* 从 Slab 分配器分配 vir_region 结构体 */
    if(!(SLABALLOC(newregion))) return NULL;

    /* 初始化所有字段 */
    memset(newregion, 0, sizeof(*newregion));
    newregion->vaddr = startv;
    newregion->length = length;
    newregion->flags = flags;
    newregion->def_memtype = memtype;
    newregion->remaps = 0;
    newregion->id = id++;           /* 单调递增的区域唯一 ID */
    newregion->lower = newregion->higher = NULL;  /* AVL 树节点初始化（Rust 中不需要） */
    newregion->parent = vmp;

    /* 分配 physblocks 指针数组，calloc 保证初始化为全 NULL（稀疏映射） */
    if(!(newphysregions = calloc(slots, sizeof(struct phys_region *)))) {
        SLABFREE(newregion);
        return NULL;
    }
    newregion->physblocks = newphysregions;

    return newregion;
}
```

**关键点**:
- `SLABALLOC` 是 Slab 分配器宏，分配固定大小的内核对象
- `physblocks` 数组初始化为全 NULL，表示没有物理页映射
- 区域创建时不分配物理页，按需分配（懒分配）

#### 2.6.2 map_free — 释放区域

**调用场景**：`munmap` 释放整个区域、`map_copy_region` 失败回滚、`split_region` 释放旧区域等。

**核心作用**：释放 `vir_region` 及其所有 `phys_region` 引用。遍历 `physblocks[]` 数组，对每个已映射的 `phys_region` 调用 `pb_unreferenced`（减少 `phys_block` 引用计数，可能释放物理页），然后释放数组本身和 `vir_region` 结构体。

**源码位置**: `region.c:568`

```c
int map_free(struct vir_region *region)
{
    int r;

    /* 释放区域内所有 phys_region 的引用 */
    if((r=map_subfree(region, 0, region->length)) != OK) return r;

    /* 调用 memtype 的 ev_delete 回调（如 shared_delete 减少 remaps） */
    if(region->def_memtype->ev_delete)
        region->def_memtype->ev_delete(region);

    /* 释放 physblocks 数组和 vir_region 结构体 */
    free(region->physblocks);
    region->physblocks = NULL;
    SLABFREE(region);
    return OK;
}

static int map_subfree(struct vir_region *region,
    vir_bytes start, vir_bytes len)
{
    struct phys_region *pr;
    vir_bytes end = start + len;
    vir_bytes voffset;

    /* 遍历范围内每个虚拟页 */
    for(voffset = start; voffset < end; voffset += VM_PAGE_SIZE) {
        /* 跳过未分配物理页的槽位 */
        if(!(pr = physblock_get(region, voffset))) continue;

        /* 从 phys_block 的引用链表中移除，减少 refcount；
         * rm=1 表示同时从 physblocks[] 数组清除该槽位 */
        pb_unreferenced(region, pr, 1);

        /* 释放 phys_region 结构体本身 */
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

#### 2.6.3 map_lookup — 查找区域

**调用场景**：缺页处理、CoW、页表操作等几乎所有需要根据虚拟地址定位区域的操作。

**核心作用**：在进程的区域映射表中 O(log n) 查找包含指定虚拟地址的 `vir_region`，并可选地返回该地址处的 `phys_region`。是虚拟地址 → 区域映射的核心查询接口。

**源码位置**: `region.c:616`

```c
struct vir_region *map_lookup(struct vmproc *vmp,
    vir_bytes offset, struct phys_region **physr)
{
    struct vir_region *r;

    /* 在区域映射表中搜索 ≤ offset 的最大区域（Minix3: AVL_LESS_EQUAL; Rust: RegionMap::find） */
    if((r = region_search(&vmp->vm_regions_avl, offset, AVL_LESS_EQUAL))) {
        vir_bytes ph;
        /* 验证 offset 确实落在该区域内 */
        if(offset >= r->vaddr && offset < r->vaddr + r->length) {
            /* 计算页内偏移 */
            ph = offset - r->vaddr;
            if(physr) {
                /* 通过 physblocks[] 数组获取对应的 phys_region */
                *physr = physblock_get(r, ph);
                /* 一致性检查：phys_region 的 offset 必须与计算值一致 */
                if(*physr) assert((*physr)->offset == ph);
            }
            return r;
        }
    }
    return NULL;
}
```

**查找逻辑**: 区域映射表搜索 O(log n) → 检查地址是否在区域内 → 计算页内偏移 → 获取 `phys_region`。

#### 2.6.4 split_region — 区域分割

**调用场景**：`munmap` 释放区域中间部分时，需要将一个区域分割成前后两段。

**核心作用**：将一个 `vir_region` 在指定偏移处一分为二。为前后两半各创建新的 `vir_region`，通过 `pb_reference` 将原区域的 `phys_region` 引用迁移到新区域（`phys_block` 引用计数先增后减，净效果不变），然后删除原区域、插入两个新区域。

**源码位置**: [region.c:1150](minix3/minix/servers/vm/region.c#L1150)

```c
static int split_region(struct vmproc *vmp, struct vir_region *vr,
    struct vir_region **vr1, struct vir_region **vr2, vir_bytes split_len)
{
    struct vir_region *r1 = NULL, *r2 = NULL;
    vir_bytes rem_len = vr->length - split_len;
    vir_bytes voffset;

    /* 该 memtype 必须支持分割操作 */
    if(!vr->def_memtype->ev_split) {
        printf("VM: split region not implemented for %s\n",
            vr->def_memtype->name);
        return EINVAL;
    }

    /* 创建两个新区域：r1 为前半部分，r2 为后半部分 */
    if(!(r1 = region_new(vmp, vr->vaddr, split_len, vr->flags,
        vr->def_memtype))) {
        goto bail;
    }
    if(!(r2 = region_new(vmp, vr->vaddr+split_len, rem_len, vr->flags,
        vr->def_memtype))) {
        map_free(r1);
        goto bail;
    }

    /* 迁移 r1 的 phys_region 引用：pb_reference 内部调用 physblock_set */
    for(voffset = 0; voffset < r1->length; voffset += VM_PAGE_SIZE) {
        struct phys_region *ph, *phn;
        if(!(ph = physblock_get(vr, voffset))) continue;
        /* pb_reference: 分配新 phys_region + 链入 phys_block + 设置 physblocks[] */
        if(!(phn = pb_reference(ph->ph, voffset, r1, ph->memtype)))
            goto bail;
    }

    /* 迁移 r2 的 phys_region 引用，offset 从 0 重新计算 */
    for(voffset = 0; voffset < r2->length; voffset += VM_PAGE_SIZE) {
        struct phys_region *ph, *phn;
        /* 源区域中 r2 部分的偏移 = split_len + voffset */
        if(!(ph = physblock_get(vr, split_len + voffset))) continue;
        /* 新区域中 offset 从 0 开始 */
        if(!(phn = pb_reference(ph->ph, voffset, r2, ph->memtype)))
            goto bail;
    }

    /* 调用 memtype 的 ev_split 回调（如 anon_split、mappedfile_split） */
    vr->def_memtype->ev_split(vmp, vr, r1, r2);

    /* 替换原区域：从映射表移除旧区域，插入两个新区域
     * Minix3: region_remove/region_insert 操作 AVL 树
     * Rust: RegionMap::remove + RegionMap::insert 操作 BTreeMap */
    region_remove(&vmp->vm_regions_avl, vr->vaddr);
    map_free(vr);   /* 释放旧区域（phys_region 已迁移，不会释放物理页） */
    region_insert(&vmp->vm_regions_avl, r1);
    region_insert(&vmp->vm_regions_avl, r2);

    *vr1 = r1;
    *vr2 = r2;
    return OK;

  bail:
    /* 失败时整体释放已创建的新区域 */
    if(r1) map_free(r1);
    if(r2) map_free(r2);
    return ENOMEM;
}
```

**关键点**：分割后物理页引用计数增加（`pb_reference` 增加引用），原区域被 `map_free` 释放时 `pb_unreferenced` 减少引用，净效果为引用计数不变。

#### 2.6.5 map_pf — 缺页处理

**调用场景**：进程访问未映射的虚拟页（读/写缺页）或写入只读的 CoW 页时，由内核缺页处理路径调用。

**核心作用**：确保指定虚拟页有可用的物理映射。若页未映射则创建新的 `phys_region`，然后委托给 `memtype` 的 `ev_pagefault` 回调完成具体填充（anon 分配物理页、file 从磁盘读取等），最后写入进程页表。

**源码位置**: [region.c:664](minix3/minix/servers/vm/region.c#L664)

```c
int map_pf(struct vmproc *vmp,
    struct vir_region *region, vir_bytes offset, int write,
    vfs_callback_t pf_callback, void *state, int len, int *io)
{
    struct phys_region *ph;
    int r = OK;

    offset -= offset % VM_PAGE_SIZE;    /* 页对齐 */

    /* 查找该偏移处的 phys_region */
    if(!(ph = physblock_get(region, offset))) {
        struct phys_block *pb;
        /* 槽位为空：分配新 phys_block（phys=MAP_NONE，待填充） */
        if(!(pb = pb_new(MAP_NONE)))
            return ENOMEM;
        /* 创建 phys_region 并链入 phys_block + 设置 physblocks[] */
        if(!(ph = pb_reference(pb, offset, region, region->def_memtype))) {
            pb_free(pb);
            return ENOMEM;
        }
    }

    assert(ph);
    assert(ph->ph);

    /* 若需要写入但页不可写，或页未就绪，调用 memtype 的缺页回调 */
    if(!write || !ph->memtype->writable(ph)) {
        /* ev_pagefault 由具体 memtype 实现：
         * - anon: 分配物理页
         * - file: 从 VFS 读取文件内容
         * - cache: 从 param.pb_cache 获取缓存页 */
        if((r = ph->memtype->ev_pagefault(vmp,
            region, ph, write, pf_callback, state, len, io)) == SUSPEND) {
            return SUSPEND;    /* VFS 异步回调，挂起当前操作 */
        }
        if(r != OK) {
            /* 缺页处理失败，回滚刚创建的映射 */
            if(ph) pb_unreferenced(region, ph, 1);
            return r;
        }
    }

    assert(ph->ph->phys != MAP_NONE);

    /* 将物理页映射写入进程页表 */
    if((r = map_ph_writept(vmp, region, ph)) != OK)
        return r;

    return OK;
}
```

缺页处理是区域映射的核心操作之一。当进程访问未映射或只读页时触发：

1. **查找/创建 phys_region**：若 `physblock_get` 返回 NULL，分配新 `phys_block`（`pb_new(MAP_NONE)`）并通过 `pb_reference` 创建映射
2. **调用 memtype 回调**：`ph->memtype->ev_pagefault(vmp, region, ph, write, ...)` 由具体内存类型处理缺页逻辑（如 anon 分配物理页、file 从磁盘读取、cache 从缓存获取）
3. **写入页表**：`map_ph_writept` 将物理页映射到进程页表

若 `ev_pagefault` 返回 `SUSPEND`，函数返回挂起状态（VFS 回调异步完成）。若返回错误，`pb_unreferenced` 回滚刚创建的映射。

**map_ph_writept / map_writept — 页表写入**：

`map_pf` 完成缺页处理后，调用 `map_ph_writept(vmp, region, ph)`（[region.c:257](minix3/minix/servers/vm/region.c#L257)）将物理页映射写入进程页表。这是映射层与页表层的连接点——`map_ph_writept` 根据 `vir_region.flags` 和 `phys_region.memtype` 计算 PTE 标志，调用 `pt_writemap` 写入页表项。

`map_writept`（[region.c:906](minix3/minix/servers/vm/region.c#L906)）是批量版本，遍历 `vir_region` 的所有已映射 `phys_region`，逐个调用 `map_ph_writept` 写入页表。fork 复制区域后调用 `map_writept` 将所有映射写入子进程页表。

Rust 重写中，这两个函数对应页表操作层，详见 [07-pagetable-ops.md](07-pagetable-ops.md)。

#### 2.6.6 map_unmap_region — munmap 核心

**调用场景**：`munmap` 系统调用释放进程虚拟地址空间的一部分。

**核心作用**：从 `vir_region` 中释放指定偏移和长度的映射。先通过 `map_subfree` 释放范围内的 `phys_region` 引用，然后根据释放位置（整体/头部/尾部/中间）调整区域形态：整体释放则删除区域，头部释放需前移 `vaddr` 并 `memmove` 数组，尾部释放直接缩短 `length`，中间释放则先 `split_region` 再分别处理。

**源码位置**: [region.c:1065](minix3/minix/servers/vm/region.c#L1065)

```c
int map_unmap_region(struct vmproc *vmp, struct vir_region *r,
    vir_bytes offset, vir_bytes len)
{
    vir_bytes regionstart;
    int freeslots = phys_slot(len);

    if(offset+len > r->length || (len % VM_PAGE_SIZE))
        return EINVAL;

    regionstart = r->vaddr + offset;

    /* 释放范围内的 phys_region 引用 */
    map_subfree(r, offset, len);

    if(r->length == len) {
        /* 情况1：释放整个区域 */
        region_remove(&vmp->vm_regions_avl, r->vaddr);  /* Rust: RegionMap::remove */
        map_free(r);
    } else if(offset == 0) {
        /* 情况2：释放头部（低地址端）
         * 需要 ev_lowshrink 回调处理 memtype 特有逻辑 */
        struct phys_region *pr;
        vir_bytes voffset;
        int remslots;

        if(!r->def_memtype->ev_lowshrink)
            return EINVAL;
        if(r->def_memtype->ev_lowshrink(r, len) != OK)
            return EINVAL;

        region_remove(&vmp->vm_regions_avl, r->vaddr);  /* Rust: RegionMap::remove */

        /* vaddr 前移，跳过被释放的部分 */
        USE(r, r->vaddr += len;);

        remslots = phys_slot(r->length);
        region_insert(&vmp->vm_regions_avl, r);  /* Rust: RegionMap::insert */

        /* 调整剩余 phys_region 的 offset（前移 len） */
        for(voffset = len; voffset < r->length; voffset += VM_PAGE_SIZE) {
            if(!(pr = physblock_get(r, voffset))) continue;
            USE(pr, pr->offset -= len;);
        }
        /* memmove physblocks 数组，填补头部空缺 */
        if(remslots)
            memmove(r->physblocks, r->physblocks + freeslots,
                remslots * sizeof(struct phys_region *));
        USE(r, r->length -= len;);
    } else if(offset + len == r->length) {
        /* 情况3：释放尾部，直接缩短 length */
        r->length -= len;
    }
    /* 情况4：释放中间部分 → 由调用者先 split_region 再递归处理 */

    /* 清除页表中对应的映射 */
    if(pt_writemap(vmp, &vmp->vm_pt, regionstart,
      MAP_NONE, len, 0, WMF_OVERWRITE) != OK)
        return ENOMEM;

    return OK;
}
```

`munmap` 系统调用的核心实现，从区域中释放指定偏移和长度的映射：

1. **释放物理页引用**：`map_subfree(r, offset, len)` 遍历范围内的 phys_region 调用 `pb_unreferenced`
2. **处理区域形态变化**：
   - **整个区域释放**（`r->length == len`）：`region_remove` + `map_free`
   - **头部释放**（`offset == 0`）：调用 `ev_lowshrink` 回调，调整 `vaddr`，`memmove` physblocks 数组
   - **尾部释放**：直接缩短 `length`
   - **中间释放**：由调用者先 `split_region` 将区域一分为二，再分别处理

### 2.7 侵入式链表操作细节

> `pb_link`/`pb_unreferenced` 的流程级分析见 [10-phys-pagestate.md](10-phys-pagestate.md) §2.5。本节侧重于链表结构在 `phys_region` 语境下的语义。

#### 2.7.1 next_ph_list — 物理块引用链表

`next_ph_list` 连接**引用同一个 `phys_block` 的所有 `phys_region`**，形成单向链表。

```
phys_block (refcount=3)
    │
    ├── firstregion ──→ phys_region A (进程1, offset=0x1000)
    │                       │
    │                       └── next_ph_list ──→ phys_region B (进程2, offset=0x2000)
    │                                                │
    │                                                └── next_ph_list ──→ phys_region C (进程3)
    │                                                                            │
    │                                                                            └── NULL
```

**关键特性**:

1. **头插法插入**: 新的 `phys_region` 总是插入链表头部（`pb_link()`）
2. **不按虚拟地址排序**: 链表顺序仅反映插入顺序
3. **跨进程链接**: 同一物理块可被不同进程的 `phys_region` 引用
4. **O(n) 移除**: `pb_unreferenced` 需要遍历链表查找前驱节点

#### 2.7.2 pb_link — 插入链表（头插法）

**调用场景**：`pb_reference` 创建新映射引用、`mem_cow` 将 phys_region 链入新 phys_block 时调用。

**核心作用**：将 `phys_region` 以头插法链入 `phys_block` 的引用链表，设置 offset/parent/ph 字段，并将 `refcount++`。

**源码位置**: [pb.c:61](minix3/minix/servers/vm/pb.c#L61)

```c
void pb_link(struct phys_region *newphysr, struct phys_block *newpb,
    vir_bytes offset, struct vir_region *parent)
{
    USE(newphysr,
        newphysr->offset = offset;
        newphysr->ph = newpb;
        newphysr->parent = parent;
        newphysr->next_ph_list = newpb->firstregion;  // 头插法：新节点指向原链头
        newpb->firstregion = newphysr;);               // 新节点成为新链头
    newpb->refcount++;
}
```

> `USE` 宏在 `MEMPROTECT` 构建中会执行 `slabunlock`/`slablock` 操作，在普通构建中直接执行代码。

#### 2.7.3 pb_unreferenced — 从链表移除

**调用场景**：`map_subfree` 释放区域映射、`mem_cow` 从旧 phys_block 解链、`map_pf` 失败回滚等。

**核心作用**：将 `phys_region` 从 `phys_block` 的引用链表中移除，`refcount--`。若引用计数降为 0，调用 `ev_unreference` 释放物理内存并 `SLABFREE(pb)`。`rm` 参数控制是否同时从 `physblocks[]` 数组清除该槽位。

**源码位置**: [pb.c:96](minix3/minix/servers/vm/pb.c#L96)

```c
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb = pr->ph;

    assert(pb->refcount > 0);
    pb->refcount--;

    /* 从 phys_block 的引用链表中移除 pr */
    if (pb->firstregion == pr) {
        /* pr 是链头，直接后移 */
        pb->firstregion = pr->next_ph_list;
    } else {
        /* O(n) 遍历查找 pr 的前驱节点 */
        struct phys_region *others;
        for (others = pb->firstregion; others; others = others->next_ph_list) {
            assert(others->ph == pb);
            if (others->next_ph_list == pr) {
                others->next_ph_list = pr->next_ph_list;
                break;
            }
        }
        assert(others);    /* 必须在链表中找到 pr */
    }

    if (pb->refcount == 0) {
        /* 最后一个引用被移除，释放物理页 */
        assert(!pb->firstregion);
        int r;
        /* 调用 memtype 的 ev_unreference 回调释放物理内存 */
        if((r = pr->memtype->ev_unreference(pr)) != OK)
            panic("unref failed, %d", r);
        SLABFREE(pb);
    }

    pr->ph = NULL;    /* 标记已解链 */

    /* rm=1: 从 vir_region.physblocks[] 数组中清除该槽位 */
    if (rm) physblock_set(region, pr->offset, NULL);
}
```

**移除操作**: O(n) 遍历查找前驱 → 修改前驱的 `next_ph_list` → refcount-- → 检查是否释放。

**`rm` 参数**: 控制是否从 `vir_region.physblocks` 数组中移除。`rm=1` 时调用 `physblock_set(region, pr->offset, NULL)`。

#### 2.7.4 侵入式链表在生产代码中的真实用途

| 用途 | 源码位置 | 说明 |
|------|---------|------|
| `pb_link` 头插法插入 | [pb.c:63-66](minix3/minix/servers/vm/pb.c) | 链表维护 |
| `pb_unreferenced` 从链表移除节点 | [pb.c:100-115](minix3/minix/servers/vm/pb.c) | O(n) 查找前驱 |
| sanity check 验证 refcount 一致性 | [region.c:234-248](minix3/minix/servers/vm/region.c) | 仅调试构建 |
| mappedfile 缓存命中时迁移 phys_region | [mem_file.c:122-124](minix3/minix/servers/vm/mem_file.c) | 从旧 phys_block 解链，链入缓存页的 phys_block |
| CoW 时遍历所有引用者设为只读 | — | **不存在**。`map_ph_writept` 只操作单个 `phys_region` |
| fork 后遍历所有引用者 | — | **不存在**。`map_writept` 逐页独立操作 |

**结论**：在 Minix3 的**生产代码**中，`firstregion` 链表的遍历仅用于链表维护本身和调试检查。唯一的非维护性用途是 mappedfile 缓存命中时的迁移操作。CoW 和 fork 都不遍历链表。

### 2.8 mem_cow — 写时复制

**调用场景**：进程写入 CoW 保护的只读页时，由 `anon_pagefault`（写缺页）调用。

**核心作用**：为共享的 `phys_region` 分配独立的物理页，将原页内容复制到新页，然后替换映射（从旧 `phys_block` 解链，链入新 `phys_block`）。CoW 后 `memtype` 强制降级为 `mem_type_anon`——无论原来是什么类型，复制后都是独立匿名页。

**源码位置**: [pb.c:136](minix3/minix/servers/vm/pb.c#L136)

```c
int mem_cow(struct vir_region *region,
    struct phys_region *ph, phys_bytes new_page_cl, phys_bytes new_page)
{
    struct phys_block *pb;

    /* 若调用方未提供新物理页，按区域的分配标志分配 */
    if(new_page == MAP_NONE) {
        u32_t allocflags = vrallocflags(region->flags);
        if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM)
            return ENOMEM;
        new_page = CLICK2ABS(new_page_cl);
    }

    /* 原页必须有物理映射（不能对未映射页做 CoW） */
    assert(ph->ph->phys != MAP_NONE);

    /* 将原页内容复制到新页 */
    if(sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE) != OK)
        panic("VM: abscopy failed\n");

    /* 为新物理页创建 phys_block */
    if(!(pb = pb_new(new_page))) {
        free_mem(new_page_cl, 1);
        return ENOMEM;
    }

    /* 从旧 phys_block 解链（refcount--），链入新 phys_block */
    /* rm=0：不从 physblocks[] 移除，因为 pb_link 紧接着会重新设置同一槽位 */
    pb_unreferenced(region, ph, 0);
    pb_link(ph, pb, ph->offset, region);

    /* CoW 后 memtype 强制降级为 anon：无论原来是什么类型，
     * 复制后的独立页都是匿名页 */
    ph->memtype = &mem_type_anon;

    return OK;
}
```

`mem_cow` 是写时复制的核心实现，流程：

1. **分配新物理页**：若调用方未提供（`new_page == MAP_NONE`），按区域的 `vrallocflags` 分配
2. **复制内容**：`sys_abscopy` 将原页内容复制到新页
3. **替换映射**：`pb_unreferenced` 从旧 phys_block 解链（refcount--），`pb_link` 链入新 phys_block
4. **降级 memtype**：CoW 后 memtype 强制设为 `mem_type_anon`——无论原来是什么类型（文件映射、缓存等），复制后都是独立匿名页

注意 `pb_unreferenced` 的 `rm=0` 参数：不从 `physblocks[]` 数组移除（因为 `pb_link` 紧接着会重新设置同一槽位）。

### 2.9 错误路径

#### 2.9.1 SLABALLOC 分配失败

`pb_reference` 中 `SLABALLOC(pr)` 失败时返回 NULL，调用者需要处理：

```c
// region.c:818 - map_copy_region
struct phys_region *pr2 = pb_reference(pr->ph, voffset, dst, pr->memtype);
if(!pr2) {
    // 回滚已复制的引用
    // ...
    return ENOMEM;
}
```

`region_new` 中 `SLABALLOC(newregion)` 和 `calloc(newphysregions)` 失败时返回 NULL。

#### 2.9.2 physblock_set 越界

`physblock_get` 中的 `assert(offset < region->length)` 保证偏移量在范围内。越界访问是编程错误，debug 构建会 panic。

#### 2.9.3 map_copy_region 部分失败

fork 复制区域时，如果中途 `pb_reference` 失败，直接 `map_free(newvr)` 整体释放新区域。`map_free` 内部通过 `map_subfree` 遍历所有已映射的 phys_region 调用 `pb_unreferenced` 释放。

#### 2.9.4 ev_unreference 失败

`pb_unreferenced` 中 `ev_unreference` 返回非 OK 时 `panic`。这是不可恢复的错误。

---

## 3. Rust 设计

Minix3 的 `vir_region` + `phys_region` + `phys_block` 三层结构在 Rust 重写中简化为 `VirRegion` + `PageSlot` 两层结构，物理页状态由全局 `PageState` 数组管理（设计决策详见 [10-phys-pagestate.md](10-phys-pagestate.md) §3）。

| Minix3 | minix-rs | 变化 |
|--------|----------|------|
| `vir_region` | `VirRegion` | `physblocks` 类型从 `phys_region**` 变为 `Vec<Option<PageSlot>>` |
| `phys_region` | `PageSlot`（内嵌 VirRegion） | Copy 类型，消除独立堆分配和侵入式链表 |
| `phys_block` | `PageState`（全局数组元素） | 全局固定数组替代 slab 分配 |
| `phys_region.ph` → `phys_block*` | `PageSlot.pfn` → `u32` | PFN 索引替代裸指针 |
| `phys_region.next_ph_list` | 消除 | refcount 是唯一追踪机制 |
| `phys_block.firstregion` | 消除 | 同上 |
| `vir_region.parent` → `vmproc*` | `VirRegion.parent_slot` → `Option<UserSlot>` | 进程槽索引替代裸指针 |
| `vir_region.param` → `union` | `VrParam` → `enum` | 类型安全的联合体 |

**消除 `phys_region` 独立结构体的理由**：

1. **侵入式链表与 Rust 借用检查冲突**：`next_ph_list` 跨越不同 `VirRegion`，所有链表操作必须 `unsafe`
2. **堆分配不必要**：`phys_region` 天然属于 `vir_region`，用 `Box` 分配增加了分配开销和内存碎片
3. **链表的实际价值有限**：生产代码中 `firstregion` 链表仅用于链表维护本身（`pb_unreferenced` 摘除节点）和调试检查，CoW 和 fork 都不遍历链表
4. **PFN 索引替代裸指针**：`PageSlot.pfn` 是 `u32` 全局索引，无悬垂指针风险

### 3.1 PageSlot 设计

```rust
#[derive(Debug, Clone, Copy)]
pub(crate) struct PageSlot {
    pfn: u32,
    offset: VirBytes,
    memtype: Option<&'static dyn MemType>,
}
```

**设计要点**：

#### 3.1.1 pfn 替代 NonNull\<PhysBlock\>

Minix3 的 `phys_region.ph` 是 `phys_block*` 裸指针。Rust 重写用 PFN（Page Frame Number）替代：

| 维度 | `phys_block*` | `u32` (PFN) |
|------|--------------|-------------|
| 悬垂指针风险 | 有（phys_block 释放后指针失效） | 无（PFN 是全局索引，始终有效） |
| 查找效率 | 指针直接访问 O(1) | 数组索引访问 O(1) |
| 所有权语义 | 不明确（多个指针可指向同一对象） | 明确（PFN 是索引，所有权在 PageFrames） |
| 大小 | 8 字节（64 位） | 4 字节 |

**PFN_NONE 语义**：`pfn = u32::MAX` 表示尚未分配物理页，等价于 Minix3 的 `MAP_NONE`。`u32::MAX` 不会与任何合法 PFN 冲突（合法 PFN 范围为 `[0, total_pages-1]`）。如果需要更严格的区分，可用 `Option<u32>` 替代 `u32`，代价是多 4 字节/页。

#### 3.1.2 Copy 类型

`PageSlot` 是 `Copy` 类型，内嵌在 `Vec<Option<PageSlot>>` 中。对比 Minix3 的 `phys_region`：

| 操作 | Minix3 | minix-rs |
|------|--------|--------|
| fork 复制映射 | `SLABALLOC` + `pb_link` + `physblock_set` | `PageSlot` Copy + refcount++ |
| 释放映射 | `pb_unreferenced` + `SLABFREE` | `take()` + refcount-- |
| 内存开销 | 每映射 ~40 字节（64 位）+ slab 元数据 | 每映射 ~20 字节（pfn:4 + offset:8 + memtype:16） |

**消除堆分配**：`PageSlot` 无需 `Box` 分配，fork 时直接复制，无需 `SLABALLOC`。

#### 3.1.3 memtype 保留在 PageSlot 级别

Minix3 中 CoW 后 `ph->memtype = &mem_type_anon`——一个原本是 mappedfile 或 cache 类型的页面，CoW 后变为 anon。这意味着**同一个 VirRegion 内不同页面可以有不同 memtype**，PageSlot 级别的 memtype 是必要的。

源码证据：[pb.c:156](minix3/minix/servers/vm/pb.c#L156) `mem_cow` 函数中 `ph->memtype = &mem_type_anon;`

#### 3.1.4 消除 next_ph_list 和 parent

- **next_ph_list**：侵入式链表被消除，refcount 是唯一的状态追踪机制
- **parent**：PageSlot 天然存在于 VirRegion 中，无需回指

### 3.2 VirRegion 设计

```rust
pub(crate) struct VirRegion {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub physblocks: Vec<Option<PageSlot>>,
    pub flags: VrFlags,
    pub parent_slot: Option<UserSlot>,
    pub def_memtype: Option<&'static dyn MemType>,
    pub remaps: i32,
    pub id: i32,
    pub param: VrParam,
}
```

**与 Minix3 vir_region 的差异**:

| 字段 | C 类型 | Rust 类型 | 设计理由 |
|------|--------|-----------|---------|
| `physblocks` | `struct phys_region**` | `Vec<Option<PageSlot>>` | PageSlot 是 Copy 类型，无需 Box 分配 |
| `parent_slot` | `struct vmproc*` | `Option<UserSlot>` | 进程槽索引替代裸指针，避免悬垂引用 |
| `def_memtype` | `mem_type_t*` | `Option<&'static dyn MemType>` | trait 对象替代函数指针结构体 |
| `flags` | `u16_t` | `VrFlags(u16)` | 封装为类型安全的结构体 |
| `param` | `union` | `enum VrParam` | Rust enum 提供类型安全的联合体 |
| AVL 字段 | `lower`, `higher`, `factor` | **已移除** | Rust 实现使用 `BTreeMap<VirBytes, VirRegion>` 外部管理区域，无需在 `VirRegion` 中内嵌树节点字段（详见 [13-region-avl.md](13-region-avl.md)） |

#### 3.2.1 VrFlags 设计

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(crate) struct VrFlags: u16 {
        const WRITABLE = 0x001;
        const PHYS64K = 0x004;
        const LOWER16MB = 0x008;
        const LOWER1MB = 0x010;
        const SHARED = 0x040;
        const UNINITIALIZED = 0x080;
        const ANON = 0x100;
        const DIRECT = 0x200;
        const PREALLOC_MAP = 0x400;
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(crate) struct PageAllocFlags: u32 {
        const CLEAR = 0x01;
        const CONTIG = 0x02;
        const ALIGN64K = 0x04;
        const LOWER16MB = 0x08;
        const LOWER1MB = 0x10;
        const ALIGN16K = 0x40;
    }
}

impl VrFlags {
    pub(crate) fn to_alloc_flags(&self) -> PageAllocFlags {
        let mut af = PageAllocFlags::empty();
        if self.contains(Self::PHYS64K) { af |= PageAllocFlags::ALIGN64K; }
        if self.contains(Self::LOWER16MB) { af |= PageAllocFlags::LOWER16MB; }
        if self.contains(Self::LOWER1MB) { af |= PageAllocFlags::LOWER1MB; }
        if !self.contains(Self::UNINITIALIZED) { af |= PageAllocFlags::CLEAR; }
        af
    }
}
```

与 Minix3 的 `VR_*` 标志和 `vrallocflags()` 一一对应。使用 `bitflags!` 宏自动生成 `BitOr`、`BitAnd`、`Extend` 等 trait impl，比手动 newtype 更完整。`to_alloc_flags()` 语义与 Minix3 的 `vrallocflags` 完全一致：默认设置 `CLEAR`（清零），仅在 `VR_UNINITIALIZED` 时跳过。

#### 3.2.2 VrParam 联合体设计

```rust
#[derive(Debug, Clone)]
pub(crate) enum VrParam {
    Direct { phys: PhysBytes },
    Shared { ep: i32, vaddr: VirBytes, id: i32 },
    PbCache { pfn: u32 },
    File { inited: bool, offset: u64, clearend: u16 },
}

impl Default for VrParam {
    fn default() -> Self {
        Self::Direct { phys: PhysBytes(0) }
    }
}
```

| 方面 | C union | Rust enum |
|------|---------|-----------|
| 类型安全 | 无，靠 flags 判断 | 编译器保证，只能访问当前变体 |
| `File` 字段 | 有 `fdref` 指针 | 无 `fdref`（暂未实现） |
| 默认值 | 需手动初始化 | `Default` trait |
| 内存布局 | 所有字段共享内存 | 变体独占 + 判别式 |

**`VrParam::Direct` 语义**：`VR_DIRECT` 表示该区域映射的是一段固定的物理地址（如设备 MMIO 寄存器），VM 不负责分配/释放物理页。pagefault 时直接用 `param.phys + offset` 作为物理地址填入页表项。典型场景：驱动通过 `vm_map_phys` 系统调用映射设备寄存器（[mmap.c:351](minix3/minix/servers/vm/mmap.c#L351)）。

**TODO**: `VrParam::File` 缺少 `fdref` 字段。Minix3 的 `param.file.fdref` 是指向文件描述符引用计数结构的指针，用于跟踪文件映射的生命周期。Rust 重写中需要设计对应的文件引用机制（可能是 `Arc<FdRef>` 或类似方案），在实现文件映射（`mem_type_mappedfile`）时必须补全。

### 3.3 核心操作

#### 3.3.0 设计决策总览

| 操作 | Minix3 实现 | Rust 重写 | 设计理由 |
|------|-----------|----------|---------|
| map_lookup | AVL 树搜索 + physblock_get | BTreeMap 搜索 + Vec 索引 | BTreeMap 提供标准库质量的 O(log n)，无需自定义 AVL |
| split_region | pb_reference 迁移 + map_free 旧区域 | VirRegion::split (move 语义) | PageSlot 是 Copy 类型，无需 SLABALLOC/pb_link |
| map_page | pb_new + pb_reference + pb_link + physblock_set | PageSlot 设置 + refcount++ | 四步合并为两步 |
| fork_region | map_copy_region + pb_reference | PageSlot Copy + refcount++ | 同上 |
| unmap_page | pb_unreferenced + SLABFREE | take() + refcount-- + 延迟释放 | 延迟释放避免递归借用 |
| cow_resolve_core | mem_cow + pb_unreferenced + pb_link | unmap_page + map_page + 延迟释放 | 同上 |

**map_lookup 设计决策**：Minix3 使用自定义 AVL 树搜索 `region_search(&vmp->vm_regions_avl, offset, AVL_LESS_EQUAL)`，Rust 重写使用 `BTreeMap::range` 搜索。BTreeMap 的 `range(..=addr).next_back()` 提供等价的"≤ addr 的最大键"语义，无需在 `VirRegion` 中内嵌树节点字段。详细分析见 [13-region-avl.md](13-region-avl.md)。

**split_region 设计决策**：Minix3 的 `split_region` 为前后两半各创建新 `vir_region`，通过 `pb_reference` 将原区域的 `phys_region` 引用迁移到新区域（refcount 先增后减，净效果不变）。Rust 重写直接复制 `PageSlot`（Copy 语义）+ 递增 refcount，无需 `SLABALLOC`/`pb_link`/`physblock_set` 三步操作。offset 在 r2 中从 0 重新计算，与 Minix3 行为一致。

#### 3.3.1 map_page — 创建映射（替代 pb_new + pb_reference + pb_link + physblock_set）

```rust
impl VirRegion {
    fn map_page(
        &mut self,
        frames: &mut PageFrames,
        offset: VirBytes,
        pfn: u32,
        memtype: &'static dyn MemType,
    ) {
        let page_idx = (offset.0 / PAGE_SIZE) as usize;
        let slot = PageSlot::new(pfn, offset, Some(memtype));
        self.physblocks[page_idx] = Some(slot);
        if let Some(state) = frames.get_mut(pfn) {
            state.refcount = state.refcount.saturating_add(1);
        }
    }
}
```

Minix3 的四步操作（`pb_new` + `pb_reference` + `pb_link` + `physblock_set`）简化为两步：设置 PageSlot + 递增 refcount。无堆分配，无链表操作。`saturating_add` 防止 u16 溢出导致未定义行为。

#### 3.3.2 map_lazy — 延迟分配（替代 MAP_NONE + pb_new(MAP_NONE)）

```rust
const PFN_NONE: u32 = u32::MAX;

impl PageSlot {
    fn is_mapped(&self) -> bool {
        self.pfn != PFN_NONE
    }
}

impl VirRegion {
    fn map_lazy(&mut self, offset: VirBytes) {
        let page_idx = (offset.0 / PAGE_SIZE) as usize;
        if page_idx < self.physblocks.len() {
            self.physblocks[page_idx] = Some(PageSlot::new(PFN_NONE, offset, self.def_memtype));
        }
    }
}
```

`PFN_NONE = u32::MAX` 替代 `MAP_NONE`。延迟分配的页有 PageSlot（记录 memtype），但 `pfn = u32::MAX` 表示尚未分配物理页。memtype 取自 `VirRegion.def_memtype`，与 Minix3 中 `phys_region` 继承区域默认 memtype 的行为一致。

#### 3.3.3 fork_region — fork 复制（替代 map_copy_region + pb_reference）

```rust
pub(crate) fn fork_region(
    src: &VirRegion,
    frames: &mut PageFrames,
) -> Result<Box<VirRegion>, ForkError> {
    let mut dst = VirRegion::new(src.vaddr, src.length, src.flags);
    dst.def_memtype = src.def_memtype;
    dst.param = src.param.clone();

    for (i, slot_opt) in src.physblocks.iter().enumerate() {
        if let Some(slot) = slot_opt {
            if slot.is_mapped() {
                if let Some(state) = frames.get_mut(slot.pfn) {
                    state.refcount = state.refcount.saturating_add(1);
                }
                if let Some(mt) = slot.memtype {
                    mt.ev_reference(frames, slot);
                }
            }
            dst.physblocks[i] = Some(*slot);
        }
    }

    if let Some(mt) = src.def_memtype {
        mt.ev_copy(src, &mut dst).map_err(ForkError::MemType)?;
    }

    dst.set_writable(false);
    Ok(Box::new(dst))
}
```

Minix3 为每个 `phys_region` 调用 `pb_reference()` → `SLABALLOC` + `pb_link` + `physblock_set`，然后可选调用 `ev_reference`（仅 cache 和 anon_contig 实现）。Rust 重写直接复制 `PageSlot`（Copy 语义）+ 递增 refcount，无堆分配。

**ev_copy 回调**：Minix3 的 `map_copy_region` 在复制完所有 `phys_region` 后，调用 `vr->def_memtype->ev_copy(vr, newvr)` 让 memtype 执行额外复制逻辑（如 `cache_copy`）。Rust 重写中 `ev_copy` 在 `fork_region` 中调用，确保子区域的 memtype 特定数据被正确初始化。

**CoW 只读标记**：`dst.set_writable(false)` 清除子区域的 WRITABLE 标志。Minix3 中 CoW 只读保护由 `map_proc_copy_range` 末尾的 `map_writept(src); map_writept(dst);` 实现——`map_writept` 遍历所有 `phys_region`，通过 `pr_writable(vr, pr)` 检查 `(vr->flags & VR_WRITABLE) && memtype->writable(pr)`，而 `anon_writable` 返回 `pr->ph->refcount == 1`。共享页（refcount > 1）自然不可写，无需 `ev_reference` 设置只读。Rust 重写中页表操作见 [16-pagefault.md](16-pagefault.md)。

#### 3.3.4 unmap_page — 解除映射（替代 pb_unreferenced）

```rust
impl VirRegion {
    fn unmap_page(
        &mut self,
        frames: &mut PageFrames,
        offset: VirBytes,
    ) -> Option<(u32, &'static dyn MemType)> {
        let page_idx = (offset.0 / PAGE_SIZE) as usize;
        let slot = self.physblocks[page_idx].take()?;
        if slot.is_mapped() {
            if let Some(state) = frames.get_mut(slot.pfn) {
                if state.refcount > 0 {
                    state.refcount -= 1;
                }
                if state.refcount == 0
                    && !state.flags.contains(PageFlags::IN_CACHE)
                {
                    if let Some(mt) = slot.memtype {
                        return Some((slot.pfn, mt));
                    }
                }
            }
        }
        None
    }
}
```

Minix3 的 `pb_unreferenced()` 需要：从侵入式链表中移除节点（O(n) 查找前驱）+ refcount-- + 检查 + ev_unreference + SLABFREE。Rust 重写只需 refcount-- + 检查，无链表操作，无结构体释放。

**ev_unreference 的延迟调用**：`unmap_page` 返回 `(pfn, memtype)` 而非直接调用 `ev_unreference`，避免 `ev_unreference` 回调中需要 `&mut PageFrames` 的递归借用问题。调用者在释放 `&mut PageFrames` 后再调用 `ev_unreference`。

#### 3.3.5 cow_resolve_core — CoW 写时复制（替代 mem_cow）

```rust
pub(crate) fn cow_resolve_core(
    region: &mut VirRegion,
    frames: &mut PageFrames,
    alloc: &mut dyn PfnAllocator,
    offset: VirBytes,
) -> Result<u32, CowCoreError> {
    let slot = region.get_slot(offset)
        .ok_or(CowCoreError::PageNotMapped)?;

    if !slot.is_mapped() {
        return Err(CowCoreError::PageNotMapped);
    }

    let old_pfn = slot.pfn;
    let refcount = frames.get(old_pfn)
        .map(|s| s.refcount)
        .unwrap_or(0);

    if refcount <= 1 {
        return Ok(old_pfn);
    }

    let new_pfn = alloc.alloc_pfn()
        .map_err(|_| CowCoreError::NoMemory)?;

    copy_page_content(frames, old_pfn, new_pfn);

    let pending = region.unmap_page(frames, offset);
    region.map_page(frames, offset, new_pfn, &MEM_TYPE_ANON);

    if let Some((pfn, mt)) = pending {
        mt.ev_unreference(frames, pfn);
        alloc.free_pfn(pfn);
    }

    Ok(new_pfn)
}

fn copy_page_content(frames: &PageFrames, src_pfn: u32, dst_pfn: u32) {
    // TODO: Copy page content from src_pfn to dst_pfn via Direct Map.
    // Equivalent to Minix3's sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE).
}
```

Minix3 的 `mem_cow()` 需要 `pb_unreferenced(region, ph, 0)` + `pb_link(ph, pb, ...)` + `ph->memtype = &mem_type_anon`。Rust 重写使用 `unmap_page` + `map_page`，无需链表操作，refcount 由这两个方法自动管理。`unmap_page` 返回的 `(pfn, memtype)` 对必须按 §3.6.2 的延迟释放模式处理——在释放 `&mut PageFrames` 后调用 `ev_unreference` 和 `free_pfn`，否则旧物理页的 `ev_unreference` 永远不会被调用，导致内存泄漏。

**CoW 后 memtype 强制变为 anon**：这是 Minix3 的真实语义——`mem_cow()` 在 pb.c:165 设置 `ph->memtype = &mem_type_anon`，`mappedfile_pagefault` 在 mem_file.c:70 也设置 `ph->memtype = &mem_type_anon`。CoW 后的页面不再是文件映射或缓存页，而是独立的匿名页。Rust 重写中 `cow_resolve_core` 使用 `&MEM_TYPE_ANON` 作为 `map_page` 的 memtype 参数，与 Minix3 语义一致。

### 3.4 MemType trait 集成

```rust
pub(crate) trait MemType: Send + Sync {
    fn name(&self) -> &'static str;

    fn ev_new(&self, _region: &mut VirRegion) -> Result<(), MemTypeError> { Ok(()) }
    fn ev_delete(&self, _region: &mut VirRegion) {}

    fn ev_reference(&self, _frames: &mut PageFrames, _slot: PageSlot) {}

    fn ev_unreference(&self, _frames: &mut PageFrames, _pfn: u32) {}

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

    fn ev_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        _region: &mut VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> { Ok(()) }

    fn ev_split(
        &self,
        _proc: &mut ActiveProc<'_>,
        _original: &VirRegion,
        _left: &mut VirRegion,
        _right: &mut VirRegion,
    ) {}

    fn ev_low_shrink(
        &self,
        _region: &mut VirRegion,
        _len: VirBytes,
    ) -> Result<(), MemTypeError> { Ok(()) }

    fn ev_sanitycheck(
        &self,
        _frames: &PageFrames,
        _slot: PageSlot,
    ) -> Result<(), MemTypeError> { Ok(()) }

    fn writable(&self, _frames: &PageFrames, _slot: PageSlot) -> bool { false }

    fn ev_copy(
        &self,
        _src: &VirRegion,
        _dst: &mut VirRegion,
    ) -> Result<(), MemTypeError> { Ok(()) }

    fn region_id(&self, _region: &VirRegion) -> u32 { 0 }
    fn ref_count(&self, _region: &VirRegion) -> i32 { 0 }
    fn pt_flags(&self, _region: &VirRegion) -> i32 { 0 }
}
```

**与 Minix3 `mem_type_t` 的对应**：

| Minix3 回调 | Rust trait 方法 | 变化 |
|-------------|----------------|------|
| `ev_new(vr)` | `ev_new(&self, region)` | 无需 `frames`，新区域尚无物理页 |
| `ev_unreference(pr)` | `ev_unreference(&self, frames, pfn)` | PFN + 全局表替代 `phys_region*` |
| `ev_pagefault(vmp, region, ph, write, cb, state, len, io)` | `ev_pagefault(&self, proc, region, frames, offset, write)` | `ActiveProc` 替代 `&mut VmProc`；`offset: VirBytes` 替代 `&mut PageSlot`；VFS 回调另见 §4.5 |
| `writable(pr)` | `writable(&self, frames, slot)` | 需要访问 `PageFrames` 查 refcount |
| `ev_copy(vr, newvr)` | `ev_copy(&self, src, dst)` | 同上 |
| `ev_reference(pr, newpr)` | `ev_reference(&self, frames, slot)` | CoW 引用通知，不返回 Result（Minix3 中仅 cache/contig 实现） |
| `ev_lowshrink(vr, len)` | `ev_low_shrink(&self, region, len)` | 无需 `proc` 参数 |
| `ev_split(vmp, vr, r1, r2)` | `ev_split(&self, proc, original, left, right)` | `original` 为不可变引用 |
| `ev_resize(vmp, vr, len)` | `ev_resize(&self, proc, region, new_len)` | 同上 |
| `regionid(vr)` | `region_id(&self, region)` | 同上 |
| `refcount(vr)` | `ref_count(&self, region)` | 同上 |
| `pt_flags(vr)` | `pt_flags(&self, region)` | 同上 |

**`Send + Sync` bound 的必要性**：`MemType` 实现是全局静态的（如 `MEM_TYPE_ANON`），可能被多个进程的 region 引用。`Send + Sync` 确保 trait object 可以安全地跨线程共享（即使当前 VM 是单线程，为未来多线程安全预留）。

**`ev_pagefault` 使用 `offset: VirBytes` 而非 `&mut PageSlot`**：pagefault handler 可能需要分配新物理页并映射到 slot，此时需要 `&mut VirRegion` 来调用 `map_page`。传入 `offset` 让 handler 通过 `region.get_slot(offset)` 获取 slot 信息，避免同时持有 `&mut VirRegion` 和 `&mut PageSlot` 的借用冲突。

**为什么用 trait object 而非 enum dispatch**：

1. Minix3 的 `mem_type_t` 本身就是函数指针表（虚表），trait object 是其 Rust 等价物
2. 6 种内存类型是固定的，虚函数调用开销在 VM 单线程环境下可忽略
3. trait object 的扩展性更好（添加新类型只需 impl，不修改 enum）
4. `&'static dyn MemType` = 16B（指针+虚表），对 PageSlot 大小的影响可接受

**TODO: enum dispatch 替代方案**：如果 `dyn MemType` 的虚函数调用开销成为瓶颈（如在频繁缺页的场景），可改用 enum dispatch：

```rust
pub(crate) enum MemTypeDispatch {
    Anon(MemTypeAnon),
    DirectPhys(MemTypeDirectPhys),
    MappedFile(MemTypeMappedFile),
    Shared(MemTypeShared),
    Cache(MemTypeCache),
    AnonContig(MemTypeAnonContig),
}
```

优势：无间接调用，编译器可内联；劣势：添加新类型需修改 enum，违反开闭原则。当前选择 trait object 是因为 6 种 memtype 是固定的，且 VM 单线程环境下虚函数调用开销可忽略。如果未来性能分析显示 dispatch 是热点，再切换到 enum dispatch。

### 3.5 与 Minix3 的语义对应表

| Minix3 概念 | minix-rs 对应 |
|-------------|------------|
| `phys_region` 结构体 | `PageSlot`（内嵌 VirRegion） |
| `phys_region.ph` | `PageSlot.pfn` |
| `phys_region.parent` | 消除（天然在 VirRegion 中） |
| `phys_region.offset` | `PageSlot.offset` |
| `phys_region.memtype` | `PageSlot.memtype` |
| `phys_region.next_ph_list` | 消除 |
| `phys_block.refcount` | `PageState.refcount` | `u8_t` → `u16`，安全裕量更大 |
| `phys_block.flags` / `PBF_INCACHE` | `PageFlags::IN_CACHE` | 标志位语义不变 |
| `vir_region.physblocks[]` | `Vec<Option<PageSlot>>` |
| `vir_region.parent` | `VirRegion.parent_slot` |
| `vir_region.param` (union) | `VrParam` (enum) |
| `pb_reference()` | `map_page()` |
| `physblock_set/get()` | `Vec` 索引访问 |
| `map_copy_region()` | `fork_region()` |
| `map_page_region()` | `map_page_region()`（结构体类型更新） |
| `map_free()` | `free_range()`（VirRegion 方法，支持部分释放） |
| `map_lookup()` | `map_lookup()`（BTreeMap 搜索 + Vec 索引） |
| `split_region()` | `VirRegion::split()`（move 语义，无需 frames 参数） |
| `mappedfile 缓存命中` | 修改 `PageSlot.pfn` + refcount++ |

### 3.6 错误路径设计

#### 3.6.1 map_page 中 alloc_phys_page 失败

返回 `Result<(), VmError>`，调用者处理。与 Minix3 `pb_new` 失败一致——`map_page_region` 返回 NULL。

#### 3.6.2 unmap_page 中 ev_unreference 的递归借用

`unmap_page` 返回 `(pfn, memtype)`，调用者延迟调用 `ev_unreference`。这避免了 `ev_unreference` 回调中需要 `&mut PageFrames` 的递归借用问题。

调用模式：

```rust
let pending = vr.unmap_page(frames, offset);
if let Some((pfn, mt)) = pending {
    mt.ev_unreference(frames, pfn);
    frames.free_phys_page(pfn);
}
```

#### 3.6.3 fork_region 中 ev_copy 失败

如果 `ev_copy` 回调失败（如 cache memtype 复制失败），需要回滚已递增的 refcount。当前 Rust 实现中 `ev_reference` 不返回错误（Minix3 中 `cache_reference` 返回 OK，`anon_contig_reference` 返回 ENOMEM），因此 refcount 回滚主要在 `ev_copy` 失败时需要：

```rust
pub(crate) fn fork_region(
    src: &VirRegion,
    frames: &mut PageFrames,
) -> Result<Box<VirRegion>, ForkError> {
    let mut dst = VirRegion::new(src.vaddr, src.length, src.flags);
    dst.def_memtype = src.def_memtype;
    dst.param = src.param.clone();

    for (i, slot_opt) in src.physblocks.iter().enumerate() {
        if let Some(slot) = slot_opt {
            if slot.is_mapped() {
                if let Some(state) = frames.get_mut(slot.pfn) {
                    state.refcount = state.refcount.saturating_add(1);
                }
            }
            dst.physblocks[i] = Some(*slot);
        }
    }

    if let Some(mt) = src.def_memtype {
        if let Err(e) = mt.ev_copy(src, &mut dst) {
            for slot_opt in dst.physblocks.iter() {
                if let Some(slot) = slot_opt {
                    if slot.is_mapped() {
                        if let Some(state) = frames.get_mut(slot.pfn) {
                            state.refcount = state.refcount.saturating_sub(1);
                        }
                    }
                }
            }
            return Err(ForkError::MemType(e));
        }
    }

    dst.set_writable(false);
    Ok(Box::new(dst))
}
```

Minix3 的 `map_copy_region` 在 `ev_reference` 失败时直接 `map_free(newvr)` 整体释放新区域。Rust 重写中由于 `ev_reference` 不返回错误，回滚逻辑集中在 `ev_copy` 失败路径。

#### 3.6.4 cow_resolve_core 中 alloc_pfn 失败

返回 `CowCoreError::NoMemory`，上层 `cow_resolve` 转换为 `CowError::NoMemory`，最终页错误处理程序向进程发 SIGSEGV。与 Minix3 行为一致——`mem_cow` 中 `pb_new` 失败时，进程收到 SIGSEGV。

#### 3.6.5 split 中 refcount 回滚

Rust 重写中，`VirRegion::split(self, split_len)` 使用 move 语义，原 region 被 consume，两个新 region 通过 `PageSlot` Copy + refcount++ 获得。由于使用 move 语义，不需要回滚——如果 refcount++ 失败（OOM），原 region 已被 consume，两个新 region 一起 drop 即可。

实际源码中 `split` 方法不调用 `ev_split`，也不需要 `frames` 参数——refcount 的递增由 `map_page` 在后续映射时处理，`split` 只负责拆分 VirRegion 的元数据（vaddr, length, physblocks）。

```rust
pub(crate) fn split(self, split_len: VirBytes) -> Result<(Self, Self), VmError> {
    if split_len.get() == 0 || split_len >= self.length {
        return Err(VmError::InvalidParam);
    }

    let left_len = split_len;
    let right_len = VirBytes(self.length.get() - split_len.get());

    let mut left = VirRegion::new(self.vaddr, left_len, self.flags);
    let mut right = VirRegion::new(
        VirBytes(self.vaddr.get() + split_len.get()),
        right_len,
        self.flags,
    );

    left.def_memtype = self.def_memtype;
    right.def_memtype = self.def_memtype;
    left.param = self.param.clone();
    right.param = self.param.clone();

    let split_idx = (split_len.get() / PAGE_SIZE) as usize;
    left.physblocks = self.physblocks[..split_idx].to_vec();
    right.physblocks = self.physblocks[split_idx..].to_vec();

    Ok((left, right))
}
```

注意：`split` 使用 move 语义（`self` 被 consume），不需要 `frames` 参数。refcount 的管理由 `map_page`/`unmap_page` 在后续操作中处理，`split` 只负责拆分元数据。

#### 3.6.6 map_pf 中分配失败回滚

Minix3 的 `map_pf` 在 `pb_new` 或 `pb_reference` 失败时直接返回 `ENOMEM`，无需回滚（刚创建的 `phys_block` 可立即释放）。`ev_pagefault` 失败时调用 `pb_unreferenced(region, ph, 1)` 回滚刚创建的映射。

Rust 重写中，`map_pf` 对应缺页处理流程。分配失败时：

```rust
fn handle_pagefault(/* ... */) -> Result<PageFaultResult, VmError> {
    let slot = vr.get_slot_mut(offset);
    match slot {
        None | Some(slot) if !slot.is_mapped() => {
            let new_pfn = frames.alloc_phys_page()?;
            vr.map_page(frames, offset, new_pfn, vr.def_memtype.unwrap());
        }
        _ => {}
    }

    let slot = vr.get_slot_mut(offset).unwrap();
    if let Err(e) = slot.memtype.unwrap().ev_pagefault(vmp, vr, slot, frames, write) {
        let pending = vr.unmap_page(frames, offset);
        if let Some((pfn, mt)) = pending {
            mt.ev_unreference(pfn, frames);
            frames.free_phys_page(pfn);
        }
        return Err(e);
    }

    Ok(PageFaultResult::Ok)
}
```

---

## 4. 实现详解

### 4.1 PageSlot 实现

```rust
const PFN_NONE: u32 = u32::MAX;

#[derive(Debug, Clone, Copy)]
pub(crate) struct PageSlot {
    pub pfn: u32,
    pub offset: VirBytes,
    pub memtype: Option<&'static dyn MemType>,
}

impl PageSlot {
    pub fn new(pfn: u32, offset: VirBytes, memtype: Option<&'static dyn MemType>) -> Self {
        Self { pfn, offset, memtype }
    }

    pub fn is_mapped(&self) -> bool {
        self.pfn != PFN_NONE
    }
}
```

**内存布局**（64 位系统）：

```
PageSlot:
  ┌──────────────────────────────────────────┐
  │ pfn: u32 (4字节)                         │
  ├──────────────────────────────────────────┤
  │ padding: 4字节                            │
  ├──────────────────────────────────────────┤
  │ offset: VirBytes (8字节)                  │
  ├──────────────────────────────────────────┤
  │ memtype: Option<&'static dyn MemType>     │
  │       (16字节: 指针+虚表)                  │
  └──────────────────────────────────────────┘
  总计: 32 字节

对比 C 的 phys_region (64位):
  ph(8) + parent(8) + offset(8) + memtype(8) + next_ph_list(8) = 40 字节
```

### 4.2 VirRegion 实现

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VrFlags(pub u16);

impl VrFlags {
    pub(crate) const WRITABLE: u16 = 0x001;
    pub(crate) const PHYS64K: u16 = 0x004;
    pub(crate) const LOWER16MB: u16 = 0x008;
    pub(crate) const LOWER1MB: u16 = 0x010;
    pub(crate) const SHARED: u16 = 0x040;
    pub(crate) const UNINITIALIZED: u16 = 0x080;
    pub(crate) const ANON: u16 = 0x100;
    pub(crate) const DIRECT: u16 = 0x200;
    pub(crate) const PREALLOC_MAP: u16 = 0x400;

    pub(crate) const fn empty() -> Self { Self(0) }
    pub(crate) const fn contains(&self, flag: u16) -> bool { (self.0 & flag) != 0 }
    pub(crate) fn insert(&mut self, flag: u16) { self.0 |= flag; }
    pub(crate) fn remove(&mut self, flag: u16) { self.0 &= !flag; }

    pub(crate) fn to_alloc_flags(&self) -> u32 {
        let mut af = PAF_ALIGN_64K;
        if self.contains(Self::LOWER16MB) { af |= PAF_LOWER16MB; }
        if self.contains(Self::LOWER1MB) { af |= PAF_LOWER1MB; }
        if self.contains(Self::PHYS64K) { af |= PAF_ALIGN_64K; }
        af
    }
}

#[derive(Debug, Clone)]
pub(crate) enum VrParam {
    Direct { phys: PhysBytes },
    Shared { ep: i32, vaddr: VirBytes, id: i32 },
    PbCache { pfn: u32 },
    File { inited: bool, offset: u64, clearend: u16 },
}

impl Default for VrParam {
    fn default() -> Self {
        Self::Direct { phys: PhysBytes(0) }
    }
}

pub(crate) struct VirRegion {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub physblocks: Vec<Option<PageSlot>>,
    pub flags: VrFlags,
    pub parent_slot: Option<UserSlot>,
    pub def_memtype: Option<&'static dyn MemType>,
    pub remaps: i32,
    pub id: i32,
    pub param: VrParam,
}

impl VirRegion {
    pub(crate) fn new(vaddr: VirBytes, length: VirBytes, flags: VrFlags) -> Self {
        let pages = ((length.0 + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
        let physblocks = (0..pages).map(|_| None).collect();
        Self {
            vaddr,
            length,
            physblocks,
            flags,
            parent_slot: None,
            def_memtype: None,
            remaps: 0,
            id: 0,
            param: VrParam::default(),
        }
    }

    pub(crate) fn end_addr(&self) -> VirBytes {
        VirBytes(self.vaddr.0 + self.length.0)
    }

    pub(crate) fn contains(&self, addr: VirBytes) -> bool {
        addr.0 >= self.vaddr.0 && addr.0 < self.end_addr().0
    }

    pub(crate) fn get_slot(&self, offset: VirBytes) -> Option<&PageSlot> {
        let idx = (offset.0 / PAGE_SIZE) as usize;
        self.physblocks.get(idx)?.as_ref()
    }

    pub(crate) fn get_slot_mut(&mut self, offset: VirBytes) -> Option<&mut PageSlot> {
        let idx = (offset.0 / PAGE_SIZE) as usize;
        self.physblocks.get_mut(idx)?.as_mut()
    }
}
```

**与 Minix3 region_new 的对应**：

| 步骤 | Minix3 | minix-rs |
|------|--------|----------|
| 分配结构体 | `SLABALLOC(newregion)` | `VirRegion::new()`（栈或 alloc） |
| 初始化字段 | `memset + 逐字段赋值` | 结构体初始化语法 |
| 分配 physblocks | `calloc(slots, sizeof(ptr))` | `(0..pages).map(\|_\| None).collect()` |
| memtype 设置 | 直接赋值 `region->def_memtype` | `with_memtype()` 工厂或事后赋值 |
| ID 分配 | `static u32_t id; id++` | 调用方负责设置 |

### 4.3 核心操作实现

#### 4.3.1 map_page_region — 分配区域

```rust
pub(crate) fn map_page_region(
    vmp: &mut VmProc,
    minv: VirBytes,
    maxv: VirBytes,
    length: VirBytes,
    flags: VrFlags,
    mapflags: u32,
    memtype: &'static dyn MemType,
    frames: &mut PageFrames,
) -> Option<VirRegion> {
    assert_eq!(length.get() % PAGE_SIZE, 0);

    let startv = region_find_slot(vmp, minv, maxv, length)?;

    let mut vr = VirRegion::new(startv, length, flags, Some(memtype));

    if let Err(_) = memtype.ev_new(&mut vr, frames) {
        return None;
    }

    if mapflags & MF_PREALLOC != 0 {
        if map_handle_memory(vmp, &mut vr, 0, length, true, frames).is_err() {
            memtype.ev_delete(&mut vr);
            return None;
        }
    }
    vr.flags.remove(VrFlags::UNINITIALIZED);

    Some(vr)
}
```

#### 4.3.2 free_range — 释放区域中的页面范围

```rust
pub(crate) fn free_range(
    &mut self,
    frames: &mut PageFrames,
    offset: VirBytes,
    len: VirBytes,
) -> Vec<(u32, &'static dyn MemType)> {
    let start_page = (offset.0 / PAGE_SIZE) as usize;
    let end_page = ((offset.0 + len.0 + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
    let mut pending_unrefs = Vec::new();

    for page in start_page..end_page.min(self.physblocks.len()) {
        let page_offset = VirBytes((page as u64) * PAGE_SIZE);
        if let Some((pfn, mt)) = self.unmap_page(frames, page_offset) {
            pending_unrefs.push((pfn, mt));
        }
    }

    pending_unrefs
}
```

**延迟释放模式**：`free_range` 收集所有需要释放的 `(pfn, memtype)` 对，返回给调用者。调用者释放 `&mut PageFrames` 后，逐个调用 `ev_unreference` 和 `free_pfn`。

与 Minix3 的 `map_free` 不同，`free_range` 是 `VirRegion` 的方法，接受 `offset` 和 `len` 参数，支持部分释放（如 `munmap` 释放区域中间的一段）。Minix3 的 `map_free` 释放整个区域，部分释放由 `map_unmap_region` 处理。

#### 4.3.3 map_lookup — 查找区域

```rust
pub(crate) fn map_lookup(
    vmp: &VmProc,
    addr: VirBytes,
) -> Option<(&VirRegion, Option<&PageSlot>)> {
    let vr = vmp.regions().find(addr)?;

    if addr.get() >= vr.vaddr.get() && addr.get() < vr.end_addr().get() {
        let offset = VirBytes(addr.get() - vr.vaddr.get());
        let slot = vr.get_slot(offset);
        Some((vr, slot))
    } else {
        None
    }
}
```

#### 4.3.4 split_region — 区域分割

```rust
pub(crate) fn split_region(
    vmp: &mut VmProc,
    vr: &mut VirRegion,
    split_len: VirBytes,
    frames: &mut PageFrames,
) -> Result<(VirRegion, VirRegion), VmError> {
    assert_eq!(split_len.get() % PAGE_SIZE, 0);
    assert!(split_len.get() > 0 && split_len.get() < vr.length.get());

    let rem_len = VirBytes(vr.length.get() - split_len.get());

    let mut r1 = VirRegion::new(vr.vaddr, split_len, vr.flags, vr.def_memtype);
    let mut r2 = VirRegion::new(
        VirBytes(vr.vaddr.get() + split_len.get()),
        rem_len,
        vr.flags,
        vr.def_memtype,
    );

    for i in 0..r1.page_count() {
        if let Some(slot) = vr.get_slot(VirBytes(i as u64 * PAGE_SIZE)) {
            if slot.is_mapped() {
                frames.get_mut(slot.pfn).unwrap().refcount += 1;
            }
            r1.physblocks[i] = Some(*slot);
        }
    }

    for i in 0..r2.page_count() {
        let src_offset = VirBytes(split_len.get() + (i as u64) * PAGE_SIZE);
        if let Some(slot) = vr.get_slot(src_offset) {
            if slot.is_mapped() {
                frames.get_mut(slot.pfn).unwrap().refcount += 1;
            }
            let mut new_slot = *slot;
            new_slot.offset = VirBytes(i as u64 * PAGE_SIZE);
            r2.physblocks[i] = Some(new_slot);
        }
    }

    if let Some(mt) = vr.def_memtype {
        mt.ev_split(vmp, vr, &mut r1, &mut r2);
    }

    Ok((r1, r2))
}
```

### 4.4 MemType 各实现要点

> 详细的 MemType trait 定义和各实现见 [12-memtype.md](12-memtype.md)。本节仅概述各 memtype 在 Rust 重写下的关键变化。

#### MemTypeAnon

- `ev_pagefault`：首次分配（PFN_NONE → 新 PFN）或 CoW（refcount≥2 + write → 新 PFN）
- `ev_unreference`：`free_phys_page(pfn)`
- `is_writable`：`refcount == 1`

#### MemTypeDirectPhys

- `ev_pagefault`：计算物理地址 = `param.phys + offset`，设置 PFN
- `ev_unreference`：不释放物理页
- `is_writable`：`slot.is_mapped()`
- `ev_copy`：复制 `param`

#### MemTypeMappedFile

- `ev_pagefault`：查找缓存页（命中→修改 PageSlot.pfn + refcount++；未命中→VFS IPC）
- `ev_unreference`：`free_phys_page(pfn)`
- `is_writable`：`false`（Minix3 的 mappedfile 永远不可写，通过 CoW 实现写入）

**mappedfile 缓存命中**：Minix3 的 `pb_unreferenced(region, ph, 0); pb_link(ph, cp->page, ...)` 在 Rust 重写中简化为：

```rust
fn mappedfile_cache_hit(slot: &mut PageSlot, cache_pfn: u32, frames: &mut PageFrames) {
    slot.pfn = cache_pfn;
    frames.get_mut(cache_pfn).unwrap().refcount += 1;
}
```

#### MemTypeShared

- `ev_pagefault`：通过 `getsrc()` 查找源进程的源区域，获取物理页引用
- `ev_unreference`：委托 anon
- `is_writable`：`slot.is_mapped()`

#### MemTypeCache

- `ev_pagefault`：从 `param.pb_cache` 获取缓存页的 PFN，设置 PageSlot
- `ev_unreference`：委托 anon
- `is_writable`：`slot.is_mapped()`

#### MemTypeAnonContig

- `ev_new`：一次性分配连续物理内存，设置所有 PageSlot 的 PFN
- `ev_pagefault`：panic（不应该发生）
- `ev_reference`：拒绝（不支持 fork）
- `ev_resize`：拒绝
- `is_writable`：与 anon 相同

### 4.5 VFS 异步回调

Minix3 的 `ev_pagefault` 接受 `vfs_callback_t cb` 和 `void *state` 参数，用于文件映射缺页时异步请求 VFS 读取数据。这是微内核特有的问题。

```rust
pub(crate) enum PageFaultResult {
    Ok,
    Suspend,
    Error(VmError),
}
```

VFS 异步回调是独立于 PageSlot/VirRegion 设计的问题。Rust 重写不改变 VFS 交互的基本模式。

**中间态保护**：文件映射缺页时，VM 向 VFS 发 IPC 请求后，该物理页处于"已分配但数据未就绪"的状态。如果需要防止其他进程在此期间访问该页，可在 `PageFlags` 中添加 `PENDING_IO` 标志。但这属于 VFS 交互层的设计，与核心映射数据结构正交。Minix3 的 C 代码中没有显式的 PENDING_IO 保护——它依赖 VM 单线程和 VFS 请求的串行化。

---

## 5. 测试要点

### 5.1 PageSlot 测试

| 测试场景 | 验证内容 |
|---------|---------|
| `new_unmapped` 创建 | `is_mapped() == false`，`pfn() == None`，memtype 正确 |
| `new_mapped` 创建 | `is_mapped() == true`，`pfn() == Some(pfn)`，offset 正确 |
| Copy 语义 | 复制后两个 PageSlot 独立，修改一个不影响另一个 |
| CoW 后 memtype 变更 | `set_memtype(&mem_type_anon)` 后，memtype 正确更新 |
| PFN_NONE 边界 | `pfn = u32::MAX` 时 `is_mapped()` 返回 false |

### 5.2 VirRegion 测试

| 测试场景 | 验证内容 |
|---------|---------|
| 创建 | vaddr、length、physblocks 长度正确 |
| contains | 地址包含判断，边界条件 |
| flags 操作 | insert/remove 标志位 |
| split | 分割后左右区域属性正确，PageSlot 正确迁移 |
| split invalid | 无效分割参数处理 |

### 5.3 核心操作测试

| 测试场景 | 验证内容 |
|---------|---------|
| map_page | PageSlot 正确设置，refcount 正确递增 |
| map_lazy | PageSlot 存在但 pfn=u32::MAX（PFN_NONE），refcount 不变 |
| unmap_page | PageSlot 被清除，refcount 正确递减，返回正确的 pending unref |
| fork_region | 所有 PageSlot 正确复制，refcount 正确递增 |
| fork_region 部分失败 | 回滚已复制的 refcount |
| cow_resolve_core | 旧页 refcount--、新页 refcount=1、PageSlot.pfn 更新、memtype 强制变为 anon |
| free_range | 所有 PageSlot 清除，refcount 正确递减，pending unref 收集完整 |
| split_region | r1/r2 的 PageSlot 正确迁移，refcount 正确递增 |

### 5.4 错误路径测试

| 测试场景 | 验证内容 |
|---------|---------|
| map_page 分配失败 | 返回错误，无副作用 |
| unmap_page 递归借用 | pending unref 延迟调用正确 |
| mem_cow 分配失败 | 返回错误，原 PageSlot 不变 |
| VrFlags::to_alloc_flags | VR_LOWER16MB→PAF_LOWER16MB、VR_PHYS64K→PAF_ALIGN_64K 等转换正确 |
| split_region 失败回滚 | ev_split 失败时，已递增的 refcount 被正确回滚 |
| map_pf 分配失败 | alloc_phys_page 失败返回错误，无副作用；ev_pagefault 失败时 unmap_page 回滚 |
| PFN 边界 | pfn=u32::MAX（PFN_NONE）、pfn=total_pages-1、pfn 越界 |

---

## 6. 参见

- [10-phys-pagestate.md](10-phys-pagestate.md) — PageFrames API（本文档的前置）
- [12-memtype.md](12-memtype.md) — MemType trait 定义和各实现
- [13-region-avl.md](13-region-avl.md) — 区域映射表（BTreeMap）组织与操作
- [15-cow-mechanism.md](15-cow-mechanism.md) — CoW 机制详解
- [16-pagefault.md](16-pagefault.md) — 页错误处理
