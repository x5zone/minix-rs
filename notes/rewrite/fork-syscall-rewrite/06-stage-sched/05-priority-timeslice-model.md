# 05-priority-timeslice-model: 优先级与时间片模型

> **状态**: pending（最小骨架，待改写）
> **定位**: 调度模型（服务 handler 的概念基础）
> **源码**: `minix3/minix/include/minix/config.h:66-77`、`schedule.c:41,44`、`servers/pm/utility.c:nice_to_priority`
> **Rust 模块**: `priority.rs`
> **draft 素材**: `draft/04-sched-quantum.md`（素材，概念部分拆分）

## 核心点

- 优先级常量体系：`NR_SCHED_QUEUES`(16)/`TASK_Q`(0)/`MAX_USER_Q`(0)/`USER_Q`(默认)/`MIN_USER_Q`(15)
- `max_priority`（上限） vs `priority`（当前，可被 noquantum 降低）
- time_slice 单位：**ms**（对接内核 `p_quantum_size_ms`，修正 draft "ticks" 错误，S-6）
- `DEFAULT_USER_TIME_SLICE`(200)/`USER_QUANTUM`(200)/`USER_DEFAULT_CPU`(-1)
- nice → priority 映射（`nice_to_priority`，概念引用 04-stage-pm/16）
- `is_system_proc`（schedule.c:44，`parent == RS_PROC_NR`）

## 边界

- **前置依赖**: 04
- **不覆盖（移交）**: handler 中的具体使用（06~08）、内核侧优先级校验（09）
