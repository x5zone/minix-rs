# 12-kernel-interface: 内核契约（双向）

> **状态**: pending（最小骨架，待改写）
> **定位**: 调度器注册与时间片通知契约（sched ↔ kernel）
> **源码**: `minix3/minix/kernel/system/do_schedctl.c`、`kernel/proc.c:1860-1910`
> **Rust 模块**: 内核侧已实现（`01-stage-kernel/11-scheduling-primitives.md` §4.5/§3.8）
> **draft 素材**: 无（新增，契约汇总）

## 核心点

- `sys_schedctl`/`do_schedctl`（do_schedctl.c:7-49）：`SCHEDCTL_FLAG_KERNEL` 分支（内核调度 + `p_scheduler=NULL`） vs 注册分支（`p_scheduler=caller`，S-1 类型化）
- `notify_scheduler`（proc.c:1860）：`RTS_NO_QUANTUM` 出队、`SCHEDULING_NO_QUANTUM` 消息构造、accounting 7 字段（`acnt_queue/deqs/ipc_sync/ipc_async/preempt/cpu/cpu_load`）、`reset_proc_accounting`、`mini_send`（FROM_KERNEL）
- `proc_no_time`（proc.c:1893）：PREEMPTIBLE 双分支（用户调度通知 vs 内核调度重置）
- 与 `01-stage-kernel/11-scheduling-primitives.md` 的分工声明（实现细节不重复）

## 边界

- **前置依赖**: 09
- **不覆盖（移交）**: 内核调度原语内部（enqueue/dequeue/pick_proc，01-stage-kernel/11）
