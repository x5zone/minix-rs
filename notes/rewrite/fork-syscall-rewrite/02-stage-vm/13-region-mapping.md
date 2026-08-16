# 13-region-mapping: 区域映射——地址空间的框架操作族

> **分类**: 阶段 5 — 地址空间数据结构（框架层）
> **源码**: `minix3/minix/servers/vm/region.c`（1555 行）+ `minix3/minix/servers/vm/region.h`（`vir_region` 结构 :37-65、VR_* 标志 :69-78）+ `minix3/minix/servers/vm/phys_region.h`（`phys_region` 结构 :8-21）
> **Rust 模块**: `os/servers/vm/src/region/region_map.rs`（533 行：`RegionMap` :39 / `SearchType` :19 / `find_slot` :157）+ `os/servers/vm/src/region/vir_region.rs`（522 行：`VrFlags` :17-42 / `VrParam` :44 / `VirRegion` :57 / `split` :272 / `free_range` :336）+ `os/servers/vm/src/region/mod.rs`（199 行：`free_region_pages` :23 / `map_pin_memory` :128）+ 消费模块（`os/servers/vm/src/fork.rs` `handle_memory_once` :33 / `fork_region` :92、`os/servers/vm/src/munmap.rs` :155-192、`os/servers/vm/src/brk.rs` :106/:109）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/11-phys-pagestate.md`（phys_block/phys_region 生命周期 + PageFrames refcount）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/12-memtype.md`（memtype 策略层）
> **说明**: 区域映射**框架层**语义模块：**Minix3 region.c 的 22 个框架函数（建/查/填/复制/扩/缩/释放/工具）→ minix-rs 的 `RegionMap`（BTreeMap）+ `VirRegion`（Vec<PageSlot>）+ 分散到消费模块的顶层函数**。12 管"做什么"（memtype 策略），本文档管"何时调用"（框架流程）。**不覆盖**：查找语义（14）、页错误状态机消费（16）、CoW 分裂（17）、fork 全流程（18）、munmap 服务（21）、RS Live Update（25）、查询（26）。

---

## 1. 概念：区域映射生命周期

### 1.0 章节引言

进程的虚拟地址空间不是一个连续的整体，而是一组**互不重叠的区间**（region）——代码段、堆、栈、mmap 文件各占一段。VM 要做的事情有两类：**静态的**（区间属性：可写？匿名？共享？）和**动态的**（页挂载：这个虚拟页现在有没有物理页支撑？）。

本文档回答三个问题：

1. **区域映射解决什么问题**——地址空间分段 + 按页挂载的框架职责（§1.1-§1.2）。
2. **框架操作族有哪些**——建/查/填/复制/扩/缩/释放七类操作，各自在什么时机被调用（§1.4-§1.8）。
3. **Rust 怎么建模**——BTreeMap + Vec\<PageSlot\> 替代 C 的 AVL + 指针数组（§1.9）。

它在地址空间数据结构阶段的位置：

```
11（物理页状态：refcount/标志）→ 12（内存语义：类型回调）→ ★13（区域映射：框架操作）
→ 14（区域查找：BTreeMap 语义）→ 15（主循环）→ 16/17（页错误 + CoW 消费框架）
→ 18/21/25/26（fork/munmap/RS/查询消费具体操作）
```

### 1.1 区域映射解决什么问题

11-phys-pagestate §1.4 把三层结构分解为五个子问题。区域映射（13）承担其中两个框架面：

| # | 子问题 | 归属 | 13 的框架职责 |
|---|--------|------|--------------|
| P1 | 虚拟地址空间分段 | **13** | vir_region 区间的建立/复制/扩展/收缩/拆除 |
| P2 | 虚拟页→物理页映射 | **13** | physblocks[] 槽位的挂载/摘除（physblock_get/set 契约） |
| P3 | 物理页共享与引用计数 | 11 | 13 只读写 refcount（map_page++ / unmap_page--） |
| P4 | 多种内存类型的行为差异 | 12 | 13 按契约调用 memtype 回调（ev_new/ev_pagefault/ev_copy/ev_resize/ev_split/ev_lowshrink/ev_delete） |
| P5 | 延迟分配与按需填充 | 16/17 | 13 提供 map_pf 框架，缺页状态机（16）消费 |

**框架与策略的分工**（承接 12 §1.3）：区域层代码只写一遍（map_pf/复制/释放），行为差异全部封在 memtype 回调里。框架函数的职责是**决定何时调用哪个回调、参数怎么组装、错误怎么传播**。

### 1.2 vir_region：地址空间的分段单元

`vir_region`（region.h:37-65）描述一段连续虚拟地址 `[vaddr, vaddr+length)`：

| 字段 | 含义 |
|------|------|
| `vaddr` / `length` | 虚拟地址范围（页对齐） |
| `physblocks[]` | 每页一个槽位的指针数组（NULL = 未映射） |
| `flags` | VR_* 标志（权限 + 类型） |
| `parent` | 归属进程（vmproc） |
| `def_memtype` | 默认内存类型（12 的策略） |
| `remaps` / `id` | 共享源 remap 计数 / 唯一 id |
| `param` | 类型特有参数联合体（VR_DIRECT 物理地址 / shared 源 / file fdref / pb_cache） |

VR_* 标志（region.h:69-78）分两组：

- **权限/分配约束**：`VR_WRITABLE`（0x001，可写）、`VR_PHYS64K`（0x004，物理 64K 对齐）、`VR_LOWER16MB`（0x008）、`VR_LOWER1MB`（0x010）、`VR_UNINITIALIZED`（0x080，分配后不清零）
- **映射类型**：`VR_SHARED`（0x040，共享可见）、`VR_ANON`（0x100，匿名）、`VR_DIRECT`（0x200，直接映射不归 VM 管）、`VR_PREALLOC_MAP`（0x400，RS 预分配）

