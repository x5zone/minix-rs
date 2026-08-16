# 14-region-lookup: 区域查找——AVL 到 BTreeMap 的有序索引

> **分类**: 阶段 5 — 地址空间数据结构（索引面）
> **源码**: `minix3/minix/servers/vm/cavl_if.h`（接口：`avl_search_type` :25-33 / `region_avl` :67-73 / `region_iter` :158-175 / `AVL_IMPL_*` :194-211）+ `minix3/minix/servers/vm/cavl_impl.h`（1208 行实现：`init` :178 / `balance` :195 / `insert` :321 / `search` :456 / `remove` :545 / `start_iter` :979 / `incr_iter` :1100）+ `minix3/minix/servers/vm/regionavl_defs.h`（宏实例化）+ `minix3/minix/servers/vm/regionavl.c`（编译单元）+ `minix3/minix/servers/vm/unavl.h`（宏清理）+ `minix3/minix/servers/vm/region.c`（`region_find_slot_range` :302 / `region_find_slot` :399）
> **Rust 模块**: `os/servers/vm/src/region/region_map.rs`（533 行：`SearchType` :19 / `RegionMap` :39 / `search` :77 / `find` :58 / `find_slot` :157 / `insert` :219 / `iter` :246）+ 消费接线 `os/servers/vm/src/vmproc/vmproc_handle.rs`（`regions` :457 / `regions_mut` :469）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md`（区域生命周期 + RegionMap 操作面）
> **说明**: 区域**索引面**语义模块：**Minix3 的 Walt Karas AVL 宏模板（cavl_if/cavl_impl × regionavl_defs 实例化）→ minix-rs 的 `BTreeMap<VirBytes, VirRegion>` 封装（RegionMap）**——ARCH A-4。13 管"区域生命周期操作"，本文档管"有序索引的查找语义"。**不覆盖**：区域生命周期（13）、查询服务（26）、RS 热更新（25）、mmap/munmap 服务（20/21）。

---

## 1. 概念：有序区域索引

### 1.0 章节引言

一个进程可以有几十个区域（代码/堆/栈/mmap 文件/共享段），VM 的每个服务路径都要回答两类问题：

1. **这个地址属于哪个区域？**——页错误、brk、查询都要做"地址 → 区域"解析。
2. **空着的地址范围在哪？**——mmap 分配新区域时要找不与其他区域重叠的洞。

如果区域只是线性数组，这两个问题都是 O(n)。地址空间越大、区域越多，线性扫描越不可接受。VM 用**有序索引**把两者降到 O(log n)：所有区域按 `vaddr` 有序组织，支持五向搜索与中序遍历。

本文档回答三个问题：

1. **有序索引解决什么问题**——地址解析、空槽查找、区间不重叠不变量（§1.1）。
2. **Minix3 的 AVL 索引怎么工作**——Walt Karas 宏模板、平衡、五向搜索、路径栈迭代器（§1.2-§1.5）。
3. **Rust 为什么用 BTreeMap 替代、语义怎么对齐**——ARCH A-4（§1.6-§1.7）。

它在地址空间数据结构阶段的位置：

```
11（物理页状态）→ 12（内存语义）→ 13（区域生命周期：框架操作）→ ★14（区域索引：查找语义）
→ 15（主循环）→ 16/17（页错误 + CoW 消费 find）→ 18/20/21/25/26（服务消费各查找入口）
```

### 1.1 有序索引解决什么问题

| 问题 | 无索引的代价 | 有序索引解法 | 消费方 |
|------|-------------|-------------|--------|
| 地址 → 区域 | O(n) 线性扫描 | 五向搜索（≤ 定位 + contains 过滤） | map_lookup（13 §2.4）、页错误、brk、查询 |
| 空槽查找 | 逐段试探 | region_find_slot（间隙扫描 + hint） | map_page_region（mmap/brk/RS） |
| 区间不重叠不变量 | 无法保证 | insert 时 AVL 键唯一（重复键返回） | 所有区域建立路径 |
| 按序遍历 | 需要排序 | 中序遍历（start_iter_least + incr） | map_writept/统计/调试/复制 |

**索引是区域的"投影"**：区域的生命周期操作（建/复制/释放，13）通过索引落地——insert 建立投影、remove 摘除投影、search 查询投影。索引本身不拥有区域语义，只维护"按 vaddr 有序 + 键唯一"两个不变量。

### 1.2 AVL 树：自平衡二叉搜索树

AVL 树是二叉搜索树的一种自平衡变体：**任意节点的左右子树高度差（平衡因子）∈ {-1, 0, 1}**。插入/删除后若某节点平衡因子越界（±2），通过旋转恢复。这保证高度 ≤ 1.44·log₂(n+2)，查找/插入/删除均 O(log n)。

Minix3 的具体形态：

- **节点 = vir_region 本身**：`lower`/`higher` 指针 + `factor` 平衡因子内嵌在 region.h:63-65（侵入式节点，无独立分配）。
- **键 = vaddr**：比较宏按 `vaddr` 大小三态比较（regionavl_defs.h:15-17）。
- **最大深度 30**：`AVL_MAX_DEPTH 30`（regionavl_defs.h:6，"good for 2 million nodes"）——树的深度上界直接硬编码为迭代器路径栈的容量。

### 1.3 五向搜索（avl_search_type）

`avl_search_type`（cavl_if.h:25-33）是**位标志枚举**：

```c
AVL_EQUAL = 1,             // 精确匹配
AVL_LESS = 2,              // 严格小于
AVL_GREATER = 4,           // 严格大于
AVL_LESS_EQUAL = AVL_EQUAL | AVL_LESS,      // ≤
AVL_GREATER_EQUAL = AVL_EQUAL | AVL_GREATER // ≥
```

语义：`region_search(tree, k, st)` 返回满足 st 的**最大/最小键节点**——`AVL_LESS` 返回 < k 的最大节点，`AVL_GREATER` 返回 > k 的最小节点，组合位则允许等于。实现用 `target_cmp` 技巧（cavl_impl.h:456-493）：一次下行遍历同时记录候选（match_h），命中相等时按 st 决定接受或转向。

VM 的实际用法：

- `AVL_LESS_EQUAL` → 地址包含查找的定位步（region.c:628 map_lookup）。
- `AVL_GREATER` / `AVL_LESS` → 空槽查找的间隙扫描（region.c:321/:328/:386）。
- `AVL_GREATER_EQUAL` / `AVL_EQUAL` / `AVL_LESS` → utility.c:233/:252/:264 transfer_mmap_regions 的区间定位（10 已述，Rust 侧 DEFERRED）。
- `AVL_LESS` → rs.c:290 找 mmap 基址以下的区域（25 承接）。

### 1.4 路径栈迭代器（region_iter）

中序遍历需要"记住走到哪了"。Minix3 的迭代器是**路径栈**（cavl_if.h:158-175）：

```c
typedef struct {
    region_avl *tree_;        // 被迭代的树
    unsigned long branch;     // 路径位图：第 n 位 = 第 n 层走右（1）还是左（0）
    int depth;                // 当前深度（~0 表示无效）
    region_t *path_h[AVL_MAX_DEPTH - 1];  // 根到当前节点的路径
} region_iter;
```

- `region_start_iter(tree, iter, k, st)`（cavl_impl.h:979）：**按搜索类型定位起点**——从根下行记录路径，停在第一个满足 st 的节点。
- `region_start_iter_least`（:1039）：一路向左到最小节点。
- `region_incr_iter`（:1100）：若当前节点有右子树，进入右子树最左；否则沿路径栈**向上退**到"上次走左的分支"（branch 位图判定）——经典中序后继。
- `region_decr_iter`（:1145）：对称的前驱。

**迭代器的角色**：VM 大量"遍历全部区域"的路径都靠它——map_writept（region.c:912-924）、map_free_proc、map_sanitycheck、get_region_info（26）、调试打印（13 §2.10）。

### 1.5 空槽查找：region_find_slot

分配新区域（map_page_region）要回答"`[minv, maxv)` 内哪里能放下 length 字节"。C 实现两段式：

- **region_find_slot**（region.c:399）：先用 `vm_region_top`（上次插入末尾，vmproc.h:22）当 hint 试 `[minv, hint)`，失败再试全范围 `[minv, maxv)`——**时间局部性优化**：连续 mmap 的分配地址通常单调上升。
- **region_find_slot_range**（region.c:302）：核心扫描——`FREEVRANGE_TRY(rangestart, rangeend)` 宏计算可用区间 `[max(start,minv), min(end,maxv))`，若 ≥ length 则**取高端** `startv = frend - length`（地址向高端对齐，利于堆/栈增长方向）；`FREEVRANGE` 宏先试"内缩一页"（start+PAGE_SIZE, end-PAGE_SIZE）再试全区间——**优先避免与相邻区域共享页表边界**。
- 特例：`maxv == 0` 表示"就放在 minv 这里"（`maxv = minv + length`，region.c:315-327）。
- 找到后写回 `vmp->vm_region_top = startv + length`（region.c:391）作为下次 hint。

### 1.6 Rust 模型：BTreeMap 替代 AVL（ARCH A-4）

Rust 的替代方案是 `BTreeMap<VirBytes, VirRegion>`（region_map.rs:39）。**为什么是 BTreeMap 而不是手动移植 AVL**：

| 维度 | 手写 AVL（C 现状） | BTreeMap |
|------|-------------------|----------|
| 节点 | 侵入式（VirRegion 内嵌 lower/higher/factor） | 非侵入（VirRegion 无树字段，可 Clone/move/split） |
| 平衡 | 手写 balance/旋转（cavl_impl.h:195-320） | 标准库维护（B 树节点内多元素） |
| 迭代器 | 手写路径栈 + branch 位图 | 标准库原生迭代器（升序） |
| 内存安全 | 裸指针 + 宏模板 | 所有权 + 借用检查 |
| 代码量 | cavl_impl.h 1208 行宏模板 | RegionMap 封装 ~260 行 |

**语义等价性**（13 §3.1 已述操作面，本文档补查找面）：

| C AVL 操作 | BTreeMap 等价 | 说明 |
|-----------|--------------|------|
| `region_search(k, AVL_LESS_EQUAL)` | `range(..=k).next_back()` | 键 ≤ k 的最大 |
| `region_search(k, AVL_LESS)` | `range(..k).next_back()` | 键 < k 的最大 |
| `region_search(k, AVL_GREATER)` | `range((Excluded(k), ..)).next()` | 键 > k 的最小 |
| `region_search(k, AVL_GREATER_EQUAL)` | `range((Excluded(k), ..)).next()` 或 `get(k)` | 键 ≥ k 的最小 |
| `region_search(k, AVL_EQUAL)` | `get(&k)` | 精确 |
| `region_search_least` | `iter().next()` | 最小 |
| `region_search_greatest` | `iter().next_back()` | 最大 |
| `region_start_iter_least` + incr | `iter()` | 升序遍历 |
| `region_insert` | `insert`（重叠/重复检查） | 不变量前置 |
| `region_remove(k)` | `remove(&k)` | 按键摘除 |

**为什么安全**：BTreeMap 由标准库保证平衡与内存安全，VM 侧只需保证"键不重叠"这个业务不变量——由 `insert` 的重叠预检（region_map.rs:219-227）承担。

### 1.7 对照 Redox / Linux

- **Linux**：VMA（vm_area_struct）长期用 `rbtree`（红黑树）组织，按 vm_start 排序——与 Minix3 AVL 同构（平衡因子 → 红黑着色）。**Linux 6.1 起改用 maple tree（B 树）**管理 VMA——动机与 minix-rs 相同：减少指针追逐、提升缓存局部性、简化并发（写时复制树）。AVL → BTreeMap 正是同一演进方向的微内核版本。
- **Redox**：无 per-region 索引结构——地址空间是内核页表的直接视图（`AddressSpace`），区域属性直接编码在页表/内核堆分配中，"找区域"由页表查询替代。Minix3/Rust 的"区域索引 + 搜索语义"是更强的抽象（支持空槽分配、区间不变量、多类型区域的差异化查询）。

### 1.8 本章小结

- 有序索引把"地址解析"与"空槽分配"从 O(n) 降到 O(log n)，并维护"键唯一 + 有序"两个不变量。
- Minix3 用 Walt Karas AVL 宏模板：侵入式节点 + 手写平衡 + 路径栈迭代器。
- Rust 用 BTreeMap：**同样的复杂度语义，算法从手写变库调用，不变量从断言变类型**。

---

## 2. C 源码分析

### 2.0 本章定位

cavl 四件套 + regionavl.c 实例化 + AVL 核心算法 + region.c 的 region_find_slot* 深挖 + 消费面调用点表。行号 `sed -n` 实证。

### 2.1 cavl 宏模板机制（C 的"泛型"）

Walt Karas 的 AVL 库是**宏模板**：一份通用实现通过宏参数实例化成任意键/句柄类型。

| 文件 | 作用 |
|------|------|
| `cavl_if.h` | 接口生成：类型（region_avl/region_iter/avl_search_type）+ 函数原型 |
| `cavl_impl.h` | 实现生成：函数体（balance/insert/search/remove/迭代器） |
| `regionavl_defs.h` | 实例化参数（AVL_UNIQUE/HANDLE/KEY/MAX_DEPTH/比较宏） |
| `unavl.h` | 宏清理（undef 全部参数，防止污染） |
| `regionavl.c` | 编译单元：include 四件套触发实例化 |

**实例化参数**（regionavl_defs.h:1-17）：

```c
#define AVL_UNIQUE(id) region_ ## id      /* 函数名前缀 → region_init/region_insert/... */
#define AVL_HANDLE region_t *             /* 节点句柄 = vir_region 指针 */
#define AVL_KEY vir_bytes                 /* 键 = 虚拟地址 */
#define AVL_MAX_DEPTH 30                  /* 迭代器路径栈容量（~2M 节点） */
#define AVL_GET_LESS/SET_LESS/GREATER/... /* 字段访问宏 → lower/higher/factor */
#define AVL_COMPARE_*                     /* 三态比较（> / < / =） */
```

**实现选择掩码**（cavl_if.h:194-211）：`AVL_IMPL_INIT`/`INSERT`/`SEARCH`/`REMOVE`/`START_ITER`/`INCR_ITER` 等 16 位掩码可选编译——region.c 未定义 `AVL_IMPL_MASK`，走 `AVL_IMPL_ALL`（cavl_impl.h:172），全部函数生成。

### 2.2 AVL 结构：region_avl 根 + 侵入式节点

```c
/* cavl_if.h:67-73 */
typedef struct { AVL_HANDLE root; } region_avl;   /* 树根 = 进程的 vm_regions_avl */

