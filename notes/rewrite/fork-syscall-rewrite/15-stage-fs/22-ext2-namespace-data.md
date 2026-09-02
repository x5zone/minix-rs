# 22-ext2-namespace-data: Ext2 命名空间/数据/元数据

> **状态**: pending（最小骨架，待改写）
> **定位**: 磁盘变体第 2 篇：读写与命名空间
> **源码**: `minix3/minix/fs/ext2/path.c`、`open.c`、`link.c`、`read.c`、`write.c`、`protect.c`、`stadir.c`、`time.c`、`utility.c`、`inode.c`
> **Rust 模块**: `os/fs/ext2`
> **draft 素材**: 无（新建）

## 核心点

- ext2 inode（inode.c）：ext2 磁盘 inode 格式（块指针数组、组内编号）与内存模型
- path.c：目录项查找（ext2 dir entry 格式：inode/rec_len/name_len/type）
- open/create、link（硬链接/符号链接/rename）、read/write（块映射变体：直接/间接/双重间接）
- protect/stadir/time/utility：元数据与辅助
- 与 mfs 语义对照：API 面一致（fdr 回调），磁盘格式与分配策略不同（A-11 格式兼容决策面）

## 边界

- **前置依赖**: 21
- **不覆盖（移交）**: ext2 分配器（21）、mfs 完整语义（07~17）
