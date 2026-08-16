# 22-sdev: socket 驱动 I/O

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 9 — 设备 I/O
> **源码**: `sdev.c` 全文件
> **Rust 模块**: （未实现）sdev 模块
> **draft 素材**: 无（新建）

## 核心点

- sdev_socket/sdev_bind/sdev_connect/sdev_listen/sdev_accept：socket 驱动面
- sdev_readwrite/sdev_ioctl/sdev_setsockopt/sdev_getsockopt/sdev_getsockname/sdev_getpeername/sdev_shutdown/sdev_close/sdev_select
- sdev_suspend/sdev_finish/do_accept_reply：阻塞挂起与续作（resume_* 在 24）
- sdev_sendrec/sdev_simple/sdev_get：驱动消息收发
- sdev_stop/sdev_cancel/sdev_reply：主循环 IS_SDEV_RS 回复处理

## 边界

- smap 表不覆盖（19）
- socket 系统调用面不覆盖（24）
