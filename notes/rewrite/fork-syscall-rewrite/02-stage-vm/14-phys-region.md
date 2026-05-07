# 14-phys-region: 物理区域 (phys_region)

> **分类**: VM私有  
> **源码**: `minix3/minix/servers/vm/phys_region.h`  
> **说明**: 连接虚拟区域和物理块的桥梁，管理虚拟到物理的映射关系

---

## 1. 概述

### 1.1 桥梁作用

`phys_region` 是连接虚拟区域 (`vir_region`) 和物理块 (`phys_block`) 的桥梁：

```
┌─────────────────────────────────────────────────────────────────┐
│                        vir_region                                │
│  vaddr=0x1000, length=0x3000                                     │
│  ┌─────────────┬─────────────┬─────────────┐                    │
│  │ phys_region │ phys_region │ phys_region │ (每页一个)          │
│  │  offset=0   │ offset=0x1000│offset=0x2000│                    │
│  └──────┬──────┴──────┬──────┴──────┬──────┘                    │
│         │             │             │                            │
│         ▼             ▼             ▼                            │
│  ┌─────────────┐┌─────────────┐┌─────────────┐                  │
│  │ phys_block  ││ phys_block  ││ phys_block  │                  │
│  │ phys=0x8000 ││ phys=0x9000 ││ phys=0xA000 │                  │
│  │ refcount=1  ││ refcount=2  ││ refcount=1  │                  │
│  └─────────────┘└──────┬──────┘└─────────────┘                  │
│                        │                                         │
│         ┌──────────────┘ (共享内存/CoW)                          │
│         │                                                        │
│         ▼                                                        │
│  ┌─────────────┐                                                 │
│  │ phys_region │ (另一个 vir_region 的)                          │
│  └─────────────┘                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 为什么需要这个中间层

**问题**：为什么不直接让 `vir_region` 指向 `phys_block`？

**答案**：

| 场景 | 直接映射的问题 | phys_region 的解决方案 |
|------|---------------|----------------------|
| **共享内存** | 一个物理页被多个进程映射，无法追踪 | 每个 `phys_region` 独立，共享同一 `phys_block` |
| **CoW** | fork 后无法区分父子进程的映射 | 各自的 `phys_region`，共享 `phys_block`，refcount 追踪 |
| **稀疏映射** | 大区域可能只有部分页面有物理页 | 每页一个 `phys_region`，未映射的为 NULL |
| **内存类型** | 同一区域不同页面可能有不同属性 | `phys_region.memtype` 独立设置 |

### 1.3 与 Minix3 的对应关系

| Minix3 结构体 | Rust 实现 | 说明 |
|--------------|----------|------|
| `struct phys_region` | `PhysRegion` | 虚拟到物理的映射单元 |
| `struct phys_block` | `PhysBlock` | 物理内存块，支持引用计数 |
| `ph->phys` | `PhysBlock.phys` | 物理地址 |
| `ph->refcount` | `PhysBlock.refcount` | 引用计数 |
| `ph->firstregion` | `PhysBlock.first_region` | 引用链表头 |
| `pr->offset` | `PhysRegion.offset` | 在 vir_region 中的偏移 |
| `pr->memtype` | `PhysRegion.memtype` | 内存类型回调 |
| `pr->next_ph_list` | `PhysRegion.next_ph_list` | 同一 phys_block 的下一个引用 |

---

## 2. C 源码分析

### 2.1 phys_region 结构体

**源码定义** (`phys_region.h:9`):

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

| 字段 | 类型 | 说明 |
|------|------|------|
| `ph` | `phys_block*` | 指向物理块，可能为 NULL（未分配物理页） |
| `parent` | `vir_region*` | 所属虚拟区域，yielded 时为 NULL |
| `offset` | `vir_bytes` | 在 vir_region 中的字节偏移 |
| `memtype` | `mem_type_t*` | 内存类型回调（anon、file 等） |
| `next_ph_list` | `phys_region*` | 链向同一 phys_block 的其他引用 |

**与 vir_region 和 phys_block 的关系**:

```
vir_region (vaddr=0x1000, length=0x3000)
    │
    ├── physblocks[0] ──→ phys_region ──→ phys_block (phys=0x8000, refcount=1)
    │                         │
    │                         └─ offset=0, memtype=anon
    │
    ├── physblocks[1] ──→ phys_region ──→ phys_block (phys=0x9000, refcount=2)
    │                         │                  │
    │                         └─ offset=0x1000   └─ firstregion → ...
    │
    └── physblocks[2] ──→ NULL (未分配)
```

**关键约束**:

1. **一对一映射**: 每个 `phys_region` 只属于一个 `vir_region`
2. **多对一引用**: 多个 `phys_region` 可引用同一 `phys_block`（共享/COW）
3. **偏移对齐**: `offset` 必须是页大小的倍数
4. **生命周期**: `phys_region` 生命周期由 `vir_region` 管理

### 2.2 链表结构

#### 2.2.1 next_ph_list - 物理块引用链表

**源码定义** (`phys_region.h:20`):

```c
struct phys_region {
    struct phys_block  *ph;            // 指向物理块
    struct vir_region  *parent;        // 所属虚拟区域
    vir_bytes           offset;        // 区域内偏移
    struct phys_region *next_ph_list;  // 同一 phys_block 的共享链表
};
```

**链表作用**:

`next_ph_list` 连接**引用同一个 `phys_block` 的所有 `phys_region`**，形成单向链表。这在以下场景中至关重要：

| 场景 | 链表用途 |
|------|---------|
| **CoW（写时复制）** | fork 后，父子进程共享物理页。写入时需要遍历链表，将所有映射改为只读 |
| **共享内存** | 多个进程映射同一物理内存，需要追踪所有引用者 |
| **引用计数管理** | 释放物理页时，需要从链表中移除对应的 `phys_region` |

**链表结构示意**:

```
phys_block (refcount=3)
    │
    ├── firstregion ──→ phys_region A (进程1, offset=0x1000)
    │                       │
    │                       └── next_ph_list ──→ phys_region B (进程2, offset=0x2000)
    │                                                │
    │                                                └── next_ph_list ──→ phys_region C (进程3, offset=0x3000)
    │                                                                            │
    │                                                                            └── NULL
    │
    └── 物理地址: 0x8000
```

**关键特性**:

1. **头插法插入**: 新的 `phys_region` 总是插入链表头部（`pb_link()` 函数）
2. **不按虚拟地址排序**: 链表顺序仅反映插入顺序，与虚拟地址无关
3. **跨进程链接**: 同一物理块可被不同进程的 `phys_region` 引用
4. **双向关联**: `phys_block.firstregion` 指向链表头，`phys_region.next_ph_list` 指向下一个节点

#### 2.2.2 链表操作

**核心函数** (`pb.c`):

##### 1. pb_link - 插入链表（头插法）

```c
void pb_link(struct phys_region *newphysr, struct phys_block *newpb,
    vir_bytes offset, struct vir_region *parent)
{
    // 设置 phys_region 字段
    newphysr->offset = offset;
    newphysr->ph = newpb;
    newphysr->parent = parent;

    // 头插法：新节点插入链表头部
    newphysr->next_ph_list = newpb->firstregion;
    newpb->firstregion = newphysr;

    // 增加引用计数
    newpb->refcount++;
}
```

**操作步骤**:

```
插入前:
  phys_block.firstregion → A → B → NULL

插入 C:
  1. C->next_ph_list = firstregion (即 A)
  2. firstregion = C
  3. refcount++

插入后:
  phys_block.firstregion → C → A → B → NULL
```

##### 2. pb_unreferenced - 从链表移除

```c
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb = pr->ph;
    
    // 减少引用计数
    pb->refcount--;

    // 从链表中移除
    if(pb->firstregion == pr) {
        // 情况1：移除的是头节点
        pb->firstregion = pr->next_ph_list;
    } else {
        // 情况2：移除的是中间或尾部节点
        struct phys_region *others;
        for(others = pb->firstregion; others; others = others->next_ph_list) {
            if(others->next_ph_list == pr) {
                others->next_ph_list = pr->next_ph_list;
                break;
            }
        }
    }

    // 如果引用计数为0，释放物理块
    if(pb->refcount == 0) {
        assert(!pb->firstregion);
        pr->memtype->ev_unreference(pr);
        SLABFREE(pb);
    }

    pr->ph = NULL;
}
```

**移除操作示意**:

```
移除前:
  firstregion → A → B → C → NULL

移除 B:
  1. 找到 B 的前驱 A
  2. A->next_ph_list = B->next_ph_list (即 C)
  3. refcount--

移除后:
  firstregion → A → C → NULL
```

##### 3. 遍历链表

```c
// 遍历引用同一物理块的所有 phys_region
struct phys_region *others;
for(others = pb->firstregion; others; others = others->next_ph_list) {
    assert(others->ph == pb);  // 验证一致性
    
    // 对每个引用者执行操作
    // 例如：CoW 时设置只读映射
    map_ph_writept(vmp, others->parent, others);
}
```

**遍历场景**:

| 场景 | 遍历目的 |
|------|---------|
| **CoW 触发** | 遍历所有引用者，将页表项改为只读 |
| **引用计数验证** | 检查链表长度是否等于 `refcount` |
| **内存回收** | 确认没有引用者后释放物理页 |

### 2.3 偏移量管理

#### 2.3.1 offset - 在 vir_region 中的偏移

**源码定义** (`phys_region.h:12`):

```c
typedef struct phys_region {
    struct phys_block  *ph;
    struct vir_region  *parent;
    vir_bytes           offset;  // 在 vir_region 中的字节偏移
    // ...
} phys_region_t;
```

**offset 的作用**:

`offset` 表示该 `phys_region` 在所属 `vir_region` 中的**字节偏移量**，是连接虚拟地址和物理区域的关键纽带。

| 计算方向 | 公式 | 说明 |
|---------|------|------|
| **虚拟地址 → offset** | `offset = virtual_addr - vr->vaddr` | 从进程虚拟地址计算区域内偏移 |
| **offset → 虚拟地址** | `virtual_addr = vr->vaddr + offset` | 从偏移计算进程虚拟地址 |
| **offset → 数组索引** | `index = offset / VM_PAGE_SIZE` | 从偏移计算 physblocks 数组索引 |

**关键约束**:

```c
// region.c:60
struct phys_region *physblock_get(struct vir_region *region, vir_bytes offset)
{
    int i;
    struct phys_region *foundregion;
    
    // 约束1: 必须是页大小的倍数
    assert(!(offset % VM_PAGE_SIZE));
    
    // 约束2: 必须在区域范围内
    assert(offset < region->length);
    
    // 计算数组索引
    i = offset / VM_PAGE_SIZE;
    
    // 从数组中获取 phys_region
    if((foundregion = region->physblocks[i]))
        assert(foundregion->offset == offset);  // 约束3: offset 必须一致
    
    return foundregion;
}
```

**约束总结**:

1. **对齐要求**: `offset % VM_PAGE_SIZE == 0`（必须是页大小的倍数）
2. **范围限制**: `0 <= offset < vir_region.length`（必须在区域范围内）
3. **一致性**: `physblocks[offset / PAGE_SIZE]->offset == offset`（数组位置与offset一致）

**使用场景**:

##### 1. 页表映射

```c
// region.c:153 - map_ph_writept()
r = pt_writemap(vmp, &vmp->vm_pt, 
    vr->vaddr + pr->offset,  // 虚拟地址 = 区域起始 + 偏移
    pb->phys,                 // 物理地址
    VM_PAGE_SIZE, 
    PTF_PRESENT | PTF_USER | rw, 
    WMF_VERIFY);
```

**地址转换示意**:

```
进程虚拟地址空间:
  vaddr=0x400000                vaddr=0x403000
       │                             │
       └──────── vir_region ─────────┘
                 length=0x3000
                 
       ┌──────────┬──────────┬──────────┐
       │ offset=0 │offset=0x1000│offset=0x2000│
       │  page 0  │   page 1   │   page 2   │
       └────┬─────┴─────┬─────┴─────┬─────┘
            │           │           │
            ▼           ▼           ▼
       phys_block   phys_block   phys_block
       phys=0x8000  phys=0x9000  phys=0xA000

访问虚拟地址 0x401500:
  1. 找到 vir_region (vaddr=0x400000)
  2. offset = 0x401500 - 0x400000 = 0x1500
  3. page_index = 0x1500 / 0x1000 = 1
  4. phys_region = physblocks[1]
  5. phys_addr = physblocks[1]->ph->phys + (0x1500 % 0x1000)
                = 0x9000 + 0x500 = 0x9500
