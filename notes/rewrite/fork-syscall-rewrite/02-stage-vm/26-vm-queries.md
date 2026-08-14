# 26-vm-queries: 查询类服务

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → CALLMAP → `INFO`/`GETPHYS`/`GETREF`/`GETRUSAGE`/region_info/usage
> **源码**: `minix3/minix/servers/vm/utility.c:do_info` 等、`mmap.c:do_get_*`、`region.c:get_*`
> **Rust 模块**: `query.rs`
> **draft 素材**: `draft/22-vm-queries.md`（素材）

## 核心点

- `do_info`/`do_get_phys`/`do_get_refcount`/`do_getrusage`
- `get_usage_info`/`get_usage_info_kernel`/`get_region_info`
- `map_get_phys`/`map_get_ref`

## 边界

- **前置依赖**: 13/15
- **不覆盖（移交）**: 区域生命周期（13）
