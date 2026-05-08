# 15-cow-mechanism.md Review Todo

## Document Review
- 文档无旧式 `*mut` 语法，无需修改

## Rust Code Review
- `on_pagefault` 逻辑正确：无物理块→NeedNewPage, refcount<2或非写→Handled, 不可写→AccessViolation, 否则→NeedCow
- `prepare_cow` 为占位实现（识别需设只读的页面但未实际修改页表），文档已说明
- `mem_cow`（分配新页+复制数据+更新引用）尚未实现，属于后续阶段

## 已知设计差异（Allowed Evolution，不修改）
- `prepare_cow` 未实际操作页表（待页表模块完善后实现）
- `mem_cow` 未实现（CoW 实际执行逻辑待后续阶段）
- 页表只读标志设置待 `PageFlags` 模块完善

## Ground Truth 验证
- Minix3 `anon_pagefault`: refcount<2或非写→不触发CoW ✓
- Minix3 `mem_cow`: 分配新页+复制+更新引用（待实现）
- Minix3 `map_writept`: 设置只读页表项（待实现）