```

##### 2. 地址查找

```c
// region.c:617 - map_lookup()
struct vir_region *map_lookup(struct vmproc *vmp,
    vir_bytes offset, struct phys_region **physr)
{
    struct vir_region *r;

    // 在 AVL 树中查找包含该地址的区域
    if((r = region_search(&vmp->vm_regions_avl, offset, AVL_LESS_EQUAL))) {
        vir_bytes ph;
        if(offset >= r->vaddr && offset < r->vaddr + r->length) {
            // 计算区域内偏移
            ph = offset - r->vaddr;
            if(physr) {
                *physr = physblock_get(r, ph);
                if(*physr) 
                    assert((*physr)->offset == ph);
            }
            return r;
        }
    }
    return NULL;
}
```

##### 3. 区域调整

```c
// region.c:1124 - 区域收缩时调整 offset
if(pr->offset >= offset) {
    assert(pr->offset >= len);
    pr->offset -= len;  // 调整偏移量
}
```

**设计意义**:

| 设计要点 | 说明 |
|---------|------|
| **解耦虚拟地址** | `phys_region` 不直接存储虚拟地址，通过 `offset` 间接计算 |
| **支持区域移动** | 移动 `vir_region` 时只需修改 `vaddr`，无需调整每个 `phys_region` |
| **数组索引优化** | `offset / PAGE_SIZE` 直接作为数组索引，O(1) 访问 |
| **内存节省** | 相比存储完整虚拟地址，offset 通常更小（32位足够） |

### 2.4 phys_block 引用

**源码定义** (`phys_region.h:10`):

```c
typedef struct phys_region {
    struct phys_block  *ph;      // 指向物理块（可能为 NULL）
    struct vir_region  *parent;
    vir_bytes           offset;
    // ...
} phys_region_t;
```

**ph 指针的作用**:

`ph` 是指向 `phys_block` 的指针，是 `phys_region` 的核心字段，建立了虚拟区域到物理内存的映射。

| 访问内容 | 代码示例 | 说明 |
|---------|---------|------|
| **物理地址** | `pr->ph->phys` | 获取物理内存地址，用于页表映射 |
| **引用计数** | `pr->ph->refcount` | 判断是否共享（CoW） |
| **链表头** | `pr->ph->firstregion` | 遍历所有引用此物理块的区域 |
| **标志位** | `pr->ph->flags` | 检查物理块状态（如是否在缓存中） |

**NULL 值的含义**:

`ph` 可能为 `NULL`，表示该虚拟页**尚未分配物理内存**（延迟分配）。

```c
// region.c:686 - 检查是否有物理页
if(!(ph = physblock_get(region, offset))) {
    // 没有物理页，需要分配
}
```

**使用场景**:

##### 1. 页表映射

```c
// region.c:261 - map_ph_writept()
int map_ph_writept(struct vmproc *vmp, struct vir_region *vr,
    struct phys_region *pr)
{
    struct phys_block *pb = pr->ph;
    
    assert(pb);  // 必须有物理块
    assert(pb->refcount > 0);
    
    // 使用物理地址进行页表映射
    pt_writemap(vmp, &vmp->vm_pt, 
        vr->vaddr + pr->offset,  // 虚拟地址
        pb->phys,                 // 物理地址
        VM_PAGE_SIZE, 
        flags, 
        WMF_OVERWRITE);
}
```

##### 2. CoW 判断

```c
// region.c:886 - 判断是否需要 CoW
if(ph->ph->refcount != 1) {
    // refcount > 1，说明被共享，需要 CoW
    // 复制物理页内容
    sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE);
}
```

##### 3. 引用计数管理

```c
// pb.c:96 - pb_unreferenced()
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb = pr->ph;
    
    assert(pb->refcount > 0);
    pb->refcount--;  // 减少引用计数
    
    if(pb->refcount == 0) {
        // 引用计数为0，释放物理块
        assert(!pb->firstregion);
        pr->memtype->ev_unreference(pr);
        SLABFREE(pb);
    }
    
    pr->ph = NULL;  // 清空指针
}
```

##### 4. 内存统计

```c
// region.c:1526 - 计算加权内存使用
weighted += VM_PAGE_SIZE / pr->ph->refcount;
// 共享内存按引用计数分摊计算
```

**指针生命周期**:

```
phys_region 生命周期:
  │
  ├─ 创建时: ph = NULL (未分配物理页)
  │
  ├─ 分配物理页: ph = pb_new(phys)
  │               pb_link(pr, pb, offset, region)
  │
  ├─ 使用中: 访问 ph->phys, ph->refcount 等
  │
  └─ 释放时: pb_unreferenced(region, pr, 1)
             pr->ph = NULL
```

**关键约束**:

1. **非空检查**: 使用前必须检查 `ph != NULL`
2. **引用计数一致性**: `ph->refcount` 必须大于 0
3. **链表一致性**: `ph->firstregion` 链表中必须包含当前 `phys_region`
4. **所有权**: `phys_region` 不拥有 `phys_block`，仅引用（共享所有权）

**设计意义**:

| 设计要点 | 说明 |
|---------|------|
| **间接引用** | 通过指针实现多对一映射（多个 phys_region 共享一个 phys_block） |
| **延迟分配** | `ph = NULL` 支持按需分配物理页 |
| **引用计数** | 配合 `refcount` 实现自动内存管理 |
| **共享内存** | 多个进程可以通过不同的 `phys_region` 共享同一物理页 |

---

## 3. Rust 设计决策

### 3.1 PhysRegion 结构

**Rust 实现对比**:

```rust
#[derive(Debug)]
pub struct PhysRegion {
    pub ph: Option<*mut PhysBlock>,      // 指向物理块（可能为 NULL）
    pub parent: Option<*mut VirRegion>,  // 所属虚拟区域
    pub offset: VirBytes,                 // 区域内偏移
    pub next_ph_list: Option<*mut PhysRegion>, // 链表下一个节点
}
```

**字段设计决策**:

| 字段 | Minix3 C | Rust 实现 | 设计理由 |
|------|----------|-----------|---------|
| `ph` | `struct phys_block *ph` | `Option<*mut PhysBlock>` | NULL → None，显式表达可能为空 |
| `parent` | `struct vir_region *parent` | `Option<*mut VirRegion>` | 同上，支持 yielded 状态 |
| `offset` | `vir_bytes offset` | `VirBytes` | 类型别名，保持语义清晰 |
| `next_ph_list` | `struct phys_region *next_ph_list` | `Option<*mut PhysRegion>` | 链表节点，NULL 表示链尾 |

**为什么使用裸指针？**

Minix3 使用 C 指针实现多对一映射和链表结构。Rust 中使用裸指针的原因：

1. **多对一映射**: 多个 `PhysRegion` 共享同一 `PhysBlock`，无法用单一所有权表达
2. **循环引用**: `PhysBlock.first_region` → `PhysRegion` → `PhysRegion.next_ph_list` 形成循环
3. **性能**: 避免智能指针的运行时开销（引用计数已手动管理）
4. **兼容性**: 与 C 代码交互时更直接

**生命周期管理**:

```
所有权关系:
  VirRegion (拥有)
      │
      └─→ Vec<Option<Box<PhysRegion>>>  // physblocks 数组
              │
              └─→ PhysRegion (被拥有)
                      │
                      ├─→ ph: *mut PhysBlock (引用，不拥有)
                      │       └─→ refcount 管理生命周期
                      │
                      ├─→ parent: *mut VirRegion (反向引用)
                      │
                      └─→ next_ph_list: *mut PhysRegion (链表引用)

PhysBlock 生命周期:
  创建: pb_new() → refcount = 0
  引用: pb_link() → refcount++
  释放: pb_unreferenced() → refcount--
        refcount == 0 → SLABFREE(pb)
```

**安全性保证**:

虽然使用裸指针，但通过以下机制保证安全：

1. **生命周期约束**: `PhysRegion` 由 `VirRegion` 拥有，生命周期绑定
2. **引用计数**: `PhysBlock.refcount` 确保不会过早释放
3. **封装 unsafe**: 所有裸指针操作封装在方法中，外部使用安全
4. **断言检查**: Debug 模式下验证指针有效性

**示例：安全的链表操作**:

```rust
impl PhysRegion {
    /// 插入到物理块的引用链表（头插法）
    pub fn link_to_block(&mut self, block: *mut PhysBlock, parent: *mut VirRegion) {
        unsafe {
            // 安全性：调用者确保 block 和 parent 有效
            self.ph = Some(block);
            self.parent = Some(parent);
            
            // 头插法
            self.next_ph_list = (*block).first_region;
            (*block).first_region = Some(self as *mut PhysRegion);
            (*block).refcount = (*block).refcount.saturating_add(1);
        }
    }
}
```

**内存布局**:

```
PhysRegion 内存布局 (64位系统):
  ┌─────────────────────────────────────┐
  │ ph: Option<*mut PhysBlock> (16字节) │
  ├─────────────────────────────────────┤
  │ parent: Option<*mut VirRegion>      │
  │       (16字节)                       │
  ├─────────────────────────────────────┤
  │ offset: VirBytes (8字节)            │
  ├─────────────────────────────────────┤
  │ next_ph_list: Option<*mut ...>      │
  │       (16字节)                       │
  └─────────────────────────────────────┘
  总计: 56 字节 (含填充)

对比 C 结构体:
  struct phys_region {
      struct phys_block  *ph;        // 8字节
      struct vir_region  *parent;    // 8字节
      vir_bytes           offset;    // 8字节
      struct phys_region *next_ph_list; // 8字节
  };
  总计: 32 字节
```

**Rust 额外开销**: Option 的 discriminant 占用额外空间，但换来空值安全。

### 3.2 链表安全

**链表操作的潜在风险**:

使用裸指针实现链表存在以下风险：

| 风险 | 说明 | 后果 |
|------|------|------|
| **悬垂指针** | 访问已释放的节点 | 未定义行为、崩溃 |
| **循环引用** | 链表形成环 | 内存泄漏 |
| **并发访问** | 多线程同时修改 | 数据竞争 |
| **断链** | 节点未正确连接 | 内存泄漏、访问错误 |
| **重复插入** | 同一节点插入两次 | 链表损坏 |

**安全性保证机制**:

##### 1. 不变量维护

每个链表操作必须维护以下不变量：

```rust
// 不变量1: 引用计数一致性
// refcount == 链表中节点数量
assert!(pb.refcount == count_nodes_in_list(pb.first_region));

// 不变量2: 双向关联
// 每个节点的 ph 指针必须指向所属的 PhysBlock
for node in list:
    assert!(node.ph == Some(&pb));

// 不变量3: 无环
// 链表遍历必须终止
assert!(no_cycles_in_list(pb.first_region));

// 不变量4: 唯一性
// 每个节点只能出现在一个链表中
assert!(node.ph.is_none() || node.ph == Some(current_block));
```

##### 2. 安全的插入操作

```rust
impl PhysRegion {
    /// 安全的链表插入（头插法）
    pub fn link_to_block(&mut self, block: *mut PhysBlock, parent: *mut VirRegion) {
        unsafe {
            // 前置条件检查
            debug_assert!(!block.is_null());
            debug_assert!(!parent.is_null());
            debug_assert!(self.ph.is_none()); // 确保未在其他链表中
            
            // 设置字段
            self.ph = Some(block);
            self.parent = Some(parent);
            
            // 头插法：O(1) 时间复杂度
            self.next_ph_list = (*block).first_region;
            (*block).first_region = Some(self as *mut PhysRegion);
            
            // 维护引用计数
            (*block).refcount = (*block).refcount.saturating_add(1);
            
            // 后置条件检查
            debug_assert!((*block).refcount > 0);
            debug_assert!(self.ph.is_some());
        }
    }
}
```

**安全性保证**：
- 前置条件：确保指针有效，节点未在其他链表中
- 操作原子性：所有步骤在同一个 unsafe 块中完成
- 后置条件：验证引用计数和指针状态

##### 3. 安全的移除操作

```rust
impl PhysRegion {
    /// 安全的链表移除
    pub fn unlink_from_block(&mut self) -> bool {
        if let Some(block) = self.ph {
            unsafe {
                // 前置条件检查
                debug_assert!((*block).refcount > 0);
                debug_assert!(self.is_in_list(block));
                
                // 减少引用计数
                (*block).refcount = (*block).refcount.saturating_sub(1);
                
                // 从链表中移除
                if (*block).first_region == Some(self as *mut PhysRegion) {
                    // 情况1：移除头节点
                    (*block).first_region = self.next_ph_list;
                } else if let Some(first) = (*block).first_region {
                    // 情况2：移除中间或尾部节点
                    let mut current = first;
                    loop {
                        let next = (*current).next_ph_list;
                        if next == Some(self as *mut PhysRegion) {
                            (*current).next_ph_list = self.next_ph_list;
                            break;
                        }
                        match next {
                            Some(n) => current = n,
                            None => {
                                // 错误：节点不在链表中
                                debug_assert!(false, "Node not found in list");
                                break;
                            }
                        }
                    }
                }
                
                // 清空指针
                self.ph = None;
                self.next_ph_list = None;
                
                // 后置条件检查
                debug_assert!(self.ph.is_none());
                debug_assert!(self.next_ph_list.is_none());
                
                // 返回是否应该释放物理块
                (*block).refcount == 0
            }
        } else {
            false
        }
    }
    
