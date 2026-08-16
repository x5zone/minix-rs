# 30-fcntl-lock: fcntl 与 POSIX 记录锁

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 14 — 控制与杂项
> **源码**: `misc.c:117-275`（do_fcntl）、`lock.c` 全文件、`lock.h`、`const.h`（NR_LOCKS=8）
> **Rust 模块**: （未实现）fcntl/lock 模块
> **draft 素材**: 无（新建）

## 核心点

- do_fcntl 全命令：F_DUPFD/F_GETFD/F_SETFD/F_GETFL/F_SETFL/F_GETLK/F_SETLK/F_SETLKW
- lock_op：POSIX 记录锁（flock 结构、冲突检测、区域计算）
- lock_revive：锁释放后的阻塞进程恢复（FP_BLOCKED_ON_FLOCK）
- NR_LOCKS=8 锁表与 nr_locks 全局

## 边界

- fd 表操作不覆盖（14）