/* region.h:63-65 — vir_region 内嵌 AVL 字段 */
/* AVL fields */
struct vir_region *lower, *higher;   /* 左/右子树指针 */
int factor;                          /* 平衡因子 ∈ {-1,0,1} */
```

- `region_avl` 嵌在 `vmproc`（vmproc.h:21）；`vm_region_top`（vmproc.h:22）是空槽查找 hint。
- 平衡因子含义：-1 左高、0 等高、1 右高。越界 ±2 触发旋转。

### 2.3 平衡与插入

**balance**（cavl_impl.h:195-320）是核心旋转器，处理四种失衡：

- 右子树深且右孙深（RR）→ 左旋；左子树深且左孙深（LL）→ 右旋。
- 右子树深但左孙深（RL）→ 先右旋再左旋（双旋转）；左子树深但右孙深（LR）→ 先左旋再右旋。
- 旋转后按 `bf` 的符号更新三个节点的平衡因子（四种 case 更新表，cavl_impl.h:211-318）。

**insert**（cavl_impl.h:321-455）：

1. 下行找插入点，用 `branch` 位图记录路径（第 n 位 = 第 n 层走右/左）；同时记录**最后一个不平衡节点**（unbal）及其深度。
2. 键重复 → 直接返回已存在节点（**不插入**——AVL 键唯一保证）。
3. 新节点作为叶子挂上，从 unbal 处更新平衡因子并向下传播。
4. unbal 因子到 ±2 → 调用 balance 旋转，重新接回 parent_unbal。

**与 BTreeMap 的对应**：C 的"重复键返回、不插入"→ Rust `insert` 的 `Ok(Some(old))` 替换语义（region_map.rs:219-227，BTreeMap::insert 天然同键替换）；C 的平衡维护 → 标准库内部。

### 2.4 五向搜索

**search**（cavl_impl.h:456-493）用 `target_cmp` 技巧实现五向：

```c
if (st & AVL_LESS)      target_cmp = 1;   /* 允许键比目标大 */
else if (st & AVL_GREATER) target_cmp = -1; /* 允许键比目标小 */
else                    target_cmp = 0;   /* 必须相等 */

