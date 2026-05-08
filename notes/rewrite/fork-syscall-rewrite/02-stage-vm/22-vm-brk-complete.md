# 22-vm-brk-complete: brk 完整实现

> **分类**: VM服务
> **源码**: `minix3/minix/servers/vm/break.c`, `region.c:map_region_extend_upto_v`, `mem_anon.c:anon_resize`
> **说明**: 将 18-vm-brk 的设计落地为可运行的实现——补全区域扩展/收缩、物理页分配/释放、页表更新

---

## 1. 概述

### 1.1 本文档的定位

18-vm-brk 详细描述了 brk 的 IPC 接口、消息格式、Minix3 C 源码分析。但 18 的 Ch4（实现详解）是占位状态——没有给出完整的 Rust 实现。

本文档是**实现文档**：把 18 的设计落地为可编译、可运行的 Rust 代码，补全以下缺失：

| 问题 | 18 的状态 | 22 的目标 |
|------|-----------|----------|
| `do_brk()` | 无 IPC 对接 | 完整实现，验证 endpoint + 调用 real_brk |
| `real_brk()` | 不存在 | 完整实现，调用 `extend_region_upto()` |
| `extend_region_upto()` | 不存在 | 完整实现，查找区域 + 扩展/无操作 |
| 区域扩展 | 无实现 | 两种路径：有 ev_resize / 无 ev_resize |
| 区域收缩 | 无实现 | 释放物理页 + 缩减 length + 更新页表 |
| 物理页分配 | 延迟分配（缺页时） | brk 只扩展虚拟区域，物理页按需分配 |
| 页表更新 | 无实现 | 扩展时映射新页，收缩时取消映射 |

### 1.2 brk 的特殊性

brk 与 fork/exit 不同，它**只操作调用者自身的地址空间**，不需要 PM 协调，不需要跨进程通信。这使得 brk 的实现相对独立：

```
用户进程 → VM_BRK IPC → VM do_brk() → real_brk() → extend_region_upto() → 返回
```

**关键设计决策**：brk 扩展时**不立即分配物理页**，而是扩展 VirRegion 的 length，新页在首次访问时通过缺页异常分配（延迟分配 / demand paging）。这避免了 brk 调用的延迟，也节省了物理内存。

### 1.3 与其他文档的关系

| 文档 | 关系 |
|------|------|
| 18-vm-brk | 前置：IPC 接口、消息格式、C 源码分析 |
| 12-vir-region | VirRegion 结构和操作 |
| 13-region-avl | AVL 树搜索（find_less, find_slot） |
| 14-phys-region | PhysRegion 引用计数管理 |
| 20-cow-exec-pagefault | 缺页处理：brk 扩展后的物理页分配 |
| 21-vm-exit | 退出时释放 brk 扩展的内存 |

---

## 2. C 源码分析：brk 完整链路

> 本章补全 18 未详细分析的区域扩展/收缩的底层逻辑。

### 2.1 do_brk → real_brk → map_region_extend_upto_v

```c
/* break.c */
int do_brk(message *msg) {
    int proc;
    if (vm_isokendpt(msg->m_source, &proc) != OK) return EINVAL;
    return real_brk(&vmproc[proc], (vir_bytes) msg->m_lc_vm_brk.addr);
}

int real_brk(struct vmproc *vmp, vir_bytes v) {
    if (map_region_extend_upto_v(vmp, v) == OK) return OK;
    return ENOMEM;
}
```

**极简入口**：do_brk 只做 endpoint 验证，real_brk 只调用 `map_region_extend_upto_v`。所有复杂逻辑都在后者中。

### 2.2 map_region_extend_upto_v 完整分析

