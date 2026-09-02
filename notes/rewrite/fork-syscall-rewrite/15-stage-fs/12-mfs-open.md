# 12-mfs-open: MFS open/create/mkdir/mknod/slink/seek

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现：命名空间创建
> **源码**: `minix3/minix/fs/mfs/open.c`
> **Rust 模块**: `os/fs/mfs`（open）
> **draft 素材**: 无（新建）

## 核心点

- `new_node`：alloc_inode（位图）→ search_dir(ENTER)（目录扩展）→ 失败回滚（free_inode）
- `fs_create`：new_node + 已有文件存在性/类型检查（I_TYPE 不匹配 → EEXIST/ENOTDIR 语义）、非空名/太长校验
- `fs_mkdir`：new_node + `.` 与 `..` 目录项写入、目录计数（nlink）语义
- `fs_mknod`：设备节点（I_BLOCK_SPECIAL/I_CHAR_SPECIAL）、普通文件空 inode
- `fs_slink`：符号链接内容写入（数据块）
- `fs_seek`：i_seek 标志（ISEEK/NO_SEEK，影响 atime 更新与 read/write 行为）

## 边界

- **前置依赖**: 09/11
- **不覆盖（移交）**: 链接/重命名/截断（13）、数据写入（15）
