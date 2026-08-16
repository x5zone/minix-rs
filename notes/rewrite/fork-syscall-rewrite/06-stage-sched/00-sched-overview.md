# 00-sched-overview: SCHED 整体架构概览

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 0 总览（启动主线图 + 文档导航）
> **源码**: `minix3/minix/servers/sched/`（main.c 137 行、schedule.c 369 行、utility.c 74 行）、契约对端（libsys/PM/RS/kernel）
> **Rust 模块**: `os/servers/sched/` 全部
> **draft 素材**: `draft/00-sched-overview.md`（素材，fork 视角，需重写导航）

## 核心点

- SCHED 是什么：用户态调度器（双层调度模型的策略面）
- 双层调度模型：内核执行面（就绪队列/上下文切换） vs 用户策略面（优先级/时间片/CPU 选择）
- 启动主线图：RS 启动 → main → sef_local_startup → 主循环（参见 plan.md §1.2）
- 文档导航：16 篇的叙事逻辑（阶段 1~7 + 99）
- 与 Kernel/PM/RS 的职责边界

## 边界

- **前置依赖**: 无
- **不覆盖（移交）**: 一切机制细节（见 01~14）