    /// 检查节点是否在链表中
    fn is_in_list(&self, block: *mut PhysBlock) -> bool {
        unsafe {
            let mut current = (*block).first_region;
            while let Some(ptr) = current {
                if ptr == self as *const PhysRegion {
                    return true;
                }
                current = (*ptr).next_ph_list;
            }
            false
        }
    }
}
```

##### 4. 安全的遍历操作

```rust
impl PhysRegion {
    /// 安全的链表遍历
    pub fn iterate_block_refs<F>(block: &PhysBlock, mut f: F)
    where
        F: FnMut(&PhysRegion),
    {
        let mut current = block.first_region;
        let mut visited = std::collections::HashSet::new();
        
        while let Some(ptr) = current {
            // 环检测
            if visited.contains(&ptr) {
                debug_assert!(false, "Cycle detected in list");
                break;
            }
            visited.insert(ptr);
            
            unsafe {
                let region = &*ptr;
                
                // 验证节点一致性
                debug_assert!(region.ph == Some(block as *const PhysBlock as *mut PhysBlock));
                
                f(region);
                current = region.next_ph_list;
            }
        }
    }
}
```

##### 5. 错误处理策略

| 错误类型 | 检测方法 | 处理策略 |
|---------|---------|---------|
| **悬垂指针** | Debug 模式下验证指针有效性 | panic 并打印调试信息 |
| **循环引用** | 遍历时记录访问节点 | 检测到环时终止遍历 |
| **断链** | 引用计数与链表长度不一致 | panic 并打印调试信息 |
| **重复插入** | 检查 `ph.is_none()` | panic 并打印调试信息 |
| **节点丢失** | 遍历时找不到目标节点 | panic 并打印调试信息 |

**测试策略**:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_link_unlink_consistency() {
        // 测试插入和移除的一致性
        let mut block = PhysBlock::new(0x8000);
        let mut region = PhysRegion::new(VirBytes(0x1000));
        
        region.link_to_block(&mut block, std::ptr::null_mut());
        assert_eq!(block.refcount, 1);
        assert!(block.first_region.is_some());
        
        region.unlink_from_block();
        assert_eq!(block.refcount, 0);
        assert!(block.first_region.is_none());
    }
    
    #[test]
    fn test_multiple_regions() {
        // 测试多个节点的情况
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = PhysRegion::new(VirBytes(0x1000));
        let mut region2 = PhysRegion::new(VirBytes(0x2000));
        
        region1.link_to_block(&mut block, std::ptr::null_mut());
        region2.link_to_block(&mut block, std::ptr::null_mut());
        
        // 验证链表结构
        assert_eq!(block.refcount, 2);
        
        // 验证遍历
        let mut count = 0;
        PhysRegion::iterate_block_refs(&block, |_| count += 1);
        assert_eq!(count, 2);
    }
    
    #[test]
    fn test_unlink_middle_node() {
        // 测试移除中间节点
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = PhysRegion::new(VirBytes(0x1000));
        let mut region2 = PhysRegion::new(VirBytes(0x2000));
        let mut region3 = PhysRegion::new(VirBytes(0x3000));
        
        region1.link_to_block(&mut block, std::ptr::null_mut());
        region2.link_to_block(&mut block, std::ptr::null_mut());
        region3.link_to_block(&mut block, std::ptr::null_mut());
        
        // 移除中间节点
        region2.unlink_from_block();
        
        // 验证链表完整性
        let mut count = 0;
        PhysRegion::iterate_block_refs(&block, |_| count += 1);
        assert_eq!(count, 2);
    }
}
```

**最佳实践**:

1. **封装 unsafe**: 所有裸指针操作封装在方法中，外部使用安全
2. **断言检查**: Debug 模式下验证所有不变量
3. **原子操作**: 链表操作在同一个 unsafe 块中完成
4. **测试覆盖**: 测试所有边界情况（空链表、单节点、多节点、移除头/中/尾）
5. **文档注释**: 明确说明前置条件、后置条件和安全性保证

### 3.3 引用关系

**三层结构概览**:

Minix3 内存管理采用三层结构，从虚拟地址到物理内存的映射关系如下：

```
进程虚拟地址空间
    │
    ├─→ VirRegion (虚拟区域)
    │       │
    │       ├─→ vaddr: 虚拟地址起始
    │       ├─→ length: 区域长度
    │       └─→ physblocks: Vec<Option<Box<PhysRegion>>>
    │               │
    │               └─→ PhysRegion (物理区域)
    │                       │
    │                       ├─→ ph: *mut PhysBlock (指向物理块)
    │                       ├─→ parent: *mut VirRegion (反向引用)
    │                       ├─→ offset: 在虚拟区域中的偏移
    │                       └─→ next_ph_list: 链表下一个节点
    │                               │
    │                               └─→ PhysBlock (物理块)
    │                                       │
    │                                       ├─→ phys: 物理地址
    │                                       ├─→ refcount: 引用计数
    │                                       └─→ first_region: 链表头
    │
    └─→ 页表映射
            │
            └─→ 物理内存
```

**Minix3 源码定义**:

```c
// vir_region (region.h:50)
typedef struct vir_region {
    vir_bytes vaddr;                    // 虚拟地址起始
    vir_bytes length;                   // 区域长度
    struct phys_region **physblocks;    // 物理区域数组
    u16_t flags;                        // 标志位
    struct vmproc *parent;              // 所属进程
    // ...
} region_t;

// phys_region (phys_region.h:10)
typedef struct phys_region {
    struct phys_block  *ph;             // 指向物理块
    struct vir_region  *parent;         // 所属虚拟区域
    vir_bytes           offset;         // 偏移量
    struct phys_region *next_ph_list;   // 链表下一个
    // ...
} phys_region_t;

// phys_block (region.h:23)
struct phys_block {
    phys_bytes           phys;          // 物理地址
    struct phys_region  *firstregion;   // 链表头
    u8_t                 refcount;      // 引用计数
    u8_t                 flags;         // 标志位
};
```

**引用关系详解**:

##### 1. VirRegion → PhysRegion (一对多)

**关系类型**: 所有权关系

```rust
pub struct VirRegion {
    pub vaddr: VirBytes,
    pub length: VirBytes,
    pub physblocks: Vec<Option<Box<PhysRegion>>>,  // 拥有 PhysRegion
    // ...
}
```

**特点**：
- **所有权**: `VirRegion` 拥有 `PhysRegion` 的所有权
- **生命周期**: `PhysRegion` 的生命周期绑定到 `VirRegion`
- **数量关系**: 一个 `VirRegion` 可以有多个 `PhysRegion`（每个页对应一个）
- **索引方式**: 通过偏移量计算索引：`index = offset / PAGE_SIZE`

**示例**：

```rust
// 创建一个 3 页的虚拟区域
let vr = VirRegion::new(VirBytes(0x1000), VirBytes(12288));  // 3 pages

// physblocks 数组有 3 个元素
assert_eq!(vr.physblocks.len(), 3);

// 访问第 2 页的 PhysRegion
let pr = vr.physblocks[1].as_ref();  // offset = 4096
```

##### 2. PhysRegion → PhysBlock (多对一)

**关系类型**: 引用关系（共享）

```rust
pub struct PhysRegion {
    pub ph: Option<*mut PhysBlock>,  // 引用，不拥有
    // ...
}

pub struct PhysBlock {
    pub phys: u64,
    pub refcount: u8,                 // 引用计数
    pub first_region: Option<*mut PhysRegion>,  // 链表头
    // ...
}
```

**特点**：
- **共享性**: 多个 `PhysRegion` 可以引用同一个 `PhysBlock`（CoW）
- **引用计数**: `PhysBlock.refcount` 记录引用数量
- **链表管理**: 通过 `first_region` 和 `next_ph_list` 遍历所有引用者
- **生命周期**: 独立于 `PhysRegion`，由引用计数管理

**示例**：

```rust
// fork 后，父子进程共享同一物理块
let block = PhysBlock::new(0x8000);

let mut parent_pr = PhysRegion::new(VirBytes(0x1000));
let mut child_pr = PhysRegion::new(VirBytes(0x1000));

parent_pr.link_to_block(&mut block, parent_vr);
child_pr.link_to_block(&mut block, child_vr);

// 引用计数为 2
assert_eq!(block.refcount, 2);

// 遍历所有引用者
let mut count = 0;
PhysRegion::iterate_block_refs(&block, |_| count += 1);
assert_eq!(count, 2);
```

##### 3. PhysRegion → VirRegion (反向引用)

**关系类型**: 反向引用

```rust
pub struct PhysRegion {
    pub parent: Option<*mut VirRegion>,  // 反向引用
    // ...
}
```

**特点**：
- **反向引用**: 用于从 `PhysRegion` 找到所属的 `VirRegion`
- **可能为空**: `yielded` 状态下 `parent` 为 `None`
- **用途**: CoW 时需要访问 `VirRegion` 的信息（如虚拟地址）

**示例**：

```rust
// CoW 操作时需要访问 parent
impl PhysRegion {
    pub fn get_virtual_addr(&self) -> Option<VirBytes> {
        unsafe {
            self.parent.map(|vr| {
                VirBytes((*vr).vaddr.0 + self.offset.0)
            })
        }
    }
}
```

**引用关系图示**:

```
进程 A (父进程)                      进程 B (子进程)
    │                                    │
    ├─→ VirRegion_A                      ├─→ VirRegion_B
    │       │                            │       │
    │       ├─→ PhysRegion_A1            │       ├─→ PhysRegion_B1
    │       │       │                    │       │       │
    │       │       └──────┐    ┌────────┘       │       │
    │       │              ↓    ↓                │       │
    │       │          PhysBlock_1               │       │
    │       │              ↑    ↑                │       │
    │       │       ┌──────┘    └────────┐       │       │
    │       │       │                    │       │       │
    │       ├─→ PhysRegion_A2            │       ├─→ PhysRegion_B2
    │       │       │                    │       │       │
    │       │       └─→ PhysBlock_2      │       │       │
    │       │                            │       │       │
    │       └─→ PhysRegion_A3            │       └─→ PhysRegion_B3
    │               │                    │               │
    │               └─→ PhysBlock_3      │               │
    │                                    │               │
    └─→ 独立的物理块                     └─→ 共享的物理块 (CoW)

图例：
  VirRegion: 虚拟区域
  PhysRegion: 物理区域
  PhysBlock: 物理块
  → : 引用关系
  ─→ : 所有权关系
```

**引用计数管理**:

| 操作 | 引用计数变化 | 说明 |
|------|-------------|------|
| **创建 PhysBlock** | refcount = 0 | 初始状态 |
| **链接 PhysRegion** | refcount++ | `pb_link()` |
| **fork 复制** | refcount++ | 共享物理块 |
| **CoW 复制** | refcount-- (旧) <br> refcount++ (新) | 写时复制 |
| **解除链接** | refcount-- | `pb_unreferenced()` |
| **释放 PhysBlock** | refcount == 0 | 自动释放 |

**关键函数**:

```rust
// 1. 建立引用关系
pub fn link_to_block(&mut self, block: *mut PhysBlock, parent: *mut VirRegion) {
    // 设置引用
    self.ph = Some(block);
    self.parent = Some(parent);
    
    // 插入链表
    self.next_ph_list = (*block).first_region;
    (*block).first_region = Some(self);
    
    // 增加引用计数
    (*block).refcount++;
}

// 2. 解除引用关系
pub fn unlink_from_block(&mut self) -> bool {
    // 减少引用计数
    (*block).refcount--;
    
    // 从链表移除
    // ...
    
    // 返回是否应该释放
    (*block).refcount == 0
}

// 3. 遍历所有引用者
pub fn iterate_block_refs<F>(block: &PhysBlock, f: F) {
    let mut current = block.first_region;
    while let Some(ptr) = current {
        f(&*ptr);
        current = (*ptr).next_ph_list;
    }
}
```

**设计意义**:

1. **灵活性**: 三层结构支持多种内存映射模式
2. **共享性**: 多对一关系支持 CoW 和共享内存
3. **效率**: 通过链表快速遍历所有引用者
4. **安全性**: 引用计数确保物理块不会过早释放
5. **可维护性**: 清晰的层次结构便于理解和维护

---

## 4. 实现详解

### 4.1 创建物理区域

**创建流程概览**:

创建物理区域涉及三个关键步骤：分配、初始化、链接。

```
创建流程:
  1. 分配 PhysRegion 内存
     ↓
  2. 初始化 PhysRegion 字段
     ↓
  3. 链接到 PhysBlock (pb_link)
     ↓
  4. 设置到 VirRegion (physblock_set)
```

**Minix3 源码分析**:

##### 1. pb_reference() - 创建并引用物理块

