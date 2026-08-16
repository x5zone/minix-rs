# 19-vm-brk: VM_BRK——堆的扩展与收缩（brk/sbrk 服务）

> **分类**: 阶段 7 — IPC 服务（进程生命周期）
> **源码**: `minix3/minix/servers/vm/break.c`（`do_brk` :44-57 / `real_brk` :62-69）+ `minix3/minix/servers/vm/region.c`（`map_region_extend_upto_v` :1002-1060）+ `minix3/minix/servers/vm/mem_anon.c`（`anon_resize` :115-130）+ `minix3/minix/lib/libc/sys/brk.c`（libc 封装 :24-34）+ `minix3/minix/include/minix/com.h`（`VM_BRK` :636）+ `minix3/minix/include/minix/ipc.h`（`mess_lc_vm_brk` :918-926）
> **Rust 模块**: `os/servers/vm/src/brk.rs`（`handle_brk` :62-83 / `grow_heap` :85-126 / `shrink_heap` :128-204 / `BrkError` :37-41）+ `os/servers/vm/src/ipc/dispatcher.rs`（`dispatch_brk` :123-137 / 主循环 VM_BRK 分支 :1038-1039 / 错误映射 :1218-1225）+ `os/servers/vm/src/vm_server.rs`（`handle_brk` :1214-1218）+ `os/libs/minix-types/src/ipc/vm.rs`（`VmBrkIn` :190-193 / `VmBrkOut` :197-199 / `decode_message` :699-720 / `EncodeToM1` :722-727）+ `os/libs/minix-types/src/ipc/message.rs`（`m_lc_vm_brk` union :132 / `MessLcVmBrk` :1522-1536）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md`（区域结构 + extend/split）+ `14-region-lookup.md`（AVL/BTreeMap 查找语义）+ `15-ipc-dispatch.md`（主循环分发与回复）+ `05-physical-memory.md`（物理页分配/释放）
> **说明**: 本文档管 **brk 服务**——VM 如何响应 `VM_BRK` 请求扩展/收缩堆区域：调用者验证 → 三态编排（grow/shrink/no-change）→ 区域扩展（惰性页分配）或收缩（真正释放物理页）→ 回复。**不覆盖**：用户态 malloc/sbrk 分配器（libc 上层）、栈增长（`STACK_CHANGED` 标志在 break.c:38-39 声明但 do_brk 不处理）、`map_page_region` 通用映射（13 范围）。

---

## 1. 概念：堆是数据段顶部的"向上生长区"

### 1.0 章节引言

fork（18）创建了共享地址空间；brk 回答三个编排问题：

1. **堆在哪**——堆不是独立 vmproc 字段，而是"数据段区域顶部的虚拟区间"（§1.1）。
2. **怎么扩展**——VM 把新 brk 地址对齐到页，扩展顶部区域（物理页惰性分配，§1.3/§2.4）。
3. **怎么收缩**——C 静默忽略收缩；Rust 真正释放物理页（§1.4，本文档关键差异）。

它在整个 02-stage-vm 中的位置：

```
13（区域结构）→ 14（查找）→ 15（分发）→ ★19（brk 服务：堆扩展/收缩）
→ 20（mmap 通用映射）→ 22（exit 释放）
```

### 1.1 堆在地址空间中的位置

Minix3 的内存模型在 `break.c:3-17` 头注释中定义：text → data → gap（未使用）→ stack。数据段向上生长、栈向下生长，两者从 gap 两端相向而行，相遇则进程必须被杀死。

```
低地址                             高地址
├─ text ──┬── data/bss/heap ──┬── gap ──┬── stack ─┤
          ↑ region_top（堆顶）   ↑ 可扩展空间  ↓ 栈底
