# 17-mfs-maint: MFS sync/统计/常量

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现：维护面
> **源码**: `minix3/minix/fs/mfs/misc.c`、`stats.c`、`const.h`、`clean.h`、`glo.h`
> **Rust 模块**: `os/fs/mfs`（misc）
> **draft 素材**: 无（新建）

## 核心点

- `fs_sync`：脏 inode 写回 + lmfs_flushall（fsync/同步挂载面）
- `count_free_bits`：位图空闲统计（statvfs/block usage 数据源）
- const.h 常量契约：V2_NR_DZONES=7/V2_NR_TZONES=10、NR_INODES=512、SUPER_MAGIC 族、ROOT_INODE=1、BOOT_BLOCK/SUPER_BLOCK_BYTES/START_BLOCK、LOOK_UP/ENTER/DELETE/IS_EMPTY、IN_CLEAN/IN_DIRTY、ATIME/CTIME/MTIME、WMAP_FREE
- clean.h 宏：MARKDIRTY（只读 FS 脏写 → 告警 + 栈回溯）
- glo.h 全局：err_code/fs_dev/used_zones/cch、mfs_table 声明

## 边界

- **前置依赖**: 08
- **不覆盖（移交）**: 各常量使用点（各文档）