**源码位置**: [pb.c:71](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/pb.c#L71)

```c
struct phys_region *pb_reference(struct phys_block *newpb,
    vir_bytes offset, struct vir_region *region, mem_type_t *memtype)
{
    struct phys_region *newphysr;

    // 1. 分配 PhysRegion 内存
    if(!SLABALLOC(newphysr)) {
        printf("vm: pb_reference: couldn't allocate phys region\n");
        return NULL;
    }

    // 2. 设置内存类型
    newphysr->memtype = memtype;

    // 3. 链接到 PhysBlock
    pb_link(newphysr, newpb, offset, region);

    // 4. 设置到 VirRegion
    physblock_set(region, offset, newphysr);

    return newphysr;
}
```

**关键步骤**：
1. **分配内存**: 使用 SLABALLOC 从 slab 分配器分配 PhysRegion
2. **设置 memtype**: 设置内存类型（如 anon、file-backed 等）
3. **链接**: 调用 `pb_link()` 建立引用关系
4. **注册**: 调用 `physblock_set()` 将 PhysRegion 注册到 VirRegion

##### 2. pb_link() - 链接到物理块

**源码位置**: [pb.c:61](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/pb.c#L61)

```c
void pb_link(struct phys_region *newphysr, struct phys_block *newpb,
    vir_bytes offset, struct vir_region *parent)
{
    // 设置字段
    newphysr->offset = offset;
    newphysr->ph = newpb;
    newphysr->parent = parent;

    // 头插法插入链表
    newphysr->next_ph_list = newpb->firstregion;
    newpb->firstregion = newphysr;

    // 增加引用计数
    newpb->refcount++;
}
```

**操作详解**：
1. **设置 offset**: 记录在 VirRegion 中的偏移量
2. **设置 ph**: 指向 PhysBlock
3. **设置 parent**: 反向引用 VirRegion
4. **插入链表**: 头插法，O(1) 时间复杂度
5. **增加引用计数**: 维护引用计数一致性

##### 3. physblock_set() - 设置到虚拟区域

**源码位置**: [region.c:72](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/region.c#L72)

```c
void physblock_set(struct vir_region *region, vir_bytes offset,
    struct phys_region *newphysr)
{
    int i;
    struct vmproc *proc;

    // 验证参数
    assert(!(offset % VM_PAGE_SIZE));
    assert(offset < region->length);

    // 计算索引
    i = offset / VM_PAGE_SIZE;

    proc = region->parent;
    assert(proc);

    if(newphysr) {
        // 确保该位置为空
        assert(!region->physblocks[i]);
        assert(newphysr->offset == offset);

        // 更新进程内存统计
        proc->vm_total += VM_PAGE_SIZE;
        if (proc->vm_total > proc->vm_total_max)
            proc->vm_total_max = proc->vm_total;
    } else {
        // 移除 PhysRegion
        assert(region->physblocks[i]);
        proc->vm_total -= VM_PAGE_SIZE;
    }

    // 设置指针
    region->physblocks[i] = newphysr;
}
```

**关键操作**：
1. **验证偏移量**: 确保页对齐且在范围内
2. **计算索引**: `index = offset / PAGE_SIZE`
3. **更新统计**: 维护进程的内存使用统计
4. **设置指针**: 将 PhysRegion 指针存入数组

**Rust 实现**:

```rust
impl PhysRegion {
    /// 创建并引用物理块
    ///
    /// 对应 Minix3: `pb_reference()`
    ///
    /// # Arguments
    ///
    /// * `block` - 物理块指针
    /// * `offset` - 在虚拟区域中的偏移量
    /// * `parent` - 所属虚拟区域
    ///
    /// # Returns
    ///
    /// 返回新创建的 PhysRegion
    pub fn create_and_link(
        block: *mut PhysBlock,
        offset: VirBytes,
        parent: *mut VirRegion,
    ) -> Box<Self> {
        // 1. 创建 PhysRegion
        let mut region = Box::new(PhysRegion::new(offset));

        // 2. 链接到 PhysBlock
        region.link_to_block(block, parent);

        region
    }
}

impl VirRegion {
    /// 设置物理区域到指定偏移量
    ///
    /// 对应 Minix3: `physblock_set()`
    ///
    /// # Arguments
    ///
    /// * `offset` - 偏移量（必须页对齐）
    /// * `phys_region` - 物理区域（Option<Box<PhysRegion>>）
    pub fn set_phys_region(&mut self, offset: VirBytes, phys_region: Option<Box<PhysRegion>>) {
        const PAGE_SIZE: u64 = 4096;

        // 验证参数
        debug_assert_eq!(offset.0 % PAGE_SIZE, 0, "offset must be page-aligned");
        debug_assert!(offset.0 < self.length.0, "offset must be within region");

        // 计算索引
        let index = (offset.0 / PAGE_SIZE) as usize;

        if let Some(pr) = &phys_region {
            // 添加 PhysRegion
            debug_assert!(self.physblocks[index].is_none(), "slot must be empty");
            debug_assert_eq!(pr.offset, offset, "offset mismatch");

            // 更新统计（如果有进程引用）
            // self.parent.vm_total += PAGE_SIZE;
        } else {
            // 移除 PhysRegion
            debug_assert!(self.physblocks[index].is_some(), "slot must not be empty");

            // 更新统计
            // self.parent.vm_total -= PAGE_SIZE;
        }

        // 设置指针
        self.physblocks[index] = phys_region;
    }

    /// 获取指定偏移量的物理区域
    ///
    /// 对应 Minix3: `physblock_get()`
    ///
    /// # Arguments
    ///
    /// * `offset` - 偏移量（必须页对齐）
    ///
    /// # Returns
    ///
    /// 返回 PhysRegion 的引用，如果不存在则返回 None
    pub fn get_phys_region(&self, offset: VirBytes) -> Option<&PhysRegion> {
        const PAGE_SIZE: u64 = 4096;

        // 验证参数
        debug_assert_eq!(offset.0 % PAGE_SIZE, 0, "offset must be page-aligned");
        debug_assert!(offset.0 < self.length.0, "offset must be within region");

        // 计算索引
        let index = (offset.0 / PAGE_SIZE) as usize;

        // 返回引用
        self.physblocks[index].as_ref().map(|boxed| boxed.as_ref())
    }
}
```

**使用示例**:

```rust
// 创建虚拟区域（3 页）
let mut vr = VirRegion::new(VirBytes(0x1000), VirBytes(12288));

// 创建物理块
let mut block = Box::new(PhysBlock::new(0x8000));
let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

// 创建并链接物理区域到第 2 页
let phys_region = PhysRegion::create_and_link(
    block_ptr,
    VirBytes(4096),  // offset = 1 * PAGE_SIZE
    &mut vr as *mut VirRegion,
);

// 设置到虚拟区域
vr.set_phys_region(VirBytes(4096), Some(phys_region));

// 验证
assert!(vr.get_phys_region(VirBytes(4096)).is_some());
assert!(vr.get_phys_region(VirBytes(0)).is_none());
```

**错误处理**:

| 错误情况 | Minix3 处理 | Rust 处理 |
|---------|------------|----------|
| **内存分配失败** | 返回 NULL | panic（Box::new 不会失败） |
| **偏移量未对齐** | assert 失败 | debug_assert! panic |
| **偏移量越界** | assert 失败 | debug_assert! panic |
| **位置已占用** | assert 失败 | debug_assert! panic |

**性能考虑**:

1. **分配开销**: PhysRegion 应使用 slab 分配器（后续优化）
2. **链表操作**: 头插法 O(1)，适合频繁插入
3. **数组访问**: O(1) 时间复杂度，适合频繁查找
4. **内存统计**: 每次设置都需要更新，可考虑批量更新

**测试用例**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_link() {
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(0x1000),
            std::ptr::null_mut(),
        );

        // 验证字段
        assert_eq!(region.offset, VirBytes(0x1000));
        assert_eq!(region.ph, Some(block_ptr));

        // 验证引用计数
        assert_eq!(block.refcount, 1);
    }

    #[test]
    fn test_set_and_get_phys_region() {
        let mut vr = VirRegion::new(VirBytes(0x1000), VirBytes(12288));
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let phys_region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(4096),
            std::ptr::null_mut(),
        );

        // 设置
        vr.set_phys_region(VirBytes(4096), Some(phys_region));

        // 获取
        let pr = vr.get_phys_region(VirBytes(4096));
        assert!(pr.is_some());
        assert_eq!(pr.unwrap().offset, VirBytes(4096));

        // 未设置的页
        assert!(vr.get_phys_region(VirBytes(0)).is_none());
    }
}
```

### 4.2 查找物理区域

**查找方式**:

通过偏移量查找物理区域是 O(1) 操作，直接通过数组索引访问。

**Minix3 源码分析**:

**源码位置**: [region.c:60](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/region.c#L60)

```c
struct phys_region *physblock_get(struct vir_region *region, vir_bytes offset)
{
    int i;
    struct phys_region *foundregion;

    // 验证参数
    assert(!(offset % VM_PAGE_SIZE));        // 必须页对齐
    assert(offset < region->length);         // 必须在范围内

    // 计算索引
    i = offset / VM_PAGE_SIZE;

    // 获取 PhysRegion
    if((foundregion = region->physblocks[i]))
        assert(foundregion->offset == offset);  // 验证一致性

    return foundregion;
}
```

**关键步骤**：
1. **验证偏移量**: 确保页对齐且在虚拟区域范围内
2. **计算索引**: `index = offset / PAGE_SIZE`
3. **数组访问**: 直接通过索引访问 physblocks 数组
4. **一致性检查**: 验证找到的 PhysRegion 的 offset 字段

**时间复杂度**: O(1)

**Rust 实现**:

```rust
impl VirRegion {
    /// 获取指定偏移量的物理区域
    ///
    /// 对应 Minix3: `physblock_get()`
    ///
    /// # Arguments
    ///
    /// * `offset` - 偏移量（必须页对齐）
    ///
    /// # Returns
    ///
    /// 返回 PhysRegion 的引用，如果不存在则返回 None
    ///
    /// # Panics
    ///
    /// Debug 模式下，如果偏移量未页对齐或越界会 panic
    pub fn get_phys_region(&self, offset: VirBytes) -> Option<&PhysRegion> {
        const PAGE_SIZE: u64 = 4096;

        // 验证参数
        debug_assert_eq!(offset.0 % PAGE_SIZE, 0, "offset must be page-aligned");
        debug_assert!(offset.0 < self.length.0, "offset must be within region");

        // 计算索引
        let index = (offset.0 / PAGE_SIZE) as usize;

        // 返回引用
        self.physblocks[index].as_ref().map(|boxed| boxed.as_ref())
    }

    /// 获取指定偏移量的物理区域（可变引用）
    ///
    /// # Arguments
    ///
    /// * `offset` - 偏移量（必须页对齐）
    ///
    /// # Returns
    ///
    /// 返回 PhysRegion 的可变引用，如果不存在则返回 None
    pub fn get_phys_region_mut(&mut self, offset: VirBytes) -> Option<&mut PhysRegion> {
        const PAGE_SIZE: u64 = 4096;

        // 验证参数
        debug_assert_eq!(offset.0 % PAGE_SIZE, 0, "offset must be page-aligned");
        debug_assert!(offset.0 < self.length.0, "offset must be within region");

        // 计算索引
        let index = (offset.0 / PAGE_SIZE) as usize;

        // 返回可变引用
        self.physblocks[index].as_mut().map(|boxed| boxed.as_mut())
    }

    /// 通过虚拟地址查找物理区域
    ///
    /// 便利方法，自动计算偏移量
    ///
    /// # Arguments
    ///
    /// * `vaddr` - 虚拟地址
    ///
    /// # Returns
    ///
    /// 返回 PhysRegion 的引用，如果不存在则返回 None
    pub fn get_phys_region_by_vaddr(&self, vaddr: VirBytes) -> Option<&PhysRegion> {
        let offset = VirBytes(vaddr.0 - self.vaddr.0);
        self.get_phys_region(offset)
    }

    /// 检查指定偏移量是否有物理区域
    ///
    /// # Arguments
    ///
    /// * `offset` - 偏移量（必须页对齐）
    ///
    /// # Returns
    ///
    /// 返回 true 如果存在 PhysRegion
    pub fn has_phys_region(&self, offset: VirBytes) -> bool {
        self.get_phys_region(offset).is_some()
    }
}
```

**使用示例**:

```rust
// 创建虚拟区域（3 页）
let mut vr = VirRegion::new(VirBytes(0x1000), VirBytes(12288));

// 创建并设置物理区域
let mut block = Box::new(PhysBlock::new(0x8000));
let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;
let phys_region = PhysRegion::create_and_link(
    block_ptr,
    VirBytes(4096),
    &mut vr as *mut VirRegion,
);
vr.set_phys_region(VirBytes(4096), Some(phys_region));

// 查找物理区域
let pr = vr.get_phys_region(VirBytes(4096));
assert!(pr.is_some());
assert_eq!(pr.unwrap().offset, VirBytes(4096));

// 未设置的页
assert!(vr.get_phys_region(VirBytes(0)).is_none());

// 通过虚拟地址查找
let pr2 = vr.get_phys_region_by_vaddr(VirBytes(0x1000 + 4096));
assert!(pr2.is_some());

// 检查是否存在
assert!(vr.has_phys_region(VirBytes(4096)));
assert!(!vr.has_phys_region(VirBytes(0)));
```

**查找场景**:

| 场景 | 使用方法 | 说明 |
|------|---------|------|
| **页错误处理** | `get_phys_region()` | 查找是否有物理区域 |
| **CoW 判定** | `get_phys_region()` | 检查引用计数 |
| **页表映射** | `get_phys_region()` | 获取物理地址 |
| **内存统计** | `get_phys_region()` | 遍历所有物理区域 |

**性能考虑**:

1. **数组访问**: O(1) 时间复杂度，非常高效
2. **边界检查**: Rust 自动进行边界检查，无需手动验证
3. **Option 处理**: 使用 Option 类型，避免空指针
4. **缓存友好**: 数组连续存储，缓存命中率高

**错误处理**:

| 错误情况 | Minix3 处理 | Rust 处理 |
|---------|------------|----------|
| **偏移量未对齐** | assert 失败 | debug_assert! panic |
| **偏移量越界** | assert 失败 | debug_assert! panic + 数组越界 panic |
| **PhysRegion 不存在** | 返回 NULL | 返回 None |

**测试用例**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_phys_region() {
        let mut vr = VirRegion::new(VirBytes(0x1000), VirBytes(12288));
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let phys_region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(4096),
            std::ptr::null_mut(),
        );
        vr.set_phys_region(VirBytes(4096), Some(phys_region));

        // 查找存在的物理区域
        let pr = vr.get_phys_region(VirBytes(4096));
        assert!(pr.is_some());
        assert_eq!(pr.unwrap().offset, VirBytes(4096));

        // 查找不存在的物理区域
        assert!(vr.get_phys_region(VirBytes(0)).is_none());
        assert!(vr.get_phys_region(VirBytes(8192)).is_none());
    }

    #[test]
    fn test_get_phys_region_by_vaddr() {
        let mut vr = VirRegion::new(VirBytes(0x1000), VirBytes(12288));
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let phys_region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(4096),
            std::ptr::null_mut(),
        );
        vr.set_phys_region(VirBytes(4096), Some(phys_region));

        // 通过虚拟地址查找
        let pr = vr.get_phys_region_by_vaddr(VirBytes(0x1000 + 4096));
        assert!(pr.is_some());
        assert_eq!(pr.unwrap().offset, VirBytes(4096));
    }

    #[test]
    fn test_has_phys_region() {
        let mut vr = VirRegion::new(VirBytes(0x1000), VirBytes(12288));
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let phys_region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(4096),
            std::ptr::null_mut(),
        );
        vr.set_phys_region(VirBytes(4096), Some(phys_region));

        assert!(vr.has_phys_region(VirBytes(4096)));
        assert!(!vr.has_phys_region(VirBytes(0)));
    }
}
```