while (h != NULL) {
    cmp = COMPARE_KEY_NODE(k, h);
    if (cmp == 0) {
        if (st & AVL_EQUAL) { match_h = h; break; }
        cmp = -target_cmp;            /* 相等但不容许 → 转向 */
    } else if (target_cmp != 0)
        if (同号) match_h = h;         /* 记录候选：满足方向条件的最近节点 */
    h = cmp < 0 ? h->lower : h->higher;
}
return match_h;
```

**核心洞察**：一次下行同时维护"最近满足方向条件的候选"——`AVL_LESS` 时每经过一个键 < k 的节点就更新候选，最终候选就是 < k 的最大节点。

**search_least**（:496）/ **search_greatest**（:521）：一路向左/向右到叶——O(h) 找最小/最大。**search_root**（:513）：返回树根（map_free_proc 的"摘根循环"用，region.c:593）。

### 2.5 删除

**remove**（cavl_impl.h:545-761）：

1. 按键下行找目标 + 记录路径。
2. 目标有两个孩子 → 用**中序后继**替换（后继摘出，目标位置放后继），维护 branch 路径。
3. 从被删点向上回溯，逐层更新平衡因子；越界 → balance 旋转（可能级联）。
4. 返回被摘除节点句柄（调用方负责释放——region.c:598 摘根后 map_free）。

**与 BTreeMap 的对应**：`remove(&k)` 返回 `Option<VirRegion>`（region_map.rs:229-231），所有权完整交给调用方（munmap 的 re-insert、exit 的释放）。

### 2.6 路径栈迭代器

- **init_iter**（cavl_impl.h:962）：`depth = ~0`（无效标记）。
- **start_iter**（:979）：按 st 定位起点——与 search 同构的下行 + 路径记录；`depth = ~0` 表示空树/无满足节点。
- **start_iter_least**（:1039）：一路向左，路径栈压满左链。
- **get_iter**（:1087）：`depth == 0` 返回树根，否则返回 `path_h[depth-1]`。
- **incr_iter**（:1100）：中序后继——有右子树 → 右子树最左；否则沿 branch 位图**向上退到上次走左的分支**，没有则迭代结束（depth = ~0）。
- **decr_iter**（:1145）：对称前驱。

**与 BTreeMap 的对应**：`iter()`（region_map.rs:246）= BTreeMap 原生迭代器（升序、内存安全、可 `next_back`）；路径栈/branch 位图全部消失。

### 2.7 空槽查找深挖：region_find_slot_range / region_find_slot

**region_find_slot_range**（region.c:302-397，以下为摘录，省略声明行与断言）：

```c
/* maxv == 0 特例：就放 minv 这里 */
if(maxv == 0) { maxv = minv + length; if(maxv <= minv) return SLOT_FAIL; }
assert(!(length % VM_PAGE_SIZE));
if(minv + length > maxv) return SLOT_FAIL;

