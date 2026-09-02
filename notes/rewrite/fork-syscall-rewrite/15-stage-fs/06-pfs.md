# 06-pfs: PFS：管道文件服务器

> **状态**: pending（最小骨架，待改写）
> **定位**: boot 第一个挂载的 FS + 最小完整 server 样例
> **源码**: `minix3/minix/fs/pfs/pfs.c`
> **Rust 模块**: `os/fs/pfs`
> **draft 素材**: 无（新建）

## 核心点

- PFS 定位：管道 + clone 设备（VFS pipe.c/cdev.c 经 req_newnode(PFS_PROC_NR) 创建节点）；boot 顺序最先挂载（VFS do_init_root → mount_pfs）
- 512 inode 静态表：i_num 1-based（0 保留）、free 链表（LIST_HEAD free_inodes）、pfs_findnode 校验
- pfs_mount：无根节点（memset 0 根属性，VFS 忽略）、RES_64BIT；pfs_unmount busy 告警
- 回调表 9 项：mount/unmount/newnode/putnode/read/write/trunc/stat/chmod；未实现回调 → ENOSYS
- newnode：管道（S_ISFIFO）与设备节点语义、i_data 缓冲、i_start 读写游标；read/write 的管道阻塞语义边界（阻塞在 VFS 侧）
- 最小 server 样例价值：完整展示 fsdriver 框架消费方式（SEF 启动 + fsdriver_task + 表驱动）

## 边界

- **前置依赖**: 01/02
- **不覆盖（移交）**: 管道 VFS 侧实现（05-stage-vfs/17-pipe）、磁盘 FS 语义（07~17）
