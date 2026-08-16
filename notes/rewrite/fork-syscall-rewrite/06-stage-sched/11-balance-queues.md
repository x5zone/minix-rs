# 11-balance-queues: 队列平衡与定时器

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → CLOCK notify → `balance_queues`（稳态策略）
> **源码**: `minix3/minix/servers/sched/schedule.c:334-369`、`main.c:66-72`
> **Rust 模块**: `balancer.rs`
> **draft 素材**: `draft/04-sched-quantum.md`（素材，balance 部分拆分）

## 核心点

- `init_scheduling`（schedule.c:334）：`balance_timeout = BALANCE_TIMEOUT(5s) × sys_hz()`、`sys_setalarm`
- `balance_queues`（schedule.c:353）：IN_USE 扫描、`priority > max_priority` 恢复一级、`schedule_process_local`、重设 alarm
- CLOCK notify 链路（main.c:66-72，`is_ipc_notify` → `balance_queues`）
- 策略可替换（S-9：C 注释 "policy will soon be changed" → `QueueBalancer` trait）
- 与 `do_noquantum` 的"降级/恢复"对称性

## 边界

- **前置依赖**: 02/05
- **不覆盖（移交）**: 时钟中断内核侧（01-stage-kernel/15-clock-timer.md）、`sys_setalarm` 实现细节
