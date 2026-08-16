# 17-cow-mechanism: 写时复制——引用计数与页表权限的协作协议

> **分类**: 阶段 6 — 页错误与运行时机制（CoW 机制）
> **源码**: `minix3/minix/servers/vm/pb.c`（168 行：`pb_new` :32-52 / `pb_free` :54-59 / `pb_link` :61-71 / `pb_reference` :73-91 / `pb_unreferenced` :96-134 / `mem_cow` :136-168）+ `minix3/minix/servers/vm/mem_anon.c`（`mem_type_anon` :33-46 / `anon_unreference` :56-62 / `anon_pagefault` :64-97 / `anon_writable` :105-113）+ `minix3/minix/servers/vm/region.c`（`pr_writable` :130-134 / `map_ph_writept` :257-295 / `map_writept` :906 / `map_copy_region` :820-849）+ `minix3/minix/servers/vm/mem_file.c`（`mappedfile_writable` :173-177 / `cow_block` :59-77）+ `minix3/minix/servers/vm/mem_shared.c`（`shared_pagefault` :122 / `shared_writable` :161）
> **Rust 模块**: `os/servers/vm/src/fork.rs`（`fork_region` :92-140）+ `os/servers/vm/src/region/vir_region.rs`（`prepare_cow` :256-270 / `needs_cow` :230-239 / `map_page` :163-176 / `unmap_page` :189-211 / `is_writable` :143-145）+ `os/servers/vm/src/region/page_state.rs`（`PageFlags::COW` :35 / `PageState` :41-48）+ `os/servers/vm/src/vmproc/vmproc_handle.rs`（`setup_cow_for_all_regions` :488-495 / `write_page_table_mappings` :505-542）+ `os/servers/vm/src/memtype.rs`（writable 回调族）+ `os/servers/vm/src/cow_exec_pf.rs`（`cow_resolve_core` :85-128）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/11-phys-pagestate.md`（物理页引用计数）+ `12-memtype.md`（`writable`/`ev_copy`/`ev_reference` 回调语义）+ `16-pagefault.md`（页错误状态机如何消费 CoW）
> **说明**: 本文档管 **CoW 机制本身**——共享如何建立（refcount++）、写保护如何设置（PTE 只读）、首次写入如何分裂（`mem_cow`）、分裂后所有权如何转移。**不覆盖**：页错误状态机（16）、fork 全流程（18，含 `map_proc_copy`/`do_fork`）、file-backed `cow_block` 的 VFS 交互（23）。

---

## 1. 概念：引用计数决定语义，PTE 决定谁触发

### 1.0 章节引言

16 讲了页错误"怎么被处理"（状态机与 SUSPEND 协议），其中 CoW 只是一个动作分支（`NeedCow → cow_resolve`）。本文档回答三个问题：

1. **共享怎么建立**——fork 时父子进程如何共享物理页而不复制（§1.2、§2.1）。
2. **写保护怎么设置**——为什么共享页的页表项是只读的，谁在什么时候写入（§1.3、§2.2）。
3. **分裂怎么做**——首次写入时如何分配新页、复制内容、转移所有权（§1.4、§2.3）。

它在整个 02-stage-vm 中的位置：

```
11（引用计数）→ 12（memtype 回调）→ 13（区域映射）
→ ★17（CoW 机制：共享→保护→分裂）
→ 16（页错误状态机消费 cow_resolve）→ 18（fork 建立共享）→ 23（file-backed cow_block）
```

### 1.1 CoW 动机：fork 的延迟复制

传统 fork 把父进程地址空间整体复制到子进程：耗时 O(地址空间大小)、内存翻倍。CoW 把它变成 O(1)：

1. fork 时**不复制物理页**，父子进程共享同一物理页，只把引用计数 +1。
2. 共享页的 PTE 被写成**只读**——无论父还是子，写操作都触发 #PF。
3. 首次写入触发页错误 → VM 分配新页、复制内容、把触发者切换到私有页。
4. 典型负载（fork → exec）下大部分页面从不被写入，复制完全避免。

### 1.2 生命周期四阶段

| 阶段 | 操作 | C 位置 | Rust 位置 |
|------|------|--------|----------|
| 1. 建立共享 | `pb_reference` refcount++；子区域 PTE 待写 | region.c:836-837（map_copy_region 内） | fork.rs:114-133（fork_region 内） |
| 2. 写保护 | `map_writept` → `map_ph_writept` → `pr_writable` 判只读 | region.c:995-996 / :271-274 / :130-134 | vir_region.rs:256-270 + vmproc_handle.rs:505-542 |
| 3. 触发判定 | `anon_pagefault`：`write && refcount > 1` → `mem_cow` | mem_anon.c:89-96 | memtype.rs:247-255（NeedCow） |
| 4. 分裂 | `mem_cow`：分配 + 复制 + 换块 + 切 anon | pb.c:136-168 | cow_exec_pf.rs:85-128（cow_resolve_core） |

### 1.3 引用计数语义

| refcount | 状态 | 写行为 |
|----------|------|--------|
| 0 | 物理页未被引用（allocator 空闲或刚分配未挂接） | 不应有映射 |
| 1 | 私有页，唯一所有者 | PTE 可写，直接写 |
| ≥2 | 共享页（fork 后） | PTE 只读，写触发 CoW 分裂 |

**核心不变量**：`refcount == 1` 是"可写"的充分必要条件（匿名内存）。分裂后旧页 refcount 减一、新页 refcount = 1——**每次分裂恰好把一个共享页变成两个私有页**。

### 1.4 分裂后 memtype 切换

`mem_cow` 的最后一步是 `ph->memtype = &mem_type_anon`（pb.c:165）——无论原来是匿名还是文件映射，私有副本一律变成匿名内存。理由：**私有副本与文件/源不再共享**，修改不能写回原文件，后续页错误交给 `anon_pagefault` 处理。

### 1.5 对照：Redox 与 Linux

- **Linux**：`mm/memory.c` 的 `do_wp_page()`——写保护页错误时检查 `page_mapcount(page)`：>1 则 `wp_page_copy`（分配新页 + `copy_user_highpage` + 更新 PTE），==1 则 `wp_page_reuse`（只改 PTE 权限，不拷贝）。对应 minix-rs 的 `cow_resolve_core` 快速路径（`refcount <= 1` 直接返回，cow_exec_pf.rs:103-105）。Linux 的 refcount 语义更复杂（`_mapcount` 只计页表映射数，另有 `_refcount` 计内核引用），Minix3/Rust 的 `refcount` 只计 phys_region 映射数。
- **Redox**：内核态 `page_fault_handler` 直接处理，CoW 通过 `AddressSpace` 的页表操作实现，无用户态服务器参与。Minix3 把"何时分裂"的策略留给用户态 VM，内核只负责转发异常与解除阻塞。
- **对照要点**：三家都基于"页表只读 + 引用计数 > 1 → 写时复制"的 x86 页保护机制；差异在处理者位置与 refcount 粒度。

### 1.6 小结

CoW = 引用计数（语义）+ 页表权限（触发）+ 分裂动作（所有权转移）。理解四阶段后，C 源码的 pb.c 原语、region.c 写保护、mem_anon.c 判定可以对照阅读。

---

## 2. C 源码分析

### 2.1 建立共享：pb_reference / pb_link（pb.c）

`pb_link`（pb.c:61-71）是引用计数原语：

```c
void pb_link(struct phys_region *newphysr, struct phys_block *newpb,
	vir_bytes offset, struct vir_region *parent)
{
USE(newphysr,
	newphysr->offset = offset;
	newphysr->ph = newpb;
	newphysr->parent = parent;
	newphysr->next_ph_list = newpb->firstregion;
	newpb->firstregion = newphysr;);
	newpb->refcount++;
}
```

- 侵入式链表：`phys_block.firstregion` 头插 `phys_region`（:68-69），`refcount++`（:70）。
- `pb_reference`（pb.c:73-91）＝ `SLABALLOC` 新 `phys_region` + `pb_link` + `physblock_set`（:88）——fork 复制区域时对每个已映射 slot 调用它。

**fork 复制上下文**（region.c:820-849，`map_copy_region` 内）：

```c
for(p = 0; p < phys_slot(vr->length); p++) {
	if(!(ph = physblock_get(vr, p*VM_PAGE_SIZE))) continue;
	newph = pb_reference(ph->ph, ph->offset, newvr, vr->def_memtype);  /* :836-837 refcount++ */
	if(!newph) { map_free(newvr); return NULL; }
	if(ph->memtype->ev_reference)                                     /* :841-842 */
		ph->memtype->ev_reference(ph, newph);                         /* ← 返回值被忽略 */
}
```

> **C 缺陷 1（真实）**：`ev_reference` 的返回值在 region.c:841-842 被丢弃。`anon_contig_reference` 返回 ENOMEM（连续匿名内存不允许共享）时，fork 仍然继续——子进程照样获得共享区域，且失败信息丢失。Rust `fork_region` 检查该返回值并回滚已增引用（§3.1）。

### 2.2 写保护：pr_writable / map_ph_writept（region.c）

共享建立后，**所有区域重写页表**（map_proc_copy_range 尾部 `map_writept(src); map_writept(dst);`，region.c:995-996），共享页被写成只读。

**判定门** `pr_writable`（region.c:130-134）：

```c
static int pr_writable(struct vir_region *vr, struct phys_region *pr)
{
	assert(pr->memtype->writable);
	return ((vr->flags & VR_WRITABLE) && pr->memtype->writable(pr));
}
```

**写入者** `map_ph_writept`（region.c:257-295）：

```c
int flags = PTF_PRESENT | PTF_USER;
if(pr_writable(vr, pr)) flags |= PTF_WRITE;
else flags |= PTF_READ;                                  /* :271-274 只读 */
if(vr->def_memtype->pt_flags) flags |= vr->def_memtype->pt_flags(vr);  /* :277-278 */
if(pt_writemap(vmp, &vmp->vm_pt, vr->vaddr + pr->offset,  /* :280-285 */
		pb->phys, VM_PAGE_SIZE, flags, ...) != OK) ...