```c
/* region.c:1002 */
int map_region_extend_upto_v(struct vmproc *vmp, vir_bytes v)
{
    vir_bytes offset = v, limit, extralen;
    struct vir_region *vr, *nextvr;
    struct phys_region **newpr;
    int newslots, prevslots, addedslots, r;

    /* 1. 页对齐 */
    offset = roundup(offset, VM_PAGE_SIZE);

    /* 2. AVL_LESS 搜索：找到 vaddr < offset 的最大区域 */
    if(!(vr = region_search(&vmp->vm_regions_avl, offset, AVL_LESS))) {
        printf("VM: nothing to extend\n");
        return ENOMEM;
    }

    /* 3. 如果新地址已在区域内，无需操作 */
    if(vr->vaddr + vr->length >= v) return OK;

    /* 4. 计算扩展量 */
    limit = vr->vaddr + vr->length;
    assert(vr->vaddr <= offset);
    newslots = phys_slot(offset - vr->vaddr);
    prevslots = phys_slot(vr->length);
    assert(newslots >= prevslots);
    addedslots = newslots - prevslots;
    extralen = offset - limit;
    assert(extralen > 0);

    /* 5. 检查是否会与下一个区域冲突 */
    if((nextvr = getnextvr(vr))) {
        assert(offset <= nextvr->vaddr);
    }
    if(nextvr && nextvr->vaddr < offset) {
        printf("VM: can't grow into next region\n");
        return ENOMEM;
    }

    /* 6a. 无 ev_resize 回调：创建新的匿名区域 */
    if(!vr->def_memtype->ev_resize) {
        if(!map_page_region(vmp, limit, 0, extralen,
            VR_WRITABLE | VR_ANON, 0, &mem_type_anon)) {
            return ENOMEM;
        }
        return OK;
    }

    /* 6b. 有 ev_resize 回调：扩展 physblocks 数组 */
    if(!(newpr = realloc(vr->physblocks,
        newslots * sizeof(struct phys_region *)))) {
        return ENOMEM;
    }
    vr->physblocks = newpr;
    memset(vr->physblocks + prevslots, 0,
        addedslots * sizeof(struct phys_region *));

    r = vr->def_memtype->ev_resize(vmp, vr, offset - vr->vaddr);
    return r;
}
```

**两条路径的关键区别**：

| 方面 | 路径 6a（无 ev_resize） | 路径 6b（有 ev_resize） |
|------|------------------------|------------------------|
| 触发条件 | `def_memtype->ev_resize == NULL` | `def_memtype->ev_resize != NULL` |
| 区域处理 | 创建**新的独立区域** | 扩展**现有区域** |
| physblocks | 新区域有自己的数组 | `realloc` 扩展现有数组 |
| 内存类型 | 新区域用 `mem_type_anon` | 现有区域保持原 memtype |
| 典型场景 | 非 anon 区域（如 mem_direct） | 匿名内存区域（堆） |

**为什么匿名内存有 ev_resize**：匿名内存区域（堆）扩展时，只需要增加 `vr->length`，新页通过缺页分配。`anon_resize` 实现极简：

```c
/* mem_anon.c:115 */
static int anon_resize(struct vmproc *vmp, struct vir_region *vr, vir_bytes l)
{
    /* 收缩被静默忽略（对 brk 来说可以接受） */
    if(l <= vr->length) return OK;

    assert(vr);
    assert(vr->flags & VR_ANON);
    assert(!(l % VM_PAGE_SIZE));

    vr->length = l;   /* 只增加 length！ */

    return OK;
}
```

**关键洞察**：`anon_resize` 只修改 `vr->length`，不分配物理页，不更新页表。物理页在缺页时分配，页表在缺页处理时更新。这就是**延迟分配**的核心——brk 只扩展虚拟地址空间的"承诺"，不立即兑现物理内存。

### 2.3 getnextvr — 获取下一个区域

```c
/* region.c:112 */
static struct vir_region *getnextvr(struct vir_region *vr)
{
    struct vir_region *nextvr;
    region_iter v_iter;

    region_start_iter(&vr->parent->vm_regions_avl, &v_iter,
        vr->vaddr, AVL_EQUAL);
    assert(region_get_iter(&v_iter) == vr);
    region_incr_iter(&v_iter);
    nextvr = region_get_iter(&v_iter);

    if(!nextvr) return NULL;
    assert(vr->vaddr < nextvr->vaddr);
    assert(vr->vaddr + vr->length <= nextvr->vaddr);
    return nextvr;
}
```

**实现方式**：使用 AVL 树的中序遍历迭代器，定位当前节点后前进一步。中序遍历保证按 vaddr 递增顺序。