`flags` 与 `def_memtype` 正交：flags 是静态类型/权限标记，def_memtype 是动态行为策略。例如 `VR_ANON` 区域通常配 `mem_type_anon`，但文件映射区域没有专用 flag——类型由 `def_memtype == mem_type_mappedfile` 隐式决定。Rust 中由 `VrFlags`（bitflags）+ `VrParam`（enum）+ `def_memtype: Option<&'static dyn MemType>` 三者分别承担，消除了 C union 的变体歧义。

### 1.3 phys_region 中间层（回顾）

`phys_region`（phys_region.h:8-21）是**每个虚拟页一个的映射记录**：`ph`（指向共享的 phys_block）、`parent`、`offset`（页在区域中的偏移）、`memtype`（该页的内存类型，可不同于区域默认）、`next_ph_list`（同一 phys_block 的反向链表节点）。

11 已详述其生命周期；13 只需记住两条框架契约：

- **physblock_get**（region.c:60）——按 offset 取槽位，NULL 表示未映射。
- **physblock_set**（region.c:72）——写槽位并更新进程记账（`vm_total += VM_PAGE_SIZE`，峰值 `vm_total_max` 同步）。

### 1.4 框架操作族总览

region.c 的框架函数按生命周期归类：

| 类别 | 函数 | 触发时机 | 调用的 memtype 回调 |
|------|------|---------|--------------------|
| 建 | `map_page_region`（:463） | mmap/brk/RS 预分配 | ev_new、ev_pagefault（MF_PREALLOC 时经 map_handle_memory） |
| 查 | `map_lookup`（:616） | 所有按地址找区域的路径 | — |
| 填 | `map_pf`（:664） | 页错误/预填 | ev_pagefault |
| 批量填 | `map_handle_memory`（:756）/`map_pin_memory`（:779）/`map_writept`（:906） | VFS 异步续作/RS LU/全量页表同步 | ev_pagefault、pt_flags |
| 复制 | `map_copy_region`（:802）/`map_proc_copy`（:933）/`map_proc_copy_range`（:944） | fork | ev_copy、ev_reference |
| 扩展 | `map_region_extend_upto_v`（:1002） | brk 增长 | ev_resize（或回退 anon） |
| 收缩/分割 | `map_unmap_region`（:1065）/`split_region`（:1150）/`map_unmap_range`（:1222） | munmap | ev_lowshrink、ev_split |
| 释放 | `map_subfree`（:527）/`map_free`（:568）/`map_free_proc`（:589） | exit/失败回滚 | ev_delete、ev_unreference（经 pb_unreferenced） |
| 工具 | `physblock_get/set`、`vrallocflags`（:645）、`physregions`（:1546）、`map_region_lookup_type`（:1303） | 各处 | — |

**读法**：这张表回答"13 在 VM 启动时序和主循环的哪个位置被调用"——`map_region_init`（:36）挂在 `init_vm()` 启动链（空钩子），其余操作全部由 15 主循环分发的服务（18 fork / 20-21 mmap/munmap / 19 brk / 25 RS / 16 页错误）触发。

### 1.5 map_pf：页错误框架（三阶段）

`map_pf`（region.c:664）是区域层最核心的框架函数，单页填充，三阶段：

```
map_pf(vmp, region, offset, write, pf_callback, state, len, io)
  ├─ 阶段 1：槽位不存在 → pb_new(MAP_NONE) + pb_reference（建 phys_region，refcount++）
  ├─ 阶段 2：不可写或需处理 → ph->memtype->ev_pagefault(...)
  │     ├─ 返回 SUSPEND → 挂起（VFS 异步 IO，等 reply 续作）
  │     ├─ 返回错误 → pb_unreferenced + 返回 errno
  │     └─ 返回 OK → 页已就绪（phys != MAP_NONE）
  └─ 阶段 3：map_ph_writept（按 pr_writable 组装 PTF_* 标志写页表）
```

阶段 2 是框架与策略的分界线：**框架决定"要不要调用"（write && !writable 时），策略决定"做什么"**（anon 分配页 / directphys 算 PA / shared 递归源 / cache 查缓存 / file 发 VFS 请求）。`SUSPEND` 是伪返回码——VM 收到 VFS reply 后由 16 的 handle_memory 状态机续作。

### 1.6 map_copy_region：复制即共享（fork 支撑）

fork 的地址空间复制不走"深拷贝"，而是**结构复制 + 物理页共享**：

- `map_copy_region`（:802）创建完整的新 `vir_region` 数据结构（region_new + ev_copy 类型参数复制），逐页 `pb_reference`（refcount++）**不复制物理页**。
- 新区域处于 **limbo** 状态：先复制、后由调用方挂进目标进程的 AVL——这样 sanity check 始终看到一致的树。
- 失败回滚：任意一步失败 → `map_free(newvr)` 归还已引用的页。
- `map_proc_copy`（:933）清空目标 AVL 后调 `map_proc_copy_range`（:944）复制源进程全部区域；复制完成后 `map_writept(src)` + `map_writept(dst)` 重建页表映射。

**为什么 refcount 是关键**：复制后父子进程的两个 phys_region 指向同一 phys_block（refcount=2）。写入时 17 的 CoW 分裂只替换写入方的 phys_region.ph，另一方的映射不受影响——这正是"复制即共享 + 写时分裂"的最小机制。

### 1.7 收缩与分割：unmap 三分支

`map_unmap_region`（:1065）从区域内 offset 起收缩 len 字节，先 `map_subfree` 摘除页（pb_unreferenced），再按三种情形处理区域边界：

1. **整区消失**（`length == len`）：AVL 摘链 + map_free。
2. **低端收缩**（`offset == 0`）：ev_lowshrink 回调 + vaddr 平移 + 所有 phys_region 的 offset 同步平移 + memmove 槽位数组。
3. **高端收缩**（`offset + len == length`）：直接 `length -= len`。

最后 `pt_writemap(MAP_NONE, len, ...)` 摘除页表映射。

