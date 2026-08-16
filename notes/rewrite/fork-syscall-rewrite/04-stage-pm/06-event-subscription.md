# 06: event-subscription

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 2 主循环与异步协议
> **源码**: minix3/minix/servers/pm/event.c（75/131/171/219/317）
> **Rust 模块**: event 订阅模块（未实现，A-9 缺口）
> **draft 素材**: 无

## 核心点

PROC_EVENT 订阅/发布设施：subs[NR_SUBS=4]、do_proceventmask、do_proc_event_reply、publish_event、resume_event、remove_sub、EVENT_CALL/NO_EVENTSUB、与 exit/signal 的串行化衔接

## 边界

- **前置依赖**: 04
- **不覆盖（移交）**: exit 流程（09）、signal 恢复（13）、订阅方（SysV IPC server，07-stage-ds 范围）
