# 01-sched-init-main: 启动入口与 SEF 生命周期

> **状态**: pending（最小骨架，待改写）
> **定位**: RS 启动 sched → `main()` → `sef_local_startup()`
> **源码**: `minix3/minix/servers/sched/main.c:22-98,111-137`
> **Rust 模块**: `main.rs`、`sef.rs`
> **draft 素材**: 无（新增，draft 无 main 专属文档）

## 核心点

- `main()`（main.c:22）：主循环骨架（`sef_receive_status` → dispatch → reply）
- `sef_local_startup()`（main.c:111）：`sef_setcb_init_fresh` + `sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL)`
- `sef_cb_init_fresh()`（main.c:126）：`sys_getmachine(&machine)` + `init_scheduling()`（调用点，定义归 11）
- `machine` 全局（`processors_count`/`bsp_id`，type.h:123-124）——10 的前置
- SEF 生命周期（S-7：Rust SEF trait，对齐 03-stage-rs A-7）

## 边界

- **前置依赖**: 00
- **不覆盖（移交）**: 消息分发细节（02）、各 handler 实现（06~08）、`init_scheduling` 定义（11）