`map_unmap_range`（:1222）处理跨区域范围：找到第一个命中区域后迭代，若范围**落在区域中间**则先 `split_region`（:1150，ev_split 回调 + pb_reference 分拆成两半，bail 回滚），再对中间段调 `map_unmap_region`。**可分割性是 memtype 策略**：directphys 的 ev_split 为 NULL → EINVAL（设备映射区间是原子整体），anon 的 ev_split 是空实现（自由分割）。

### 1.8 RS Live Update 预分配

RS 热更新（25）需要"把进程内存全部弄到物理内存里"的确定性保证。region.c 提供两个配套机制：

- **`map_page_region(MF_PREALLOC)`**（:491）：建区域时立即 `map_handle_memory` 预填全部页（rs.c:317 用 `VR_ANON|VR_WRITABLE|VR_UNINITIALIZED, MF_PREALLOC` 建预分配区域）。
- **`map_region_lookup_type`**（:1303）：按标志线性扫描区域树（rs.c:177/:334 在 swap 进程槽前后查找 `VR_PREALLOC_MAP` 区域，复用旧物理页避免重新分配）。

预分配区域在 swap 完成后被复用/释放——这是 25 的细节，13 只保证两个框架入口存在。

### 1.9 Rust 模型总览

C 的"指针网络"在 Rust 中变成"索引数组"三件套：

```
C:  vmproc.vm_regions_avl (AVL) ── vir_region ── physblocks[] (phys_region* 数组)
                                                   └── phys_region ── phys_block (refcount + firstregion 链表)

Rust: ActiveProc.regions (RegionMap = BTreeMap<VirBytes, VirRegion>)
                              └── VirRegion.physblocks (Vec<PageSlot>)   ← PageSlot 内嵌 pfn/offset/memtype
                                                                          └── PageFrames[PFN].refcount
```

- **RegionMap**（region_map.rs:39）：BTreeMap 按 vaddr 有序，`find`（:58）= C map_lookup，`find_slot`（:157）= C region_find_slot_range。
- **VirRegion.physblocks**（vir_region.rs:57-71）：`Vec<PageSlot>`，`PageSlot::EMPTY`（pfn=PFN_NONE 哨兵）替代 NULL 指针，省 Option 判别开销。
- **PageFrames**：phys_block 的集中替代（11），refcount 由 map_page/unmap_page 读写。
- **框架操作分散**：map_free → `free_region_pages`（mod.rs:23）、map_pin_memory → `map_pin_memory`（mod.rs:128）、map_copy_region → `fork_region`（fork.rs:92）、map_handle_memory → `handle_memory_once`（fork.rs:33）。

### 1.10 对照 Redox / Linux

- **Linux**：`struct vm_area_struct` + `rb_tree`/interval tree 与 vir_region + AVL 同构；`vma_ops`（fault/open/close）与 memtype 回调同构；`fork()` 时 `dup_mmap()` 对每个 vma 做 `vma->vm_ops->open()` + 页表复制（COW 只读标记）——与 `map_proc_copy` + ev_copy 同构。差异：Linux 用 rmap（反向映射，`struct anon_vma`/`address_space`）跟踪"页被哪些 vma 引用"，Minix3 用 phys_block.firstregion 链表；Rust 用 PageFrames[PFN] 集中 refcount。
- **Redox**：无独立 vir_region 对象——地址空间是内核页表的直接视图（`AddressSpace` 封装页表 + 内核堆分配），区域属性（如 COW）由页表标志直接表达，没有 per-region 策略回调。Minix3/Rust 的"区域 + 类型回调"是更强的抽象（支持共享内存、文件映射、设备映射的差异化行为）。

### 1.11 本章小结

- 区域映射框架 = **七类操作族**（建/查/填/复制/扩/缩/释放），全部围绕 vir_region 区间 + physblocks 槽位展开。
- **框架管流程，类型管行为**：每个操作在正确的时机调用 memtype 回调，行为差异封在 12。
- Rust 建模核心：**指针网络 → 索引数组**（AVL → BTreeMap、phys_region* → Vec\<PageSlot\>、firstregion 链表 → PageFrames[PFN]）。

---

## 2. C 源码分析

### 2.0 本章定位

region.c（1555 行）是区域框架的完整实现。按生命周期分组逐函数实证（行号 `sed -n` 验证）。**不覆盖**：`region_find_slot*` 细节（14 查找语义）、查询函数 `map_get_phys`/`map_get_ref`/`get_usage_info`/`get_region_info`（26）、`copy_abs2region`（plan §5.4 死函数跳过）、SANITYCHECKS 门控族（A-7 cfg 替代）。

### 2.1 结构与标志（region.h + phys_region.h）

- `vir_region` 结构：region.h:37-65（vaddr/length/physblocks/flags/parent/def_memtype/remaps/id/param/AVL 字段）。
- `phys_block` 结构：region.h:23-35（phys/firstregion/refcount u8/flags + SANITYCHECKS seencount）。
- `phys_region` 结构：phys_region.h:8-21（ph/parent/offset/written/memtype/next_ph_list）。
- VR_* 标志：region.h:69-78（见 §1.2）；`MF_PREALLOC 0x01`：region.h:82。

### 2.2 physblock_get / physblock_set（region.c:60 / :72）

```c
struct phys_region *physblock_get(struct vir_region *region, vir_bytes offset)  /* :60 */
{
	assert(!(offset % VM_PAGE_SIZE));
	assert(offset < region->length);
	i = offset/VM_PAGE_SIZE;
	if((foundregion = region->physblocks[i]))
		assert(foundregion->offset == offset);
	return foundregion;
}

void physblock_set(struct vir_region *region, vir_bytes offset,
	struct phys_region *newphysr)                                            /* :72 */
{
	proc = region->parent;
	if(newphysr) {
		proc->vm_total += VM_PAGE_SIZE;
		if (proc->vm_total > proc->vm_total_max)
			proc->vm_total_max = proc->vm_total;
	} else {
		proc->vm_total -= VM_PAGE_SIZE;
	}
	region->physblocks[i] = newphysr;
}
```

契约要点：