#define FREEVRANGE_TRY(rangestart, rangeend) {        \
    frstart = MAX(rangestart, minv); frend = MIN(rangeend, maxv); \
    if(frend > frstart && (frend - frstart) >= length) { \
        startv = frend-length; foundflag = 1; } }      /* 取高端 */
#define FREEVRANGE(start, end) {                       \
    FREEVRANGE_TRY((start)+VM_PAGE_SIZE, (end)-VM_PAGE_SIZE); /* 先内缩一页 */
    if(!foundflag) FREEVRANGE_TRY(start, end); }       /* 再全区间 */

/* 从 ≥ maxv 的区域开始，向低地址扫间隙 */
region_start_iter(&vmp->vm_regions_avl, &iter, maxv, AVL_GREATER_EQUAL);
lastregion = region_get_iter(&iter);
if(!lastregion) {  /* maxv 之上无区域 → 检查最后一个区域之后到 VM_DATATOP */
    region_start_iter(&vmp->vm_regions_avl, &iter, maxv, AVL_LESS);
    lastregion = region_get_iter(&iter);
    FREEVRANGE(lastregion ? lastregion->vaddr+lastregion->length : 0, VM_DATATOP);
}
if(!foundflag) while((vr = region_get_iter(&iter)) && !foundflag) {
    region_decr_iter(&iter);
    nextvr = region_get_iter(&iter);
    FREEVRANGE(nextvr ? nextvr->vaddr+nextvr->length : 0, vr->vaddr); /* 间隙 */
}
/* 找到后写回 hint */
vmp->vm_region_top = startv + length;
return startv;
```

要点：

- **从高往低扫**：以 `AVL_GREATER_EQUAL(maxv)` 定位第一个 ≥ maxv 的区域，用 `decr_iter` 逆序遍历间隙——分配偏好**高端对齐**（`startv = frend - length`）。
- **FREEVRANGE 两段式**：先试"内缩一页"的间隙（避免新区域与邻居共享页表边界效应），不行再试全区间。
- **hint 写回**（:391）：`vm_region_top` 供下次快速分配。

**region_find_slot**（region.c:399-415，hint 加速）：

```c
v = region_find_slot_range(vmp, minv, hint, length);  /* 先试 hint 范围 */
if(v != SLOT_FAIL) return v;
return region_find_slot_range(vmp, minv, maxv, length); /* 再全范围 */
```

**Rust 对应 find_slot**（region_map.rs:157-217）：同语义 + **页对齐增强**（try_gap 内 round_up/round_down）+ checked 溢出（maxv==0 时 `checked_add`，溢出返回 None）。C 依赖调用方保证页对齐；Rust 在框架层强制。

### 2.8 AVL API 消费面（调用点表）

| 调用方 | 操作 | 位置 |
|--------|------|------|
| region.c map_page_region | region_insert | :507 |
| region.c map_free_proc | region_search_root + region_remove | :593/:598 |
| region.c map_lookup | region_search(AVL_LESS_EQUAL) | :628 |
| region.c map_writept | start_iter_least + get_iter + incr_iter | :912/:914/:924 |
| region.c map_sanitycheck | start_iter_least + get_iter | :783-793 |
| region.c map_proc_copy_range | search_least/search_greatest + start_iter + get_iter + incr_iter | :951-965 |
| region.c map_unmap_range | start_iter(AVL_LESS_EQUAL/GREATER) + get_iter | :1236-1239 |
| utility.c transfer_mmap_regions | search(AVL_GREATER_EQUAL/AVL_EQUAL/AVL_LESS) | :233/:252/:264（10 DEFERRED） |
| rs.c 热更新 | region_search(AVL_LESS) | :116/:119/:290（25 承接） |
| fdref.c mappedfile 统计 | start_iter_least + get_iter + incr_iter | :69-74 |
| mmap.c do_munmap 前检查 | region_search_root | :540 |
| exit.c / fork.c | region_init | exit.c:37/:47、fork.c:62 |
| main.c init_vm | map_region_init（13 空钩子） | main.c:468 |

**读法**：这张表是 14 的"边界证据"——AVL 的每个操作都有消费方，Rust RegionMap 必须逐项对齐（§3.6 差异清单给出对应状态）。

### 2.9 本章小结

- cavl 是宏模板泛型：一份实现 × 实例化参数 = 类型安全的 AVL。
- 核心算法：balance（四型旋转）、insert（unbal 追踪）、search（target_cmp 五向）、remove（后继替换 + 级联平衡）、迭代器（路径栈 + branch 位图）。
- region_find_slot* 是 VM 特有的空槽分配算法（高端对齐 + hint 优化）。
- 消费面覆盖 region.c 全部生命周期操作 + utility/rs/fdref/mmap 的查询路径。

---

## 3. Rust 设计决策

### 3.1 D1: BTreeMap 替代 AVL（ARCH A-4）

`RegionMap { regions: BTreeMap<VirBytes, VirRegion> }`（region_map.rs:39）替代 regionavl：

- **非侵入节点**：VirRegion 无 lower/higher/factor（对比 region.h:63-65），可 Clone/move/split——13 的 split/remove/re-insert 流程因此可行。
- **标准库保证**：平衡（B 树）、迭代器、内存安全、Drop 自动清理。
- **O(log n) 语义等价**：五向搜索/最小最大/升序遍历全部映射（§1.6 表）。
- **ARCH 标注三处一致**：本文 §3.1、13-region-mapping §3.1、region_map.rs:1-5 模块注释。

### 3.2 D2: SearchType 枚举替代位标志

C 的 `avl_search_type` 是位标志（cavl_if.h:25-33），允许 `AVL_LESS|AVL_GREATER` 这类**非法组合**（搜索方向矛盾）。Rust 用互斥枚举（region_map.rs:19-29）：

```rust
pub(crate) enum SearchType {
    Equal, Less, Greater, LessEqual, GreaterEqual,  // 恰好五向，组合不可表达
}
impl Default for SearchType { fn default() -> Self { Self::Equal } }
```

### 3.3 D3: 迭代器 = BTreeMap 原生

`iter`/`iter_mut`（:246/:250）= BTreeMap 迭代（vaddr 升序）；`traverse`（:237）回调式遍历；`clear`（:254）清空。C 的 region_iter（路径栈 + branch 位图，cavl_if.h:158-175）整体消失。**收益**：无深度上限（C 的 AVL_MAX_DEPTH 30 是路径栈硬限制）、无手写后继算法、迭代中可安全 drop。

### 3.4 D4: find_slot 页对齐增强 + checked 运算

C 的 region_find_slot_range 依赖调用方保证页对齐（`assert(!(length % VM_PAGE_SIZE))`），FREEVRANGE 不做对齐处理。Rust 的 `find_slot`（:157-217）：

- **maxv==0 特例**：`minv.checked_add(length)` → 溢出返回 None（C 用 `maxv <= minv` 判定）。
- **try_gap 闭包**：`frend - frstart >= length` 后**页对齐**（`round_up(frstart)` / `round_down(frend)`），对齐后仍 ≥ length 才返回 `frend' - length`——非对齐间隙自动跳过。
- **c09 回归测试**（:493/:521）覆盖非对齐边界与亚页间隙。