### 4.3 释放物理区域

**释放流程**:

释放物理区域涉及解引用 PhysBlock、从链表移除、可能的物理块释放。

```
释放流程:
  1. 减少引用计数 (refcount--)
     ↓
  2. 从链表移除 PhysRegion
     ↓
  3. 如果 refcount == 0
     ├─ 调用 memtype->ev_unreference()
     └─ 释放 PhysBlock
     ↓
  4. 清空 PhysRegion 的 ph 指针
     ↓
  5. 从 VirRegion 移除（可选）
```

**Minix3 源码分析**:

**源码位置**: [pb.c:95](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/pb.c#L95)

```c
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
    struct phys_block *pb;

    pb = pr->ph;
    assert(pb->refcount > 0);

    // 1. 减少引用计数
    USE(pb, pb->refcount--;);

    // 2. 从链表移除
    if(pb->firstregion == pr) {
        // 头节点：直接更新 firstregion
        USE(pb, pb->firstregion = pr->next_ph_list;);
    } else {
        // 非头节点：遍历链表找到前驱
        struct phys_region *others;
        for(others = pb->firstregion; others; others = others->next_ph_list) {
            assert(others->ph == pb);
            if(others->next_ph_list == pr) {
                USE(others, others->next_ph_list = pr->next_ph_list;);
                break;
            }
        }
        assert(others); // 否则不在链表中
    }

    // 3. 引用计数为 0，释放 PhysBlock
    if(pb->refcount == 0) {
        assert(!pb->firstregion);
        int r;
        if((r = pr->memtype->ev_unreference(pr)) != OK)
            panic("unref failed, %d", r);
        SLABFREE(pb);
    }

    // 4. 清空 ph 指针
    pr->ph = NULL;

    // 5. 从 VirRegion 移除（可选）
    if(rm) physblock_set(region, pr->offset, NULL);
}
```

**关键步骤**：

1. **减少引用计数**: `refcount--`，表示少一个引用者
2. **从链表移除**: 
   - 头节点：直接更新 `firstregion`
   - 非头节点：遍历链表找到前驱，更新其 `next_ph_list`
3. **释放 PhysBlock**: 如果 `refcount == 0`，调用 `ev_unreference()` 并释放
4. **清空指针**: 将 `ph` 设为 NULL
5. **从数组移除**: 如果 `rm` 参数为真，从 `physblocks` 数组移除

**参数说明**:

| 参数 | 类型 | 说明 |
|------|------|------|
| `region` | `vir_region*` | 所属虚拟区域 |
| `pr` | `phys_region*` | 要释放的物理区域 |
| `rm` | `int` | 是否从 VirRegion 移除 |

**Rust 实现**:

```rust
impl PhysRegion {
    /// 释放物理区域（解引用物理块）
    ///
    /// 对应 Minix3: `pb_unreferenced()`
    ///
    /// # Arguments
    ///
    /// * `region` - 所属虚拟区域（可选）
    /// * `remove_from_vr` - 是否从 VirRegion 移除
    ///
    /// # Safety
    ///
    /// 调用者必须确保：
    /// - 当前 PhysRegion 已链接到 PhysBlock
    /// - PhysBlock 的引用计数 > 0
    pub unsafe fn unreferenced(
        &mut self,
        region: Option<*mut VirRegion>,
        remove_from_vr: bool,
    ) {
        let block_ptr = self.ph.expect("PhysRegion must be linked to a block");

        // 1. 减少引用计数
        (*block_ptr).refcount = (*block_ptr).refcount.saturating_sub(1);
        debug_assert!((*block_ptr).refcount >= 0, "refcount must not be negative");

        // 2. 从链表移除
        self.unlink_from_block();

        // 3. 引用计数为 0，释放 PhysBlock
        if (*block_ptr).refcount == 0 {
            debug_assert!(
                (*block_ptr).first_region.is_none(),
                "first_region must be None when refcount is 0"
            );

            // 调用 memtype 的 unreference 回调
            // TODO: 实现 memtype->ev_unreference()
            // let r = self.memtype.ev_unreference(self);
            // if r != OK { panic!("unref failed"); }

            // 释放 PhysBlock
            // TODO: 使用 slab 分配器释放
            // SLABFREE(block_ptr);
        }

        // 4. 清空 ph 指针
        self.ph = None;

        // 5. 从 VirRegion 移除（可选）
        if remove_from_vr {
            if let Some(vr_ptr) = region {
                (*vr_ptr).set_phys_region(self.offset, None);
            }
        }
    }

    /// 从 PhysBlock 的链表中移除
    ///
    /// # Safety
    ///
    /// 调用者必须确保 `self.ph` 指向有效的 PhysBlock
    pub unsafe fn unlink_from_block(&mut self) {
        let block_ptr = self.ph.expect("PhysRegion must be linked to a block");

        // 头节点
        if (*block_ptr).first_region == Some(self as *const PhysRegion as *mut PhysRegion) {
            (*block_ptr).first_region = self.next_ph_list;
        } else {
            // 非头节点：遍历链表找到前驱
            let mut current = (*block_ptr).first_region;
            let mut found = false;

            while let Some(current_ptr) = current {
                let current_ref = &mut *current_ptr;
                if current_ref.next_ph_list == Some(self as *const PhysRegion as *mut PhysRegion) {
                    current_ref.next_ph_list = self.next_ph_list;
                    found = true;
                    break;
                }
                current = current_ref.next_ph_list;
            }

            debug_assert!(found, "PhysRegion must be in the list");
        }

        self.next_ph_list = None;
    }
}

impl VirRegion {
    /// 释放指定偏移量的物理区域
    ///
    /// # Arguments
    ///
    /// * `offset` - 偏移量（必须页对齐）
    ///
    /// # Returns
    ///
    /// 返回被释放的 PhysRegion，如果不存在则返回 None
    pub fn release_phys_region(&mut self, offset: VirBytes) -> Option<Box<PhysRegion>> {
        const PAGE_SIZE: u64 = 4096;

        // 验证参数
        debug_assert_eq!(offset.0 % PAGE_SIZE, 0, "offset must be page-aligned");
        debug_assert!(offset.0 < self.length.0, "offset must be within region");

        // 计算索引
        let index = (offset.0 / PAGE_SIZE) as usize;

        // 取出 PhysRegion
        if let Some(mut phys_region) = self.physblocks[index].take() {
            // 解引用 PhysBlock
            unsafe {
                phys_region.unreferenced(Some(self as *mut VirRegion), false);
            }

            // 更新内存统计
            // TODO: 实现 vm_total 更新
            // self.parent.vm_total -= PAGE_SIZE;

            Some(phys_region)
        } else {
            None
        }
    }

    /// 释放所有物理区域
    ///
    /// 遍历所有物理区域并释放
    pub fn release_all_phys_regions(&mut self) {
        const PAGE_SIZE: u64 = 4096;
        let num_pages = (self.length.0 / PAGE_SIZE) as usize;

        for i in 0..num_pages {
            if let Some(mut phys_region) = self.physblocks[i].take() {
                unsafe {
                    phys_region.unreferenced(Some(self as *mut VirRegion), false);
                }
            }
        }

        // 重置内存统计
        // TODO: 实现 vm_total 更新
        // self.parent.vm_total = 0;
    }
}
```

**使用示例**:

```rust
// 创建虚拟区域（3 页）
let mut vr = VirRegion::new(VirBytes(0x1000), VirBytes(12288));

// 创建并设置物理区域
let mut block = Box::new(PhysBlock::new(0x8000));
let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;
let phys_region = PhysRegion::create_and_link(
    block_ptr,
    VirBytes(4096),
    std::ptr::null_mut(),
);
vr.set_phys_region(VirBytes(4096), Some(phys_region));

// 释放物理区域
let released = vr.release_phys_region(VirBytes(4096));
assert!(released.is_some());

// 验证已释放
assert!(vr.get_phys_region(VirBytes(4096)).is_none());

// 释放所有物理区域
vr.release_all_phys_regions();
```

**释放场景**:

| 场景 | 调用方式 | 说明 |
|------|---------|------|
| **进程退出** | `release_all_phys_regions()` | 释放所有物理区域 |
| **munmap** | `release_phys_region()` | 释放指定区域 |
| **CoW 写时复制** | `unreferenced()` | 解引用旧块 |
| **区域合并** | `release_phys_region()` | 释放多余区域 |

**引用计数管理**:

```
初始状态:
  PhysBlock { refcount: 1 }
  PhysRegion A → PhysBlock

fork 后:
  PhysBlock { refcount: 2 }
  PhysRegion A → PhysBlock ← PhysRegion B

释放 A:
  PhysBlock { refcount: 1 }
  PhysRegion B → PhysBlock

释放 B:
  PhysBlock { refcount: 0 }
  → 释放 PhysBlock
```

**性能考虑**:

1. **链表遍历**: 非头节点需要 O(n) 遍历，但通常链表很短
2. **引用计数**: 使用原子操作保证线程安全（后续实现）
3. **内存释放**: 延迟释放策略，减少内存分配器压力
4. **批量释放**: `release_all_phys_regions()` 批量处理更高效

**错误处理**:

| 错误情况 | Minix3 处理 | Rust 处理 |
|---------|------------|----------|
| **refcount == 0** | assert 失败 | debug_assert! panic |
| **不在链表中** | assert 失败 | debug_assert! panic |
| **ev_unreference 失败** | panic | panic |
| **PhysRegion 不存在** | 返回 | 返回 None |

**测试用例**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_release_phys_region() {
        let mut vr = VirRegion::new(VirBytes(0x1000), VirBytes(12288));
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let phys_region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(4096),
            std::ptr::null_mut(),
        );
        vr.set_phys_region(VirBytes(4096), Some(phys_region));

        // 验证存在
        assert!(vr.get_phys_region(VirBytes(4096)).is_some());

        // 释放
        let released = vr.release_phys_region(VirBytes(4096));
        assert!(released.is_some());

        // 验证已释放
        assert!(vr.get_phys_region(VirBytes(4096)).is_none());
    }

    #[test]
    fn test_release_all_phys_regions() {
        let mut vr = VirRegion::new(VirBytes(0x1000), VirBytes(12288));

        // 创建多个物理区域
        for i in 0..3 {
            let mut block = Box::new(PhysBlock::new(0x8000 + i * 4096));
            let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;
            let phys_region = PhysRegion::create_and_link(
                block_ptr,
                VirBytes(i * 4096),
                std::ptr::null_mut(),
            );
            vr.set_phys_region(VirBytes(i * 4096), Some(phys_region));
        }

        // 释放所有
        vr.release_all_phys_regions();

        // 验证全部释放
        for i in 0..3 {
            assert!(vr.get_phys_region(VirBytes(i * 4096)).is_none());
        }
    }

    #[test]
    fn test_unlink_from_block() {
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let mut region1 = Box::new(PhysRegion::new(VirBytes(0)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(4096)));

        // 链接两个区域
        region1.link_to_block(block_ptr, std::ptr::null_mut());
        region2.link_to_block(block_ptr, std::ptr::null_mut());

        // 验证引用计数
        assert_eq!(block.refcount, 2);

        // 解绑第一个
        unsafe { region1.unlink_from_block(); };
        assert_eq!(block.refcount, 2); // 引用计数在 unreferenced 中减少

        // 验证链表
        assert_eq!(block.first_region, Some(region2.as_ref() as *const PhysRegion as *mut PhysRegion));
    }
}
```

### 4.4 链表维护

**设计说明**:

Minix3 中的 PhysRegion 链表**不需要排序和合并**，原因如下：

**1. 链表特性**:

PhysRegion 链表是**单向链表**，连接所有引用同一个 PhysBlock 的 PhysRegion。

```
PhysBlock
  ↓
  firstregion → PhysRegion A (offset=0x1000, parent=VR1)
                  ↓
                PhysRegion B (offset=0x2000, parent=VR2)
                  ↓
                PhysRegion C (offset=0x0000, parent=VR3)
                  ↓
                NULL
