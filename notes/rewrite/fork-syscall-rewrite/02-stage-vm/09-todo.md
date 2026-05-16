# 09-todo: 搬迁的必要性 TODO

> 生成日期：2026-05-16
> 修订日期：2026-05-16（review 后修正）
> 背景：发现 09 文档的三阶段描述存在根本性错误——将搬迁动机误述为"Direct Map 扩展"，
> 而实际动机是"消除 BumpBuf 的连续 PA 约束"。同时，搬迁的 Rust 代码尚未实现。

## 概念模型

### BumpBuf 的连续 PA 约束

当前 `create_default_allocator()` 从 `free_regions[0]` 的 Direct Map 区域分配元数据：

```
free_regions[0].base → [bitmap | page_cache | ... ] → Direct Map VA
                      ↑ 连续 PA（BumpBuf 要求）  ↑ VA = PA + BASE（VA 连续性跟随 PA 连续性）
```

这是自举阶段的硬性约束：HeapArena 就位之前，VM 没有任何机制将碎片化的物理页缝合为连续 VA。

### 搬迁的本质

搬迁不是"Direct Map 扩展"（那是 Phase 1 的子问题），而是**消除 BumpBuf 的连续 PA 约束**：

```
搬迁前: BumpBuf 元数据占据 free_regions[0] 开头的连续 PA 页（永久占用，不可释放）
搬迁后: 元数据迁移到 HeapArena（碎片化 PA + 连续 VA），原来的连续 PA 页释放回分配器
```

### 正确的三阶段模型

| 阶段 | 核心约束 | 状态 |
|------|---------|------|
| Phase 1（自举） | BumpBuf 要求连续 PA，元数据占据 free_regions[0] | 临时方案，有硬性约束 |
| Phase 2（搬迁） | 元数据从 BumpBuf 迁移到 HeapArena，释放连续 PA | 约束消除 |
| Phase 3（可选升级） | 分配器策略选择（bitmap → buddy 等） | 策略选择，非必需 |

Direct Map 扩展是 Phase 1 的子问题（如果 PA > 1GB，需要扩展 Direct Map 才能让 BumpBuf 访问更多物理页），但不是搬迁的核心动机。

### 关键设计约束

**搬迁不依赖 GlobalAlloc 分配元数据**。新元数据空间通过 `HeapArena::grow()` 分配（逐页映射物理页），不通过 `Box::new()` 或 `Vec::new()`。但搬迁期间 GlobalAlloc **已可用**（`register_page_alloc()` + `init_vm_self_pt()` 在搬迁前完成），因此 `reloc_array_info()` 返回的元数据信息可以存入栈上的固定大小数组——调用方知道 BitmapAllocator 恰好有 2 个数组（bitmap + page_cache），不需要动态容器。

**搬迁后无代码通过 Direct Map VA 访问旧元数据位置**。`BitmapAllocator` 通过 `self.bitmap` 和 `self.page_cache` slice 访问元数据，搬迁只需更新这两个 slice 指针。搬迁后旧 VA 不再被任何代码引用，旧 PA 页可以安全释放。

