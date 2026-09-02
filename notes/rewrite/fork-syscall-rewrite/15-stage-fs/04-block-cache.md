# 04-block-cache: 块缓存（libminixfs）

> **状态**: pending（最小骨架，待改写）
> **定位**: 磁盘 FS 的缓存基础（mfs/ext2/isofs 共用）
> **源码**: `minix3/minix/lib/libminixfs/cache.c`、`inc.h`
> **Rust 模块**: `minix-fs`（block cache）
> **draft 素材**: 无（新建）

## 核心点

- 缓冲区池：`lmfs_buf_pool`、`nr_bufs`、`buf` 数组、`bufs_in_use`
- `struct buf` 字段（libminixfs.h）：data/lmfs_dev/lmfs_blocknr/lmfs_count/lmfs_bytes/lmfs_flags/lmfs_inode 关联
- hash 表：`buf_hash`、`find_block`、挂链；LRU：front/rear、`rm_lru`/`raisecount`/`lowercount`
- `lmfs_get_block`/`get_block_ino`：NORMAL/NO_READ/PEEK 三种 how；`lmfs_put_block`、freeblock
- dirty 管理：`lmfs_markdirty`/`markclean`/`lmfs_isclean`；写回：`lmfs_flushall`/`flushdev`/`free_unused_blocks`/`rw_scattered`
- readahead：`lmfs_readahead`/`lmfs_readahead_limit`/`sort_blocks`、LMFS_MAX_PREFETCH
- VM 二级缓存：`lmfs_may_use_vmcache`、VMC_* 标志、`lmfs_get_block_ino` 的 inode 关联（依赖 02-stage-vm/24-page-cache）
- 块大小与统计：`lmfs_set_blocksize`/`lmfs_fs_block_size`、`lmfs_set_blockusage`/`change_blockusage`、`cache_resize`/`cache_heuristic_check`
- `lmfs_zero_block_ino`/`lmfs_free_block`/`lmfs_invalidate`：块失效与 VM 通知

## 边界

- **前置依赖**: 01
- **不覆盖（移交）**: 块驱动通信（05）、inode 缓存（09）