- **offset 页对齐 + 槽位一致性断言**：get 时若槽非 NULL 必须 `foundregion->offset == offset`；set 时新槽必须 offset 匹配、旧槽必须存在。
- **记账内联**：`vm_total`（进程占用的物理页总量）与 `vm_total_max`（峰值，getrusage maxrss 数据源，region.c:1444 消费）在 set 时更新——Rust 侧由消费模块的 `sub_total`（munmap.rs:151/:167/:186）承担。

### 2.3 map_page_region（region.c:463）+ region_new（:424）+ find_slot（:302/:399）

```c
struct vir_region *map_page_region(struct vmproc *vmp, vir_bytes minv,
	vir_bytes maxv, vir_bytes length, u32_t flags, int mapflags,
	mem_type_t *memtype)                                                     /* :463 */
{
	startv = region_find_slot(vmp, minv, maxv, length);   /* :474 */
	if (startv == SLOT_FAIL) return NULL;

	newregion = region_new(vmp, startv, length, flags, memtype);  /* :479 */
	if (newregion->def_memtype->ev_new)
		ev_new(newregion);                                    /* :489 */
	if (mapflags & MF_PREALLOC)
		map_handle_memory(vmp, newregion, 0, length, 1, NULL, 0, 0);  /* :492-498 */
	newregion->flags &= ~VR_UNINITIALIZED;                    /* :504 */
	region_insert(&vmp->vm_regions_avl, newregion);           /* :507 */
	return newregion;
}
```

- **region_find_slot**（:399）：用 `vm_region_top` 提示（上次插入的末尾）先试 `[minv, hint)` 再试全范围；**细节移交 14**（AVL 迭代 + FREEVRANGE 宏）。
- **region_new**（:424）：`SLABALLOC` 分配 vir_region + `calloc(slots)` 分配 physblocks 指针数组 + 静态 `id++` 自增（区域唯一 id）。
- **ev_new 时机**：在**插入 AVL 之前**调用——回调失败时"ev_new 会自己释放并移除区域"（region.c:487 注释），返回 NULL。
- **MF_PREALLOC**：建区域后立即全量预填（RS 热更新用，§1.8）。
- **VR_UNINITIALIZED 清除**：预分配完成后区域进入"已初始化"状态（`vrallocflags` 用该标志决定是否 PAF_CLEAR，§2.10）。

### 2.4 map_lookup（region.c:616）

```c
struct vir_region *map_lookup(struct vmproc *vmp, vir_bytes offset,
	struct phys_region **physr)                                               /* :616 */
{
	r = region_search(&vmp->vm_regions_avl, offset, AVL_LESS_EQUAL); /* :628 */
	if(r && offset >= r->vaddr && offset < r->vaddr + r->length) {
		ph = offset - r->vaddr;
		if(physr) { *physr = physblock_get(r, ph); ... }
		return r;
	}
	return NULL;
}
```

**less_equal + contains 过滤**：AVL 找 ≤ offset 的最大区域，再验证 offset 是否落在其 `[vaddr, vaddr+length)` 内。这是所有"地址 → 区域"解析的入口（页错误、brk、查询）。Rust 对应 `RegionMap::find`（region_map.rs:58，BTreeMap `range(..=addr).next_back()` + `contains_addr` 过滤）。

### 2.5 填充族：map_pf（:664）/ map_handle_memory（:756）/ map_pin_memory（:779）/ map_writept（:906）/ map_ph_writept（:257）

**map_pf**（:664）三阶段已在 §1.5 概述。补充关键断言与错误路径：

- `offset -= offset % VM_PAGE_SIZE`（页对齐）；`assert(!(write && !(region->flags & VR_WRITABLE)))`——**写请求且区域不可写是框架层 bug**（调用方已做权限检查）。
- 阶段 1 失败：`pb_new`/`pb_reference` 返回 NULL → ENOMEM（pb_reference 失败时 `pb_free(pb)` 归还）。
- 阶段 2 `ev_pagefault` 失败：`pb_unreferenced(region, ph, 1)`（rm=1 摘除槽）→ 返回 errno。
- 阶段 3 `map_ph_writept` 失败：ENOMEM（页表页不足）。
- SANITYCHECKS 下 `pt_checkrange` 验证写映射结果（:744-748）。

**map_ph_writept**（:257）组装页表标志：`PTF_PRESENT|PTF_USER` 基线，`pr_writable(vr, pr)`（:132，`VR_WRITABLE && memtype->writable(pr)`）时加 `PTF_WRITE`，再叠加 `def_memtype->pt_flags(vr)`（如 directphys 的 NO_CACHE）。写表成功标记 `pr->written = 1`（SANITYCHECKS 下 WMF_OVERWRITE 保护首次写）。

**map_handle_memory**（:756）：`for(offset = start; offset < lim; offset += VM_PAGE_SIZE) map_pf(...)`——逐页循环的批量填充。**16 的 handle_memory_start/step/once 状态机**就是把这个循环拆成可挂起的步骤（VFS 异步 IO 时每次一步）。

**map_pin_memory**（:779）：遍历进程所有区域，对每个区域 `map_handle_memory(0, length, wrflag=1)`——失败即 panic（RS LU 要求确定性）。Rust 改为返回 `PinMemoryError`（mod.rs:155）。

**map_writept**（:906）：遍历所有区域所有槽位调 `map_ph_writept`——**全量页表重建**（fork 复制后父子同步页表，region.c:995-996 在 map_proc_copy_range 尾部调用）。

### 2.6 复制族：map_copy_region（:802）/ map_proc_copy（:933）/ map_proc_copy_range（:944）

```c
struct vir_region *map_copy_region(struct vmproc *vmp, struct vir_region *vr)  /* :802 */
{
	newvr = region_new(vr->parent, vr->vaddr, vr->length, vr->flags, vr->def_memtype); /* :821 */
	USE(newvr, newvr->parent = vmp;);

	if(vr->def_memtype->ev_copy && (r = ev_copy(vr, newvr)) != OK) {  /* :826 */
		map_free(newvr);
		return NULL;
	}

	for(p = 0; p < phys_slot(vr->length); p++) {                    /* :832 */
		if(!(ph = physblock_get(vr, p*VM_PAGE_SIZE))) continue;
		newph = pb_reference(ph->ph, ph->offset, newvr, vr->def_memtype); /* :835 */
		if(!newph) { map_free(newvr); return NULL; }
		if(ph->memtype->ev_reference)
			ev_reference(ph, newph);                                /* :839 */
	}
	return newvr;
}
```