### 3.5 D5: 无侵入式节点 → 业务不变量前置

AVL 的"键唯一 + 有序"由算法内建保证；BTreeMap 同样内建（同键替换）。**业务不变量"区域不重叠"**由 `insert` 前置检查承担（region_map.rs:219-227）：`find_overlap` 预检 → 重叠返回 `Err(region)`（调用方先 unmap，MAP_FIXED 语义）；同键替换返回 `Ok(Some(old))`。

### 3.6 语义差异清单（C ↔ Rust 诚实标注）

| 维度 | Minix3 | minix-rs | 判定 |
|------|--------|----------|------|
| 索引结构 | Walt Karas AVL（cavl_impl.h 1208 行） | BTreeMap（标准库） | ✅ ARCH A-4 |
| 搜索类型 | avl_search_type 位标志 | SearchType 互斥枚举 | ✅ 强化（非法组合不可表达） |
| 迭代器 | region_iter 路径栈（深度上限 30） | BTreeMap 原生迭代器 | ✅ 强化（无深度上限） |
| 节点 | 侵入式（region.h:63-65） | 非侵入 | ✅ 强化 |
| 空槽查找 | region_find_slot*（hint + 高端对齐） | find_slot（+ 页对齐/checked） | ✅ 等价 + 强化 |
| SearchType 家族消费 | utility.c/rs.c 搜索调用 | find_less/find_greater/less_equal/greater_equal 无生产消费方（仅测试） | ⚠️ API 就绪待接线（10/25 DEFERRED） |
| find_by_end | 无直接对应（C 用 getnextvr 相邻检查） | find_by_end/find_mut_by_end（brk.rs:108 消费） | ✅ Rust 新增便利 API |
| 对照测试 | — | BTreeMap↔AVL 等价对照测试不存在 | ⚠️ 诚实标注（§5.3） |

