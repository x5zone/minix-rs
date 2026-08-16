# 13-pm-interaction: PM 客户端契约

> **状态**: pending（最小骨架，待改写）
> **定位**: 客户端契约（PM 调用面，sched 的主要客户端）
> **源码**: `minix3/minix/lib/libsys/sched_start.c`、`sched_stop.c`、`servers/pm/schedule.c`、`pm/forkexit.c:425`
> **Rust 模块**: pm 客户端面（未实现）
> **draft 素材**: `draft/02-sched-inherit.md`（素材，PM 调用部分拆分）

## 核心点

- libsys `sched_inherit`（sched_start.c:11）：消息构造 + `*newscheduler_e` 回读（调度器委托语义）
- libsys `sched_start`（sched_start.c:46）：`NONE` 短路 / `KERNEL` → `sys_schedctl(SCHEDCTL_FLAG_KERNEL)` / 用户调度器 → `SCHEDULING_START`
- libsys `sched_stop`（sched_stop.c:9）：`KERNEL/NONE` 短路
- PM 调用面：`sched_init`（INIT 接管）、`sched_start_user`（fork 路径：`nice_to_priority`/PRIV_PROC 父 → INIT_PROC_NR 特例）、`sched_nice`、`sched_stop`（exit，forkexit.c:425）
- `mp_scheduler` 字段语义、`USER_Q`/`USER_QUANTUM` 使用

## 边界

- **前置依赖**: 02/04/05
- **不覆盖（移交）**: PM nice 系统调用用户面（`do_getsetpriority`/`get_nice_value`，04-stage-pm/16-scheduling.md）
