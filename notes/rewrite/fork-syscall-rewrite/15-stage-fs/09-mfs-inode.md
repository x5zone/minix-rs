# 09-mfs-inode: MFS inode 表与缓存

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现：核心数据结构
> **源码**: `minix3/minix/fs/mfs/inode.c`、`inode.h`、`type.h`
> **Rust 模块**: `os/fs/mfs`（inode）
> **draft 素材**: 无（新建）

## 核心点

- inode 表：NR_INODES=512 静态数组、i_count 引用计数（0=free）、hash（INODE_HASH_SIZE=128）+ unused TAILQ 双索引
- `get_inode`：hash 命中（i_count 0→1 移除 unused 链）/未命中（取 free slot + rw_inode 读盘）、inode_cache_hit/miss 统计
- `put_inode`/`find_inode`/`dup_inode`/`alloc_inode`（alloc_bit IMAP + wipe）、`free_inode`
- 磁盘 inode `d2_inode`（type.h）：mode/nlinks/uid/gid/size/atime/mtime/ctime/zone[10]；`rw_inode` 双向转换（norm 字节序）
- `update_times`（ATIME/CTIME/MTIME 标志位）与 IN_MARKDIRTY/IN_MARKCLEAN
- inode 标志：i_dirt/i_update/i_seek/i_mountpoint/i_zsearch/i_last_dpos
- `init_inode_cache`：hash/unused 链初始化

## 边界

- **前置依赖**: 07/08
- **不覆盖（移交）**: 路径查找（11）、数据通路（14/15）