```

**关键点**：
- 链表顺序是**插入顺序的逆序**（头插法）
- 每个 PhysRegion 的 offset 是**固定的**，由其所属的 VirRegion 决定
- 链表用于**遍历所有引用者**，而非按地址访问

**2. 为什么不需要排序**:

| 原因 | 说明 |
|------|------|
| **offset 固定** | 每个 PhysRegion 的 offset 由其所属 VirRegion 决定，不可改变 |
| **访问方式** | 通过 VirRegion 的 physblocks 数组访问，O(1) 时间复杂度 |
| **链表用途** | 用于遍历所有引用者（如 CoW、释放），不需要按 offset 查找 |
| **性能考虑** | 排序会增加插入复杂度，但不会提升查找性能 |

**3. 为什么不需要合并**:

| 原因 | 说明 |
|------|------|
| **一对一关系** | 每个 PhysRegion 对应一个 VirRegion 中的一个页面 |
| **独立生命周期** | PhysRegion 的生命周期由其所属 VirRegion 管理 |
| **引用计数** | 合并不能减少引用计数，无实际意义 |
| **VirRegion 层面** | 合并相邻 VirRegion 是在更高层次进行 |

**Minix3 源码分析**:

**源码位置**: [pb.c:68](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/pb.c#L68)

```c
void pb_link(struct phys_region *newphysr, struct phys_block *newpb,
    vir_bytes offset, struct vir_region *parent)
{
    USE(newphysr,
        newphysr->offset = offset;
        newphysr->ph = newpb;
        newphysr->parent = parent;
        newphysr->next_ph_list = newpb->firstregion;  // 头插法
        newpb->firstregion = newphysr;);
    newpb->refcount++;
}
```

**关键观察**：
- 使用**头插法**插入链表：`newphysr->next_ph_list = newpb->firstregion`
- **不排序**：直接插入到链表头部，O(1) 时间复杂度
- **不合并**：每个 PhysRegion 独立存在

**链表操作**:

**1. 插入操作**:

```rust
impl PhysRegion {
    /// 插入到物理块的引用链表（头插法）
    ///
    /// 对应 Minix3: `pb_link()`
    ///
    /// # Safety
    ///
    /// 调用者必须确保：
    /// - `block` 指针有效
    /// - 当前 `PhysRegion` 未在其他链表中（`self.ph.is_none()`）
    pub fn link_to_block(&mut self, block: *mut PhysBlock, parent: *mut VirRegion) {
        unsafe {
            debug_assert!(!block.is_null(), "block pointer must not be null");
            debug_assert!(self.ph.is_none(), "PhysRegion must not already be in a list");

            self.ph = Some(block);
            self.parent = Some(parent);

            // 头插法：插入到链表头部
            self.next_ph_list = (*block).first_region;
            (*block).first_region = Some(self as *mut PhysRegion);
            (*block).refcount = (*block).refcount.saturating_add(1);

            debug_assert!((*block).refcount > 0, "refcount must be positive after link");
        }
    }
}
```

**2. 删除操作**:

```rust
impl PhysRegion {
    /// 从 PhysBlock 的链表中移除
    ///
    /// # Safety
    ///
    /// 调用者必须确保 `self.ph` 指向有效的 PhysBlock
    pub unsafe fn unlink_from_block(&mut self) {
        let block_ptr = self.ph.expect("PhysRegion must be linked to a block");

        // 头节点
        if (*block_ptr).first_region == Some(self as *mut PhysRegion) {
            (*block_ptr).first_region = self.next_ph_list;
        } else {
            // 非头节点：遍历链表找到前驱
            let mut current = (*block_ptr).first_region;
            let mut found = false;

            while let Some(current_ptr) = current {
                let current_ref = &mut *current_ptr;
                if current_ref.next_ph_list == Some(self as *mut PhysRegion) {
                    current_ref.next_ph_list = self.next_ph_list;
                    found = true;
                    break;
                }
                current = current_ref.next_ph_list;
            }

            debug_assert!(found, "PhysRegion must be in the list");
        }

        self.next_ph_list = None;
    }
}
```

**3. 遍历操作**:

```rust
impl PhysBlock {
    /// 遍历所有引用此物理块的物理区域
    ///
    /// # Returns
    ///
    /// 返回迭代器，按链表顺序遍历所有 PhysRegion
    pub fn iter_regions(&self) -> impl Iterator<Item = &PhysRegion> {
        PhysRegionIterator {
            current: self.first_region,
            _marker: std::marker::PhantomData,
        }
    }

    /// 遍历所有引用此物理块的物理区域（可变引用）
    pub fn iter_regions_mut(&mut self) -> impl Iterator<Item = &mut PhysRegion> {
        PhysRegionMutIterator {
            current: self.first_region,
            _marker: std::marker::PhantomData,
        }
    }
}

/// PhysRegion 迭代器
struct PhysRegionIterator<'a> {
    current: Option<*mut PhysRegion>,
    _marker: std::marker::PhantomData<&'a PhysRegion>,
}

impl<'a> Iterator for PhysRegionIterator<'a> {
    type Item = &'a PhysRegion;

    fn next(&mut self) -> Option<Self::Item> {
        self.current.map(|ptr| unsafe {
            let region = &*ptr;
            self.current = region.next_ph_list;
            region
        })
    }
}
```

**使用示例**:

```rust
// 创建物理块
let mut block = Box::new(PhysBlock::new(0x8000));
let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

// 创建三个物理区域
let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
let mut region3 = Box::new(PhysRegion::new(VirBytes(0x0000)));

// 链接到物理块（头插法）
region1.link_to_block(block_ptr, std::ptr::null_mut());
region2.link_to_block(block_ptr, std::ptr::null_mut());
region3.link_to_block(block_ptr, std::ptr::null_mut());

// 链表顺序：region3 → region2 → region1
// 注意：顺序是插入顺序的逆序

// 遍历所有引用者
let mut count = 0;
for region in block.iter_regions() {
    count += 1;
    println!("PhysRegion offset: {:?}", region.offset);
}
assert_eq!(count, 3);
assert_eq!(block.refcount, 3);
```

**链表维护场景**:

| 场景 | 操作 | 说明 |
|------|------|------|
| **fork** | 插入 | 新建 PhysRegion 并链接到 PhysBlock |
| **CoW 写时复制** | 删除+插入 | 解绑旧块，链接新块 |
| **munmap** | 删除 | 从链表移除 PhysRegion |
| **进程退出** | 批量删除 | 遍历所有 PhysRegion 并删除 |

**性能分析**:

| 操作 | 时间复杂度 | 说明 |
|------|-----------|------|
| **插入** | O(1) | 头插法，直接插入 |
| **删除（头节点）** | O(1) | 直接更新 first_region |
| **删除（非头节点）** | O(n) | 需要遍历找到前驱 |
| **遍历** | O(n) | 遍历所有节点 |

**优化建议**:

1. **双向链表**: 如果删除操作频繁，可考虑使用双向链表，删除复杂度降为 O(1)
2. **引用计数**: 使用原子操作保证线程安全（后续实现）
3. **批量操作**: 批量删除时，可先遍历收集，再统一删除

**与 VirRegion 的关系**:

```
VirRegion (虚拟区域)
  ├─ physblocks[0] → PhysRegion A → PhysBlock X
  ├─ physblocks[1] → PhysRegion B → PhysBlock X  (共享)
  ├─ physblocks[2] → PhysRegion C → PhysBlock Y
  └─ physblocks[3] → NULL (未分配)

PhysBlock X (共享物理块)
  └─ first_region → PhysRegion A → PhysRegion B → NULL
```

**关键点**：
- VirRegion 通过数组管理 PhysRegion，O(1) 访问
- PhysBlock 通过链表管理引用者，遍历所有引用者
- 两者是**正交**的，互不影响

**测试用例**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_link_order() {
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x0000)));

        region1.link_to_block(block_ptr, std::ptr::null_mut());
        region2.link_to_block(block_ptr, std::ptr::null_mut());
        region3.link_to_block(block_ptr, std::ptr::null_mut());

        // 验证链表顺序（插入顺序的逆序）
        let regions: Vec<_> = block.iter_regions().collect();
        assert_eq!(regions.len(), 3);
        assert_eq!(regions[0].offset, VirBytes(0x0000)); // 最后插入
        assert_eq!(regions[1].offset, VirBytes(0x2000));
        assert_eq!(regions[2].offset, VirBytes(0x1000)); // 最先插入
    }

    #[test]
    fn test_unlink_head() {
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));

        region1.link_to_block(block_ptr, std::ptr::null_mut());
        region2.link_to_block(block_ptr, std::ptr::null_mut());

        // 删除头节点（region2）
        unsafe { region2.unlink_from_block(); };

        // 验证链表
        assert_eq!(block.first_region, Some(region1.as_ref() as *const PhysRegion as *mut PhysRegion));
        assert_eq!(block.refcount, 2); // 引用计数在 unreferenced 中减少
    }

    #[test]
    fn test_unlink_middle() {
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x0000)));

        region1.link_to_block(block_ptr, std::ptr::null_mut());
        region2.link_to_block(block_ptr, std::ptr::null_mut());
        region3.link_to_block(block_ptr, std::ptr::null_mut());

        // 删除中间节点（region2）
        unsafe { region2.unlink_from_block(); };

        // 验证链表
        let regions: Vec<_> = block.iter_regions().collect();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].offset, VirBytes(0x0000));
        assert_eq!(regions[1].offset, VirBytes(0x1000));
    }
}
```

---

## 5. fork 相关操作

### 5.1 复制物理区域

**复制流程**:

fork 时复制物理区域，实际上是**共享 PhysBlock**，增加引用计数。

```
fork 复制流程:
  1. 创建新的 VirRegion
     ↓
  2. 遍历原 VirRegion 的所有 PhysRegion
     ↓
  3. 对每个 PhysRegion:
     ├─ 调用 pb_reference() 创建新 PhysRegion
     ├─ 链接到相同的 PhysBlock
     └─ 增加 PhysBlock 的引用计数
     ↓
  4. 返回新的 VirRegion
```

**Minix3 源码分析**:

