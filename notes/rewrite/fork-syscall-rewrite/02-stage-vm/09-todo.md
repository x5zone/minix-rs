# 09-todo: 搬迁的必要性 TODO

> 生成日期：2026-05-16
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

搬迁不是"Direct Map 扩展"（那是 Phase 2 的子问题），而是**消除 BumpBuf 的连续 PA 约束**：

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

---

## TODO 列表

### R0: 代码 — 实现搬迁

**R0-1**: `BitmapAllocator` 添加搬迁接口
- 添加 `reloc_metadata()` 方法：返回 `(bitmap_ptr, bitmap_bytes), (cache_ptr, cache_bytes)` 元数据信息
- 添加 `update_metadata_ptrs()` 方法：更新 bitmap 和 page_cache 的切片指针
- 位置：`os/servers/vm/src/phys_mem/bitmap_alloc.rs`
- 约束：不依赖 GlobalAlloc（搬迁在 VmAllocator 就绪后执行，但不应通过 Box/Vec 分配）

**R0-2**: `PhysAllocator` trait 添加搬迁方法
- 添加 `fn reloc_metadata(&self) -> Vec<(*const u8, usize)>` 或固定数组返回
- 添加 `fn update_metadata_ptrs(&mut self, new_ptrs: &[(*mut u8, usize)])`
- 默认实现返回空（不支持搬迁的分配器无需实现）
- 位置：`os/servers/vm/src/phys_mem/alloc_trait.rs`
- 注意：`Vec` 依赖 GlobalAlloc，搬迁期间可用。如果不想依赖 Vec，可用固定大小数组。

**R0-3**: `VmServer` 添加搬迁步骤
- 在 `init()` 的 Phase 2 中执行搬迁
- 搬迁流程：
  1. 调用 `phys_alloc.reloc_metadata()` 获取旧元数据信息
  2. 通过 HeapArena 分配新 VA 空间（`HeapArena::grow()`）
  3. `memcpy` 旧数据到新位置
  4. 调用 `phys_alloc.update_metadata_ptrs()` 更新指针
  5. 释放旧 PA 页（`phys_alloc.free_mem()` 或 `reserve_pages` 反向操作）
- 位置：`os/servers/vm/src/vm_server.rs`
- 注意：搬迁必须在 `register_page_alloc()` + `init_vm_self_pt()` 之后（HeapArena 依赖这两个）

**R0-4**: 搬迁测试
- 测试搬迁后 bitmap 数据一致性
- 测试搬迁后 alloc/free 仍然正确
- 测试搬迁后旧 PA 页被释放（free_pages 增加）
- 位置：`os/servers/vm/src/vm_server.rs` tests 模块

**R0-5**: 代码注释语言审查
- review.md 规则：Rust 代码注释应为英文
- 检查所有新增和修改的 .rs 文件，确保无中文注释
- 位置：所有 `os/servers/vm/src/` 下的 .rs 文件

### R1: 文档 — 09-vm-relocation.md 重写

**R1-1**: 重写 §1 里程碑定位和三阶段描述
- 将"Phase 1 并非临时方案"改为"Phase 1 是临时方案，BumpBuf 有连续 PA 硬性约束"
- 将"Phase 2 属于能力扩展而非缺陷补救"改为"Phase 2 是约束消除，搬迁的核心动机是消除 BumpBuf 的连续 PA 约束"
- 位置：09-vm-relocation.md L7-26

**R1-2**: 重写 §1.1 方案四视角
- 当前描述"VA 分配步骤消失"过于绝对，应限定为"物理页的 VA 分配步骤消失"
- 添加 BumpBuf 连续 PA 约束的讲解
- 添加搬迁后释放连续 PA 的价值说明
- 位置：09-vm-relocation.md L62-70

**R1-3**: 重写 §3.1 搬迁策略选择
- 策略一（原地升级）的缺点需要补充"连续 PA 约束无法消除"
- 策略二（复制搬迁）的优点需要补充"释放连续 PA 页"
- 位置：09-vm-relocation.md L1030-1125

**R1-4**: 重写 §4 实现详解
- 删除方案三的 PtRegion 搬迁代码（已过时）
- 添加 HeapArena 搬迁的 Rust 实现代码
- 添加搬迁时序图
- 位置：09-vm-relocation.md L1159-1404

