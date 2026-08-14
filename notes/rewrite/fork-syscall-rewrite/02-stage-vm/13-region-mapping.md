# 13-region-mapping: 区域映射（vir_region / phys_region）

> **状态**: pending（最小骨架，待改写）
> **定位**: `init_vm()` → `map_region_init()`；服务路径 → `map_*` 操作
> **源码**: `minix3/minix/servers/vm/region.c`（1555 行）、`region.h`
> **Rust 模块**: `region/region_map.rs`、`region/vir_region.rs`
> **draft 素材**: `draft/11-region-mapping.md`（素材）

## 核心点

- `map_region_init`、`map_page_region`、`map_unmap_*`、`map_free(_proc)`
- `map_proc_copy(_range)`、`map_copy_region`（static）、`map_pf`、`map_handle_memory`、`map_pin_memory`、`map_writept`、`map_ph_writept`
- `physblock_get/set`、`map_region_lookup_type`（RS LU 预分配查找）、`vrallocflags`、`physregions`
- 调试打印：`map_printmap`/`printregionstats`

## 边界

- **前置依赖**: 11/12
- **不覆盖（移交）**: 查找索引（14）、查询（26）
