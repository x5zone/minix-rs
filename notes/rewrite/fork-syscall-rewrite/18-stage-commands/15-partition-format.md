# 15-分区与格式化

> **状态**: pending（最小骨架，待改写）
> **定位**: 分区与格式化
> **源码**: `minix3/minix/commands/{fdisk,part,partition,autopart,repartition,format,devsize}/、sbin/newfs_{ext2fs,msdos,udf,v7fs}/、usr.sbin/makefs/`
> **Rust 模块**: `os/commands/sbin/mkfs`
> **draft 素材**: 无（新建）

## 核心点

- - 分区命令契约表（7 命令）：fdisk/part/partition/autopart/repartition（分区表读写）/format/devsize
- - 格式化命令：newfs_* 族（ext2fs/msdos/udf/v7fs）+ makefs/mkfs 的格式参数面
- - 分区表格式面（MBR 语义）与设备大小查询（devsize）
- - 与 16-stage-drivers 存储驱动接口的消费关系

## 边界

- - **前置依赖**: 14
- - **不覆盖（移交）**: 分区格式解析库实现（16-stage-drivers 或独立库）