### 2.4 brk 收缩的完整链路

Minix3 的 `real_brk` 只调用 `map_region_extend_upto_v`，而后者**不处理收缩**——`anon_resize` 对收缩静默忽略（`if(l <= vr->length) return OK`）。

**收缩在哪里处理？** 答案是：Minix3 的 brk **不真正收缩**。`anon_resize` 忽略收缩请求，但返回 OK。这意味着 brk 传入的地址小于当前堆顶时，堆顶不变，但调用成功。

**为什么 Minix3 不实现 brk 收缩？**

1. **简化实现**：收缩需要释放物理页、更新页表、调整 physblocks 数组，复杂度高
2. **实际影响小**：收缩的内存通常不多，不释放也不会导致 OOM
3. **malloc 的行为**：用户态 malloc 通常不调用 brk 收缩，而是缓存已分配的内存

**但 Rust 实现应该支持收缩**：作为教学项目，完整实现更有价值。收缩逻辑与 21-vm-exit 中的 `map_subfree` 类似——遍历 PhysRegion，减少引用计数，释放物理页。

### 2.5 完整链路总结

```
brk 扩展 (new_addr > current_brk):
  do_brk() → real_brk(vmp, new_addr)
    → map_region_extend_upto_v(vmp, new_addr)
      ├── 页对齐 offset
      ├── region_search(AVL_LESS) → 找到堆区域
      ├── if offset <= vr.vaddr + vr.length → 无操作，返回 OK
      ├── getnextvr() → 检查不与下一个区域冲突
      ├── if !ev_resize → map_page_region() 创建新匿名区域
      └── if ev_resize → realloc physblocks + anon_resize(vr, new_length)
            └── vr.length = new_length  ← 只改一个字段！

brk 收缩 (new_addr < current_brk):
  do_brk() → real_brk(vmp, new_addr)
    → map_region_extend_upto_v(vmp, new_addr)
      ├── region_search(AVL_LESS) → 找到堆区域
      ├── if offset <= vr.vaddr + vr.length → anon_resize 忽略收缩，返回 OK
      └── （Minix3 不实现收缩）

brk 无操作 (new_addr == current_brk):
  → offset 在区域内 → 直接返回 OK
```

---

## 3. Rust 设计决策

### 3.1 现有代码状态

| 组件 | 现有状态 | brk 需要的操作 |
|------|---------|---------------|
| `RegionAvl::find_less()` | ✅ 已实现 | 对应 `region_search(AVL_LESS)` |
| `RegionAvl::find_slot()` | ✅ 已实现 | 对应 `region_find_slot()` |
| `RegionAvl::insert()` | ✅ 已实现 | 插入新区域 |
| `VirRegion::new()` | ✅ 已实现 | 创建新区域 |
| `VirRegion::free_range()` | ✅ 已实现 | 释放范围内物理页 |
| `MemType::on_resize()` | ✅ 已实现（默认空） | 区域扩展回调 |
| `AnonymousMemory::on_resize()` | ✅ 已实现（默认空） | 需要实现：只修改 length |
| `VmProc::add_total()` / `sub_total()` | ✅ 已实现 | 内存统计更新 |
| `do_brk()` | ❌ 不存在 | IPC 处理 |
| `real_brk()` | ❌ 不存在 | 堆调整核心 |
| `extend_region_upto()` | ❌ 不存在 | 区域扩展 |
| `shrink_region()` | ❌ 不存在 | 区域收缩（Minix3 未实现） |
| `map_page_region()` | ❌ 不存在 | 创建新区域并插入 |

### 3.2 设计原则

**原则 1：brk 不分配物理页**

与 Minix3 一致，brk 只扩展 VirRegion 的 length，物理页在缺页时分配。这保持了延迟分配的设计，减少 brk 调用的延迟。

**原则 2：支持收缩**

与 Minix3 不同，Rust 实现**支持 brk 收缩**。原因：
1. 教学项目，完整实现更有价值
2. 收缩逻辑与 exit 的 `free_range()` 类似，可以复用
3. 避免内存泄漏的演示

**原则 3：两条扩展路径**