**源码位置**: [region.c:802](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/region.c#L802)

```c
struct vir_region *map_copy_region(struct vmproc *vmp, struct vir_region *vr)
{
    /* map_copy_region 创建 vir_region 数据结构的完整副本，
     * 直接链接到相同的 phys_blocks，但处于 limbo 状态，
     * 即调用者必须将 vir_region 链接到进程。
     * 因此它不增加 phys_block 的 refcount；
     * 调用者必须在链接后执行此操作。
     * 这样做的原因是保持此函数内的健全性检查正常工作。
     */
    struct vir_region *newvr;
    struct phys_region *ph;
    int r;
    vir_bytes p;

    // 1. 创建新的 VirRegion
    if(!(newvr = region_new(vr->parent, vr->vaddr, vr->length, vr->flags, vr->def_memtype)))
        return NULL;

    USE(newvr, newvr->parent = vmp;);

    // 2. 调用 memtype 的 ev_copy 回调（如果有）
    if(vr->def_memtype->ev_copy && (r=vr->def_memtype->ev_copy(vr, newvr)) != OK) {
        map_free(newvr);
        printf("VM: memtype-specific copy failed (%d)\n", r);
        return NULL;
    }

    // 3. 遍历所有 PhysRegion
    for(p = 0; p < phys_slot(vr->length); p++) {
        struct phys_region *newph;

        // 跳过未分配的页面
        if(!(ph = physblock_get(vr, p*VM_PAGE_SIZE))) continue;

        // 创建新 PhysRegion 并链接到相同的 PhysBlock
        newph = pb_reference(ph->ph, ph->offset, newvr, vr->def_memtype);

        if(!newph) { map_free(newvr); return NULL; }

        // 调用 memtype 的 ev_reference 回调（如果有）
        if(ph->memtype->ev_reference)
            ph->memtype->ev_reference(ph, newph);
    }

    return newvr;
}
```

**关键步骤**：

1. **创建新 VirRegion**: 使用相同的参数创建新的虚拟区域
2. **memtype 回调**: 调用 `ev_copy()` 处理特定内存类型
3. **遍历 PhysRegion**: 遍历所有已分配的物理区域
4. **创建新 PhysRegion**: 调用 `pb_reference()` 创建新物理区域
5. **链接到 PhysBlock**: 新 PhysRegion 链接到相同的 PhysBlock
6. **增加引用计数**: `pb_reference()` 内部增加 `refcount`

**引用计数变化**:

```
fork 前:
  PhysBlock { refcount: 1 }
  PhysRegion P1 → PhysBlock

fork 后:
  PhysBlock { refcount: 2 }
  PhysRegion P1 → PhysBlock ← PhysRegion P2
```

**Rust 实现**:

```rust
impl VirRegion {
    /// 复制虚拟区域（fork 时使用）
    ///
    /// 对应 Minix3: `map_copy_region()`
    ///
    /// 创建新的 VirRegion，共享所有 PhysBlock。
    ///
    /// # Arguments
    ///
    /// * `new_parent` - 新的父进程（可选）
    ///
    /// # Returns
    ///
    /// 返回新的 VirRegion
    pub fn fork_copy(&self, new_parent: Option<*mut VmProc>) -> Result<Box<Self>, VmError> {
        const PAGE_SIZE: u64 = 4096;

        // 1. 创建新的 VirRegion
        let mut new_vr = Box::new(VirRegion {
            vaddr: self.vaddr,
            length: self.length,
            flags: self.flags,
            parent: new_parent,
            physblocks: vec![None; self.physblocks.len()],
            // TODO: 其他字段
        });

        // 2. 遍历所有 PhysRegion
        for (i, phys_region_opt) in self.physblocks.iter().enumerate() {
            if let Some(phys_region) = phys_region_opt {
                // 3. 创建新 PhysRegion 并链接到相同的 PhysBlock
                let block_ptr = phys_region.ph.expect("PhysRegion must have a block");

                let new_phys_region = PhysRegion::create_and_link(
                    block_ptr,
                    phys_region.offset,
                    new_vr.as_mut() as *mut VirRegion,
                );

                // 4. 设置到新 VirRegion
                new_vr.physblocks[i] = Some(new_phys_region);

                // 5. 调用 memtype 的 ev_reference 回调（如果有）
                // TODO: 实现 memtype->ev_reference()
                // phys_region.memtype.ev_reference(phys_region, &new_phys_region);
            }
        }

        Ok(new_vr)
    }
}

impl PhysRegion {
    /// 创建并引用物理块（fork 时使用）
    ///
    /// 对应 Minix3: `pb_reference()`
    ///
    /// # Arguments
    ///
    /// * `block` - 物理块指针
    /// * `offset` - 在虚拟区域中的偏移量
    /// * `parent` - 所属虚拟区域
    ///
    /// # Returns
    ///
    /// 返回新创建的 PhysRegion
    pub fn create_and_link(
        block: *mut PhysBlock,
        offset: VirBytes,
        parent: *mut VirRegion,
    ) -> Box<Self> {
        // 1. 创建 PhysRegion
        let mut region = Box::new(PhysRegion::new(offset));

        // 2. 链接到 PhysBlock（增加引用计数）
        region.link_to_block(block, parent);

        region
    }
}
```

**使用示例**:

```rust
// 创建原始虚拟区域（3 页）
let mut vr1 = VirRegion::new(VirBytes(0x1000), VirBytes(12288));

// 创建并设置物理区域
let mut block = Box::new(PhysBlock::new(0x8000));
let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;
let phys_region = PhysRegion::create_and_link(
    block_ptr,
    VirBytes(4096),
    std::ptr::null_mut(),
);
vr1.set_phys_region(VirBytes(4096), Some(phys_region));

// 验证初始状态
assert_eq!(block.refcount, 1);

// fork 复制
let vr2 = vr1.fork_copy(None).unwrap();

// 验证共享
assert_eq!(block.refcount, 2); // 引用计数增加

// 验证两个 VirRegion 都有 PhysRegion
assert!(vr1.get_phys_region(VirBytes(4096)).is_some());
assert!(vr2.get_phys_region(VirBytes(4096)).is_some());

// 验证它们指向相同的 PhysBlock
let pr1 = vr1.get_phys_region(VirBytes(4096)).unwrap();
let pr2 = vr2.get_phys_region(VirBytes(4096)).unwrap();
assert_eq!(pr1.ph, pr2.ph); // 相同的 PhysBlock
```

**fork 场景**:

```
父进程:
  VirRegion A
    ├─ PhysRegion P1 → PhysBlock X (refcount=2)
    ├─ PhysRegion P2 → PhysBlock Y (refcount=1)
    └─ PhysRegion P3 → NULL

子进程 (fork 后):
  VirRegion B
    ├─ PhysRegion P4 → PhysBlock X (refcount=2, 共享)
    ├─ PhysRegion P5 → PhysBlock Y (refcount=1, 独立)
    └─ PhysRegion P6 → NULL
```

**关键点**：
- **共享 PhysBlock**: fork 不复制物理页，只共享
- **增加引用计数**: 每个 PhysBlock 的 refcount 增加
- **独立 PhysRegion**: 每个 VirRegion 有自己的 PhysRegion
- **CoW 延迟**: 实际复制延迟到写时进行

**性能考虑**:

| 方面 | 说明 |
|------|------|
| **内存开销** | 仅分配 PhysRegion（几十字节），不复制物理页（4KB） |
| **时间复杂度** | O(n)，n 为页面数量 |
| **引用计数** | 原子操作保证线程安全（后续实现） |
| **页表映射** | 设置为只读，触发 CoW |

**错误处理**:

| 错误情况 | Minix3 处理 | Rust 处理 |
|---------|------------|----------|
| **内存分配失败** | 返回 NULL | 返回 Err(VmError::NoMemory) |
| **memtype->ev_copy 失败** | 释放并返回 NULL | 返回 Err(VmError::CopyFailed) |
| **pb_reference 失败** | 释放并返回 NULL | 返回 Err(VmError::NoMemory) |

**测试用例**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fork_copy() {
        // 创建原始虚拟区域
        let mut vr1 = VirRegion::new(VirBytes(0x1000), VirBytes(12288));

        // 创建物理区域
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;
        let phys_region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(4096),
            std::ptr::null_mut(),
        );
        vr1.set_phys_region(VirBytes(4096), Some(phys_region));

        // 验证初始状态
        assert_eq!(block.refcount, 1);

        // fork 复制
        let vr2 = vr1.fork_copy(None).unwrap();

        // 验证引用计数
        assert_eq!(block.refcount, 2);

        // 验证两个 VirRegion 都有 PhysRegion
        assert!(vr1.get_phys_region(VirBytes(4096)).is_some());
        assert!(vr2.get_phys_region(VirBytes(4096)).is_some());

        // 验证共享相同的 PhysBlock
        let pr1 = vr1.get_phys_region(VirBytes(4096)).unwrap();
        let pr2 = vr2.get_phys_region(VirBytes(4096)).unwrap();
        assert_eq!(pr1.ph, pr2.ph);
    }

    #[test]
    fn test_fork_copy_multiple_pages() {
        // 创建原始虚拟区域（3 页）
        let mut vr1 = VirRegion::new(VirBytes(0x1000), VirBytes(12288));

        // 创建多个物理区域
        for i in 0..3 {
            let mut block = Box::new(PhysBlock::new(0x8000 + i * 4096));
            let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;
            let phys_region = PhysRegion::create_and_link(
                block_ptr,
                VirBytes(i * 4096),
                std::ptr::null_mut(),
            );
            vr1.set_phys_region(VirBytes(i * 4096), Some(phys_region));
        }

        // fork 复制
        let vr2 = vr1.fork_copy(None).unwrap();

        // 验证所有页面都被复制
        for i in 0..3 {
            assert!(vr1.get_phys_region(VirBytes(i * 4096)).is_some());
            assert!(vr2.get_phys_region(VirBytes(i * 4096)).is_some());
        }
    }

    #[test]
    fn test_fork_copy_empty_pages() {
        // 创建原始虚拟区域（无物理区域）
        let vr1 = VirRegion::new(VirBytes(0x1000), VirBytes(12288));

        // fork 复制
        let vr2 = vr1.fork_copy(None).unwrap();

        // 验证两个 VirRegion 都没有 PhysRegion
        for i in 0..3 {
            assert!(vr1.get_phys_region(VirBytes(i * 4096)).is_none());
            assert!(vr2.get_phys_region(VirBytes(i * 4096)).is_none());
        }
    }
}
```

### 5.2 CoW 准备

**设置只读映射**:

CoW 准备的核心是将共享物理页的页表项设置为**只读**，以触发写时复制。

```
CoW 准备流程:
  1. 遍历所有共享的 PhysRegion
     ↓
  2. 检查是否可写: `pr_writable()`
     ↓
  3. 如果共享 (refcount > 1)
     └─ 清除 PTF_WRITE 标志 (设置为只读)
     ↓
  4. 更新页表映射: `pt_writemap()`
```

**Minix3 源码分析**:

##### 1. pr_writable() - 判断是否可写

**源码位置**: [region.c:130](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/region.c#L130)

```c
static int pr_writable(struct vir_region *vr, struct phys_region *pr)
{
    assert(pr->memtype->writable);
    return ((vr->flags & VR_WRITABLE) && pr->memtype->writable(pr));
}
```

##### 2. anon_writable() - 匿名内存的可写判断

**源码位置**: [mem_anon.c:105](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/mem_anon.c#L105)

```c
static int anon_writable(struct phys_region *pr)
{
    assert(pr->ph->refcount > 0);
    if(pr->ph->phys == MAP_NONE)
        return 0;
    if(pr->parent->remaps > 0)
        return 1;
    return pr->ph->refcount == 1;  // 只有独占时可写
}
```

##### 3. 页表映射设置

**源码位置**: [region.c:274](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/region.c#L274)

```c
if(pr_writable(vr, pr))
    flags |= PTF_WRITE;  // 可写
else
    flags |= PTF_READ;   // 只读

pt_writemap(vmp, &vmp->vm_pt, vr->vaddr + pr->offset,
            pb->phys, VM_PAGE_SIZE, flags, ...);
```

**Rust 实现**:

```rust
/// 页表标志
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtFlags {
    /// 只读
    ReadOnly,
    /// 可写
    Writable,
    /// 可执行
    Executable,
    /// 用户模式
    User,
    /// 存在
    Present,
}

impl VirRegion {
    /// 准备 CoW: 设置共享页面为只读
    ///
    /// # Safety
    ///
    /// 调用者必须确保:
    /// - 所有 PhysRegion 都已正确初始化
    pub unsafe fn prepare_cow(&mut self) {
        const PAGE_SIZE: u64 = 4096;
        let num_pages = (self.length.0 / PAGE_SIZE) as usize;

        for i in 0..num_pages {
            if let Some(phys_region) = &self.physblocks[i] {
                // 获取物理块
                let block_ptr = phys_region.ph.expect("PhysRegion must have a block");
                let block = &*block_ptr;

                // 判断是否需要设置为只读
                if block.refcount > 1 && self.is_writable() && phys_region.is_writable() {
                    // 清除可写标志
                    self.set_writable(false);
                    
                    // 更新页表映射
                    let vaddr = self.vaddr.0 + i as u64 * PAGE_SIZE;
                    let paddr = block.phys;
                    
                    // 调用页表设置函数 (mock 实现)
                    MockPageTable::set_page_flags(
                        vaddr,
                        paddr,
                        &[PtFlags::ReadOnly, PtFlags::Present, PtFlags::User],
                    );
                }
            }
        }
    }

    /// 判断区域是否可写
    pub fn is_writable(&self) -> bool {
        self.flags.contains(VrFlags::WRITABLE)
    }

    /// 设置区域可写性
    pub fn set_writable(&mut self, writable: bool) {
        if writable {
            self.flags.insert(VrFlags::WRITABLE);
        } else {
            self.flags.remove(VrFlags::WRITABLE);
        }
    }
}

impl PhysRegion {
    /// 判断物理区域是否可写
    ///
    /// 对应 Minix3: `anon_writable()`
    /// 只有引用计数为 1 时才可写（独占）
    pub fn is_writable(&self) -> bool {
        match self.get_refcount() {
            Some(1) => true,
            _ => false,
        }
    }
}
```

**使用示例**:

```rust
// 创建虚拟区域（可写）
let mut vr = VirRegion::new(
    VirBytes(0x1000),
    VirBytes(4096),
    VrFlags::WRITABLE | VrFlags::ANON,
);

// 创建物理块
let mut block = Box::new(PhysBlock::new(0x8000));
let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

// 创建物理区域并链接
let phys_region = PhysRegion::create_and_link(
    block_ptr,
    VirBytes(0),
    &mut vr as *mut VirRegion,
);
vr.set_phys_region(VirBytes(0), Some(phys_region));

// fork 复制（增加引用计数）
let mut vr2 = vr.fork_copy(None).unwrap();

// 验证初始状态（可写）
assert!(vr.is_writable());
assert!(vr.get_phys_region(VirBytes(0)).unwrap().is_writable());

// 准备 CoW
unsafe { vr.prepare_cow(); }

// 验证设置后状态（只读）
assert!(!vr.is_writable());
assert!(!vr.get_phys_region(VirBytes(0)).unwrap().is_writable());
```

**CoW 触发流程**:

```
1. fork 后:
   - PhysBlock { refcount: 2 }
   - 父子进程的页表项均为可写

2. CoW 准备:
   - 检测到 refcount > 1
   - 将父子进程的页表项均设置为只读

3. 写操作:
   - CPU 触发写保护异常
   - 内核调用 VM 服务器处理
   - VM 检查 refcount > 1 → 执行 CoW
   - 分配新物理页，复制内容
   - 更新子进程页表指向新物理页
   - 设置新页表项为可写
```

**性能考虑**:

1. **批量操作**: 遍历所有页面，批量设置页表
2. **条件判断**: 只处理 refcount > 1 的共享页面
3. **原子操作**: 页表更新需要原子性（硬件支持）
4. **延迟处理**: 实际复制延迟到第一次写操作

**错误处理**:

| 错误情况 | Minix3 处理 | Rust 处理 |
|---------|------------|----------|
| **页表更新失败** | panic | 返回 Result<(), VmError> |
| **空指针** | assert 失败 | expect 并 panic |
| **未对齐地址** | assert 失败 | debug_assert! panic |

**测试用例**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_cow() {
        // 创建虚拟区域
        let mut vr = VirRegion::new(
            VirBytes(0x1000),
            VirBytes(4096),
            VrFlags::WRITABLE | VrFlags::ANON,
        );

        // 创建物理块
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        // 创建物理区域并链接
        let phys_region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(0),
            std::ptr::null_mut(),
        );
        vr.set_phys_region(VirBytes(0), Some(phys_region));

        // 验证初始状态
        assert!(vr.is_writable());
        assert_eq!(block.refcount, 1);
        assert!(vr.get_phys_region(VirBytes(0)).unwrap().is_writable());

        // fork 复制
        let mut vr2 = vr.fork_copy(None).unwrap();
        assert_eq!(block.refcount, 2);

        // 准备 CoW
        unsafe { vr.prepare_cow(); }

        // 验证设置后状态
        assert!(!vr.is_writable());
        assert!(!vr.get_phys_region(VirBytes(0)).unwrap().is_writable());

        // 验证子进程也被设置为只读
        unsafe { vr2.prepare_cow(); }
        assert!(!vr2.is_writable());
    }

    #[test]
    fn test_anon_writable() {
        // 创建物理块
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        // 创建物理区域
        let mut phys_region = PhysRegion::new(VirBytes(0));
        phys_region.link_to_block(block_ptr, std::ptr::null_mut());

        // 引用计数 = 1 → 可写
        assert!(phys_region.is_writable());

        // 创建第二个物理区域（引用计数 = 2）
        let mut phys_region2 = PhysRegion::new(VirBytes(4096));
        phys_region2.link_to_block(block_ptr, std::ptr::null_mut());

        // 引用计数 = 2 → 不可写
        assert!(!phys_region.is_writable());
        assert!(!phys_region2.is_writable());
    }
}
```

