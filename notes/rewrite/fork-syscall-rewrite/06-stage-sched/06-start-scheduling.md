# 06-start-scheduling: do_start_scheduling（调度接管）

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → `SCHEDULING_INHERIT`/`START` → `do_start_scheduling`（fork 次主线核心）
> **源码**: `minix3/minix/servers/sched/schedule.c:140-252`
> **Rust 模块**: `scheduling/start.rs`
> **draft 素材**: `draft/02-sched-inherit.md` + `draft/03-sched-start.md`（素材，合并）

## 核心点

- 双消息 assert（INHERIT/START 统一入口）+ `accept_message`
- `sched_isemtyendpt`(child) + slot 填充（endpoint/parent/max_priority）+ `max_priority >= NR_SCHED_QUEUES → EINVAL`
- `endpoint == parent` init 特例（USER_Q/DEFAULT_USER_TIME_SLICE/BSP）
- START 分支（priority=max_priority、time_slice=msg.quantum，RS 系统进程路径） vs INHERIT 分支（priority/time_slice 父继承）
- `sys_schedctl(0, ep, 0, 0, 0)` 调度接管 → `flags=IN_USE`
- `pick_cpu` + `schedule_process(SCHEDULE_CHANGE_ALL)` + **EBADCPU 重试**（`cpu_proc[cpu]=CPU_DEAD`）
- 回复 `scheduler=SCHED_PROC_NR`
- **fork 次主线路径图**（PM → sched_inherit → INHERIT 分支，plan.md §1.3）

## 边界

- **前置依赖**: 02/04/05
- **不覆盖（移交）**: `schedule_process` 内部（09）、`pick_cpu` 内部（10）、PM 调用面（13）
