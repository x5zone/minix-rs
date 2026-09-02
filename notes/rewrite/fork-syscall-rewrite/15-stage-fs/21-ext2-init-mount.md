# 21-ext2-init-mount: Ext2 启动/挂载/分配器

> **状态**: pending（最小骨架，待改写）
> **定位**: 磁盘变体第 1 篇：启动与分配
> **源码**: `minix3/minix/fs/ext2/main.c`、`table.c`、`mount.c`、`super.c`、`balloc.c`、`ialloc.c`、`misc.c`
> **Rust 模块**: `os/fs/ext2`
> **draft 素材**: 无（新建）

## 核心点

- ext2 定位：libminixfs 磁盘变体（完整 fsdriver 实现），与 mfs 的差异展开（变体 diff 原则 plan §3.6）
- SEF init：optset 解析（sb/orlov/mfsalloc/reserved/prealloc）、le_CPU 断言
- fs_mount：ext2 super 校验（magic/版本）、块组描述符、readonly 处理
- super.c：ext2 superblock 字段与内存换算（inode/块大小）
- balloc：**块分配 + prealloc 预分配策略**、位图管理
- ialloc：**Orlov 分配器 vs mfsalloc**（目录智能分配）、inode 位图
- misc.c：sync 等维护面
- 与 mfs 对照：块组结构、预分配、Orlov 为 ext2 特有

## 边界

- **前置依赖**: 04~10
- **不覆盖（移交）**: ext2 命名空间/数据（22）
