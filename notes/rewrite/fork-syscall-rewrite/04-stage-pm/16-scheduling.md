# 16: scheduling

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 7 调度
> **源码**: minix3/minix/servers/pm/schedule.c（20/55/89）、minix3/minix/servers/pm/utility.c:nice_to_priority(91)、minix3/minix/servers/pm/main.c:get_nice_value(276)、minix3/minix/servers/pm/misc.c:do_getsetpriority(239)
> **Rust 模块**: sched 客户端（未实现，A-8）
> **draft 素材**: 无

## 核心点

sched_init（INIT 接管）、sched_start_user（sched_inherit/nice_to_priority/PRIV_PROC 父继承特例）、sched_nice（SCHEDULING_SET_NICE）、nice_to_priority（nice→queue）、get_nice_value（queue→nice）、do_getsetpriority（PRIO_PROCESS/权限/root nice 降低限制）

## 边界

- **前置依赖**: 01/04
- **不覆盖（移交）**: 内核调度器/就绪队列（01-stage-kernel/11-scheduling-primitives.md）、SCHED 服务实现（06-stage-sched）
