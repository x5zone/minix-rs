# 06-page-allocator: 页分配器

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环前 → `alloc_cycle()` 补保留页池；服务路径 → `vm_allocpage`
> **源码**: `minix3/minix/servers/vm/pagetable.c:295-395`、`alloc.c:reservedqueue_*`
> **Rust 模块**: `alloc_page.rs`、`critical_pool.rs`
> **draft 素材**: `draft/05-vm-allocpage.md`（素材）

## 核心点

- `vm_allocpage`/`vm_allocpages`/`vm_mappages`/`vm_freepages`
- `vm_pagelock`/`vm_addrok`
- 保留页池（spares）、`alloc_cycle`、`get_vm_self_pages`

## 边界

- **前置依赖**: 05
- **不覆盖（移交）**: pt 结构（07）
