# 06-ds-boot-mapping: SEF 启动映射与 RS 握手

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 启动映射（`sef_cb_init_fresh` 锚点）
> **源码**: `store.c:229-285`、`kernel/table.c:44-64`、`kernel/main.c:196,265-267`、`rs.h:165-183`、`sef.h:44-53,85`
> **Rust 模块**: `boot.rs`、`sef.rs`
> **draft 素材**: `draft/tmp_main.c.md` + `draft/tmp_store.c.md`

## 核心点

- boot 两层语义：登记顺序第一（`table.c:52`）vs 执行顺序 RTS_VMINHIBIT（`kernel/main.c:265-267`）
- `sef_cb_init_fresh`：复位两张表 → `sys_safecopyfrom(RS_PROC_NR, rproctab_gid)` → 循环 `map_service`
- `map_service`：label→endpoint 登记，owner="rs"，`update_subscribers(dsp,1)`
- STATEFUL 重启语义（`SEF_CB_INIT_RESTART_STATEFUL`：重启保留状态，仅 fresh 复位）
- A-6：LU 状态转移缺口（magic 插桩 → 显式序列化）

## 边界

- **前置依赖**: 01/03/05 + RS `02`/`11`
- **不覆盖（移交）**: 客户端 `ds_publish_label` 调用面（12）