保留 Minix3 的两条路径设计：
- 有 `on_resize` 回调的区域：扩展现有区域（匿名内存→堆）
- 无 `on_resize` 回调的区域：创建新的匿名区域

**原则 4：不使用 map_page_region**

Minix3 的 `map_page_region()` 是一个通用函数，创建新区域并插入 AVL 树。Rust 中可以用 `RegionAvl::insert()` + `VirRegion::new()` 组合实现，不需要单独的 `map_page_region()` 函数。

### 3.3 与 Minix3 的关键差异

| 方面 | Minix3 | minix-rs |
|------|--------|----------|
| 收缩处理 | `anon_resize` 静默忽略 | 实现完整收缩：释放物理页 + 缩减 length |
| 区域扩展 | `realloc(physblocks)` + `memset` | `Vec::resize()` |
| 新区域创建 | `map_page_region()` 通用函数 | `VirRegion::new()` + `RegionAvl::insert()` |
| 下一个区域查找 | `getnextvr()` 使用迭代器 | `RegionAvl::find_greater()` |
| 内存统计 | 分散在 pb.c/region.c | `VmProc::add_total()` / `sub_total()` |
| 页对齐 | `roundup()` 宏 | `(addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)` |

---

## 4. Rust 实现详解

### 4.1 do_brk — IPC 处理入口

```rust
/// 处理 VM_BRK 请求
///
/// 对应 Minix3 的 do_brk()。
pub fn do_brk(
    msg: &BrkMessage,
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
) -> Result<VirBytes, BrkError> {
    let slot = table.vm_isokendpt(msg.source)
        .map_err(|_| BrkError::InvalidEndpoint)?;

    let mut vmp = table.get_active(slot)
        .ok_or(BrkError::ProcessNotActive)?;

    real_brk(&mut vmp, VirBytes(msg.addr), page_alloc)
}
```

### 4.2 real_brk — 堆调整核心

```rust
/// 实际执行堆调整
///
/// 对应 Minix3 的 real_brk()。
/// 支持扩展和收缩（与 Minix3 不同，Minix3 不支持收缩）。
fn real_brk(
    vmp: &mut ActiveProc<'_>,
    new_addr: VirBytes,
    page_alloc: &mut VmPageAllocator,
) -> Result<VirBytes, BrkError> {
    let offset = page_align_up(new_addr);

    // 查找堆区域（vaddr < offset 的最大区域）
    let current_end = {
        let region = vmp.regions().find_less(offset)
            .ok_or(BrkError::NoRegionToExtend)?;
        region.vaddr + region.length
    };

    if offset <= current_end {
        // 新地址在现有区域内，可能需要收缩
        return shrink_if_needed(vmp, offset, page_alloc);
    }

    // 扩展区域
    extend_region_upto(vmp, offset, page_alloc)
}
```

### 4.3 extend_region_upto — 区域扩展

```rust
/// 将区域扩展到指定地址
///
/// 对应 Minix3 的 map_region_extend_upto_v()。
fn extend_region_upto(
    vmp: &mut ActiveProc<'_>,
    offset: VirBytes,
    page_alloc: &mut VmPageAllocator,
) -> Result<VirBytes, BrkError> {
    const PAGE_SIZE: u64 = 4096;

    // 1. 查找可扩展的区域
    let (vr_vaddr, vr_length, has_resize, vr_end) = {
        let region = vmp.regions().find_less(offset)
            .ok_or(BrkError::NoRegionToExtend)?;

        if region.vaddr + region.length >= offset {
            return Ok(offset);  // 已在范围内
        }

        (
            region.vaddr,
            region.length,
            region.def_memtype.is_some(),
            region.vaddr + region.length,
        )
    };

    // 2. 检查不与下一个区域冲突
    if let Some(next) = vmp.regions().find_greater(vr_vaddr) {
        if next.vaddr < offset {
            return Err(BrkError::WouldOverlap);
        }
    }

    let extralen = VirBytes(offset.0 - vr_end.0);

    // 3. 两条路径
    if has_resize {
        // 路径 A：有 on_resize 回调，扩展现有区域
        extend_existing_region(vmp, vr_vaddr, offset, extralen, page_alloc)
    } else {
        // 路径 B：无 on_resize 回调，创建新的匿名区域
        create_new_anon_region(vmp, vr_end, extralen, page_alloc)
    }
}
```