- **limbo 契约**：新区域不挂 AVL（调用方挂），phys_block refcount **不加**（注释 :806-809：调用方挂链后再加，保持 sanity check 一致）。实际代码 `pb_reference` 会加 refcount——注释与实现的差异是 C 的历史遗留，Rust 侧 `fork_region`（fork.rs:92）直接复制槽位 + `ev_copy` + fdref ref，语义更直白。
- **ev_copy**：类型特有参数复制（shared 源 / file fdref / directphys 物理地址）。
- **ev_reference**：页级引用回调（anon 无操作；cache 的 `cow_block` 等）。
- SANITYCHECKS 断言 `physregions(vr) == physregions(newvr)`（:851）——复制后槽位数量一致。

**map_proc_copy**（:933）：`region_init(&dst->vm_regions_avl)` 清空目标，然后 `map_proc_copy_range(dst, src, NULL, NULL)`（全量）。

**map_proc_copy_range**（:944）：从 `start_src_vr` 到 `end_src_vr`（默认全树最小→最大）逐区域 `map_copy_region` + `region_insert`，任一失败 → `map_free_proc(dst)` 回滚。SANITYCHECKS 下逐页断言 `orig_ph->ph == new_ph->ph`（物理页共享验证）。尾部 `map_writept(src)` + `map_writept(dst)`（:995-996）重建页表。**18 的 fork 全流程消费此函数**。

### 2.7 map_region_extend_upto_v（region.c:1002）

brk 增长路径（19 消费）：把包含 offset 的区域扩展到覆盖 v。两个分支：

1. **memtype 无 ev_resize**（如 anon）：`map_page_region(limit, 0, extralen, VR_WRITABLE|VR_ANON, ..., &mem_type_anon)`——**在区域末尾新建一个 anon 区域**（不合并，两个相邻区域）。
2. **有 ev_resize**：`realloc(physblocks)` 扩容槽位数组 + memset 清零新增槽 + `ev_resize(vmp, vr, offset - vr->vaddr)` 让类型自行处理（如 anon_contig 预分配更多连续页）。

错误路径：找不到包含区域（AVL_LESS 失败）→ ENOMEM；下一个区域挡住（`nextvr->vaddr < offset`）→ ENOMEM。

### 2.8 收缩与分割：map_unmap_region（:1065）/ split_region（:1150）/ map_unmap_range（:1222）

**map_unmap_region**（:1065）三分支已在 §1.7 概述。补充实现细节：

- 低端收缩时 `pr->offset -= len` 平移所有剩余 phys_region 的偏移，`memmove(r->physblocks, r->physblocks + freeslots, ...)` 压缩槽位数组，`r->length -= len`、`r->vaddr += len`。
- 尾部 `pt_writemap(regionstart, MAP_NONE, len, 0, WMF_OVERWRITE)` 摘页表映射（region.c:1139-1141）——**页表与元数据同步**在框架层完成。

**split_region**（:1150）：把一个区域沿 split_len 切成 vr1/vr2：

- 断言页对齐；`ev_split` 为 NULL → EINVAL（不可分割类型）。
- 两个 region_new + 各自逐页 pb_reference（槽位分给 r1/r2）。
- `ev_split(vmp, vr, r1, r2)` 让类型调整参数（如 file 区域 split 后 offset/clearend 分裂）。
- 摘旧链 + `map_free(vr)` + 插 r1/r2；bail 时 `map_free(r1)`/`map_free(r2)` 回滚。

**map_unmap_range**（:1222）：页对齐输入（`unmap_start -= o; length += o; roundup`），迭代命中区域：

- 范围完全覆盖区域 → 直接 unmap_region。
- 范围在区域中间 → 先 split_region 切成三段语义（实际两刀：head/remainder，再对 remainder 按 length 切），对中间段 unmap_region。
- 迭代器维护：`nextvr` 在 unmap 后重新定位（AVL 结构已变）。

### 2.9 释放族：map_subfree（:527）/ map_free（:568）/ map_free_proc（:589）

```c
static int map_subfree(struct vir_region *region, vir_bytes start, vir_bytes len)  /* :527 */
{
	for(voffset = start; voffset < end; voffset += VM_PAGE_SIZE) {
		if(!(pr = physblock_get(region, voffset))) continue;
		assert(pr->offset >= start && pr->offset < end);
		pb_unreferenced(region, pr, 1);   /* rm=1：摘槽 + refcount-- + 归零时 ev_unreference + free */
		SLABFREE(pr);                     /* 释放 phys_region 对象 */
	}
	return OK;
}

int map_free(struct vir_region *region)                                    /* :568 */
{
	map_subfree(region, 0, region->length);
	if(region->def_memtype->ev_delete)
		ev_delete(region);                 /* 类型级清理（如 anon_contig 归还连续页） */
	free(region->physblocks);
	SLABFREE(region);
	return OK;
}

int map_free_proc(struct vmproc *vmp)                                      /* :589 */
{
	while((r = region_search_root(&vmp->vm_regions_avl))) {  /* 摘根 → map_free → 直到空 */
		region_remove(&vmp->vm_regions_avl, r->vaddr);
		map_free(r);
	}
	region_init(&vmp->vm_regions_avl);                       /* 重置 AVL */
	return OK;
}
```

**pb_unreferenced（rm=1）语义**（pb.c:96，11 已述）：refcount--；归零时调 `ev_unreference`（anon 还物理页）并 `pb_free`。**释放顺序**：C 先摘页（map_subfree → pb_unreferenced → ev_unreference），再 ev_delete（类型级），最后释放区域对象。Rust 的 `free_region_pages`（mod.rs:23）顺序不同：pt.unmap → ev_delete → free_range（摘槽 + 收集 pending）→ ev_unreference + free_pfn → fdref deref。**顺序差异无害**：PFN 模型下所有 memtype 的 ev_unreference 均为 no-op（物理页由框架 free_pfn 归还，12 §3.4 D4），ev_delete 只清区域参数（cache pfn / file fdref）。