---

## 6. 测试与验证

### 6.1 基本操作测试

**测试目标**:

验证物理区域的创建、查找、释放等基本操作的正确性。

**Minix3 测试方法**:

Minix3 使用 `SANITYCHECKS` 宏进行运行时验证：

**源码位置**: [sanitycheck.h:8](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/sanitycheck.h#L8)

```c
#if SANITYCHECKS
#define MYASSERT(c) do { if(!(c)) { \
    printf("VM:%s:%d: %s failed\n", file, line, #c); \
    panic("sanity check failed"); } } while(0)

// 验证所有指针有效性
ALLREGIONS(MYSLABSANE(vr), MYSLABSANE(pr); MYSLABSANE(pr->ph); MYSLABSANE(pr->parent));
#endif
```

**Rust 测试实现**:

```rust
#[cfg(test)]
mod basic_operation_tests {
    use super::*;

    /// 测试创建物理区域
    #[test]
    fn test_create_phys_region() {
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(0x1000),
            std::ptr::null_mut(),
        );

        assert_eq!(region.offset, VirBytes(0x1000));
        assert_eq!(region.ph, Some(block_ptr));
        assert!(region.parent.is_none());
        assert_eq!(block.refcount, 1);
    }

    /// 测试查找物理区域
    #[test]
    fn test_lookup_phys_region() {
        let mut vr = VirRegion::new(
            VirBytes(0x1000),
            VirBytes(12288),
            VrFlags::empty(),
        );

        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let phys_region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(4096),
            std::ptr::null_mut(),
        );
        vr.set_phys_region(VirBytes(4096), phys_region);

        let found = vr.get_phys_region(VirBytes(4096));
        assert!(found.is_some());
        assert_eq!(found.unwrap().offset, VirBytes(4096));

        assert!(vr.get_phys_region(VirBytes(0)).is_none());
        assert!(vr.get_phys_region(VirBytes(8192)).is_none());
    }

    /// 测试释放物理区域
    #[test]
    fn test_release_phys_region() {
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let mut region = PhysRegion::new(VirBytes(0x1000));
        region.link_to_block(block_ptr, std::ptr::null_mut());

        assert_eq!(block.refcount, 1);

        let should_free = region.unlink_from_block();
        assert!(should_free);
        assert_eq!(block.refcount, 0);
        assert!(region.ph.is_none());
    }

    /// 测试完整的生命周期
    #[test]
    fn test_full_lifecycle() {
        let mut vr = VirRegion::new(
            VirBytes(0x1000),
            VirBytes(4096),
            VrFlags(VrFlags::WRITABLE | VrFlags::ANON),
        );

        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let phys_region = PhysRegion::create_and_link(
            block_ptr,
            VirBytes(0),
            std::ptr::null_mut(),
        );
        vr.set_phys_region(VirBytes(0), phys_region);

        assert!(vr.get_phys_region(VirBytes(0)).is_some());
        assert_eq!(block.refcount, 1);

        vr.free_range(VirBytes(0), VirBytes(4096));

        assert!(vr.get_phys_region(VirBytes(0)).is_none());
        assert_eq!(block.refcount, 0);
    }

    /// 测试边界条件
    #[test]
    fn test_boundary_conditions() {
        let region = PhysRegion::new(VirBytes(0));

        assert!(!region.has_phys_block());
        assert!(region.get_refcount().is_none());
        assert!(region.get_phys_addr().is_none());

        assert!(region.validate_offset(VirBytes(0)));
        assert!(!region.validate_offset(VirBytes(4097)));
    }

    /// 测试引用计数
    #[test]
    fn test_reference_counting() {
        let mut block = PhysBlock::new(0x8000);
        
        assert_eq!(block.refcount, 0);

        block.add_ref();
        assert_eq!(block.refcount, 1);

        block.add_ref();
        assert_eq!(block.refcount, 2);

        assert!(block.release_ref());
        assert_eq!(block.refcount, 1);

        assert!(!block.release_ref());
        assert_eq!(block.refcount, 0);
    }
}
```

**测试覆盖率**:

| 测试项 | 覆盖内容 | Minix3 对应 |
|-------|---------|------------|
| `test_create_phys_region` | 创建、初始化、链接 | `pb_reference()` |
| `test_lookup_phys_region` | 查找、偏移计算 | `physblock_get()` |
| `test_release_phys_region` | 解除链接、引用计数 | `pb_unreferenced()` |
| `test_full_lifecycle` | 完整生命周期 | `map_free()` |
| `test_boundary_conditions` | 边界条件、空值处理 | `MYASSERT()` |
| `test_reference_counting` | 引用计数操作 | `refcount` 操作 |

### 6.2 链表操作测试

**测试目标**:

验证物理块引用链表的插入、删除、遍历操作的正确性。

**Minix3 链表验证**:

**源码位置**: [region.c:196](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/region.c#L196)

```c
// 验证链表指针有效性
ALLREGIONS(
    MYSLABSANE(vr),
    MYSLABSANE(pr);
    MYSLABSANE(pr->ph);
    MYSLABSANE(pr->parent);
    // 验证链表完整性
    if(pr->ph) {
        struct phys_region *p;
        int found = 0;
        for(p = pr->ph->firstregion; p; p = p->next_ph_list)
            if(p == pr) found = 1;
        MYASSERT(found);
    }
);
```

**Rust 测试实现**:

```rust
#[cfg(test)]
mod linked_list_tests {
    use super::*;

    /// 测试链表插入（头插法）
    #[test]
    fn test_list_insert() {
        let mut block = PhysBlock::new(0x8000);
        let block_ptr = &mut block as *mut PhysBlock;

        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x3000)));

        unsafe {
            region1.link_to_block(block_ptr, std::ptr::null_mut());
            assert_eq!(block.refcount, 1);
            assert_eq!(block.first_region, Some(&mut *region1 as *mut PhysRegion));

            region2.link_to_block(block_ptr, std::ptr::null_mut());
            assert_eq!(block.refcount, 2);
            assert_eq!(block.first_region, Some(&mut *region2 as *mut PhysRegion));
            assert_eq!(region2.next_ph_list, Some(&mut *region1 as *mut PhysRegion));

            region3.link_to_block(block_ptr, std::ptr::null_mut());
            assert_eq!(block.refcount, 3);
            assert_eq!(block.first_region, Some(&mut *region3 as *mut PhysRegion));
            assert_eq!(region3.next_ph_list, Some(&mut *region2 as *mut PhysRegion));
        }
    }

    /// 测试链表删除（头部节点）
    #[test]
    fn test_list_remove_head() {
        let mut block = PhysBlock::new(0x8000);
        let block_ptr = &mut block as *mut PhysBlock;

        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));

        unsafe {
            region1.link_to_block(block_ptr, std::ptr::null_mut());
            region2.link_to_block(block_ptr, std::ptr::null_mut());
            assert_eq!(block.refcount, 2);

            let should_free = region2.unlink_from_block();
            assert!(!should_free);
            assert_eq!(block.refcount, 1);
            assert_eq!(block.first_region, Some(&mut *region1 as *mut PhysRegion));
        }
    }

    /// 测试链表删除（中间节点）
    #[test]
    fn test_list_remove_middle() {
        let mut block = PhysBlock::new(0x8000);
        let block_ptr = &mut block as *mut PhysBlock;

        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x3000)));

        unsafe {
            region1.link_to_block(block_ptr, std::ptr::null_mut());
            region2.link_to_block(block_ptr, std::ptr::null_mut());
            region3.link_to_block(block_ptr, std::ptr::null_mut());

            let should_free = region2.unlink_from_block();
            assert!(!should_free);
            assert_eq!(block.refcount, 2);

            let mut count = 0;
            PhysRegion::iterate_block_refs(&block, |_| count += 1);
            assert_eq!(count, 2);
        }
    }

    /// 测试链表删除（尾部节点）
    #[test]
    fn test_list_remove_tail() {
        let mut block = PhysBlock::new(0x8000);
        let block_ptr = &mut block as *mut PhysBlock;

        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));

        unsafe {
            region1.link_to_block(block_ptr, std::ptr::null_mut());
            region2.link_to_block(block_ptr, std::ptr::null_mut());

            let should_free = region1.unlink_from_block();
            assert!(!should_free);
            assert_eq!(block.refcount, 1);

            let mut offsets = Vec::new();
            PhysRegion::iterate_block_refs(&block, |r| offsets.push(r.offset.0));
            assert_eq!(offsets, vec![0x2000]);
        }
    }

    /// 测试链表遍历
    #[test]
    fn test_list_iteration() {
        let mut block = PhysBlock::new(0x8000);
        let block_ptr = &mut block as *mut PhysBlock;

        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x3000)));

        unsafe {
            region1.link_to_block(block_ptr, std::ptr::null_mut());
            region2.link_to_block(block_ptr, std::ptr::null_mut());
            region3.link_to_block(block_ptr, std::ptr::null_mut());
        }

        let mut count = 0;
        let mut offsets = Vec::new();
        PhysRegion::iterate_block_refs(&block, |region| {
            count += 1;
            offsets.push(region.offset.0);
        });

        assert_eq!(count, 3);
        assert_eq!(offsets, vec![0x3000, 0x2000, 0x1000]);
    }

    /// 测试链表完整性验证
    #[test]
    fn test_list_integrity() {
        let mut block = PhysBlock::new(0x8000);
        let block_ptr = &mut block as *mut PhysBlock;

        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));

        unsafe {
            region1.link_to_block(block_ptr, std::ptr::null_mut());
            region2.link_to_block(block_ptr, std::ptr::null_mut());

            assert!(region1.is_in_list(block_ptr));
            assert!(region2.is_in_list(block_ptr));

            region1.unlink_from_block();

            assert!(!region1.is_in_list(block_ptr));
            assert!(region2.is_in_list(block_ptr));
        }
    }

    /// 测试空链表
    #[test]
    fn test_empty_list() {
        let block = PhysBlock::new(0x8000);

        assert_eq!(block.refcount, 0);
        assert!(block.first_region.is_none());

        let mut count = 0;
        PhysRegion::iterate_block_refs(&block, |_| count += 1);
        assert_eq!(count, 0);
    }

    /// 测试 CoW 场景下的链表操作
    #[test]
    fn test_cow_scenario() {
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let mut parent_region = Box::new(PhysRegion::new(VirBytes(0)));
        parent_region.link_to_block(block_ptr, std::ptr::null_mut());

        let mut child_region = Box::new(PhysRegion::new(VirBytes(0)));
        child_region.link_to_block(block_ptr, std::ptr::null_mut());

        assert_eq!(block.refcount, 2);

        let mut found_parent = false;
        let mut found_child = false;
        PhysRegion::iterate_block_refs(&block, |r| {
            if r.offset.0 == 0 {
                found_parent = true;
            }
        });
        assert!(found_parent || found_child);

        let should_free = child_region.unlink_from_block();
        assert!(!should_free);
        assert_eq!(block.refcount, 1);

        assert!(parent_region.is_writable());
    }
}
```

**测试覆盖率**:

| 测试项 | 覆盖内容 | Minix3 对应 |
|-------|---------|------------|
| `test_list_insert` | 头插法、引用计数 | `pb_link()` |
| `test_list_remove_head` | 删除头部节点 | `pb_unreferenced()` |
| `test_list_remove_middle` | 删除中间节点 | `pb_unreferenced()` |
| `test_list_remove_tail` | 删除尾部节点 | `pb_unreferenced()` |
| `test_list_iteration` | 遍历所有节点 | `firstregion` 遍历 |
| `test_list_integrity` | 链表完整性 | `MYASSERT(found)` |
| `test_empty_list` | 空链表处理 | 初始状态 |
| `test_cow_scenario` | CoW 场景 | fork 后的状态 |

**测试执行命令**:

```bash
# 运行所有测试
cargo test --lib phys_region

# 运行特定测试
cargo test --lib basic_operation_tests
cargo test --lib linked_list_tests

# 显示测试输出
cargo test --lib -- --nocapture
```

---

## 7. 参见

- [12-vir-region.md](12-vir-region.md) - 虚拟区域
- [10-phys-block.md](10-phys-block.md) - 物理块
- [17-vm-fork.md](17-vm-fork.md) - fork 时的处理

---

*分类: VM私有*