**R1-5**: 重写 §1.4 方案四时序
- Phase 2 应为"搬迁"而非"Direct Map 扩展"
- 添加搬迁步骤（HeapArena::grow + memcpy + update_ptrs + free old PA）
- 位置：09-vm-relocation.md L140-180

**R1-6**: 重写 §4.6 搬迁的完整调用链
- 删除方案三的 `ReservedRegion` + `PtRegion` 代码
- 替换为 Direct Map + HeapArena 的搬迁代码
- 位置：09-vm-relocation.md L1352-1404

### R2: 文档 — 04-physical-memory.md 修正

**R2-1**: §4.1 BumpBuf 设计约束补充
- 已在之前迭代中添加"连续物理页约束"
- 需要补充：搬迁后此约束被消除，连续 PA 页被释放
- 位置：04-physical-memory.md §4.1

**R2-2**: §7 对比表修正
- 当前："Direct Map 提供物理页可达性，HeapArena 提供虚拟连续性"
- 需补充搬迁相关行："元数据位置 | BSS 静态数组 | BumpBuf（自举）→ HeapArena（搬迁后）"
- 位置：04-physical-memory.md §7 L1916

### R3: 文档 — 05-vm-allocpage.md 修正

**R3-1**: §4.3 初始化时序补充搬迁步骤
- 当前时序在 T5 后直接进入主循环
- 需要在 T5 和主循环之间添加 T6: 搬迁（relocate_metadata）
- 位置：05-vm-allocpage.md §4.3

### R4: 文档 — 06-pagetable-struct.md 修正

**R4-1**: L825 ReservedRegion 脚注修正
- 当前："后者因 Direct Map 而消除"
- 应补充：ReservedRegion 的 VA 分配职责因 Direct Map 消除，但物理页预留的元数据存储仍需搬迁（从 BumpBuf 到 HeapArena）
- 位置：06-pagetable-struct.md L825

### R5: 文档 — 07-pagetable-ops.md 修正

**R5-1**: §3.0.4 补充搬迁说明
- 当前已修正为"VM 的堆为何不由 brk 驱动"
- 需补充：自举阶段 BumpBuf 的连续 PA 约束，以及搬迁如何消除此约束
- 位置：07-pagetable-ops.md §3.0.4

### R6: 文档 — 08-slab-allocator.md 修正

**R6-1**: §4.1 BumpBuf vs HeapArena 对比补充搬迁维度
- 当前对比表有 7 个维度
- 需添加"搬迁后"维度：BumpBuf 元数据迁移到 HeapArena，释放连续 PA
- 位置：08-slab-allocator.md §4.1

**R6-2**: 附录 A.2 设计层次补充搬迁层
- Layer 2 当前为"Direct Map + HeapArena"
- 需补充搬迁在层次中的位置：Layer 2.5（BumpBuf → HeapArena 迁移）
- 位置：08-slab-allocator.md 附录 A.2

---

## 依赖链

```
R0-1 (BitmapAllocator 搬迁接口)
  └─→ R0-2 (PhysAllocator trait 搬迁方法)
       └─→ R0-3 (VmServer 搬迁步骤)
            └─→ R0-4 (搬迁测试)
                 └─→ R0-5 (注释语言审查)

R1-1 ~ R1-6 (09 文档重写) — 可与 R0 并行

R2-1, R2-2 (04 文档) — 依赖 R0-3 完成（需要实际代码作为文档依据）
R3-1 (05 文档) — 依赖 R0-3
R4-1 (06 文档) — 独立
R5-1 (07 文档) — 独立
R6-1, R6-2 (08 文档) — 独立
```

## 优先级

| 优先级 | TODO | 说明 |
|--------|------|------|
| P0 | R0-1 ~ R0-4 | 搬迁是核心功能缺口 |
| P0 | R1-1 ~ R1-6 | 09 文档概念错误必须修正 |
| P1 | R0-5 | 代码注释语言合规 |
| P1 | R2-1, R2-2 | 04 文档搬迁相关修正 |
| P1 | R3-1 | 05 文档时序修正 |
| P2 | R4-1, R5-1, R6-1, R6-2 | 06/07/08 文档补充说明 |
