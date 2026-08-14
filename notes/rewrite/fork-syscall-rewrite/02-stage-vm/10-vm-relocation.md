# 10-vm-relocation: VM 自举与元数据搬迁

> **状态**: pending（最小骨架，待改写）
> **定位**: 自举终点（静态元数据 → 动态分配后搬迁）
> **源码**: `minix3/minix/servers/vm/alloc.c`、`pagetable.c`
> **Rust 模块**: `global.rs`、`phys_mem/mod.rs`
> **draft 素材**: `draft/09-vm-relocation.md`（素材）

## 核心点

- 自举搬迁机制：静态 → 动态分配转换
- `swap_proc_slot`/`swap_proc_dyn_data`/`map_proc_dyn_data`（utility.c，Live Update 支撑）
- `transfer_mmap_regions`（utility.c:228）、`map_setparent`

## 边界

- **前置依赖**: 09
- **不覆盖（移交）**: RS/LU 服务流程（25）
