# 11-phys-pagestate: 物理页状态（phys_block / phys_region）

> **状态**: pending（最小骨架，待改写）
> **定位**: 地址空间数据结构（region 的物理侧）
> **源码**: `minix3/minix/servers/vm/pb.c`、`region.h`、`phys_region.h`
> **Rust 模块**: `region/page_state.rs`
> **draft 素材**: `draft/10-phys-pagestate.md`（素材）

## 核心点

- `phys_block`/`phys_region` 结构语义
- `pb_new`/`pb_free`/`pb_link`/`pb_reference`/`pb_unreferenced`
- `PBF_*` 页标志

## 边界

- **前置依赖**: 13 概念序（pb 是 region 的物理侧）
- **不覆盖（移交）**: vir_region 映射（13）、CoW 分裂（17）