---

## 4. 实现详解

### 4.1 SearchType + search（region_map.rs:19 / :77）

`search(key, st)` 是五向搜索的总入口，match 分派：

- `Equal` → `regions.get(&key)`（= region_search AVL_EQUAL）
- `Less` → `range(..key).next_back()`（= AVL_LESS）
- `Greater` → `range((Excluded(key), Unbounded)).next()`（= AVL_GREATER）
- `LessEqual` → 先 `get`，无则 `range(..key).next_back()`（= AVL_LESS_EQUAL）
- `GreaterEqual` → 先 `get`，无则 `range((Excluded(key), Unbounded)).next()`（= AVL_GREATER_EQUAL）

`find_less`/`find_greater`/`find_less_equal`/`find_greater_equal`（:99-:130）是 search 的薄封装——**当前仅测试消费**（utility.c/rs.c 的对应生产路径 DEFERRED，见 §3.6）。

### 4.2 find 家族（:58-:133）

- **find**（:58）= C map_lookup 的索引步：`range(..=addr).next_back()` + `contains_addr` 过滤——地址包含查找（13 §2.4 已述）。
- **find_mut**（:66）：先定位 key 再 `get_mut`——避免 range 迭代器与可变借用冲突。
- **find_by_end**（:103）/ **find_mut_by_end**（:111）：`range(..end).next_back()` + `end_addr() == end` 过滤——按区域末尾定位（brk 收缩/扩展找堆顶区域，brk.rs:108 用 find_mut_by_end）。

