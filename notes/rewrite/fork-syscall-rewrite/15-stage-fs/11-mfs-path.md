# 11-mfs-path: MFS 路径组件查找

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现：命名空间基础
> **源码**: `minix3/minix/fs/mfs/path.c`、`mfsdir.h`
> **Rust 模块**: `os/fs/mfs`（path）
> **draft 素材**: 无（新建）

## 核心点

- `fs_lookup`：单组件查找（VFS 做多组件解析）——find_inode(dir) → advance → 节点属性 + is_mountpt 回复
- `advance`：空名 ENOENT、已删目录（i_nlinks==NO_LINK）ENOENT、search_dir(LOOK_UP) → get_inode
- `search_dir`：LOOK_UP/ENTER/DELETE/IS_EMPTY 四种操作；非目录 ENOTDIR；只读检查（ENTER/DELETE）；名字截断（MFS_NAME_MAX=60）
- `struct direct`（mfsdir.h）：mfs_d_ino + mfs_d_name[60] packed，DIR_ENTRY_SIZE/NR_DIR_ENTRIES 推导
- 目录块遍历（i_last_dpos 优化）与目录扩展（ENTER 时 new_block）

## 边界

- **前置依赖**: 09
- **不覆盖（移交）**: 多组件路径解析（05-stage-vfs/13-path-lookup）、数据读取（14）