### 2.10 工具与调试

- **vrallocflags**（:645）：VR_* 标志 → 物理页分配标志（PAF_*）：`VR_PHYS64K → PAF_ALIGN64K`、`VR_LOWER16MB → PAF_LOWER16MB`、`VR_LOWER1MB → PAF_LOWER1MB`、无 `VR_UNINITIALIZED → PAF_CLEAR`（分配后清零）。Rust 对应 `VrFlags::to_alloc_flags`（vir_region.rs:33，PageAllocFlags）。
- **physregions**（:1546）：统计区域中已挂载的槽位数（sanity 断言用）。
- **map_region_lookup_type**（:1303）：按标志线性扫描找区域（RS 预分配，§1.8）。
- **map_printmap**（:98）/ **printregionstats**（:1510）：调试打印——遍历树打印区域/槽位/引用；printregionstats 统计 used/weighted（跳过 VR_DIRECT）。Rust 无直接对应（cfg 诊断替代）。
- **map_sanitycheck**（:168）：SANITYCHECKS 门控的全进程一致性检查（指针 slabsane、seencount 计数与 refcount 比对、`map_sanitycheck_pt` 逐页验证页表）。**Rust 以 `verify_refcounts`（os/servers/vm/src/sanity.rs:54）替代**（11 已述），属 A-7 cfg 设计差异。

### 2.11 本章小结

- 框架函数族围绕**两个数据结构不变量**组织：AVL 树区间不重叠（insert/remove/lookup 保证）、physblocks 槽位与 phys_region.offset 一致（physblock_get/set 断言保证）。
- **错误传播**：框架函数返回 errno（ENOMEM/EINVAL/EFAULT），SUSPEND 是唯一的"伪返回码"（挂起等 VFS reply）。
- 释放/复制/收缩都有**回滚路径**（map_free 链），保证中途失败不泄漏。

---

## 3. Rust 设计决策

### 3.1 D1: BTreeMap 替代 AVL（ARCH A-4）

`RegionMap { regions: BTreeMap<VirBytes, VirRegion> }`（region_map.rs:39）替代 regionavl.c 的自定义 AVL：

- **为什么**：BTreeMap 提供同样的 O(log n) 有序语义 + 标准库质量（平衡、内存安全），无需在 VirRegion 内嵌 lower/higher/factor 树字段。
- **语义承接**：`find`（:58）= map_lookup（range(..=addr).next_back() + contains_addr）；`find_slot`（:157）= region_find_slot_range（minv/maxv/length 找空槽）；`insert`（:219）返回 `Result`——重叠时返回 Err(region)（C 的 AVL insert 是断言 + 调用方保证）。
- **ARCH 标注**：三处一致（本文 §3.1、14-region-lookup §3、region_map.rs:1-5 模块注释）。

### 3.2 D2: Vec\<PageSlot\> 替代指针数组

C 的 `struct phys_region **physblocks`（NULL=未映射）→ `Vec<PageSlot>`：

- `PageSlot::EMPTY`（page_state.rs:95-99，pfn=PFN_NONE 哨兵）替代 NULL，**省 Option 判别开销**（vir_region.rs:1-9 注释）。
- `get_slot`（vir_region.rs:220）过滤 EMPTY = C physblock_get 返回 NULL；`get_slot_mut`（:225）提供可变访问。
- `map_page`（:163）：写槽 + `PageFrames[pfn].refcount++`（saturating_add）——合并了 C 的 physblock_set + pb_reference 记账。
- `unmap_page`（:189）：清槽 + refcount--；**归零且非 IN_CACHE 时返回 `(pfn, memtype)`** 供调用方 ev_unreference + free——C 的 pb_unreferenced 职责（rm=1 的 ev_unreference）被拆成"框架摘槽 + 返回待办"，由 free_region_pages 统一执行。

### 3.3 D3: 引用计数集中到 PageFrames

C 的 `phys_block.firstregion` 反向链表（region.h:28）→ `PageFrames[PFN].refcount`（page_state.rs:139，11 已述）：

- 13 只负责**读写 refcount 的框架点**：map_page（+1）、unmap_page（-1）、needs_cow（:230，refcount>1）。
- **收益**：消除链表遍历（sanity 的 n_others 计数）、消除 phys_region 独立堆对象、refcount 从 u8 升 u32（11 §3 已述）。

### 3.4 D4: 框架操作分散到消费模块

C 的框架函数全部集中在 region.c；Rust 按**调用面**分散（避免跨模块耦合）：

| C | Rust 落点 | 消费方 |
|----|----------|--------|
| map_pf / map_handle_memory | `handle_memory_once`（fork.rs:33）+ cow_exec_pf.rs | 16 页错误 / 18 fork / 22 exit |
| map_pin_memory | `map_pin_memory`（mod.rs:128） | 25 RS（rs.rs:187/:203） |
| map_free / map_subfree | `free_region_pages`（mod.rs:23） | 21 munmap / 22 exit |
| map_copy_region | `fork_region`（fork.rs:92） | 18 fork |
| map_proc_copy(_range) | `fork_regions`（fork.rs:142） | 18 fork |
| map_unmap_region / split_region | munmap.rs split + free_region_pages（munmap.rs:155-192） | 21 munmap |
| map_region_extend_upto_v | `VirRegion::extend`（vir_region.rs:127） | 19 brk（brk.rs:106/:109） |
| map_writept / map_ph_writept | `prepare_cow`（vir_region.rs:256）+ 页表同步路径 | 16/17 |
| map_lookup | `RegionMap::find`（region_map.rs:58） | 各处 |
| map_region_lookup_type | 无实现（rs.rs:242 注释） | 25 承接 |

