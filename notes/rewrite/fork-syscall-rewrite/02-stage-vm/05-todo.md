# 05-vm-allocpage.md 变更记录

## P0 修复（关键错误）

### P0-1: §4.2 ReservedRegion 缺少 high_watermark 字段
- **问题**: 文档中 ReservedRegion 结构体缺少 `high_watermark: usize` 字段
- **实际代码**: `pt_region.rs` 中 ReservedRegion 包含 `high_watermark: usize` 字段，位于 `allocated_pages` 和 `bitmap` 之间
- **修复**: 在结构体定义中添加 `high_watermark: usize` 字段

### P0-2: §4.2 ReservedRegion::alloc_page 实现不正确
- **问题**: 文档使用 `(!self.bitmap).trailing_zeros()` 从 bit 0 开始搜索空闲位，未使用 high_watermark
- **实际代码**: 使用 `let start = self.high_watermark;` 和 `let mask = !self.bitmap >> start;` 从高水位标记开始搜索
- **修复**: 替换为实际代码，确保 alloc_page 从 high_watermark 开始搜索，避免与 alloc_contig_virt 的线性分配重叠

### P0-3: §4.2 alloc_contig_virt 签名和实现错误
- **问题**: 签名为 `&self`（不可变借用），使用 `allocated_pages` 计算偏移，无 assert 检查，不递增任何计数器
- **实际代码**: 签名为 `&mut self`，使用 `high_watermark` 计算偏移，包含 `assert!(pages <= self.total_pages - self.high_watermark)`，并执行 `self.high_watermark += pages`
- **修复**: 替换为实际代码

### P0-4: §4.3 PtRegion 缺少 current_pt_base 字段
- **问题**: 文档中 PtRegion 结构体缺少 `current_pt_base: VirBytes` 字段
- **实际代码**: `pt_region.rs` 中 PtRegion 包含 `current_pt_base: VirBytes`，位于 `current_pt` 之后
- **修复**: 在结构体定义中添加 `current_pt_base: VirBytes` 字段

### P0-5: §4.3 PtRegion::virt_to_phys 方法不存在
- **问题**: 文档中为 PtRegion 实现了 `virt_to_phys` 方法，但实际代码中 PtRegion 没有此方法（virt_to_phys 仅存在于 ReservedRegion）
- **修复**: 删除整个 virt_to_phys 代码块及其说明段落

### P0-6: §3.3 into_normal 签名错误
- **问题**: 签名为 `into_normal(self)`，调用 `from_reserved_with_ops` 时使用 `&self.reserved`
- **实际代码**: 签名为 `into_normal(mut self)`，调用时使用 `&mut self.reserved`
- **修复**: 修改签名为 `into_normal(mut self)`，参数改为 `&mut self.reserved`

## P1 修复（重要问题）

### P1-4: §1.3 缺少 VMP_SPARE
- **问题**: reason 分类中未列出 `VMP_SPARE`（值为 0）
- **修复**: 在 reason 分类中添加 `VMP_SPARE`(0)

### P1-5: §4.3 from_reserved_with_ops 参数类型错误
- **问题**: 参数为 `reserved: &ReservedRegion`（不可变借用）
- **实际代码**: 参数为 `reserved: &mut ReservedRegion`（可变借用），因为内部调用了 `alloc_contig_virt` 需要 &mut
- **修复**: 修改参数类型为 `&mut ReservedRegion`

### P1-6: §4.3 PtRegion 初始化缺少 current_pt_base
- **问题**: PtRegion 初始化代码中未设置 `current_pt_base` 字段
- **实际代码**: 初始化时设置 `current_pt_base: start`
- **修复**: 在初始化代码中添加 `current_pt_base: start`

### P1-7: §4.3 expand() 缺少 current_pt_base 更新
- **问题**: expand() 中 `self.current_pt = pt_virt` 之后未更新 `current_pt_base`
- **实际代码**: 在 `self.current_pt = pt_virt` 之后有 `self.current_pt_base = VirBytes(self.start.0 + (pd_idx * 512 * PAGE_SIZE) as u64)`
- **修复**: 在 expand() 中添加 current_pt_base 更新

### P1-8: §4.2 "待修复问题"段落描述已修复的 bug
- **问题**: "待修复问题"和"推荐修复方案"段落描述的 high_watermark 缺失问题在实际代码中已经修复
- **修复**: 删除"待修复问题"和"推荐修复方案"段落，替换为 high_watermark 的设计意图说明（解释低地址区/高地址区的划分，以及 alloc_page 和 alloc_contig_virt 如何通过 high_watermark 互不干扰）

## Ch1/Ch2 Rust 内容检查

- 检查了 §1（概述）和 §2（Minix3 C 源码分析）
- Ch1 和 Ch2 仅包含 C 代码，无 Rust 内容
- 无需移动
