# 21-cdev: 字符设备 I/O

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 9 — 设备 I/O
> **源码**: `cdev.c` 全文件
> **Rust 模块**: （未实现）cdev 模块
> **draft 素材**: 无（新建）

## 核心点

- cdev_open/cdev_close/cdev_io：字符设备数据面
- cdev_map/cdev_get/cdev_clone/cdev_opcl：CTTY 重定向与克隆设备
- cdev_select/cdev_cancel：select 协作（23）
- cdev_generic_reply/cdev_reply：主循环 IS_CDEV_RS 回复处理
- grant 机制（A-12）与 make_ioctl_grant 使用

## 边界

- dmap 表不覆盖（19）
- select 的 cdev 路径不覆盖（23）