### 4.4 路径 A：扩展现有区域

```rust
/// 扩展现有区域（有 on_resize 回调）
///
/// 对应 Minix3 的 realloc + anon_resize 路径。
fn extend_existing_region(
    vmp: &mut ActiveProc<'_>,
    vr_vaddr: VirBytes,
    new_end: VirBytes,
    extralen: VirBytes,
    page_alloc: &mut VmPageAllocator,
) -> Result<VirBytes, BrkError> {
    const PAGE_SIZE: u64 = 4096;

    let new_length = VirBytes(new_end.0 - vr_vaddr.0);
    let prev_length = {
        let region = vmp.regions().find(vr_vaddr)
            .ok_or(BrkError::InternalError)?;
        region.length
    };

    // 扩展 physblocks 数组
    let added_pages = ((new_length.0 - prev_length.0) / PAGE_SIZE) as usize;
    {
        let region = vmp.regions_mut().find_mut(vr_vaddr)
            .ok_or(BrkError::InternalError)?;

        // 扩展 Vec，新槽位填充 None
        region.physblocks.resize(
            region.physblocks.len() + added_pages,
            None,
        );

        // 调用 memtype 的 on_resize 回调
        if let Some(memtype) = region.def_memtype {
            memtype.on_resize(vmp, region, new_length)
                .map_err(|_| BrkError::ResizeFailed)?;
        }
    }

    // 更新内存统计
    vmp.add_total(extralen);

    Ok(new_end)
}
```

### 4.5 AnonymousMemory::on_resize — 实现

当前 `MemType` trait 的 `on_resize` 默认实现是空操作。需要为 `AnonymousMemory` 实现：

```rust
impl MemType for AnonymousMemory {
    fn on_resize(
        &self,
        _proc: &mut ActiveProc<'_>,
        region: &mut crate::region::VirRegion,
        new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        // 收缩：释放多余的物理页
        if new_len.0 < region.length.0 {
            let shrink_offset = new_len;
            let shrink_len = VirBytes(region.length.0 - new_len.0);
            region.free_range(shrink_offset, shrink_len);
        }

        // 扩展或收缩：更新 length
        region.length = new_len;

        Ok(())
    }
}
```

**与 Minix3 的差异**：Minix3 的 `anon_resize` 忽略收缩，Rust 实现支持收缩——调用 `free_range()` 释放多余物理页。

### 4.6 路径 B：创建新的匿名区域

```rust
/// 创建新的匿名区域（无 on_resize 回调时）
///
/// 对应 Minix3 的 map_page_region() 路径。
fn create_new_anon_region(
    vmp: &mut ActiveProc<'_>,
    start: VirBytes,
    length: VirBytes,
    page_alloc: &mut VmPageAllocator,
) -> Result<VirBytes, BrkError> {
    let mut new_region = VirRegion::with_memtype(
        start,
        length,
        VrFlags(VrFlags::WRITABLE | VrFlags::ANON),
        &MEM_TYPE_ANON as &'static dyn MemType,
    );

    // 调用 memtype 的 on_new 回调
    if let Some(memtype) = new_region.def_memtype {
        memtype.on_new(&mut new_region)
            .map_err(|_| BrkError::NewRegionFailed)?;
    }

    vmp.regions_mut().insert(new_region);
    vmp.add_total(length);

    Ok(VirBytes(start.0 + length.0))
}
```

### 4.7 shrink_if_needed — 区域收缩

