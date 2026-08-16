# 01-ds-init-main: 启动入口与主循环骨架

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 启动入口
> **源码**: `minix3/minix/servers/ds/main.c`（132 行）
> **Rust 模块**: `main.rs`、`lib.rs`
> **draft 素材**: `draft/tmp_main.c.md`（逐行素材）

## 核心点

- `main`：`env_setargs` → `sef_local_startup` → 主循环（收消息 → 分类 → 回复）
- `sef_local_startup`：`sef_setcb_init_fresh` + `sef_setcb_init_restart(STATEFUL)` + `sef_llvm_ds_st_init` + `sef_startup`
- `get_work`：`sef_receive(ANY)`，`who_e`/`callnr` 单线程事件循环状态
- `reply`：`ipc_send`，`EDONTREPLY` 跳过回复
- notify 拒绝（`is_notify` → 告警 + EINVAL）与 default → EINVAL
- 主循环分发表：7 个 case（DS_PUBLISH/RETRIEVE/RETRIEVE_LABEL/DELETE/SUBSCRIBE/CHECK/GETSYSINFO）

## 边界

- **前置依赖**: 00 + kernel `12-ipc-core`/`09-vm-boot-protocol`
- **不覆盖（移交）**: SEF boot 回调体（06）、各 handler 实现（07~11）
