# 13-mfs-link: MFS link/unlink/rmdir/rename/rdlink/trunc

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现：命名空间变更
> **源码**: `minix3/minix/fs/mfs/link.c`
> **Rust 模块**: `os/fs/mfs`（link）
> **draft 素材**: 无（新建）

## 核心点

- `fs_link`：LINK_MAX/EMLINK、目录 EPERM、硬链接计数 + search_dir(ENTER)
- `fs_unlink`/`fs_rmdir`：`remove_dir`/`unlink_file`（DELETE 目录项 + nlink 递减 + 适时 free_inode）、目录非空检查（IS_EMPTY）
- `fs_rename`：跨目录/同名/目录降级（dir → non-dir）检查、源/目标删除与插入顺序、错误回滚
- `fs_slink`/`fs_rdlink`：符号链接读写（link.c 侧实现，内容在数据块）
- `fs_trunc`/`truncate_inode`：`freesp_inode` 释放块区间（start→end）
- `nextblock`/`zerozone_half`/`zerozone_range`：空洞清零辅助（zone 边界处理）

## 边界

- **前置依赖**: 09/11
- **不覆盖（移交）**: open 语义（12）、write_map WMAP_FREE 底层释放（15）
