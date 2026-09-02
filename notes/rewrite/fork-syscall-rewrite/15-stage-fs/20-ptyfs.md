# 20-ptyfs: PTYFS：伪终端文件系统

> **状态**: pending（最小骨架，待改写）
> **定位**: 虚拟树变体：/dev/pts 树（RS 运行时加载）
> **源码**: `minix3/minix/fs/ptyfs/ptyfs.c`、`node.c`
> **Rust 模块**: `os/fs/ptyfs`
> **draft 素材**: 无（新建）

## 核心点

- ptyfs 定位：/dev/pts 伪终端从设备树（fsdriver 直接实现）
- `ptyfs_mount`：**拒绝 REQ_ISROOT**（不能作根 FS）、根节点属性回复
- `ptyfs_lookup`：make_name/parse_name（数字从设备名，前导零/溢出校验）
- `ptyfs_getdents`：fsdriver_dentry_init/add/finish 组装
- `fdr_other`：**PTYFS_SET/PTYFS_DEL 协议**（ds_retrieve_label_name 校验 label=='pty'）——从设备创建/删除
- stat/chown/chmod/statvfs 实现；node.c 节点数据；ptyfs_signal（SIGTERM → terminate）
- 回调表 8 项：mount/lookup/getdents/stat/chown/chmod/statvfs/other

## 边界

- **前置依赖**: 01/02
- **不覆盖（移交）**: TTY/PTY 驱动（16-stage-drivers）、VFS 侧 /dev/pts 挂载