```

**各 memtype 的 writable 实现**：

| memtype | 实现 | 位置 | 语义 |
|---------|------|------|------|
| anon | `phys != MAP_NONE && (remaps > 0 \|\| refcount == 1)` | mem_anon.c:105-113 | 共享（refcount>1）只读 |
| mappedfile | 恒 0 | mem_file.c:173-177 | 永不可写，写必触发 CoW |
| shared | `phys != MAP_NONE` | mem_shared.c:161 | 只要映射即可写（不 CoW） |

### 2.3 分裂执行：mem_cow（pb.c:136-168）

```c
int mem_cow(struct vir_region *region,
        struct phys_region *ph, phys_bytes new_page_cl, phys_bytes new_page)
{
	if(new_page == MAP_NONE) {          /* :141-149 未预分配则自分配 */
		allocflags = vrallocflags(region->flags);
		if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) return ENOMEM;
		new_page = CLICK2ABS(new_page_cl);
	}
	assert(ph->ph->phys != MAP_NONE);
	if(sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE) != OK) {  /* :153 复制 */
		panic("VM: abscopy failed\n"); return EFAULT;
	}
	if(!(pb = pb_new(new_page))) {      /* :158 新物理块 */
		free_mem(new_page_cl, 1); return ENOMEM;
	}
	pb_unreferenced(region, ph, 0);     /* :163 解除旧引用（rm=0 保留槽位） */
	pb_link(ph, pb, ph->offset, region);/* :164 链接新块 */
	ph->memtype = &mem_type_anon;       /* :165 切换为匿名 */
	return OK;
}
```

**五步语义**：分配（或复用调用者预分配）→ `sys_abscopy` 复制 4KB → `pb_new` 建新块 → `pb_unreferenced(,0)` 旧块 refcount--（rm=0：phys_region 槽位保留，稍后 pb_link 复用）→ `pb_link` 新块 refcount=1 → memtype 切 anon。

**refcount 转移**：旧块 2→1（另一进程仍持有），新块 0→1——每次分裂把共享页拆成两个私有页。

### 2.4 触发判定：anon_pagefault（mem_anon.c:64-97）

```c
static int anon_pagefault(struct vmproc *vmp, struct vir_region *region,
	struct phys_region *ph, int write, vfs_callback_t cb, void *state,
	int len, int *io)
{
	allocflags = vrallocflags(region->flags);
	assert(ph->ph->refcount > 0);

	if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM) {   /* :75 预分配 */
		printf("anon_pagefault: out of memory\n"); return ENOMEM;
	}
	new_page = CLICK2ABS(new_page_cl);

	if(ph->ph->phys == MAP_NONE) {       /* :82-87 首次访问：直接设置物理页 */
		ph->ph->phys = new_page;
		return OK;
	}

	if(ph->ph->refcount < 2 || !write) { /* :89-92 无需 CoW */
		return OK;                       /* ← new_page 泄漏 */
	}

	assert(region->flags & VR_WRITABLE);
	return mem_cow(region, ph, new_page_cl, new_page);  /* :96 */
}
```

> **C 缺陷 2（真实）**：函数入口无条件 `alloc_mem` 预分配（:75），但 `refcount < 2 || !write` 的 early-return（:89-92）**不释放 `new_page_cl`**——该物理页从分配器永久丢失。`mem_anon.c` 中 `free_mem` 仅在 `anon_unreference`（:60）出现，此路径无释放。Rust 设计**先判定后分配**（`NeedCow` 时才 `alloc_pfn`），从结构上消除此泄漏（§3.3）。

**触发条件**：`write && refcount > 1`——读操作不触发（多个进程可安全共享只读页）；refcount == 1 是私有页，`map_pf` 的 `writable` 短路（region.c:713）直接可写。

### 2.5 引用计数维护：pb_unreferenced（pb.c:96-134）

```c
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
	pb = pr->ph;
	USE(pb, pb->refcount--;);                    /* :102 */
	/* 从 firstregion 侵入式链表移除 pr（:105-120） */
	if(pb->refcount == 0) {                      /* :122-129 */
		assert(!pb->firstregion);
		if((r = pr->memtype->ev_unreference(pr)) != OK) panic(...);
		SLABFREE(pb);
	}
	pr->ph = NULL;                                /* :131 */
	if(rm) physblock_set(region, pr->offset, NULL);  /* :133 */
}
```

**rm 参数语义**：`rm=0`（CoW 用，mem_cow:163）只解除引用、保留槽位——phys_region 即将 `pb_link` 到新块；`rm=1`（munmap/exit 用）同时清槽。refcount 归零时 `ev_unreference` 释放物理页（anon 的 `free_mem`，mem_anon.c:56-62）。

### 2.6 C 小结：符号全景

| 符号 | 位置 | 角色 |
|------|------|------|
| `pb_link` | pb.c:61-71 | refcount++ 原语（侵入式链表） |
| `pb_reference` | pb.c:73-91 | fork 复制时建立共享 |
| `pb_unreferenced` | pb.c:96-134 | refcount-- + 归零释放（rm 控制槽位） |
| `mem_cow` | pb.c:136-168 | 分裂核心：分配/复制/换块/切 anon |
| `anon_pagefault` | mem_anon.c:64-97 | 触发判定（含缺陷 2 泄漏） |
| `anon_writable` | mem_anon.c:105-113 | 匿名可写判定 |
| `pr_writable` | region.c:130-134 | 写保护判定门 |
| `map_ph_writept` | region.c:257-295 | PTE 写入（只读/可写） |
| `map_writept` | region.c:906 | 全区域重写页表 |
| `map_copy_region` | region.c:820-849 | fork 区域复制（含缺陷 1） |
| `mappedfile_writable` | mem_file.c:173-177 | 文件映射恒不可写 |
| `shared_writable` | mem_shared.c:161 | 共享内存可写判定 |

---

## 3. Rust 设计决策

> 行号以 2026-08-16 实证为准。Rust 侧现状：**四阶段全部落地**（fork_region 建立 → prepare_cow/write_page_table_mappings 保护 → ev_pagefault 判定 → cow_resolve_core 分裂），并修复 C 两处真实缺陷；file-backed `cow_block`（23 范围）不在本文档。

### 3.1 D1：fork_region——共享建立 + rollback（fork.rs:92-140）

C 的 `map_copy_region`（region.c:820-849）建立共享但忽略 `ev_reference` 失败。Rust `fork_region`：

1. **元数据继承**（:96-101）：`parent_slot`/`def_memtype`/`remaps`/`id`/`param` + `ev_copy`（:103-105，C region.c:826-830）。
2. **逐 slot refcount++**（:114-133）：`frames.get_mut(slot.pfn).refcount += 1` 并记录到 `refcounted_pfns`；`ev_reference`（:121）失败 → **逐 pfn 递减回滚**（:122-131）→ `VmForkError`。
3. **子区域只读**（:137）：`dst.set_writable(false)`——对应 C fork 后 PTE 只读的起点（后续由 write_page_table_mappings 具体写 PTE）。

### 3.2 D2：写保护——prepare_cow + write_page_table_mappings

```rust
pub(crate) fn prepare_cow(&mut self, frames: &mut PageFrames) {   // vir_region.rs:256-270
    for slot in self.physblocks.iter() {
        if slot.is_mapped() {
            if let Some(state) = frames.get_mut(slot.pfn) {
                if state.refcount > 1 {
                    state.flags.insert(PageFlags::COW);           // page_state.rs:35
                }
            }
        }
    }
}
```

- `setup_cow_for_all_regions`（vmproc_handle.rs:488-495）：所有区域 `WRITABLE` 复位 + `prepare_cow`。注释明确：**不递增 refcount**——那是 `fork_region` 已做的。
- `write_page_table_mappings`（vmproc_handle.rs:505-542）：实际 PTE 权限判定（:522-525）：
  `writable = region.is_writable() && frames.get(pfn).refcount == 1` → `read_write()`/`read_only()`。
- **一致性待核对**：`PageFlags::COW` 目前只写不读（判定用 `refcount == 1`），两处判据在 08/18 页表接线时需确认等价（见 §5.3）。

### 3.3 D3：cow_resolve_core——分裂 + 快速路径（cow_exec_pf.rs:85-128）

| C mem_cow 步骤 | Rust 对应 | 行号 |
|---------------|----------|------|
| `alloc_mem`（或复用预分配） | `alloc.alloc_pfn()` | :107-108 |
| `sys_abscopy` | `copy_page_content`（direct map + copy_nonoverlapping） | :110 / :190-206 |
| `pb_new` | 不需要（PageFrames 全局数组） | — |
| `pb_unreferenced(,0)` | `unmap_page(frames, offset)` | :112 |
| `pb_link` + memtype=anon | `map_page(offset, new_pfn, &MEM_TYPE_ANON)` | :113 |
| refcount==0 → ev_unreference + 释放 | `pending → ev_unreference + free_pfn` | :119-122 |
| — | **refcount<=1 快速路径直接返回 old_pfn**（C 无；Linux wp_page_reuse 同思路） | :103-105 |
| — | `verify_cow_consistency`（debug 断言） | :124-125 / :137-172 |

**消除 C 缺陷 2**：Rust 的 `AnonymousMemory::ev_pagefault`（memtype.rs:220-256）**只判定不分配**——`NeedCow` 才由 `handle_pagefault` 调 `cow_resolve`（cow_exec_pf.rs:41-43）触发 `alloc_pfn`。非 CoW 路径（Handled/NeedNewPage 处理）不产生多余分配，C 的预分配泄漏从结构上不存在。

### 3.4 D4：memtype writable 回调族（memtype.rs）

| Rust | 位置 | 对应 C | 语义 |
|------|------|--------|------|
| 默认 `false` | :95-97 | 无（C 断言必有） | trait 默认不可写 |
| `AnonymousMemory::writable` | :192-202 | anon_writable（mem_anon.c:105-113） | `!slot.is_mapped() → false`；`remaps > 0 → true`；否则 `refcount == 1` |
| `MappedFile::writable` | :892-894 | mappedfile_writable（mem_file.c:173-177） | 恒 false |
| `SharedMemory::writable` | :411-413 | shared_writable（mem_shared.c:161） | `slot.is_mapped()` |

### 3.5 D5：两阶段释放（unmap_page :189-211）

C 的 `pb_unreferenced` 在 refcount==0 时**同步**做 `ev_unreference` + `SLABFREE`（pb.c:122-129）。Rust 拆成两步：

- `unmap_page` 递减 refcount，**当 refcount==0 且 `!IN_CACHE`** 时返回 `Some((pfn, memtype))`（:196-207）；
- 调用方（`cow_resolve_core` :119-122、`free_forked_regions` fork.rs:162-177）再 `ev_unreference` + `free_pfn`。

`IN_CACHE` 保护（vir_region.rs:201-203）：缓存页即使 refcount==0 也不归还 allocator（24-page-cache 语义）。

### 3.6 差异清单（C ↔ Rust，诚实标注）

| # | C 语义 | Rust 现状 | 状态 |
|---|--------|----------|------|
| 1 | `ev_reference` 返回值被忽略（region.c:841-842） | `fork_region` 检查 + `refcounted_pfns` rollback（fork.rs:121-131） | ✅ 修复 |
| 2 | anon_pagefault 预分配泄漏（mem_anon.c:75-92） | 先判定后分配（NeedCow 才 alloc_pfn） | ✅ 修复 |
| 3 | 侵入式链表（firstregion/next_ph_list） | PageFrames 全局 refcount 数组 | ✅ 11 已述 |
| 4 | `pr_writable` = VR_WRITABLE && writable | `write_page_table_mappings` 判 `is_writable() && refcount==1` | ✅ 等价 |
| 5 | `PageFlags::COW` 标记 | 只写不读（判据用 refcount==1） | ⚠️ 08/18 接线核对 |
| 6 | mem_cow 的 pb_new 失败路径（pb.c:158-161） | `alloc_pfn` → `CowError::NoMemory`，无中间对象需清理 | ✅ 简化 |
| 7 | file-backed `cow_block`（mem_file.c:59-77） | MappedFile CoW 经 `NeedCow` → `cow_resolve_core`；clearend 处理未接线 | ⚠️ 23 范围 |
| 8 | SharedMemory::ev_pagefault 源进程链接（mem_shared.c:122+） | 已实现（memtype.rs:434-530，getsrc 等价 + 递归分配） | ✅ 已实现（draft 声称 stub 已过时） |
| 9 | `anon_pt_flags`（mem_anon.c:48-54） | Rust 页表 flags 由 paging 层管理 | ⚠️ 简化 |

---

## 4. 实现详解

### 4.1 fork_region：共享建立（fork.rs:92-140）

```
VirRegion::new(src.vaddr, src.length, src.flags)      :96
  元数据继承（parent_slot/def_memtype/remaps/id/param）:97-101
  ev_copy（MappedFile 克隆 param 等）                   :103-105
  fdref ref_entry（file-backed）                       :107-109
  for slot in src.physblocks:                          :114-133
    refcount += 1 → refcounted_pfns.push(pfn)          :116-118
    ev_reference → 失败则逐 pfn 回滚 → Err             :121-131
  dst.set_writable(false)                              :137