```rust
/// 如果需要，收缩区域
///
/// Minix3 不实现收缩，Rust 实现支持。
fn shrink_if_needed(
    vmp: &mut ActiveProc<'_>,
    new_end: VirBytes,
    page_alloc: &mut VmPageAllocator,
) -> Result<VirBytes, BrkError> {
    // 查找包含 new_end 的区域
    let (vr_vaddr, vr_length) = {
        let region = vmp.regions().find(new_end)
            .ok_or(BrkError::NoRegionToShrink)?;
        (region.vaddr, region.length)
    };

    let current_end = VirBytes(vr_vaddr.0 + vr_length.0);

    if new_end >= current_end {
        return Ok(current_end);  // 无需收缩
    }

    let shrink_len = VirBytes(current_end.0 - new_end.0);

    // 调用 memtype 的 on_resize 回调处理收缩
    {
        let region = vmp.regions_mut().find_mut(vr_vaddr)
            .ok_or(BrkError::InternalError)?;

        if let Some(memtype) = region.def_memtype {
            memtype.on_resize(vmp, region, VirBytes(new_end.0 - vr_vaddr.0))
                .map_err(|_| BrkError::ResizeFailed)?;
        } else {
            // 无回调：手动释放物理页 + 缩减 length
            region.free_range(new_end, shrink_len);
            region.length = VirBytes(new_end.0 - vr_vaddr.0);
        }
    }

    // 更新页表：取消收缩区域的映射
    {
        let pt = vmp.page_table_mut();
        let mut addr = new_end.0;
        while addr < current_end.0 {
            let _ = pt.unmap(VirBytes(addr));
            addr += 4096;
        }
    }

    // 刷新 TLB
    unsafe { vmp.page_table_mut().flush_tlb(); }

    // 更新内存统计
    vmp.sub_total(shrink_len);

    Ok(new_end)
}
```

### 4.8 辅助函数

```rust
const PAGE_SIZE: u64 = 4096;

fn page_align_up(addr: VirBytes) -> VirBytes {
    VirBytes((addr.0 + PAGE_SIZE - 1) & !(PAGE_SIZE - 1))
}

fn page_align_down(addr: VirBytes) -> VirBytes {
    VirBytes(addr.0 & !(PAGE_SIZE - 1))
}
```

### 4.9 错误类型

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrkError {
    InvalidEndpoint,
    ProcessNotActive,
    NoRegionToExtend,
    NoRegionToShrink,
    WouldOverlap,
    OutOfMemory,
    ResizeFailed,
    NewRegionFailed,
    InternalError,
}
```

### 4.10 完整 brk 流程（Rust）

```
VM_BRK IPC 到达
  │
  ▼
do_brk():
  ├── vm_isokendpt() → slot
  ├── get_active(slot) → ActiveProc
  └── real_brk(vmp, new_addr, page_alloc):
       ├── page_align_up(new_addr) → offset
       ├── regions.find_less(offset) → 堆区域
       │
       ├── offset <= current_end → shrink_if_needed():
       │    ├── regions.find(new_end) → 包含区域
       │    ├── memtype.on_resize(vmp, region, new_length):
       │    │    └── free_range(new_end, shrink_len) + region.length = new_length
       │    ├── pt.unmap() 取消收缩区域映射
       │    ├── pt.flush_tlb()
       │    └── sub_total(shrink_len)
       │
       └── offset > current_end → extend_region_upto():
            ├── find_greater() → 检查不冲突
            │
            ├── has_resize → extend_existing_region():
            │    ├── physblocks.resize(len + added, None)
            │    ├── memtype.on_resize(vmp, region, new_length):
            │    │    └── region.length = new_length
            │    └── add_total(extralen)
            │
            └── !has_resize → create_new_anon_region():
                 ├── VirRegion::with_memtype(start, len, WRITABLE|ANON, ANON)
                 ├── memtype.on_new(&mut new_region)
                 ├── regions.insert(new_region)
                 └── add_total(extralen)
```

---

## 5. brk 与缺页的协作

### 5.1 延迟分配的完整链路

brk 扩展区域后，新页没有物理内存。当用户首次访问这些页时：

```
brk(0x500000) → 区域扩展到 0x500000
  │
  │  （新页 0x400000-0x500000 无物理映射）
  │
  ▼  用户写入 0x420000
CPU #PF (P=0, 页不存在)
  │
  ▼
do_pagefaults() → handle_pagefault():
  ├── regions.find(0x420000) → 找到堆区域
  ├── physblocks[page_idx] → None（未分配）
  ├── 创建 PhysRegion + PhysBlock(MAP_NONE)
  ├── memtype.on_pagefault() → NeedNewPage
  ├── page_alloc.alloc_phys(1) → new_phys
  ├── pr.set_phys_addr(new_phys)
  └── write_pt_single(vmp, region, pr) → 页表映射
