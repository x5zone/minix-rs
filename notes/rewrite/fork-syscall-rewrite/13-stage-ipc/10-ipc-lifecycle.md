# 10-ipc-lifecycle: 服务生命周期与退出语义

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 跨服务协作（SIGTERM 信号回调）
> **源码**: `minix3/minix/servers/ipc/main.c:101-122`
> **Rust 模块**: `lifecycle.rs`
> **draft 素材**: 无（新建）

## 核心点

- `sef_cb_signal_handler`：仅响应 SIGTERM；`is_sem_nil() && is_shm_nil()` → `rmib_deregister(&kern_ipc_node)` + `sef_exit(0)`（干净退出）；否则告警 "exit with unclean state"（不退出）
- `init_restart` = `init_fresh`：重启重新注册 kern.ipc 子树（03）
- 动态状态丢失语义：重启后 sem_list/shm_list 全空（干净退出判定的前提）
- RS 运行时加载生命周期契约（`../03-stage-rs/`）

## 边界

- **前置依赖**: 01 + 03 + 05/08（`is_sem_nil`/`is_shm_nil`）
- **不覆盖（移交）**: 订阅掩码（09）、MIB 树结构（03）
