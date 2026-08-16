# 10-pick-cpu-smp: CPU 选择（SMP）

> **状态**: pending（最小骨架，待改写）
> **定位**: 调度机制（`pick_cpu` 策略 + CPU 负载追踪）
> **源码**: `minix3/minix/servers/sched/schedule.c:48-78`、`include/minix/type.h:123-124`
> **Rust 模块**: `cpu.rs`
> **draft 素材**: `draft/04-sched-quantum.md`（素材，CPU 部分拆分）

## 核心点

- `pick_cpu`（schedule.c:48）三分支：单核 → `bsp_id`；`is_system_proc` → BSP；用户进程 → 负载最低可用 CPU
- `cpu_proc[CONFIG_MAX_CPUS]` 负载计数 + `cpu_is_available` + `CPU_DEAD=-1` 哨兵（S-4：Option 化）
- `CONFIG_SMP`/`CONFIG_MAX_CPUS` 编译期开关 → 运行时 `machine.processors_count/bsp_id`（S-5）
- EBADCPU 重试语义（do_start_scheduling 中 `cpu_proc[cpu]=CPU_DEAD` 后重选）

## 边界

- **前置依赖**: 01/05
- **不覆盖（移交）**: 内核 SMP 迁移机制（`smp_schedule_migrate_proc`，01-stage-kernel/16-smp.md）
