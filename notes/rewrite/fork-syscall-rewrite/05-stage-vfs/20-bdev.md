# 20-bdev: 块设备 I/O

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 9 — 设备 I/O
> **源码**: `bdev.c` 全文件
> **Rust 模块**: （未实现）bdev 模块
> **draft 素材**: 无（新建）

## 核心点

- bdev_open/bdev_close/bdev_ioctl：块设备操作
- bdev_sendrec：块驱动消息收发（bsf 缓存语义）
- bdev_reply：主循环 IS_BDEV_RS 回复处理（main.c:126-128）
- bdev_up：块驱动上线事件

## 边界

- dmap 表不覆盖（19）
- bsf 锁（read.c）不覆盖（16）
