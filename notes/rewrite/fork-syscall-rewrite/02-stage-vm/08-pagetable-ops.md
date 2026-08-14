# 08-pagetable-ops: 页表操作

> **状态**: pending（最小骨架，待改写）
> **定位**: 服务路径（fork/mmap/pagefault 等经 `pt_*` 操作页表）
> **源码**: `minix3/minix/servers/vm/pagetable.c`
> **Rust 模块**: `pagetable/mod.rs`
> **draft 素材**: `draft/07-pagetable-ops.md`（素材）

## 核心点

- `pt_new`/`pt_free`/`pt_bind`/`pt_writemap`/`pt_ptmap`/`pt_map_in_range`/`pt_mapkernel`/`pt_ptalloc_in_range`/`pt_checkrange`/`pt_clearmapcache`/`pt_ptalloc`
- `pt_copy`、`pt_allocate_kernel_mapped_pagetables`、`pt_writable`
- ARCH A-2 的操作面差异（2 级 → 4 级）

## 边界

- **前置依赖**: 07
- **不覆盖（移交）**: 页分配（06）
