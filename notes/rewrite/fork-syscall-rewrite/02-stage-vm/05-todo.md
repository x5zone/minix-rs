# 05-todo: Review 修复记录

## 审查文件
`05-vm-allocpage.md`

## 审查结果

### P0 修复

#### 1. ReservedRegion 删除冗余的 high_watermark 字段

**问题**: 文档 §4.2 方案四标注明确说"删除 `high_watermark` 字段——它只为 `alloc_contig_virt` 服务，bitmap 分配从 slot 0 开始搜索即可"。但 Rust 代码 `alloc_page.rs` 中 `ReservedRegion` 仍保留 `high_watermark` 字段，且 `alloc_page()` 仍从 `high_watermark` 开始搜索。

**分析**: 方案四中 `alloc_contig_virt()` 已删除，`high_watermark` 始终为 0，搜索总是从 slot 0 开始。`high_watermark` 是方案三的遗留，在方案四中完全冗余。

**修复**: 从 `ReservedRegion` 结构体中删除 `high_watermark` 字段，`alloc_page()` 简化为直接从 bitmap 的 `trailing_zeros()` 搜索空闲位。

**文件**: `os/servers/vm/src/alloc_page.rs`

**验证**: `cargo check` 编译通过。

### 文档审查

1. **文档 §4.2 方案三代码示例**：保留作为设计演进记录，方案四标注清晰。代码已实现方案四，文档与代码最终状态一致。
2. **文档 §4.3 PtRegion**：方案三的 PtRegion 代码示例保留作为设计演进记录。代码中 `pt_region.rs` 仍存在但方案四不使用。
3. **文档 §3.7 方案四**：完整描述了方案四的核心变化，与实际代码一致。
4. **Minix3 源码引用**：`pagetable.c:333` vm_allocpages、`pagetable.c:328` pt_init_done、`pagetable.c:494-523` pt_ptalloc — 全部验证通过。
5. **VmPageAllocator**：代码已是方案四单阶段设计（无 Bootstrap/Normal），与文档 §3.7.5 一致。
6. **alloc_page()**：代码已实现 `alloc_phys() → vm_phys_to_virt(phys)`，与文档 §3.7.1 一致。

### 未修复项（P2，记录备查）

1. **pt_region.rs 仍存在**：方案四中 PtRegion 不再使用，但 `pt_region.rs` 文件仍存在于代码中。应考虑删除或标注为 deprecated。
2. **ReservedRegion::alloc_page 返回类型**：文档方案四标注说"返回类型简化为 `PhysBytes`"，但实际代码仍返回 `(VirBytes, PhysBytes)`。这是合理的——调用者仍需要 VA，只是 VA 的获取方式从 PtRegion 变为 `vm_phys_to_virt()`。文档标注可进一步澄清。