```

**关键事实**：`vmproc` 结构体**没有** `vm_brk`/`vm_data_top` 等专用字段（draft §1.3 已核实 vmproc.h）。堆顶隐含在"数据段 vir_region 的 `vaddr + length`"中——`map_region_extend_upto_v` 通过 AVL 查找该区域并扩展。Rust 侧对应 `ActiveProc::region_top`（vmproc_handle.rs:221，返回 `vm_region_top` 标量）——**Rust 显式维护 region_top 标量**，与 C 的"隐含在区域长度中"是结构差异（外部行为等价，§3.6 #7）。

### 1.2 brk/sbrk 与 libc 协议

`brk(addr)` 由**用户进程直接发给 VM**（不经 PM——与 VM_FORK 不同）。libc 封装（libc/sys/brk.c:24-34）：

```c
int brk(addr)
void *addr;
{
  message m;
  if (addr != _brksize) {
	memset(&m, 0, sizeof(m));
	m.m_lc_vm_brk.addr = addr;
	if (_syscall(VM_PROC_NR, VM_BRK, &m) < 0) return(-1);
	_brksize = addr;
  }
  return(0);
}
```

协议要点：

| 项 | 值 | 说明 |
|----|----|------|
| 消息类型 | `VM_BRK`（com.h:636，`VM_RQ_BASE+2`） | 主循环 CALLMAP 注册（main.c:545） |
| 请求载荷 | `m_lc_vm_brk.addr`（ipc.h:918-926） | 仅一个新堆顶地址，`memset` 后写入 |
| 调用者识别 | `m_source`（内核填充，防伪造） | C `do_brk` 用 `m_source` 而非消息字段（break.c:51） |
| 回复 | 仅状态（OK/ENOMEM） | `_syscall` 返回 int；`_brksize = addr` 由 libc 自己更新 |
| 跳过条件 | `addr == _brksize` 时不发消息 | libc 层去重（brk.c:27） |

**sbrk(incr)** 基于 brk：`oldsize = _brksize; if (brk(oldsize + incr) == 0) return oldsize; else return -1;`——增量语义完全在 libc 侧，VM 只看到绝对地址。

### 1.3 C 的扩展语义：只增不减

C 的 `map_region_extend_upto_v`（region.c:1002-1060）只处理"扩展"：

- `offset = roundup(v, VM_PAGE_SIZE)`（:1009）——搜索锚点向上对齐到页；
- `region_search(offset, AVL_LESS)`（:1011）——找起始地址 < offset 的最大区域（即堆/数据区域）；
- **`if(vr->vaddr + vr->length >= v) return OK;`**（:1016）——请求地址已在区域内（含收缩请求）→ 直接返回 OK，**区域不变、物理页不释放**；
- 扩展：`extralen = offset - limit`（:1025）→ 有 `ev_resize` 则 realloc physblocks + 回调（`anon_resize` 仅改 `vr->length`，:127）；无则 `map_page_region` 追加匿名区域（:1037-1045）。

**物理页永远惰性分配**：扩展只改区域元数据（length/physblocks 槽），新页首次访问时经缺页分配（16 范围）。

### 1.4 Rust 的收缩语义：真正释放（差异声明）

Rust `shrink_heap`（brk.rs:128-204）**真正收缩**：把堆顶以上的区域拆掉/移除，`free_region_pages` 递减 refcount + `free_pfn` 归还物理页，`sub_total` + `set_region_top` 下移堆顶。

| | C | Rust |
|--|---|------|
| `brk(向下)` | 返回 OK，区域不变（:1016） | 返回 OK，区域收缩 + 物理页释放 |
| `brk(向上回原位)` | 无操作（区域仍覆盖旧顶） | 重新扩展（重新分配物理页） |
| 外部可观察 API | `brk()` 返回 0 | `brk()` 返回 0（等价） |
| 资源占用 | 收缩后物理页保留 | 收缩后物理页归还（**内存回收改进**） |

**判定**：进程可观察行为（返回值/地址空间可达性）等价，差异在物理内存占用时序——这是**设计改进**（P1 级，brk.rs:12-14 模块注释声明"improving memory reclamation"），非 ARCH 演进（不改变地址空间布局语义）。本文档 §3.6 #2 诚实标注。

### 1.5 对照：Redox 与 Linux

- **Linux**：`brk()` 经内核 `do_brk`/`__do_munmap`——扩展按页对齐扩展堆 VMA（`get_unmapped_area` 检查与相邻 VMA 合并），收缩调用 `__do_munmap` 释放页。与 Rust 相同：收缩真正释放页。
- **Redox**：`brk` 由内核 `sys_brk` 处理（`address.rs`），返回新堆顶；x86_64 下同样按页对齐。Redox 收缩仅更新堆顶不归还页（`BRK_GROW_ONLY` 语义）——与 C Minix3 相似。
- **对照要点**：Minix3 把 brk 放在**用户态 VM 服务器**（微内核架构）；Linux/Redox 在内核。三家共同点：扩展惰性分配物理页；差异在收缩策略（Linux 释放 / Minix3 忽略 / Redox 忽略）。

### 1.6 小结

brk = 调用者验证（m_source）→ 三态编排（grow/shrink/no-change）→ 区域元数据操作（扩展惰性、收缩释放）。C 只增不减是"简化实现"；Rust 收缩释放是资源回收改进。理解 `map_region_extend_upto_v` 的 AVL_LESS 查找 + roundup 语义后，C 与 Rust 可逐段对照阅读。

---

## 2. C 源码分析

### 2.1 调用路径与消息格式

```
用户进程 brk(addr) → libc（_brksize 去重，brk.c:27）
  → _syscall(VM_PROC_NR, VM_BRK, &m)          m.m_lc_vm_brk.addr = addr
  → kernel 转发（m_source = 调用者 endpoint）
  → VM 主循环（main.c:545 CALLMAP）
  → do_brk(msg)                                break.c:44-57
  → real_brk(&vmproc[proc], addr)               break.c:62-69
  → map_region_extend_upto_v(vmp, v)            region.c:1002-1060
  → 回复：OK / -ENOMEM（状态即消息类型）
