# 25-rs-services: RS 服务（Live Update）

> **状态**: pending（最小骨架，待改写）
> **定位**: 主循环 → `RS_INIT` 握手 / CALLMAP → `RS_*` 服务
> **源码**: `minix3/minix/servers/vm/rs.c`（391 行）
> **Rust 模块**: `rs.rs`、`vm_server.rs:RprocTab`
> **draft 素材**: `draft/21-vm-rs-services.md`（素材）

## 核心点

- `do_rs_set_priv`/`do_rs_prepare`/`do_rs_update`/`do_rs_memctl`、`rs_memctl_*` 静态族
- `map_service`、`adjust_proc_refs`（LU 状态切换后调整引用）
- `RprocTab` 语义
- ARCH A-8 缺口契约：`RS_PREPARE`/`RS_UPDATE` 未实现（NotImplemented，fail-closed），标注 defer

## 边界

- **前置依赖**: 15/22（LU 需 10）
- **不覆盖（移交）**: 查询（26）
