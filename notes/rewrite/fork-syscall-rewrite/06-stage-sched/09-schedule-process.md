# 09-schedule-process: 参数下发（SCHED → 内核）

> **状态**: pending（最小骨架，待改写）
> **定位**: 调度机制（`schedule_process` 参数聚合 + 内核 `do_schedule`/`sched_proc`）
> **源码**: `minix3/minix/servers/sched/schedule.c:297-332`、`kernel/system/do_schedule.c`、`kernel/system.c:642-723`
> **Rust 模块**: `kernel_api/schedule.rs`
> **draft 素材**: 无（新增，draft 仅提及）

## 核心点

- `SCHEDULE_CHANGE_PRIO/QUANTUM/CPU/ALL`（schedule.c:22-30）+ `-1` 保持语义
- `niced = (max_priority > USER_Q)` 布尔推断（S-10 可显式建模）
- `sys_schedule` 消息（`mess_lsys_krn_schedule`：endpoint/quantum/priority/cpu/niced）
- 内核 `do_schedule`：`caller != p_scheduler → EPERM`（调度器身份校验）
- `sched_proc` 参数应用（system.c:642-723）：EINVAL/EBADCPU 校验、`RTS_NO_QUANTUM` 重排队、`MF_NICED`
- `schedule_process`（服务端） vs `sched_proc`（内核侧）命名对照表

## 边界

- **前置依赖**: 05/06
- **不覆盖（移交）**: 调度器注册语义 `p_scheduler`（12）、CPU 选择策略（10）
