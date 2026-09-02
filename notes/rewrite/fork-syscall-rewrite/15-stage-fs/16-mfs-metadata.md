# 16-mfs-metadata: MFS 元数据：chmod/chown/stat/statvfs/utime

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现：元数据服务
> **源码**: `minix3/minix/fs/mfs/protect.c`、`stadir.c`、`time.c`、`utility.c`
> **Rust 模块**: `os/fs/mfs`（metadata）
> **draft 素材**: 无（新建）

## 核心点

- `fs_chmod`：mode 更新 + ctime；`fs_chown`：uid/gid 更新 + mode 位清理（setuid/setgid 语义）
- `fs_stat`：inode → struct stat 全字段填充
- `fs_statvfs`：块/文件总数与空闲统计（super + count_free_bits）
- `fs_utime`：atime/mtime 设置（NULL → 当前时间语义在 VFS 侧）
- `conv2`/`conv4`：磁盘整数字节序转换辅助

## 边界

- **前置依赖**: 09
- **不覆盖（移交）**: VFS 权限检查（05-stage-vfs/29-protect）、磁盘格式（08）
