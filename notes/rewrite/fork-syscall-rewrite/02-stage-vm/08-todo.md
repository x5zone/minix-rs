# 08-todo: Review 修复记录

## 审查文件
`08-slab-allocator.md`

## 审查结果

### 无需修复

文档质量极高，Minix3 C 源码引用准确，方案四 Direct Map 变更已完整描述。

### 验证通过项

1. **方案四变更**：文档已完整描述 slab 分配器在 direct map 下的变化：
   - GlobalAlloc 对接 `alloc_phys() + vm_phys_to_virt()`（替代 malloc/free）
   - slab 元数据通过 `vm_phys_to_virt()` 访问（替代 PtRegion 特殊映射路径）
   - 与 05-vm-allocpage.md 的 `alloc_page()` 简化呼应

2. **Rust 代码现状**：`global.rs` 的 `VmAllocator` 仍使用 C 的 `malloc/free`，这是方案三的遗留。方案四要求替换为 `alloc_phys() + vm_phys_to_virt()`，但尚未实现。

### 未修复项（P2，记录备查）

1. **VmAllocator 未切换到方案四**：代码中 `VmAllocator` 仍使用 `malloc/free`，文档方案四要求使用 `alloc_phys() + vm_phys_to_virt()`。这是后续实现任务。
2. **slab 模块未独立**：文档描述了 `global.rs`、`alloc_stats.rs`、`critical_pool.rs`，但代码中没有独立的 slab 模块目录。
