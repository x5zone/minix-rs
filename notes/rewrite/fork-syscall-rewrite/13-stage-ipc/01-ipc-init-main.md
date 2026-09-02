# 01-ipc-init-main: 启动入口与主循环骨架

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 启动入口 + 主循环（所有 handler 篇的锚点）
> **源码**: `minix3/minix/servers/ipc/main.c:216-284`（main）、`:124-142`（sef_local_startup）、`:12-25`（call_vec）
> **Rust 模块**: `main.rs`、`lib.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `main`：`env_setargs` → `sef_local_startup`（`sef_setcb_init_fresh/restart` + `sef_setcb_signal_handler` + `sef_startup`）→ 主循环
- 主循环消息分类：`sef_receive_status(ANY)` → `is_ipc_notify` 忽略通知 → PM `PROC_EVENT`（→ 09）→ MIB 消息 `rmib_process`（→ 03）→ `call_vec` 分发（`IPC_BASE` 索引，越界/空槽 → `ENOSYS`）
- `call_vec` 表：`IPC_SHMGET/SHMAT/SHMDT/SHMCTL/SEMGET/SEMCTL/SEMOP` → 05~08
- 回复语义：`SUSPEND` 不回复（06 semop 挂起）；回复 `m_type = errno`；`ipc_sendnb` 失败仅告警不 panic
- 每循环收尾 `update_refcount_and_destroy()` 调用点（→ 08）
- `verbose` 打印开关（A-8）

## 边界

- **前置依赖**: 00 + kernel `12-ipc-core`
- **不覆盖（移交）**: 各 handler 实现（05~08）、MIB 子树细节（03）、进程事件（09）、SIGTERM（10）、`sef_cb_init_fresh` 体（03）