### 4.3 find_slot（:157-217）

算法骨架（§3.4 已述）：

```rust
pub(crate) fn find_slot(&self, minv, maxv, length) -> Option<VirBytes> {
    if length.0 == 0 { return None; }
    let maxv = if maxv.0 == 0 { VirBytes(minv.0.checked_add(length.0)?) } else { maxv };
    if minv.0 >= maxv.0 || minv.0.checked_add(length.0)? > maxv.0 { return None; }

    let try_gap = |gap_start, gap_end| { /* frstart/frend 截断 + 页对齐 + frend'-length */ };

    let mut prev_end = minv;
    for region in self.iter() {
        if region.vaddr >= prev_end {
            if let Some(addr) = try_gap(prev_end, region.vaddr) { return Some(addr); }
        }
        prev_end = region.end_addr().max(prev_end);
    }
    try_gap(prev_end, maxv)   // 最后一个区域之后
}
```

**与 C 的差异**：C 从 maxv 向下扫（AVL_GREATER_EQUAL 定位 + decr_iter）；Rust 从 minv 向上扫（iter + prev_end 前进）——**分配结果相同**（都返回间隙高端的 frend'-length），方向相反但语义等价（都是"第一个满足的间隙"）。C 的两段式 FREEVRANGE（先内缩一页）在 Rust 中由页对齐吸收（内缩效果≈对齐损失）。

### 4.4 insert / remove / 迭代（:219-254）

- **insert**（:219）：`find_overlap(region.vaddr, end)` 预检 → 重叠 `Err(region)`；无重叠 → `BTreeMap::insert`（同键替换 `Ok(Some(old))`）。
- **remove**（:229）：`regions.remove(&addr)` → `Option<VirRegion>`（所有权给调用方）。
- **get_mut**（:233）/ **traverse**（:237）/ **iter**（:246）/ **iter_mut**（:250）/ **clear**（:254）：集合基础操作。

### 4.5 消费面接线

| Rust 消费方 | RegionMap 方法 | C 对应 |
|------------|---------------|--------|
| mmap.rs:219/:221/:225 | find_slot | region_find_slot（map_page_region 内） |
| dispatcher.rs:504/:1397 | find_slot | region_find_slot |
| brk.rs:108 | find_mut_by_end | getnextvr 相邻检查（region.c:112-128） |
| munmap.rs:155-192 | insert（split 后 re-insert） | region_insert |
| vmproc_handle.rs:457/:469 | regions/regions_mut 访问器 | vm_regions_avl（vmproc.h:21） |

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