```

失败时 `fork_regions`（:142-157）调 `free_forked_regions`（:162-177）整体回滚——与 C `map_free_proc` 等价（18 详述）。

### 4.2 写保护：setup_cow_for_all_regions + write_page_table_mappings

```
setup_cow_for_all_regions（vmproc_handle.rs:488-495）
  └─ 每区域：flags.insert(WRITABLE) + prepare_cow（refcount>1 → PageFlags::COW）
write_page_table_mappings（vmproc_handle.rs:505-542）
  └─ 每 slot：writable = region.is_writable() && refcount==1
      → PTE read_write() / read_only()                :522-530
  └─ pt.map(vaddr, paddr, flags)                      :537-540
```

C 对照：`map_writept(src); map_writept(dst);`（region.c:995-996）——父子都要重写，因为**共享页对双方都是只读的**。Rust 的 `write_page_table_mappings` 是同一语义（父与子各自的地址空间分别调）。

### 4.3 cow_resolve_core：分裂动作（cow_exec_pf.rs:85-128）

```
slot = get_slot(offset) → PageNotMapped                 :91-96
refcount = frames.get(old_pfn).refcount                 :98-101
refcount <= 1 → return old_pfn（快速路径，无拷贝）       :103-105
new_pfn = alloc.alloc_pfn() → NoMemory                  :107-108
copy_page_content(frames, old_pfn, new_pfn)             :110
pending = unmap_page(frames, offset)                    :112
map_page(frames, offset, new_pfn, &MEM_TYPE_ANON)       :113
pending → ev_unreference + free_pfn（两阶段释放）        :119-122
debug：verify_cow_consistency                           :124-125
```

不变量（verify_cow_consistency :137-172）：old refcount ≥ 1（其他所有者仍在）、new refcount == 1、slot 指向 new_pfn。

### 4.4 memtype 决策与 writable 对比

| memtype | ev_pagefault 决策（16 已述） | writable | CoW 语义 |
|---------|------------------------------|----------|----------|
| AnonymousMemory | 未映射→NeedNewPage；refcount<2 或读→Handled；写共享→NeedCow | refcount==1 或 remaps>0 | 标准 CoW |
| MappedFile | 未映射→NeedVfsIo；映射+写→NeedCow | 恒 false | 写必 CoW（cow_block 23 范围） |
| SharedMemory | 未映射→源进程链接（§4.5） | slot.is_mapped() | **不 CoW**（真共享） |
| ContiguousAnonymous | 预分配（ev_new） | 恒 true 风格 | 不 CoW（共享不允许，ev_reference 失败） |

### 4.5 SharedMemory 源进程链接（memtype.rs:434-530）

C `shared_pagefault`（mem_shared.c:122）→ Rust 已实现等价流程：

1. 已映射 → `Handled`（:446-450，C mem_shared.c:139）；
2. 解析 `VrParam::Shared { ep, vaddr, id }`（:454-457，C getsrc）；
3. `table.vm_isokendpt` + 源区域查找 + memtype/id 校验（:466-489）；
4. 源 slot 未映射 → 为源分配页（:499-520，C `map_pf(src)` 递归语义）；
5. `region.map_page(frames, offset, src_pfn, &MEM_TYPE_SHARED)`（:527）——共享同一 PFN（C `pb_link`），refcount++。

共享内存的写不触发 CoW——所有持有者看到同一页的修改。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

**cow_exec_pf.rs**（5 个，:280-365）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_alloc_and_map | :281 | 分配 + 映射（slot 状态 + pfn） |
| test_cow_resolve | :295 | refcount=2 写 → 新 pfn、旧 1、新 1 |
| test_cow_resolve_no_sharing | :314 | refcount=1 → 原 pfn（快速路径） |
| test_cow_resolve_region | :328 | 区域批量分裂 |
| test_cow_resolve_core_refcount_one_fast_path | :350 | 快速路径不动映射 |

**memtype.rs**（CoW 相关）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_anon_writable | :1052 | refcount=1 可写 / refcount=2 不可写（writable 判定） |
| test_mapped_file_copy | :1083 | ev_copy 克隆 param |
| test_shared_pagefault_already_mapped | :1099 | 已映射 → Handled |
| test_shared_pagefault_invalid_param / zero_ep | :1136/:1170 | 参数校验 fail-closed |
| test_cache_pagefault_* | :1203-1334 | 缓存页判定 |

**page_state.rs**（:196-255）：test_page_frames_init / test_pfn_to_phys / test_phys_to_pfn / test_page_slot / test_incache / test_refcount_operations。

### 5.2 覆盖维度

- **writable 判定**：refcount 1 vs 2 翻转（test_anon_writable）。
- **分裂动作**：refcount 转移（2→1/1）、快速路径、批量区域。
- **引用计数原语**：map_page/unmap_page 的增减、IN_CACHE 保护（test_incache）。
- **共享内存**：已映射/参数校验/源链接判定。
- **不变量**：verify_cow_consistency（debug 断言 old≥1/new==1/slot 指向 new）。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 | 说明 |
|------|------|------|
| fork_region 的 rollback 直接单测 | ⚠️ 缺失 | `refcounted_pfns` 回滚路径无独立测试（fork 集成测试在 18） |
| prepare_cow → write_page_table_mappings 联合测试 | ⚠️ 缺失 | COW flag 标记 → PTE 只读的端到端无测试（页表接线未完成） |
| PageFlags::COW 读侧消费 | ⚠️ 未接线 | write_page_table_mappings 用 refcount==1 判写；COW flag 只写不读——08/18 需核对两处判据等价 |
| file-backed cow_block（clearend 处理） | ⚠️ 23 范围 | mem_file.c:59-77 的 clearend 分裂逻辑未实现 |
| SharedMemory 源链接端到端（跨进程） | ⚠️ 部分 | 判定有单测，跨进程消息级无测试 |
| anon 预分配泄漏修复的回归测试 | ⚠️ 缺失 | Rust 无泄漏是结构保证（NeedCow 才分配），无显式测试 |

### 5.4 测试统计（截至 2026-08-16）

```
$ cd os && cargo test -p minix-vm --lib
→ 360 passed / 1 failed（test_map_lazy pre-existing，13 范围，§5.3 已标注）
$ cargo test -p minix-vm --lib cow_exec_pf → 5 passed
$ cargo test -p minix-vm --lib memtype → 13 passed
$ cargo check -p minix-vm → Finished（110 warnings pre-existing，无 error）
```

---

## 6. 过渡

位置可回答性：CoW 机制是**横向基础设施**——上游由 11（引用计数）/12（memtype 回调）/13（区域映射）提供原语，下游被 16（页错误状态机 `NeedCow → cow_resolve`）与 18（fork 建立共享）双侧消费。本文档的四阶段（建立/保护/触发/分裂）对应 `fork_region`（18 详述调用方）与 `cow_resolve_core`（16 详述触发链）。

向下游的移交：

- **18-vm-fork**：`fork_region`/`fork_regions`/`setup_cow_for_all_regions` 的调用方（`do_fork` 全流程）、`map_proc_copy` 等价物、endpoint 合成。
- **23-vfs-interaction**：file-backed 的 `cow_block`（mem_file.c:59-77）——clearend 分裂、CoW 后写回语义。
- **08-pagetable-ops**：`write_page_table_mappings` 的 PTE 写入与 `PageFlags::COW` 读侧接线核对（§5.3 backlog）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/11-phys-pagestate.md` — 物理页引用计数（refcount 语义与 pb.c 原语）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/12-memtype.md` — `writable`/`ev_copy`/`ev_reference`/`ev_unreference` 回调签名
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/16-pagefault.md` — 页错误状态机消费 `NeedCow → cow_resolve`
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/18-vm-fork.md` — fork 建立共享（`fork_region` 调用方）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/23-vfs-interaction.md` — file-backed `cow_block`
- `minix3/minix/servers/vm/pb.c`（:32-52/:54-59/:61-71/:73-91/:96-134/:136-168）— 引用计数原语与 mem_cow
- `minix3/minix/servers/vm/mem_anon.c`（:33-46/:56-62/:64-97/:105-113）— 匿名内存 CoW
- `minix3/minix/servers/vm/region.c`（:130-134/:257-295/:820-849/:906/:995-996）— 写保护与 fork 复制
- `minix3/minix/servers/vm/mem_file.c`（:59-77/:173-177）、`minix3/minix/servers/vm/mem_shared.c`（:122/:161）— file/shared 差异
- `os/servers/vm/src/fork.rs`（:92-140/:142-177）、`os/servers/vm/src/region/vir_region.rs`（:163-176/:189-211/:230-239/:256-270）、`os/servers/vm/src/region/page_state.rs`（:22-57/:166-184）、`os/servers/vm/src/vmproc/vmproc_handle.rs`（:488-495/:505-542）、`os/servers/vm/src/memtype.rs`（:95-97/:192-202/:411-413/:434-530/:892-894）、`os/servers/vm/src/cow_exec_pf.rs`（:72-79/:85-128/:190-206/:216-233）— Rust 实现
