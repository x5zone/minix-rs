# 02-sched-message-surface: 消息面与主循环分发

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 dispatch（sched 的"心脏"）
> **源码**: `minix3/minix/servers/sched/main.c:52-98`、`utility.c:no_sys`
> **Rust 模块**: `dispatch.rs`
> **draft 素材**: 无（新增）

## 核心点

- 5 种消息类型表：`SCHEDULING_INHERIT`/`START`/`STOP`/`SET_NICE`/`NO_QUANTUM`（com.h:801-807）
- `sef_receive_status(ANY)` 收消息、`is_ipc_notify` 通知分类（CLOCK → `balance_queues`）
- dispatch switch 到各 handler、default → `no_sys`（utility.c:18）
- `SUSPEND` 伪返回码契约（main.c:97-99，唯一不回复路径）
- `reply()`（main.c:101）：`m_in.m_type = result` + `ipc_send` 异步回复
- `IPC_FLG_MSG_FROM_KERNEL` 校验（main.c:84-90，NO_QUANTUM 信任模型前置）
- 消息结构体 7 个（ipc.h，见 plan.md §5.2）

## 边界

- **前置依赖**: 01
- **不覆盖（移交）**: 各 handler 实现（06~08）、消息字段语义细节（09/12/13）
