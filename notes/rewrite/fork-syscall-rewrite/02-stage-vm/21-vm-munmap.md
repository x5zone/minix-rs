# 21-vm-munmap: VM_MUNMAP / MAP_PHYS / UNMAP_PHYS / SHM_UNMAP

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → CALLMAP → `VM_MUNMAP`/`MAP_PHYS`/`UNMAP_PHYS`/`SHM_UNMAP`
> **源码**: `minix3/minix/servers/vm/mmap.c:488-573`、`region.c:map_unmap_*`、`mem_directphys.c`
> **Rust 模块**: `munmap.rs`、`map_phys.rs`
> **draft 素材**: `draft/19-vm-munmap.md`（素材）

## 核心点

- `do_munmap`/`munmap_vm_lin`、`map_unmap_range/region`
- `do_unmap_phys`、`VM_SHM_UNMAP`、`phys_setphys` 调用面

## 边界

- **前置依赖**: 13/20
- **不覆盖（移交）**: mmap 建立（20）
