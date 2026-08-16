# 07-stop-scheduling: do_stop_scheduling（停止调度）

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → `SCHEDULING_STOP` → `do_stop_scheduling`（exit 路径）
> **源码**: `minix3/minix/servers/sched/schedule.c:112-137`
> **Rust 模块**: `scheduling/stop.rs`
> **draft 素材**: `draft/04-sched-quantum.md`（素材，stop 部分拆分）

## 核心点

- `accept_message` + `sched_isokendpt` 验证（EBADEPT）
- `cpu_proc[cpu]--`（CONFIG_SMP 负载计数递减）
- `flags = 0`（清除 IN_USE）
- 与 `do_start_scheduling` 的对称性（启动/停止操作对照表）

## 边界

- **前置依赖**: 02/04
- **不覆盖（移交）**: 客户端 `sched_stop` 调用面（PM forkexit.c:425、RS manager.c:461/request.c:342，见 13/14）
