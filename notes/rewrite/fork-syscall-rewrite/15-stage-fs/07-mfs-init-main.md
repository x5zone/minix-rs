# 07-mfs-init-main: MFS 启动链与包装层

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现第 1 篇：server 启动
> **源码**: `minix3/minix/fs/mfs/main.c`、`table.c`、`cache.c`
> **Rust 模块**: `os/fs/mfs`（main）
> **draft 素材**: 无（新建）

## 核心点

- SEF 启动链：`sef_local_startup` → init_fresh（inode 表清零 + cch 清零 → `init_inode_cache` → `lmfs_buf_pool(DEFAULT_NR_BUFS)` → `lmfs_may_use_vmcache(1)`）→ `fsdriver_task(&mfs_table)`
- SEF_CB_INIT_RESTART_STATEFUL + SIGTERM → fs_sync → fsdriver_terminate（graceful shutdown）
- mfs_table 31 项回调全表（mount/unmount/lookup/putnode/read/write/peek/getdents/trunc/seek/create/mkdir/mknod/link/unlink/rmdir/rename/slink/rdlink/stat/chown/chmod/utime/mountpt/statvfs/sync/driver/bread/bwrite/bpeek/bflush）
- cache.c 包装层：`get_block`（lmfs_get_block + I/O 错误 panic 策略 + PEEK ENOENT）、`alloc_zone`（alloc_bit + s_zsearch + 满盘 ENOSPC 告警）、`free_zone`（free_bit + lmfs_free_block 通知）
- glo.h 全局：err_code/fs_dev/used_zones/cch

## 边界

- **前置依赖**: 01/04/05
- **不覆盖（移交）**: super/inode 细节（08/09）、主循环（01）
