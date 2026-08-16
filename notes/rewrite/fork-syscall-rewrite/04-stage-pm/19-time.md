# 19: time

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 10 时间与系统信息
> **源码**: minix3/minix/servers/pm/time.c（22/53/71/94/110）
> **Rust 模块**: time 模块（未实现）
> **draft 素材**: 无

## 核心点

do_gettime/do_getres（CLOCK_REALTIME/MONOTONIC 选择）、do_settime/do_stime（SUPER_USER 检查、boottime 计算、sys_settime/sys_stime）、do_time（clock_time 直读）

## 边界

- **前置依赖**: 01/04
- **不覆盖（移交）**: 内核时钟实现（01-stage-kernel/21-syscall-clock.md）、getrusage（20）
