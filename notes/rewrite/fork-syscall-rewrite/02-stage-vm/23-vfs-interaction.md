# 23-vfs-interaction: VFS 异步交互

> **状态**: pending（最小骨架，待改写）
> **定位**: 跨服务协作（VFS ↔ VM 异步对话）
> **源码**: `minix3/minix/servers/vm/vfs.c`（143 行）、`fdref.c`（177 行）、`mem_file.c`（287 行）
> **Rust 模块**: `vfs_queue.rs`、`fdref.rs`
> **draft 素材**: `draft/23-vfs-interaction.md`（素材）

## 核心点

- `vfs_request`/`do_vfs_reply`/`activate` 异步请求队列
- `fdref_new/ref/deref/dedup_or_new`
- `mappedfile_setfile`、`cow_block`（file-backed COW 分裂，mem_file.c:59）

## 边界

- **前置依赖**: 12/13/15/20
- **不覆盖（移交）**: 页缓存（24）
