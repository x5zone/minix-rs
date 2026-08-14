# 20-vm-mmap: VM_MMAP / VFS_MMAP / REMAP 服务

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → CALLMAP → `VM_MMAP`/`VFS_MMAP`/`REMAP`
> **源码**: `minix3/minix/servers/vm/mmap.c`（573 行）
> **Rust 模块**: `mmap.rs`、`map_phys.rs`
> **draft 素材**: `draft/18-vm-mmap.md`（素材）

## 核心点

- `do_mmap`/`do_vfs_mmap`/`mmap_file`/`mmap_file_cont`（VFS 异步续作）/`mmap_region`
- `map_perm_check`、`do_remap`、`do_map_phys`
- ARCH A-6：64 位地址空间（`MMAP_BASE`/`MMAP_TOP`）

## 边界

- **前置依赖**: 13/15/19
- **不覆盖（移交）**: munmap（21）、map_phys（21）