```

### 5.2 brk 扩展后的区域状态

```
brk 前:
  VirRegion { vaddr: 0x200000, length: 0x200000 }
  physblocks: [Some(pr0), Some(pr1), ..., Some(pr7)]  ← 8 页，全部有物理页
  页表: 0x200000-0x3FF000 全部映射

brk(0x500000) 后:
  VirRegion { vaddr: 0x200000, length: 0x300000 }     ← length 增加
  physblocks: [Some(pr0), ..., Some(pr7), None, None, None, None]  ← 新页为 None
  页表: 0x200000-0x3FF000 映射，0x400000-0x4FF000 未映射

首次访问 0x420000 后:
  VirRegion { vaddr: 0x200000, length: 0x300000 }
  physblocks: [Some(pr0), ..., Some(pr7), None, Some(pr9), None, None]  ← pr9 已分配
  页表: 0x200000-0x3FF000 映射，0x420000 映射，其余未映射
```

### 5.3 brk 收缩后的区域状态

```
brk 前:
  VirRegion { vaddr: 0x200000, length: 0x300000 }
  physblocks: [Some(pr0), ..., Some(pr11)]  ← 12 页
  页表: 0x200000-0x4FF000 全部映射

brk(0x400000) 后:
  VirRegion { vaddr: 0x200000, length: 0x200000 }     ← length 减少
  physblocks: [Some(pr0), ..., Some(pr7)]              ← 8 页（Vec 已 resize）
  页表: 0x200000-0x3FF000 映射，0x400000-0x4FF000 已 unmap
  物理页: pr8-pr11 的引用计数减少，refcount==0 的已释放
```

---

## 6. 实现清单

### 6.1 需要修改的现有代码

| 文件 | 修改内容 | 优先级 |
|------|---------|--------|
| `memtype.rs` | `AnonymousMemory::on_resize()` 实现 | 🔴 P0 |
| `region/vir_region.rs` | 新增 `set_length()` 方法 | 🟡 P1 |

### 6.2 需要新增的代码

| 文件 | 新增内容 | 优先级 |
|------|---------|--------|
| `brk.rs` (新) | `do_brk()`, `real_brk()`, `extend_region_upto()` | 🔴 P0 |
| `brk.rs` | `shrink_if_needed()`, `extend_existing_region()`, `create_new_anon_region()` | 🔴 P0 |
| `brk.rs` | `BrkError` 错误类型 | 🔴 P0 |
| `brk.rs` | `page_align_up()`, `page_align_down()` 辅助函数 | 🟡 P1 |

### 6.3 测试计划

| 测试 | 描述 |
|------|------|
| `test_brk_extend_basic` | 基本堆扩展 |
| `test_brk_extend_no_op` | 地址在现有范围内，无操作 |
| `test_brk_shrink_basic` | 基本堆收缩，物理页释放 |
| `test_brk_shrink_then_extend` | 收缩后再扩展 |
| `test_brk_page_align` | 非页对齐地址自动对齐 |
| `test_brk_overlap_check` | 扩展到下一个区域时返回错误 |
| `test_brk_demand_paging` | 扩展后首次访问触发缺页分配 |
| `test_brk_total_tracking` | vm_total 统计正确更新 |

---

## 7. 与 21-vm-exit 的闭环

21-vm-exit 释放进程的所有内存，包括 brk 扩展的堆区域。两者形成闭环：

```
brk 扩展: add_total(extralen) + region.length += extralen
  │
  │  （进程运行，使用堆内存）
  │
  ▼
exit 释放: RegionAvl::free_all() → unlink_from_block() → free_phys()
  └── sub_total 由 clear() 中的 vm_total = default 隐式处理
```

brk 的延迟分配意味着退出时可能有些 physblocks 是 None（从未访问），这些不需要释放物理页。只有实际分配了物理页的 PhysRegion 才需要 `unlink_from_block()` + `free_phys()`。

这就是 brk 和 exit 的协作：brk 承诺虚拟地址空间，exit 兑现物理内存回收。
