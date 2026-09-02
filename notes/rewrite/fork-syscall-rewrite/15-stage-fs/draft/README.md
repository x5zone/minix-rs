# 15-stage-fs — 文件系统服务器（占位）

> **状态**: 占位（crate 已建，待实装）
> **定位**: 8 个 FS server 的实装 stage；mfs/pfs 是 boot image 成员（`minix3/minix/kernel/table.c:56,62`）

## 范围
- `os/fs/mfs`（根文件系统，boot 关键路径）
- `os/fs/pfs`（管道，boot 关键路径）
- `os/fs/procfs` / `os/fs/ptyfs`（RS 运行时加载）
- `os/fs/ext2` / `os/fs/isofs` / `os/fs/vbfs` / `os/fs/hgfs`（后置）
- `os/libs/minix-fs` 框架（inode/块缓存/IO，对应 `minix3/minix/lib/libminixfs/`）

## C 对应
- `minix3/minix/fs/`

## 占位现状
- `os/fs/*` 8 个 crate（stub，仅 init() 空实现）