```

**mess_lc_vm_brk**（ipc.h:918-926）：`{ void *addr; uint8_t padding[52]; }`——56 字节载荷中只有 addr 一个字段，位于**载荷偏移 0**。32 位下 addr（4 字节）与 `m1_i1` 重叠；64 位 minix-rs 用专用 union 成员 `m_lc_vm_brk.addr`（u64，message.rs:1522-1536）承载（§3.5 修复）。

### 2.2 do_brk（break.c:44-57）

```c
int do_brk(message *msg)
{
	int proc;

	if (vm_isokendpt(msg->m_source, &proc) != OK) {
		printf("VM: bogus endpoint VM_BRK %d\n", msg->m_source);
		return EINVAL;
	}

	return real_brk(&vmproc[proc], (vir_bytes) msg->m_lc_vm_brk.addr);
}
```

- `vm_isokendpt(msg->m_source, &proc)`（:51）——验证**消息发送者**（m_source）三步：endpoint 范围 / 与 vmproc 一致性 / VMF_INUSE（utility.c:84-101，03 范围）。失败 EINVAL。
- `(vir_bytes) msg->m_lc_vm_brk.addr`（:56）——32 位下把 `void *` 截断为 u32；64 位 minix-rs 直接读 u64（§3.5）。

### 2.3 real_brk（break.c:62-69）

```c
int real_brk(struct vmproc *vmp, vir_bytes v)
{
	if(map_region_extend_upto_v(vmp, v) == OK) {
		return OK;
	}
	return(ENOMEM);
}
```

唯一的错误码：`ENOMEM`。所有失败（无区域可扩展/与下一区域冲突/分配失败）统一映射为 ENOMEM——Rust 侧 `BrkError::OutOfMemory → VmError::OutOfMemory → ENOMEM` 对齐（§3.4）。

### 2.4 map_region_extend_upto_v 分步（region.c:1002-1060）

```c
int map_region_extend_upto_v(struct vmproc *vmp, vir_bytes v)
{
	vir_bytes offset = v, limit, extralen;
	struct vir_region *vr, *nextvr;
	struct phys_region **newpr;
	int newslots, prevslots, addedslots, r;

	offset = roundup(offset, VM_PAGE_SIZE);              /* :1009 页对齐 */

	if(!(vr = region_search(&vmp->vm_regions_avl, offset, AVL_LESS))) {  /* :1011 */
		printf("VM: nothing to extend\n");
		return ENOMEM;                                   /* :1013 无区域 */
	}

	if(vr->vaddr + vr->length >= v) return OK;           /* :1016 已覆盖（含收缩） */

	limit = vr->vaddr + vr->length;                      /* :1018 */

	assert(vr->vaddr <= offset);
	newslots = phys_slot(offset - vr->vaddr);            /* :1021 新槽数 */
	prevslots = phys_slot(vr->length);
	assert(newslots >= prevslots);
	addedslots = newslots - prevslots;
	extralen = offset - limit;                           /* :1025 扩展量 */
	assert(extralen > 0);

	if((nextvr = getnextvr(vr))) {                       /* :1028 */
		assert(offset <= nextvr->vaddr);
	}
	if(nextvr && nextvr->vaddr < offset) {               /* :1032 冲突检查 */
		printf("VM: can't grow into next region\n");
		return ENOMEM;
	}

	if(!vr->def_memtype->ev_resize) {                    /* :1037 无 resize 回调 */
		if(!map_page_region(vmp, limit, 0, extralen,     /* :1038 */
			VR_WRITABLE | VR_ANON,
			0, &mem_type_anon)) {
			printf("resize: couldn't put anon memory there\n");
			return ENOMEM;
		}
		return OK;
	}

	if(!(newpr = realloc(vr->physblocks,                 /* :1047 realloc 槽 */
		newslots * sizeof(struct phys_region *)))) {
		printf("VM: map_region_extend_upto_v: realloc failed\n");
		return ENOMEM;
	}

	vr->physblocks = newpr;                              /* :1053 */
	memset(vr->physblocks + prevslots, 0,                /* :1054 新槽置空 */
		addedslots * sizeof(struct phys_region *));

	r = vr->def_memtype->ev_resize(vmp, vr, offset - vr->vaddr);  /* :1057 */

	return r;
}
```

**分步语义**：

| 步骤 | C 行号 | 语义 |
|------|--------|------|
| 页对齐 | :1009 | `roundup`——扩展量按页对齐向上取整（`v` 未对齐时扩展超过请求，外部行为：brk 返回成功但实际顶对齐到页） |
| 区域查找 | :1011 | `AVL_LESS`——找 vaddr < offset 的最大区域（堆/数据段） |
| 已覆盖短路 | :1016 | **请求地址在区域内 → OK**（含全部收缩请求，见 §1.4） |
| 冲突检查 | :1028-1035 | `nextvr->vaddr < offset` → ENOMEM（防堆长进栈/gap 上其他区域） |
| 无 ev_resize 分支 | :1037-1045 | 直接 `map_page_region` 追加匿名区域（memtype 无 resize 回调时） |
| realloc 槽 | :1047-1055 | physblocks 扩容 + 新槽置空（NULL = 未映射，惰性） |
| ev_resize | :1057 | 委托 memtype 回调（anon 即改 length） |

### 2.5 anon_resize 与 ev_resize 回调（mem_anon.c:115-130）

```c
static int anon_resize(struct vmproc *vmp, struct vir_region *vr, vir_bytes l)
{
	/* Shrinking not implemented; silently ignored.
	 * (Which is ok for brk().)
	 */
	if(l <= vr->length)
		return OK;                       /* :120-121 收缩静默忽略 */

        assert(vr);
        assert(vr->flags & VR_ANON);
        assert(!(l % VM_PAGE_SIZE));

        USE(vr, vr->length = l;);        /* :127 仅改长度元数据 */

	return OK;
}
```

- `ev_resize` 签名：`int (*ev_resize)(struct vmproc *vmp, struct vir_region *vr, vir_bytes len)`（memtype.h:21）；anon 注册在 mem_anon.c:38。
- **注释自证**：`"Shrinking not implemented; silently ignored. (Which is ok for brk().)"`——C 作者明确选择不实现收缩。
- **物理页零分配**：realloc 的新槽是 NULL，`vr->length` 只是"允许访问的虚拟范围"；首次访问经缺页（`anon_pagefault` 的 `alloc_mem`，17 §2.1 已述）。

### 2.6 C 小结：符号全景

| 符号 | 位置 | 角色 |
|------|------|------|
| `do_brk` | break.c:44-57 | 入口：m_source 验证 + 委托 |
| `real_brk` | break.c:62-69 | ENOMEM 统一错误映射 |
| `map_region_extend_upto_v` | region.c:1002-1060 | 扩展编排（对齐/查找/冲突/realloc/回调） |
| `region_search` | regionavl.c | AVL_LESS 查找（14 范围） |
| `anon_resize` | mem_anon.c:115-130 | 收缩静默忽略 + 长度更新 |
| `ev_resize` | memtype.h:21 | memtype 可扩展性回调 |
| `mess_lc_vm_brk` | ipc.h:918-926 | 请求载荷（仅 addr） |
| `VM_BRK` | com.h:636 | 消息类型（VM_RQ_BASE+2） |
| `DATA_CHANGED`/`STACK_CHANGED` | break.c:38-39 | 标志声明（do_brk 不使用，遗留） |

---

## 3. Rust 设计决策

### 3.1 D1：handle_brk 三态编排（brk.rs:62-83）

```rust
pub(crate) fn handle_brk(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    request: &BrkRequest,
) -> Result<BrkResponse, BrkError> {
    let slot = table.vm_isokendpt(request.endpoint)?;          // :68
    let mut active = table.get_active(slot)                    // :70
        .ok_or(BrkError::ProcessNotFound)?;
    let current_brk = active.region_top();                     // :73
    let requested = request.new_brk_addr;                      // :74
    if requested.0 < current_brk.0 {                           // :76 收缩
        shrink_heap(&mut active, page_alloc, frames, requested)
    } else if requested.0 > current_brk.0 {                    // :78 扩展
        grow_heap(&mut active, page_alloc, frames, requested)
    } else {                                                   // :80 无变化
        Ok(BrkResponse { new_brk_addr: current_brk })
    }
}
```

- 对比 C：C 无显式三态——`map_region_extend_upto_v` 的 :1016 短路隐式处理 no-change/收缩。Rust 把"收缩"显式化（真正执行），"无变化"独立分支（返回当前顶）。
- `EndpointError → BrkError::ProcessNotFound`（brk.rs:30-34）：brk 不区分 INVALID-slot 与 DEAD-endpoint（与 munmap 同策略，注释 :28-29 声明）。

### 3.2 D2：grow_heap 区域扩展策略（brk.rs:85-126）

```rust
fn grow_heap(active, _page_alloc, _frames, new_brk) -> Result<BrkResponse, BrkError> {
    let current_top = active.region_top();                     // :91
    let grow_len = new_brk.0 - current_top.0;                  // :92
    if grow_len == 0 { return Ok(...); }                       // :94
    let aligned_len = VirBytes(((grow_len + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE);  // :98
    let new_end = VirBytes(current_top.0 + aligned_len.0);     // :99
    if active.regions().find_overlap(current_top, new_end).is_some() {  // :101
        return Err(BrkError::OutOfMemory);
    }
    if let Some(top_region) = active.regions_mut().find_mut(current_top) {      // :105
        top_region.extend(aligned_len)...;
    } else if let Some(top_region) = active.regions_mut().find_mut_by_end(current_top) {  // :108
        top_region.extend(aligned_len)...;
    } else {
        let new_region = VirRegion::with_memtype(current_top, aligned_len,      // :112-117
            VrFlags::WRITABLE | VrFlags::ANON, &MEM_TYPE_ANON);
        active.regions_mut().insert(new_region)...;            // :118
    }
    active.add_total(aligned_len);                             // :122
    active.set_region_top(new_end);                            // :123
    Ok(BrkResponse { new_brk_addr: new_brk })
}
```

**对齐语义等价**：C 的 `roundup(offset, VM_PAGE_SIZE)`（region.c:1009）↔ Rust `((grow_len + PAGE_SIZE-1)/PAGE_SIZE)*PAGE_SIZE`（brk.rs:98）——扩展量都向上取整到页。C 的 extralen = `roundup(v) - limit`；Rust 的 aligned_len = `roundup(new_brk - current_top)`。**边界情形等价**：两者在 `v` 未对齐时都实际扩展超过请求地址（brk 语义本身只保证"新顶 ≥ 请求"）。

**区域选择差异**：
- C：`AVL_LESS(offset)` 找"起始 < offset 的最大区域"（region.c:1011）。
- Rust：`find_mut(current_top)`（vaddr 恰为当前顶的区域，region_map.rs:66）→ 失败再 `find_mut_by_end(current_top)`（end_addr 恰为当前顶的区域，region_map.rs:111）。
- 等价性论证：Rust 的 region_top 是"数据段顶部"——堆区域要么 vaddr == region_top（独立堆区域），要么 end_addr == region_top（数据+堆同一区域）。两者恰好覆盖 C AVL_LESS 在该场景下命中的区域；`find_mut` 的 `contains_addr` 语义（region_map.rs:60-63）与 AVL_LESS 的"≤"边界一致。**差异**：C 若命中一个"起始 < offset 但 end < limit"的中间区域会 assert 失败（region.c:1020 `assert(vr->vaddr <= offset)` 通过、region.c:1023 `newslots >= prevslots` 通过但 extralen 计算越界）——C 假设堆是连续顶部区域；Rust 的 find_mut_by_end 直接编码该假设（§3.6 #5）。

**冲突检查差异**：C 只查"下一区域 vaddr < offset"（region.c:1032）；Rust 查 `find_overlap(current_top, new_end)`（region_map.rs:134-144，任何与 [current_top, new_end) 重叠的区域）——**Rust 更严格**（重叠即拒绝），C 允许"下一区域 vaddr ≥ offset"（即扩展量与下一区域恰相接时允许，`nextvr->vaddr == offset` 不触发 region.c:1032 条件）。边界差异：Rust 拒绝 `nextvr->vaddr == new_end`（相接）？——`find_overlap` 用 `r.overlaps(start, end)`（`vaddr < end && end_addr > start`，vir_region.rs:146-148），相接（vaddr == end）不重叠 → 允许。与 C 一致。

**无 ev_resize 分支**（region.c:1037-1045）在 Rust 中不存在：`VirRegion::extend`（vir_region.rs:127-140）是 memtype 无关的通用扩展（push EMPTY 槽 + 改 length），不区分 ev_resize 有无。`[ARCH: A-12]` 简化：memtype 回调族中 resize 语义并入 `VirRegion::extend`（§3.6 #6，三处一致标注：doc §3.2/§3.6 + design 19-design.v1 D2 + 代码注释 vir_region.rs:120-126）。

### 3.3 D3：shrink_heap 真正释放（brk.rs:128-204）

```rust
fn shrink_heap(active, page_alloc, frames, new_brk) -> Result<BrkResponse, BrkError> {
    let current_top = active.region_top();
    if new_brk.0 >= current_top.0 { return Ok(...); }          // 防御（调用方已保证）
    // 收集：vaddr >= new_brk → 整体移除；跨 new_brk → split 收缩
    for region in active.regions().iter() {                    // :143-149
        if region.vaddr.0 >= new_brk.0 { regions_to_remove.push(region.vaddr); }
        else if region.end_addr().0 > new_brk.0 && region.vaddr.0 < new_brk.0 {
            regions_to_shrink.push(region.vaddr);
        }
    }
    // split + 释放右半
    for vaddr in regions_to_shrink {                           // :151-185
        let raw_split = new_brk.0 - region.vaddr.0;
        let aligned_split = raw_split & !(PAGE_SIZE - 1);      // 向下对齐到页
        match region.split(split_point) {                      // vir_region.rs:272
            Ok((left, right)) => {
                free_region_pages(right, pt, frames, page_alloc);  // region/mod.rs:23
                active.sub_total(freed_len);                   // :170
                active.regions_mut().insert(left)...;
            }
            Err(_) => { /* 重插回原区域 */ }
        }
    }
    for vaddr in regions_to_remove {                           // :187-199
        free_region_pages(region, pt, frames, page_alloc);
        active.sub_total(freed_len);
    }
    active.set_region_top(new_brk);                            // :201
    Ok(BrkResponse { new_brk_addr: new_brk })
}
```

**两阶段释放**（free_region_pages，region/mod.rs:23-51）：`pt.unmap`（:30-35，非 test 下有真实页表）+ `ev_delete`（:43-45）+ `free_range` 收集 pending（:47，refcount 归零且非 IN_CACHE 的槽）+ `ev_unreference` + `free_pfn`（:48-51）+ fdref deref（:53-61，23 范围）——与 17 §3.5 两阶段释放同一原语族。

**C 对照**：C 完全没有 shrink 路径（:1016 短路）。Rust 收缩的 split 边界按页向下对齐（`raw_split & !(PAGE_SIZE-1)`，brk.rs:157）——收缩后 `region_top` 是页对齐的，与扩展的向上对齐互补（扩展向页上取整、收缩向页下取整，保证中间无半页空洞）。

### 3.4 D4：错误映射与回复（dispatcher.rs:1218-1225 + vm.rs:722-727）

| C | Rust | errno |
|---|------|-------|
| `vm_isokendpt` 失败 → EINVAL（break.c:51-53） | `ProcessNotFound → VmError::InvalidProcess`（dispatcher.rs:1221） | EINVAL |
| `map_region_extend_upto_v` 失败 → ENOMEM（break.c:68） | `OutOfMemory → VmError::OutOfMemory`（dispatcher.rs:1222） | ENOMEM |

**回复差异**：C 回复仅状态（消息类型 = OK/ENOMEM，libc `_syscall` 检查 `< 0`）；Rust `VmReply::Brk(VmBrkOut)` 额外编码 `new_addr` 到 `m1p1`（vm.rs:722-727）——**信息性富化**（回显请求地址），libc 侧忽略（`_brksize` 由 libc 自己维护），外部行为等价（§3.6 #8）。

### 3.5 D5：wire format 修复（19-P1-1，本轮）

C 协议：调用者 = `m_source`（内核填充，防伪造），载荷 = `m_lc_vm_brk.addr`（载荷偏移 0）。Rust 旧实现 `impl DecodeFromM1 for VmBrkIn` 从 `m1i1` 读 endpoint、`m1p1` 读 addr——**与 C wire format 错位**：

- 32 位下 `mess_lc_vm_brk.addr`（4 字节）与 `m1_i1` 重叠；真实 libc 发送者 `memset(&m,0)` 后只写 addr → `m1i1` = 地址低 32 位 ≠ endpoint；
- 旧 decode 的 `endpoint = m1i1` 读到地址低 32 位 → `vm_isokendpt` 失败 → **每个 brk() 都 EINVAL**；
- 旧 decode 的 `new_addr = m1p1`（偏移 16）与 C 的 addr（偏移 0）错位。

**修复**（与 16-P0-1 VM_PAGEFAULT 同族，参照其模式）：
1. `message.rs:132/:1522-1536`——新增专用 union 成员 `m_lc_vm_brk: MessLcVmBrk`（`addr: u64` @ 载荷偏移 0，C `mess_lc_vm_brk` 忠实布局）；
2. `vm.rs:699-720`——`VmBrkIn::decode_message(msg)` 读 `msg.m_source`（endpoint）+ `m_lc_vm_brk.addr`（new_addr）；删除错误的 M1 decode（`rg VmBrkIn::decode` 0 残留）；
3. `dispatcher.rs:1039`——主循环改用 `VmBrkIn::decode_message(msg)`；
4. 回归测试 `test_vm_brk_in_decode_message`（vm.rs:993-1012）——m_source=PM + addr 低 32 位非零，断言 endpoint 来自 m_source。

> **待接线**：真实 IPC transport 注入 m_source（26 范围）；届时 `m_lc_vm_brk` overlay 由发送方（未来 libc/kernel 适配层）按 C 布局写入。

### 3.6 差异清单（C ↔ Rust，诚实标注）

| # | C 语义 | Rust 现状 | 状态 |
|---|--------|----------|------|
| 1 | 收缩静默忽略（region.c:1016 + anon_resize :120） | `shrink_heap` 真正释放页（brk.rs:128-204） | ✅ 设计改进（外部 API 等价，§1.4） |
| 2 | 堆顶隐含在区域 vaddr+length | `vm_region_top` 显式标量（vmproc_handle.rs:221/:279） | ✅ 结构差异（行为等价） |
| 3 | endpoint 来自 m_source（break.c:51） | 旧 M1 decode 错位 → **本轮修复** `decode_message`（vm.rs:699-720） | ✅ 修复（19-P1-1） |
| 4 | 回复仅状态 | `VmBrkOut.new_addr` 编码 m1p1（信息性富化） | ✅ 等价（libc 忽略） |
| 5 | AVL_LESS 查找 + assert 假设堆是顶部区域 | `find_mut`/`find_mut_by_end` 直接编码该假设（region_map.rs:66/:111） | ✅ 等价（假设显式化） |
| 6 | 无 ev_resize 时 `map_page_region` 追加 | `VirRegion::extend` memtype 无关（vir_region.rs:127-140） | ✅ ARCH 简化（`[ARCH: A-12]`） |
| 7 | 冲突检查：仅下一区域 | `find_overlap` 全区间重叠检查（region_map.rs:134-144） | ✅ 更严格（相接允许，一致） |
| 8 | 无变化 → :1016 短路 OK | 显式 no-change 分支返回当前顶（brk.rs:80-81） | ✅ 等价 |

---

## 4. 实现详解

### 4.1 消息路径（dispatcher.rs:123-137 → :1038-1039）

```
主循环（dispatcher.rs:1038-1039）
  VM_BRK as usize - vm_rq_base =>
    Self::dispatch_brk(table, page_alloc, frames, VmBrkIn::decode_message(msg))
      → brk::BrkRequest { endpoint: request.endpoint, new_brk_addr: request.new_addr }  :129-132
      → brk::handle_brk(table, page_alloc, frames, &req)                                 :133
      → Ok(response) → VmReply::Brk(VmBrkOut { new_addr })                               :134
      → Err(e) → VmReply::Error(e.into())                                                :135
```

- `VmBrkIn`（vm.rs:190-193）：`endpoint`（← m_source，经 decode_message）+ `new_addr`（← m_lc_vm_brk.addr）。
- `VmBrkOut`（vm.rs:197-199）：`new_addr`（→ m1p1，encode :722-727）。
- `vm_server.rs:1214-1218`：`handle_brk` 组装 table/frames 后委托 `dispatch_brk`（测试用 MockPaging）。

### 4.2 伪码总结（brk.rs:62-204）

```
handle_brk(table, page_alloc, frames, req)                  :62-83
  slot = table.vm_isokendpt(req.endpoint)?                  → ProcessNotFound
  active = table.get_active(slot)?                          → ProcessNotFound
  requested < region_top  → shrink_heap(active, page_alloc, frames, requested)   :76-77
  requested > region_top  → grow_heap(active, page_alloc, frames, requested)     :78-79
  else                    → Ok(BrkResponse { current_top })                       :80-81

grow_heap(active, _, _, new_brk)                            :85-126
  grow_len = new_brk - region_top
  aligned_len = roundup(grow_len, PAGE_SIZE)                :98
  new_end = region_top + aligned_len
  find_overlap(region_top, new_end) → OutOfMemory           :101-103
  find_mut(region_top) | find_mut_by_end(region_top) → extend(aligned_len)
    | 都无 → 新建 VirRegion(WRITABLE|ANON, MEM_TYPE_ANON) 并 insert                :105-119
  add_total(aligned_len); set_region_top(new_end)           :122-123
  Ok(BrkResponse { new_brk })

shrink_heap(active, page_alloc, frames, new_brk)            :128-204
  收集 regions_to_remove / regions_to_shrink                 :143-149
  for r in regions_to_shrink:  split(向下页对齐) → free_region_pages(right) + sub_total   :151-185
  for r in regions_to_remove:  free_region_pages(r) + sub_total                          :187-199
  set_region_top(new_brk)                                   :201
  Ok(BrkResponse { new_brk })
```

### 4.3 收缩的 split + free_region_pages 详解

`VirRegion::split(split_len)`（vir_region.rs:272-334）：校验页对齐 + 0 < split_len < length；左区域保留原 vaddr/flags/remaps/id，右区域从 split 点开始、id+1；File 参数左右分割 offset/clearend（:291-300，23 范围）。brk 收缩用 `split_point = raw_split & !(PAGE_SIZE-1)`（brk.rs:154）保证 split 页对齐（split 校验 :273 要求）。

`free_region_pages`（region/mod.rs:23-82）完整路径：
1. `pt.unmap`（:30-35）——非 test 构建下有真实页表时逐页 unmap（`#[cfg(not(test))]` 下传 `Some(page_table_mut())`，brk.rs:164-167）；
2. `ev_delete`（:43-45）——memtype 删除钩子；
3. `free_range` 收集 pending `(pfn, memtype)`（:47）——refcount 归零且非 IN_CACHE 的槽（17 §3.5）；
4. `ev_unreference` + `free_pfn`（:48-51）——物理页归还分配器；
5. fdref deref（:53-61）——文件区域引用递减（VFS 交互，23 范围）。

**安全论证**：单线程事件循环；`active` 是唯一可变句柄；split 失败时重插原区域（brk.rs:174-178）不丢元数据；`sub_total` 与释放量严格 1:1（`freed_len` 取自被释放区域/右半的 length）。

### 4.4 19-P1-1 修复记录（本轮）

- **问题**：`VmBrkIn` 旧 decode 从 `MessageM1` 读 endpoint（m1i1）与 addr（m1p1）——C wire format 是 `m_source` + `m_lc_vm_brk.addr`（载荷偏移 0）。真实 libc 发送者 memset 后只写 addr：m1i1 = 地址低 32 位 → endpoint 错位 → 所有 brk() 返回 EINVAL。
- **修复**：`MessLcVmBrk` overlay（message.rs:1522-1536）+ `decode_message`（vm.rs:699-720）+ dispatcher 接线（dispatcher.rs:1039）+ 回归测试（vm.rs:993-1012）。
- **验证**：`cargo test -p minix-types --lib ipc::vm` → 17 passed；`cargo test -p minix-vm --lib` → 361 passed / 1 failed（test_map_lazy pre-existing 13 范围）。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-16）

**brk.rs**（8 个，:207-427）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_brk_error_to_errno | :245 | BrkError → VmError → errno 全链（EINVAL/ENOMEM） |
| test_grow_heap_creates_region_on_first_brk | :254 | 首次扩展新建区域 + 返回请求地址 |
| test_grow_heap_extends_existing_region | :273 | 连续扩展复用区域 |
| test_brk_no_change | :297 | 请求 == 当前顶 → 直接返回 |
| test_brk_process_not_found | :316 | 无效 endpoint → ProcessNotFound |
| test_shrink_heap_basic | :331 | grow 后 shrink → 返回收缩地址 |
| test_grow_heap_extends_region_at_boundary | :355 | 边界扩展不新建区域（区域数不变） |
| test_grow_heap_rejects_overlap_c18 | :391 | 重叠区域 → OutOfMemory（回归） |

**vm_server.rs**（1 个）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_vm_server_handle_brk_not_found | :1432 | 无效 endpoint → InvalidProcess |

**minix-types vm.rs**（brk 相关 2 个）：

| 测试 | 位置 | 契约 |
|------|------|------|
| test_vm_brk_in_decode_message | :993 | m_source + m_lc_vm_brk.addr 解码（19-P1-1 回归） |
| test_vm_brk_out | :1015 | VmBrkOut 字段 |

### 5.2 覆盖维度

- **错误映射**：ProcessNotFound → EINVAL、OutOfMemory → ENOMEM（全链测试）。
- **扩展**：首建区域 / 复用区域 / 边界不新建 / 重叠拒绝。
- **收缩**：grow 后 shrink 基本路径。
- **wire format**：m_source 注入 + addr 载荷解码（19-P1-1 回归）。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 状态 | 说明 |
|------|------|------|
| shrink 的 split 路径（跨区域收缩）无直接单测 | ⚠️ 缺失 | test_shrink_heap_basic 只覆盖整区域移除边界；split 分支（brk.rs:151-185）依赖区域跨 new_brk 的布局 |
| shrink 物理页实际释放断言 | ⚠️ 缺失 | 无 refcount/free_pfn 计数断言（可补：收缩后 pfn 回收到 allocator） |
| 端到端（真实消息 → handle_brk → 回复） | ⚠️ 缺失 | 依赖 TestIpcTransport 接线（15-P2-1） |
| 与栈碰撞端到端 | ⚠️ 缺失 | find_overlap 单测覆盖（:391），真实栈区域布局无集成测试 |
| grow 后 shrink 再 grow 的页回收复用 | ⚠️ 缺失 | 无分配器复用断言 |

### 5.4 测试统计（截至 2026-08-16）

```
$ cd os && cargo test -p minix-vm --lib
→ 361 passed / 1 failed（test_map_lazy pre-existing，13 范围，§5.3 已标注）
$ cargo test -p minix-vm --lib brk → 9 passed（brk.rs 8 + vm_server 1）
$ cargo test -p minix-types --lib ipc::vm → 17 passed（含 19-P1-1 回归 2 个）
$ cargo check -p minix-vm → Finished（110 warnings pre-existing，无 error）
```

---

## 6. 过渡

位置可回答性：brk 是 **13（区域结构）的消费面**——`VirRegion::extend`/`split`/`free_range` 在这里被编排；同时是 **15（分发）的一个服务分支**（VM_BRK = VM_RQ_BASE+2，main.c:545 ↔ dispatcher.rs:1038-1039）。

向下游的移交：

- **20-vm-mmap**：`map_page_region` 的通用映射语义（C region.c:1038 的无 ev_resize 分支在 Rust 并入 extend）；mmap 的 `find_overlap` 冲突检查与 brk 共享。
- **22-vm-exit**：`free_region_pages` 与 exit 的 `clear` 释放共享同一原语族（两阶段释放，17 §3.5）。
- **26-vm-queries**：真实 IPC transport 注入 m_source（`decode_message` 依赖），`m_lc_vm_brk` overlay 由发送方按 C 布局写入。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/13-region-mapping.md` — 区域结构（VirRegion::extend/split/free_range）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/14-region-lookup.md` — AVL_LESS ↔ BTreeMap 查找语义
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/15-ipc-dispatch.md` — 主循环分发与回复编码
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/16-pagefault.md` — 惰性页分配（缺页触发）与 wire-format 修复先例
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/17-cow-mechanism.md` — 两阶段释放原语（unmap_page/ev_unreference/free_pfn）
- `minix3/minix/servers/vm/break.c`（:44-69）— do_brk/real_brk
- `minix3/minix/servers/vm/region.c`（:1002-1060）— map_region_extend_upto_v
- `minix3/minix/servers/vm/mem_anon.c`（:115-130）、`minix3/minix/servers/vm/memtype.h`（:21）
- `minix3/minix/lib/libc/sys/brk.c`（:24-34）、`minix3/minix/include/minix/ipc.h`（:918-926）、`minix3/minix/include/minix/com.h`（:636）
- `os/servers/vm/src/brk.rs`（:20-34/:56-83/:85-126/:128-204/:207-427）、`os/servers/vm/src/ipc/dispatcher.rs`（:123-137/:1038-1039/:1218-1225）、`os/servers/vm/src/vm_server.rs`（:1214-1218/:1432-1445）、`os/libs/minix-types/src/ipc/vm.rs`（:186-204/:699-727/:993-1015）、`os/libs/minix-types/src/ipc/message.rs`（:132/:1522-1536）— Rust 实现
