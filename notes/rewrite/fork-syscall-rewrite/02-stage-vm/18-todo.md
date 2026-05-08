# 18-vm-brk.md Review Todo

## Document Review
- 文档无需修改

## Rust Code Review
- brk 实现与 Minix3 `do_brk`/`real_brk` 逻辑一致
- `BrkRequest`/`BrkResponse` 结构体与文档描述匹配
- `HeapState`/`HeapAdjustment` 实现了文档描述的堆扩展和收缩
- `adjust_heap` 正确处理扩展和收缩操作
- 无需修改代码

## 已知设计差异（Allowed Evolution，不修改）
- Rust 使用枚举和 Result 类型替代 Minix3 的错误码
- Rust 实现更模块化（分离为 ipc/brk.rs, handler/brk.rs, process/heap.rs）

## Ground Truth 验证
- Minix3 `do_brk`: 验证调用者→调用 real_brk ✓
- Minix3 `real_brk`: 调用 map_region_extend_upto_v ✓
- Minix3 `map_unmap_region`: 堆收缩取消物理页面映射 ✓