### 3.5 D5: 错误显式化

- **VmError**（vir_region.rs:353）：`InvalidParam` 等变体替代 C 的 printf + errno 返回——extend/split 的参数校验返回类型化错误。
- **PinMemoryError::PageNotMapped**（mod.rs:155）：替代 C 的 `panic("map_pin_memory: ...")`（region.c:790）——RS 调用方可恢复处理而非崩溃。
- **find_slot 返回 Option**：替代 C 的 `SLOT_FAIL ((vir_bytes)-1)` 哨兵（region.c:298）。

### 3.6 语义差异清单（C ↔ Rust 诚实标注）

| 维度 | Minix3 | minix-rs | 判定 |
|------|--------|----------|------|
| 区域容器 | AVL（regionavl.c） | BTreeMap（ARCH A-4） | ✅ 语义等价 |
| 槽位 | phys_region* 指针数组 | Vec\<PageSlot\>（PFN 哨兵） | ✅ 语义等价 |
| 物理页状态 | phys_block + firstregion 链表 | PageFrames[PFN] refcount | ✅ 语义等价（11） |
| 记账 | vm_total/vm_total_max（physblock_set 内联） | ActiveProc sub_total（消费面） | ✅ 等价，位置不同 |
| 框架错误 | printf + errno / SLOT_FAIL | VmError / Option | ✅ 显式化 |
| 调试打印 | map_printmap/printregionstats | 无直接对应（cfg 诊断替代） | ⚠️ 功能缺失，P2 |
| map_region_lookup_type | rs.c:177/:334 消费 | 无实现（rs.rs:242） | ⚠️ DEFERRED（25） |
| map_lazy | 无对应（C 无 lazy 槽概念） | vir_region.rs:213 孤儿 API + test_map_lazy 失败 | ⚠️ 见 §5.3 |
| ev_copy 时机 | 先复制结构再 ev_copy | fork_region 同序 | ✅ |
| limbo 语义 | map_copy_region 先复制后挂链 | fork_regions 批量复制后统一 insert | ✅ 等价 |

---

## 4. 实现详解

### 4.1 RegionMap（region_map.rs）

- **find**（:58）：`range(..=addr).next_back()` + `contains_addr` 过滤 = C map_lookup（§2.4）。
- **find_mut**（:67）：同语义 + BTreeMap 双重查找（先定位 key 再 get_mut）——避免持有 range 迭代器的同时可变借用。
- **search**（:77）+ **SearchType**（:19）：Equal/Less/Greater/LessEqual/GreaterEqual 五种定向查找 = C AVL 的 AVL_EQUAL/LESS/GREATER 枚举（14 详述）。
- **find_by_end / find_mut_by_end**（:100/:109）：按 `end_addr` 找区域（brk 收缩场景）。
- **find_overlap**（:129）/ **find_all_overlaps**（:145）：重叠检测（insert 前置检查 + munmap 范围遍历）。
- **find_slot**（:157）：C region_find_slot_range 的 Rust 版，**增强页对齐**（try_gap 闭包内 round_up/round_down，`c09` 回归测试覆盖非对齐边界）。
- **insert**（:219）：`find_overlap` 预检 → `BTreeMap::insert`（同 vaddr 替换返回 Some(old)）；重叠 → Err(region)。
- **remove**（:229）/ **iter**（:246）/ **clear**（:254）：基础集合操作。

### 4.2 VirRegion（vir_region.rs）

- **new**（:89）：`vec![PageSlot::EMPTY; pages]` 预分配槽位数组（= C region_new 的 calloc）。
- **extend**（:120）：追加 EMPTY 槽 + length 增长（= C map_region_extend_upto_v 的 realloc 分支；brk.rs:106/:109 消费）。
- **map_page**（:163）/ **unmap_page**（:189）：槽位挂载/摘除 + refcount 维护（§3.2）。
- **map_lazy**（:213）：写 PFN_NONE 槽（**孤儿 API，无生产调用方**，见 §5.3）。
- **needs_cow**（:230）：refcount>1 判定（17 消费）。
- **prepare_cow**（:256）：把 refcount>1 的页标 COW 标志（fork 后写保护，= C map_copy_region 后 pt_writemap(~PT_W)）。
- **split**（:272）：`split_len` 切成左右两半（= C split_region）：File 类型 param 特殊处理（left offset 不变 / right offset+split_len，fdref ref 两次）；其余类型克隆 param。
- **free_range**（:336）：区间摘槽 + 收集 pending（= C map_subfree 的 Rust 版）。

### 4.3 顶层函数（mod.rs）

- **free_region_pages**（:23）：完整释放链——`pt.unmap` 循环（`#[cfg(not(test))]`）→ `ev_delete` → `free_range` 收集 pending → 逐个 `ev_unreference` + `page_alloc.free_pfn` → fdref deref（`PendingFdClose` 本地持有，VFS 发送 DEFERRED，注释 :84-108）。
- **map_pin_memory**（:128）：**两阶段**——先收集全部 `(vaddr, length)` 对，再逐个 `handle_memory_once(wrflag=true)`；避免迭代 RegionMap 的同时可变借用。失败返回 `PinMemoryError::PageNotMapped`（C panic 的恢复化）。

### 4.4 消费链

- **fork**（fork.rs:33/:92/:142）：`handle_memory_once`（批量填充 + CoW 解析）→ `fork_region`（复制 + 共享页）→ `fork_regions`（全量复制 + 失败回滚 `free_forked_regions` :162）。
- **munmap**（munmap.rs:155-192）：`split` 切头/中/尾 → `free_region_pages` 释放中间段 → `sub_total` 记账 → 剩余段 re-insert。
- **brk**（brk.rs:106/:109）：`extend` 增长堆顶区域（19 详述）。
- **RS**（rs.rs:187/:203）：`map_pin_memory` 固定源/目标进程内存（25 详述）。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

