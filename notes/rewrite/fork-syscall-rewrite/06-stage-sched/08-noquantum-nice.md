# 08-noquantum-nice: do_noquantum 与 do_nice

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → `SCHEDULING_NO_QUANTUM`（do_noquantum）/ `SCHEDULING_SET_NICE`（do_nice）
> **源码**: `minix3/minix/servers/sched/schedule.c:87-109,254-295`
> **Rust 模块**: `scheduling/noquantum.rs`、`scheduling/nice.rs`
> **draft 素材**: `draft/04-sched-quantum.md`（素材，两部分拆分）

## 核心点

- `do_noquantum`（schedule.c:87）：`m_source` 直取 endpoint（**无 accept_message**，靠主循环 FROM_KERNEL 校验）、`priority < MIN_USER_Q` 降一级、`schedule_process_local`
- `do_nice`（schedule.c:254）：`accept_message`、`maxprio >= NR_SCHED_QUEUES → EINVAL`、`max_priority = priority = new_q`、`schedule_process_local` + 失败回滚（old_q/old_max_q）
- 来源信任模型不对称性（noquantum vs 其余 handler）专节

## 边界

- **前置依赖**: 02/04/05
- **不覆盖（移交）**: `schedule_process` 内部（09）、accounting 字段语义（12）、PM `sched_nice` 调用面（13）
