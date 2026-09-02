# 24-vbfs-hgfs: VBFS/HGFS：宿主共享文件夹桥接（SFFS）

> **状态**: pending（最小骨架，待改写）
> **定位**: 宿主桥接变体（后置，defer 候选）
> **源码**: `minix3/minix/lib/libsffs/`（15 个 .c）、`minix3/minix/fs/vbfs/vbfs.c`、`minix3/minix/fs/hgfs/hgfs.c`
> **Rust 模块**: `os/fs/vbfs`、`os/fs/hgfs`
> **draft 素材**: 无（新建）

## 核心点

- SFFS 框架：`sffs_init`/`sffs_loop`/`sffs_signal`、`struct sffs_table` 宿主回调（open/read/write/opendir/readdir/getattr/setattr/mkdir/unlink/rmdir/rename/queryvol）
- `sffs_params`：prefix/uid/gid/file_mask/dir_mask/case_insens（权限 mask 语义）
- 路径语义：name.c/path.c（前缀拼接）、dentry.c（目录项）、handle.c（文件句柄）、verify.c（校验）、inode.c/link.c/lookup.c/mount.c/read.c/stat.c/write.c
- vbfs：VBoxFS 桥（share 选项 → vboxfs_init → sffs_init）
- hgfs：VMware HGFS 桥（libhgfs 宿主协议）
- optset 参数解析、host FS 的 POSIX 映射

## 边界

- **前置依赖**: 01
- **不覆盖（移交）**: VBox/VMware 驱动协议（16-stage-drivers/库层）、mfs 磁盘语义（07~17）