**region_map.rs**（16 个，全部与查找语义直接相关）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_insert_and_find | :274 | 插入乱序 + find 命中/未命中 |
| test_remove | :291 | 摘除后 find None |
| test_find_overlap | :308 | 重叠检测 |
| test_traverse | :322 | 升序遍历（vaddr 有序） |
| test_iter | :339 | 迭代器升序 |
| test_search_type_less / greater | :355/:371 | 严格小于/大于 |
| test_search_type_less_equal / greater_equal | :387/:403 | 含等于 |
| test_find_slot_basic | :419 | 空槽命中 |
| test_find_slot_in_gap | :432 | 间隙内找槽 |
| test_find_slot_no_space | :446 | 无空槽 None |
| test_find_all_overlaps | :457 | 全量重叠迭代 |
| test_search_type_enum | :476 | 枚举互斥 + Default=Equal |
| test_find_slot_alignment_c09 | :493 | 非对齐边界页对齐 |
| test_find_slot_subpage_gap_c09 | :521 | 亚页间隙 None |

### 5.2 覆盖维度

- **五向搜索**：Equal/Less/Greater/LessEqual/GreaterEqual 全覆盖（:355-:413）。
- **地址包含查找**：find/insert/remove/overlap。
- **空槽查找**：基础/间隙/无空间/页对齐/亚页间隙 5 态。
- **有序遍历**：traverse/iter 升序断言。
- **枚举安全**：互斥 + Default。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 | 说明 |
|------|------|------|
| SearchType 家族生产消费 | ⚠️ API 就绪待接线 | `find_less`/`find_greater`/`find_less_equal`/`find_greater_equal` 仅测试消费；C 侧消费点 utility.c:233/:252/:264（transfer_mmap_regions，10 DEFERRED）与 rs.c:116/:119/:290（25 承接）尚无 Rust 对应路径 |
| BTreeMap↔AVL 等价对照测试 | ⚠️ 缺失 | 语义等价靠本文档 §1.6 表 + 单侧测试断言；无"同一数据同时喂 AVL 与 BTreeMap 比对结果"的测试（C AVL 无法在 Rust 测试中实例化） |
| find_slot 端到端 | ⚠️ backlog | mmap.rs/dispatcher.rs 调用点无直接集成测试（20/15 承接） |
| region_find_slot_range 的内缩一页（FREEVRANGE）语义 | ✅ 已吸收 | Rust 页对齐等效处理，c09 测试覆盖 |

### 5.4 测试统计（截至 2026-08-16）

```
$ cd os && cargo test -p minix-vm --lib
→ 360 passed / 1 failed（test_map_lazy pre-existing，13 范围，§5.3 已标注）
$ cargo test -p minix-vm --lib region_map → 16 passed
$ cargo test -p minix-vm --lib vir_region → 10 passed / 1 failed（test_map_lazy）
```

---

## 6. 过渡

14 完成阶段 5 地址空间数据结构的索引面（11 状态 → 12 策略 → 13 生命周期 → **14 索引**）。接下来：

- **15-ipc-dispatch**：主循环分发，dispatcher.rs:504/:1397 的 find_slot 调用点接线。
- **16/17**：页错误 + CoW 消费 find/map_lookup（地址→区域解析）。
- **18-vm-fork**：fork_regions 消费 search_least/search_greatest 等价（iter().next()/next_back()）。
- **20/21**：mmap 消费 find_slot；munmap 消费 find_overlap/insert。
- **25/26**：RS 热更新接线 region_search 族；查询消费 map_lookup。

位置可回答性：本文档的索引结构在 **VM 启动链 `init_vm()` 初始化 vmproc 槽时建立**（region_avl 内嵌 vmproc.h:21，Rust 侧 vmproc.rs:45 `vm_regions: MaybeUninit<RegionMap>`），查找操作全部发生在**主循环分发后的服务路径**（16/18/19/20/21/25/26）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md` — 区域生命周期 + RegionMap 操作面（本文件前置）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/16-pagefault.md` — 页错误状态机（消费 find/map_lookup）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/17-cow-mechanism.md` — CoW 分裂（消费地址解析）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/18-vm-fork.md` — fork（消费 search_least/greatest 等价）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/20-vm-mmap.md` — mmap（消费 find_slot）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/21-vm-munmap.md` — munmap（消费 find_overlap/insert）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/25-rs-services.md` — RS 热更新（接线 region_search 族）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/26-vm-queries.md` — 查询（消费 map_lookup）
- `minix3/minix/servers/vm/cavl_if.h`、`minix3/minix/servers/vm/cavl_impl.h`、`minix3/minix/servers/vm/regionavl_defs.h`、`minix3/minix/servers/vm/unavl.h`、`minix3/minix/servers/vm/regionavl.c` — C AVL 实现
- `minix3/minix/servers/vm/region.c`（`region_find_slot_range` :302 / `region_find_slot` :399）— 空槽查找
- `os/servers/vm/src/region/region_map.rs` — Rust 实现
