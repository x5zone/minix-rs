# 01-is-init-main: 启动入口与主循环骨架

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 启动入口
> **源码**: `minix3/minix/servers/is/main.c`（148 行）+ `libsys/sef.c:150,208-214`、`libsys/sef_ping.c:21`
> **Rust 模块**: `main.rs`、`lib.rs`、`sef.rs`
> **draft 素材**: `draft/tmp_main.c.md`（逐行素材）

## 核心点

- `main`：`env_setargs` → `sef_local_startup` → 主循环（收消息 → 分类 → 回复），主循环 `main.c:44-69`
- `sef_local_startup`：init_fresh/init_lu/init_restart 三合一注册 + signal handler + `sef_startup`
- `sef_cb_init_fresh`（boot 锚点）：`map_unmap_fkeys(TRUE)` → 02
- `sef_cb_signal_handler`：SIGTERM → `map_unmap_fkeys(FALSE)` + `exit(0)`（A-10）
- `get_work`：`sef_receive(ANY)`，who_e/callnr 单线程状态；失败 panic
- 主循环分类：notify + who_e==TTY_PROC_NR → do_fkey_pressed（03）；notify 非 TTY → EDONTREPLY；非 notify → 告警 + EDONTREPLY
- SEF ping 存活检查：RS ping 为 NOTIFY，`sef_receive` 透明拦截（`sef.h:121-122`），主循环不感知（A-11）
- `reply`：`ipc_send`，EDONTREPLY 跳过；失败 panic

## 边界

- **前置依赖**: 00 + kernel `12-ipc-core`/`09-vm-boot-protocol`
- **不覆盖（移交）**: fkey 注册实现（02）、转储分派（03）
