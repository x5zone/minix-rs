# 18-vtreefs: VTreeFS：虚拟树 FS 框架

> **状态**: pending（最小骨架，待改写）
> **定位**: 虚拟树变体框架（procfs 前置；devman/gpio 跨 stage 复用）
> **源码**: `minix3/minix/lib/libvtreefs/`（10 个 .c）
> **Rust 模块**: `os/fs/procfs`（框架部分，或独立 `minix-vtreefs`）
> **draft 素材**: 无（新建）

## 核心点

- VTreeFS 定位：基于 libfsdriver 的虚拟树 FS 框架（vtreefs_table 复用 fsdriver），`run_vtreefs(&hooks, nr_inodes, ...)` 入口
- `struct fs_hooks`（vtreefs.h）：init/lookup/getdents/read/rdlink 等回调契约
- inode 树：index_t 索引、`init_inodes`、inode.h 结构、树内节点增删（extra.c cbdata 数据关联）
- 路径/链接/挂载/统计：path.c/link.c/mount.c/stadir.c 的树语义
- file.c：文件内容 I/O（read/write hook 分发）；sdbm.c：哈希表辅助
- I/O 缓冲：init_buf（BUF_SIZE 缓冲区管理）
- 跨 stage 契约：`../11-stage-devman/` 复用本框架语义

## 边界

- **前置依赖**: 01/02
- **不覆盖（移交）**: procfs 具体内容（19）、devman 用法（11-stage-devman）
