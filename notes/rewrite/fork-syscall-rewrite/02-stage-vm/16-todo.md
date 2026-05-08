# 16-pagefault.md Review Todo

## Document Review
- 文档无旧式 `*mut` 语法，无需修改

## Rust Code Review
- `on_pagefault` 逻辑与 Minix3 `anon_pagefault` 一致
- 页错误处理流程与文档描述匹配
- 无需修改代码

## 已知设计差异（Allowed Evolution，不修改）
- `PagefaultResult` 缺少 `NeedAsyncIo`/`Suspended` 变体（待异步 I/O 实现后添加）
- 页错误处理入口（`handle_pagefault`）尚未与内核中断对接

## Ground Truth 验证
- Minix3 `anon_pagefault`: 无物理块→分配, refcount<2或非写→已处理, 不可写→违规, 否则→CoW ✓
- Minix3 `map_pf`: 查找 PhysRegion→调用 memtype handler ✓