**region_map.rs**（16 个）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_insert_and_find | :274 | 插入乱序 + find 命中/未命中 |
| test_remove | :291 | 摘除后 find None |
| test_find_overlap | :308 | 重叠检测 |
| test_traverse | :322 | 有序遍历（vaddr 升序） |
| test_iter | :339 | 迭代器升序 |
| test_search_type_less / greater | :355/:371 | 严格小于/大于 |
| test_search_type_less_equal / greater_equal | :387/:403 | 含等于 |
| test_find_slot_basic | :419 | 空槽命中 |
| test_find_slot_in_gap | :432 | 间隙内找槽 |
| test_find_slot_no_space | :446 | 无空槽 None |
| test_find_all_overlaps | :457 | 全量重叠迭代 |
| test_search_type_enum | :476 | 枚举互斥 + Default |
| test_find_slot_alignment_c09 | :493 | 页对齐回归 |
| test_find_slot_subpage_gap_c09 | :521 | 亚页间隙 None |

**vir_region.rs**（11 个）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_vir_region_creation | :378 | 建区域 + 槽位数组 |
| test_vir_region_contains | :386 | contains_addr 边界 |
| test_vir_region_flags | :396 | VrFlags 位操作 |
| test_map_unmap_page | :411 | 挂载/摘除 + refcount 1→0 |
| test_needs_cow | :428 | refcount>1 → COW 判定 |
| test_vir_region_split / split_invalid | :443/:455 | 分割 + 参数校验 |
| test_free_range | :466 | 区间摘槽 + pending 收集 |
| test_map_lazy | :491 | lazy 槽语义（**FAIL**，见 §5.3） |
| test_extend / extend_invalid | :503/:516 | 扩展 + 校验 |

**mod.rs**（2 个）：test_map_pin_memory_empty :176、test_map_pin_memory_non_cow_region :188。

### 5.2 覆盖维度

- **集合语义**：BTreeMap 基础操作（insert/remove/iter）+ 重叠检测 + 遍历有序性。
- **查找语义**：SearchType 五向 + find_slot 空槽 + 页对齐回归（c09 两测试）。
- **映射生命周期**：map_page/unmap_page/needs_cow/free_range/extend/split。
- **顶层函数**：map_pin_memory 空/非 COW 两态。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 | 说明 |
|------|------|------|
| **test_map_lazy FAIL**（vir_region.rs:497） | ⚠️ 13 范围遗留 | `map_lazy`（:213）写 PFN_NONE 槽，但 `get_slot`（:220）用 `is_mapped()` 过滤 → `unwrap()` panic。**语义待定**：lazy 槽应不可见（get_slot 保持过滤，测试改直接访问 physblocks）还是可见（get_slot 放开过滤，调用方自查）？当前无生产调用方（孤儿 API），修复归 13 后续轮次。pre-existing（360 passed / 1 failed）。 |
| find_slot 生产消费 | ⚠️ backlog | `RegionMap::find_slot` 被 mmap.rs:219/:221/:225 与 dispatcher.rs:504/:1397 调用，但无端到端测试（21/20 承接） |
| map_page_region 等价组合 | ⚠️ backlog | find_slot + VirRegion::new + memtype ev_new + insert 的组合路径无集成测试 |
| map_region_lookup_type | ⚠️ DEFERRED | 无 Rust 实现（rs.rs:242 注释），25-rs-services 承接 |
| map_printmap / printregionstats | ⚠️ 接受 | 调试打印无 Rust 对应，cfg 诊断替代 |
| map_writept / map_ph_writept | ⚠️ 16/17 承接 | 页表同步路径（prepare_cow + write_page_table_mappings）在页错误/CoW 文档验证 |

### 5.4 测试统计（截至 2026-08-16）

```
$ cd os && cargo test -p minix-vm --lib
→ 360 passed / 1 failed（test_map_lazy pre-existing，13 范围，§5.3）
$ cargo test -p minix-vm --lib region_map   → 16 passed
$ cargo test -p minix-vm --lib vir_region   → 10 passed / 1 failed（test_map_lazy）
$ cargo test -p minix-vm --lib map_pin_memory → 2 passed
```

---

## 6. 过渡

13 完成阶段 5 地址空间数据结构的框架面（11 状态 → 12 策略 → **13 框架**）。接下来：

- **14-region-lookup**：把 §2.3 的 `region_find_slot*` 与 RegionMap 的 SearchType/find_* 全族展开（AVL → BTreeMap ARCH A-4 详述）。
- **16/17**：消费 map_pf 框架（handle_memory_once 状态机 + CoW 分裂）。
- **18-vm-fork**：消费 map_proc_copy/map_copy_region（fork_region 全流程）。
- **19/20/21**：消费 extend（brk）/ find_slot（mmap）/ split+free（munmap）。
- **25/26**：消费 map_pin_memory + map_region_lookup_type（RS）/ map_lookup（查询）。

位置可回答性：本文档的函数全部位于 **VM 启动链 `init_vm()` 的 `map_region_init()` 锚点之后**（region.c:36 空钩子，结构在运行时由各服务建立），以及 **主循环分发后的服务路径**（18/19/20/21/25）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/11-phys-pagestate.md` — phys_block/phys_region 生命周期 + PageFrames（本文件前置）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/12-memtype.md` — memtype 策略层（框架调用点引用）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/14-region-lookup.md` — 区域查找语义（BTreeMap/AVL）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/16-pagefault.md` — 页错误状态机（消费 map_pf）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/17-cow-mechanism.md` — CoW 分裂（消费 refcount/needs_cow）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/18-vm-fork.md` — fork 全流程（消费 map_proc_copy）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/21-vm-munmap.md` — munmap（消费 split + free）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/25-rs-services.md` — RS Live Update（消费 map_pin_memory/lookup_type）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/26-vm-queries.md` — 查询（消费 map_lookup）
- `minix3/minix/servers/vm/region.c`、`minix3/minix/servers/vm/region.h`、`minix3/minix/servers/vm/phys_region.h` — C ground truth
- `os/servers/vm/src/region/region_map.rs`、`os/servers/vm/src/region/vir_region.rs`、`os/servers/vm/src/region/mod.rs` — Rust 实现
