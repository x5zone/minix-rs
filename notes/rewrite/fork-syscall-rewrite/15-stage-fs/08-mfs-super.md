# 08-mfs-super: MFS 超级块与位图

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现：磁盘元数据第 1 篇
> **源码**: `minix3/minix/fs/mfs/super.c`、`super.h`、`const.h`（部分）
> **Rust 模块**: `os/fs/mfs`（super）
> **draft 素材**: 无（新建）

## 核心点

- 磁盘布局：boot block(1) → super block(1kB 偏移) → inode map → zone map → inodes → 数据区（super.h 注释契约）
- `struct super_block`：磁盘字段（s_ninodes/s_nzones/s_imap_blocks/s_zmap_blocks/s_log_zone_size/s_flags/s_max_size/s_zones/s_magic/s_block_size/s_disk_version）vs 内存字段（s_dev/s_rd_only/s_native/s_version/s_ndzones/s_nindirs/s_isearch/s_zsearch），LAST_ONDISK_FIELD 控制
- V2/V3 magic：SUPER_MAGIC 0x137F / SUPER_V2 0x2468 / SUPER_V3 0x4d5a + REV 变体；s_native 字节序判定
- `read_super`/`write_super`（rw_super + get_block_size）、版本推导（s_ndzones/s_nindirs）
- `alloc_bit`/`free_bit`：IMAP(0)/ZMAP(1) 位图操作、NO_BIT 失败语义、bit 搜索起点优化（s_isearch/s_zsearch）
- MFSFLAG_CLEAN 标志与 MFSFLAG_MANDATORY_MASK

## 边界

- **前置依赖**: 07
- **不覆盖（移交）**: inode 生命周期（09）、挂载流程（10）
