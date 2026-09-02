# 10-mfs-mount: MFS 挂载/卸载/挂载点

> **状态**: pending（最小骨架，待改写）
> **定位**: 参考实现：boot 挂载路径
> **源码**: `minix3/minix/fs/mfs/mount.c`
> **Rust 模块**: `os/fs/mfs`（mount）
> **draft 素材**: 无（新建）

## 核心点

- `fs_mount`：bdev_open（R/W 位）→ read_super → 未识别格式 EINVAL → **unclean 自动降级只读**（bdev_close + 只读重开 + 告警）→ lmfs_set_blocksize → block usage 报告（used_zones = zones - count_free_bits）→ get_inode(ROOT_INODE) → 根节点属性回复 → 非只读写脏 super（MFSFLAG_CLEAN 清除）
- `fs_unmount`：busy 检查（in-use inode 计数告警）→ put_inode(root) → fs_sync → clean 标记（原干净才写）→ bdev_close → lmfs_invalidate → s_dev=NO_DEV
- `fs_mountpt`：get_inode → i_mountpoint EBUSY / 设备节点 ENOTDIR 检查 → 置 i_mountpoint

## 边界

- **前置依赖**: 07~09
- **不覆盖（移交）**: super 读写细节（08）、VFS 侧挂载（05-stage-vfs/18-mount）
