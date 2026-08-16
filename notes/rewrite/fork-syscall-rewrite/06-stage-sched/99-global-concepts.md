# 99-global-concepts: 全局概念

> **状态**: pending（最小骨架，待改写）
> **定位**: 全局概念（所有文档共享）
> **源码**: `minix3/minix/include/minix/com.h`、`config.h`、`type.h`
> **Rust 模块**: `os/libs/minix-types`
> **draft 素材**: `draft/99-global-concepts.md`（素材，修正旧引用路径）

## 核心点

- 双层调度模型（内核执行面 vs 用户策略面）
- 五进程表一致性（Kernel `proc`/PM `mproc`/VM `vmproc`/VFS `fproc`/SCHED `schedproc`）
- 优先级常量表（`NR_SCHED_QUEUES`/`TASK_Q`/`MAX_USER_Q`/`USER_Q`/`MIN_USER_Q`）
- `SCHEDULING_*` 5 消息常量（com.h:801-807）、错误码表
- SCHED 与 Kernel/PM/RS 职责边界

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制（见 00~14）