**`virt_to_phys()` 将旧 VA 转回 PA**。搬迁后释放旧 PA 页时，需要知道旧 metadata buffer 的物理地址。当前 `create_default_allocator()` 中 `meta_phys_base` 和 `meta_pages` 是局部变量——需要在 `BitmapAllocator` 中记录这些值，供搬迁时使用。同时 `virt_to_phys()`（[direct_map.rs](file:///workspace/os/servers/vm/src/direct_map.rs#L31)）可以将旧 VA 转回 PA，双重确认。

**搬迁在 `init()` 中作为 Phase 2（或 phase0）执行**。`VmServer::new()` 已完成 `register_page_alloc()` + `init_vm_self_pt()`，搬迁紧接其后：

```rust
pub fn new(...) -> Self { ... }     // 创建分配器 + 注册 GlobalAlloc + init_vm_self_pt
pub fn relocate(&mut self) { ... }  // 搬迁元数据：BumpBuf → HeapArena
pub fn init(&mut self) { ... }      // 正常初始化（搬迁后的状态）
```

`relocate()` 是独立步骤，与 `init()` 分离。搬迁后 `init()` 中不再有连续 PA 约束。

---

## TODO 列表

### R0: 代码 — 实现搬迁

**R0-1**: `BitmapAllocator` 添加搬迁接口

采用分步查询设计（`reloc_array_count()` + `reloc_array_info()` + `update_relocated_arrays()`），与 09 文档 §4.3 现有设计一致，避免在 trait 方法中使用 Vec（保持 trait 不依赖 alloc crate）：
- 实现 `PhysAllocator::reloc_array_count()` → 返回 `2`（bitmap + page_cache）
- 实现 `PhysAllocator::reloc_array_info(index)` → 返回 `(ptr, elem_count, elem_size)`
- 实现 `PhysAllocator::update_relocated_arrays(new_ptrs)` → 更新 `self.bitmap` 和 `self.page_cache` slice
- 添加 `fn metadata_pa_range(&self) -> (u64, usize)` → 返回旧元数据占据的 PA 基地址和页数（供搬迁后释放用）。当前 `create_default_allocator()` 中 `meta_phys_base` 和 `meta_pages` 是局部变量，需要保存到 `BitmapAllocator` 字段中。
- 位置：`os/servers/vm/src/phys_mem/bitmap_alloc.rs`

**R0-2**: `PhysAllocator` trait 添加搬迁方法

沿用 09 文档 §4.3 已有接口设计，避免 Vec 依赖：
```rust
pub trait PhysAllocator {
    // ... existing methods ...

    fn reloc_array_count(&self) -> usize { 0 }
    fn reloc_array_info(&self, _index: usize) -> (*const u8, usize, usize) {
        (core::ptr::null(), 0, 0)
    }
    fn update_relocated_arrays(&mut self, _new_ptrs: &[*mut u8]) {}
}
```
- 默认实现返回空——不支持搬迁的分配器（如测试 mock）无需实现
- 位置：`os/servers/vm/src/phys_mem/alloc_trait.rs`

**R0-2b**: `PhysAlloc` 枚举转发搬迁方法

当前 `PhysAllocator` trait 的所有方法都通过 `PhysAlloc` 枚举的 match 转发（[mod.rs:L83-L120](file:///workspace/os/servers/vm/src/phys_mem/mod.rs#L83-L120)）。新增的三个搬迁方法也需要在此处添加 match 转发：
```rust
impl PhysAllocator for PhysAlloc {
    fn reloc_array_count(&self) -> usize {
        match self {
            PhysAlloc::Bitmap(b) => b.reloc_array_count(),
            #[cfg(feature = "buddy_alloc")]
            PhysAlloc::Buddy(b) => b.reloc_array_count(),
            #[cfg(feature = "segment_tree_alloc")]
            PhysAlloc::SegmentTree(s) => s.reloc_array_count(),
        }
    }
    // ... reloc_array_info, update_relocated_arrays 同理 ...
}
```
- 位置：`os/servers/vm/src/phys_mem/mod.rs`

**R0-3**: `VmServer` 添加搬迁步骤

搬迁作为独立方法 `relocate()`，在 `new()` 之后、`init()` 之前调用：

```rust
impl VmServer {
    pub fn relocate(&mut self) {
        let phys_alloc = &mut self.page_alloc.phys_alloc_mut();
        let count = phys_alloc.reloc_array_count();
        if count == 0 { return; }

        // 1. 收集旧元数据信息（栈上固定大小数组，不依赖堆分配）
        let mut old_ptrs: [(*const u8, usize); 4] = [(core::ptr::null(), 0); 4];
        let mut total_bytes = 0usize;
        for i in 0..count {
            let (ptr, elem_count, elem_size) = phys_alloc.reloc_array_info(i);
            let bytes = elem_count * elem_size;
            old_ptrs[i] = (ptr, bytes);
            total_bytes += bytes;
        }

        // 2. 通过 HeapArena 分配新 VA 空间
        let pages = (total_bytes + CLICK_SIZE - 1) / CLICK_SIZE;
        let new_va = HEAP_ARENA.grow(pages, &mut self.page_alloc)
            .expect("relocate: HeapArena::grow failed");

        // 3. memcpy 旧数据到新位置
        let mut offset = 0usize;
        let mut new_ptrs: [*mut u8; 4] = [core::ptr::null_mut(); 4];
        for i in 0..count {
            let (old_ptr, bytes) = old_ptrs[i];
            let dst = unsafe { (new_va as *mut u8).add(offset) };
            unsafe { core::ptr::copy_nonoverlapping(old_ptr, dst, bytes); }
            new_ptrs[i] = dst;
            offset += bytes;
        }

        // 4. 更新 PhysAllocator 内部指针
        phys_alloc.update_relocated_arrays(&new_ptrs[..count]);

        // 5. 释放旧 PA 页
        //    通过 virt_to_phys() 将旧 VA 转回 PA，调用 free_mem() 释放
        let old_va = old_ptrs[0].0 as u64;  // bitmap 的起始 VA（即 metadata buffer 起始）
        let old_pa = virt_to_phys(VirBytes(old_va));
        let meta_pages = /* 从 BitmapAllocator 获取 */;
        phys_alloc.free_mem(old_pa, meta_pages);
    }
}
```

- 搬迁时机：`VmServer::new()` 之后独立调用，或作为 `init()` 的 phase0
- 搬迁前确保：`register_page_alloc()` + `init_vm_self_pt()` 已完成（在 `new()` 中）
- 搬迁后验证：`BitmapAllocator` 的 `self.bitmap` 和 `self.page_cache` slice 指向新位置，无代码通过 Direct Map VA 访问旧元数据
- 位置：`os/servers/vm/src/vm_server.rs`

**R0-3b**: `BitmapAllocator` 记录 metadata PA 范围

当前 `create_default_allocator()` 中 `meta_phys_base` 和 `meta_pages` 是局部变量，搬迁后需要这些值来释放旧 PA 页。添加 `metadata_pa_range()` 方法或字段：
```rust
impl BitmapAllocator {
    fn metadata_pa_range(&self) -> (u64, usize) { /* 返回 (pa_base, pages) */ }
}
```
- 在 `init()` 中保存 `meta_phys_base` 和 `meta_pages` 到字段
- 搬迁时通过此方法获取旧 PA 范围，调用 `free_mem()`
- 位置：`os/servers/vm/src/phys_mem/bitmap_alloc.rs`

**R0-4**: 搬迁测试
- 测试搬迁后 bitmap 数据一致性（alloc/free 行为与搬迁前一致）
- 测试搬迁后 alloc/free 仍然正确
- 测试搬迁后旧 PA 页被释放（free_pages 增加 meta_pages）
- 测试搬迁后 `self.bitmap` 和 `self.page_cache` slice 指向 HeapArena VA 范围
- 位置：`os/servers/vm/src/vm_server.rs` tests 模块

**R0-5**: 代码注释语言审查
- review.md 规则：Rust 代码注释应为英文
- 检查所有新增和修改的 .rs 文件，确保无中文注释
- 位置：所有 `os/servers/vm/src/` 下的 .rs 文件

### R1: 文档 — 09-vm-relocation.md 重写

**R1-1**: 重写 §1 里程碑定位和三阶段描述
- 将"Phase 1 并非临时方案"改为"Phase 1 是临时方案，BumpBuf 有连续 PA 硬性约束"
- 将"Phase 2 属于能力扩展而非缺陷补救"改为"Phase 2 是约束消除，搬迁的核心动机是消除 BumpBuf 的连续 PA 约束"
- 将 09 之前/之后的描述从"direct map 覆盖全部物理内存"改为"元数据从 BumpBuf 迁移到 HeapArena，连续 PA 约束消除"
- 位置：09-vm-relocation.md L7-26

**R1-2**: 重写 §1.1 方案四视角
- 当前描述"VA 分配步骤消失"过于绝对，应限定为"物理页的 VA 分配步骤消失"
- 添加 BumpBuf 连续 PA 约束的讲解
- 添加搬迁后释放连续 PA 的价值说明（DMA 等场景需要连续物理页）
- 位置：09-vm-relocation.md L62-70

**R1-3**: 重写 §1.2 "为什么需要搬迁"
- 当前描述搬迁原因是"BSS 大小固定"、"liveupdate 物理地址变化"——这是 Minix3 的动机
- Rust 版本的搬迁动机是"消除 BumpBuf 的连续 PA 约束"
- 添加对比：Minix3 搬迁的是页表结构，Rust 版本搬迁的是分配器元数据（bitmap/page_cache）
- 位置：09-vm-relocation.md L72-88

**R1-4**: 重写 §1.4 方案四时序
- Phase 2 应为"搬迁"而非"Direct Map 扩展"
- 添加搬迁步骤（HeapArena::grow + memcpy + update_ptrs + virt_to_phys + free_mem）
- T3 当前描述"bitmap.alloc_phys() 分配新页表页 → 扩展 direct map"应删除——这是 Direct Map 扩展逻辑，不是搬迁逻辑
- 位置：09-vm-relocation.md L140-180

**R1-5**: 重写 §3.1 搬迁策略选择
- 策略一（原地升级）的缺点需要补充"连续 PA 约束无法消除"
- 策略二（复制搬迁）的优点需要补充"释放连续 PA 页"
- 选择理由中补充：搬迁后连续 PA 页可被 DMA 等场景使用
- 位置：09-vm-relocation.md L1030-1125

**R1-6**: 重写 §4 实现详解（完全替换）

当前 §4.1~§4.6（L1159-1404）的代码全部基于 **PtRegion + ReservedRegion + Typestate（Bootstrap/Normal）** 架构，这些在当前代码中**已不存在**。需要：
- 删除 §4.1~§4.6 全部现有代码示例
- 删除 §4.2 的 `PtRegion::relocate_phys_allocator()` 实现
- 删除 §4.4 的 Typestate 参数 `impl PhysAllocator for BitmapAllocator`
- 替换为基于 HeapArena + `vm_self_mappages()` + `virt_to_phys()` 的搬迁实现
- 搬迁接口采用 `reloc_array_count()` / `reloc_array_info()` / `update_relocated_arrays()` 设计（保留 §4.3 的接口风格）
- 添加搬迁时序图
- 位置：09-vm-relocation.md L1159-1404

**R1-7**: 重写 §4.6 搬迁的完整调用链
- 删除方案三的 `ReservedRegion` + `PtRegion` + Typestate 调用链
- 替换为 Direct Map + HeapArena 的调用链：`VmServer::relocate() → reloc_array_info() → HeapArena::grow() → memcpy → update_relocated_arrays() → virt_to_phys() → free_mem()`
- 位置：09-vm-relocation.md L1352-1404

### R2: 文档 — 04-physical-memory.md 修正

**R2-1**: §4.1 BumpBuf 设计约束补充搬迁后语义
- 已在之前迭代中添加"连续物理页约束"
- 需要补充：搬迁后此约束被消除——元数据从 BumpBuf 迁移到 HeapArena，连续 PA 页被释放回分配器
- 位置：04-physical-memory.md §4.1

### R3: 文档 — 05-vm-allocpage.md 修正

**R3-1**: §4.3 初始化时序补充搬迁步骤
- 当前时序在 T5 后直接进入主循环
- 需要在 T5（首次堆分配）和主循环之间添加搬迁步骤：元数据从 BumpBuf 迁移到 HeapArena
- 位置：05-vm-allocpage.md §4.3

### R4: 文档 — 06-pagetable-struct.md 修正

**R4-1**: L825 ReservedRegion 脚注修正
- 当前："后者因 Direct Map 而消除"
- 应补充：ReservedRegion 的 VA 分配职责因 Direct Map 消除，但物理页预留的元数据存储仍需搬迁（从 BumpBuf 到 HeapArena），搬迁后连续 PA 页被释放
- 位置：06-pagetable-struct.md L825

### R5: 文档 — 07-pagetable-ops.md 修正

**R5-1**: §3.0.4 补充搬迁说明
- 当前已修正为"VM 的堆为何不由 brk 驱动"
- 需补充：自举阶段 BumpBuf 的连续 PA 约束，以及搬迁如何消除此约束——元数据从 Direct Map 区域迁移到 HeapArena，释放连续 PA
- 位置：07-pagetable-ops.md §3.0.4

### R6: 文档 — 08-slab-allocator.md 修正

**R6-1**: §4.1 BumpBuf vs HeapArena 对比补充搬迁维度
- 当前对比表有 7 个维度
- 需添加"搬迁后"维度：BumpBuf 元数据迁移到 HeapArena，释放连续 PA
- 补充：搬迁后 Layer 2 的元数据从 Direct Map（连续 PA）转移到 HeapArena（碎片化 PA + 连续 VA），旧的连续 PA 页释放回分配器
- 位置：08-slab-allocator.md §4.1

**R6-2**: 附录 A.2 设计层次补充搬迁说明
- Layer 2 当前为"Direct Map（物理页可达性）+ HeapArena（虚拟连续性）"
- 补充搬迁在此层次中的位置：搬迁是 Layer 2 内部的状态转换——自举阶段元数据通过 Direct Map 可达，搬迁后元数据通过 HeapArena 可达，同时释放 Direct Map 中占据的连续 PA 页
- 注意：不是新增"Layer 2.5"，而是在 Layer 2 说明中补充"搬迁前/搬迁后"的差异
- 位置：08-slab-allocator.md 附录 A.2

---

## 依赖链

```
R0-1 (BitmapAllocator 搬迁接口 + metadata_pa_range)
  └─→ R0-2 (PhysAllocator trait 搬迁方法)
       └─→ R0-2b (PhysAlloc 枚举转发)
            └─→ R0-3 (VmServer 搬迁步骤 + relocate() 方法)
                 └─→ R0-3b (metadata PA 范围记录)
                      └─→ R0-4 (搬迁测试)
                           └─→ R0-5 (注释语言审查)

R1-1 ~ R1-7 (09 文档重写) — 可与 R0 并行

R2-1 (04 文档) — 依赖 R0-3 完成（需要实际代码作为文档依据）
R3-1 (05 文档) — 依赖 R0-3
R4-1 (06 文档) — 独立
R5-1 (07 文档) — 独立
R6-1, R6-2 (08 文档) — 独立
```

## 优先级

| 优先级 | TODO | 说明 |
|--------|------|------|
| P0 | R0-1, R0-2, R0-2b, R0-3, R0-3b | 搬迁是核心功能缺口 |
| P0 | R0-4 | 搬迁测试 |
| P0 | R1-1 ~ R1-7 | 09 文档概念错误必须修正 |
| P1 | R0-5 | 代码注释语言合规 |
| P1 | R2-1 | 04 文档搬迁相关修正 |
| P1 | R3-1 | 05 文档时序修正 |
| P2 | R4-1, R5-1, R6-1, R6-2 | 06/07/08 文档补充说明 |