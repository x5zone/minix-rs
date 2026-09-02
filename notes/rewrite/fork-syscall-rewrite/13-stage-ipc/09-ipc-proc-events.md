# 09-ipc-proc-events: PM 进程事件订阅与处理

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 跨服务协作（主循环 PM `PROC_EVENT` 分支）
> **源码**: `minix3/minix/servers/ipc/main.c:144-214` + `minix3/minix/include/minix/syslib.h:289-294`
> **Rust 模块**: `events.rs`
> **draft 素材**: 无（新建）

## 核心点

- `event_mask` + `SEM_EVENTS=0x01` 订阅掩码
- `update_sub`：0↔非 0 变化时 `proceventmask(PROC_EVENT_EXIT | PROC_EVENT_SIGNAL)` 订阅/退订；**退订后遗留事件仍须正确回复**（防死锁设计）
- `update_sem_sub`：sem 模块的订阅需求开关（05/06 调用）
- `got_proc_event`：读 `m_pm_lsys_proc_event.endpt/event`、EXIT 判定、`sem_process_event(endpt, has_exited)`（→06）、回执 `PROC_EVENT_REPLY`（`asynsend3(AMF_NOREPLY)`）
- 订阅生命周期：首 sem 集合创建（05 do_semget）订阅、末集合移除（05 remove_set）退订

## 边界

- **前置依赖**: 01 + 06（`sem_process_event` 消费方）
- **不覆盖（移交）**: 阻塞取消内部实现（06）
