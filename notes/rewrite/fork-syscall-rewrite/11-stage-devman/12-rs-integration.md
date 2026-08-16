# 12-rs-integration: RS 集成契约

> **状态**: pending（最小骨架，待改写）
> **定位**: RS 加载 devman + bind/unbind 握手（阶段 6 外部消费者）
> **源码**: `minix3/minix/servers/rs/manager.c:840-851,897-909,1742`、`minix/include/minix/rs.h:139,182`、`etc/system.conf:422-429`
> **Rust 模块**: `rs_contract.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- `publish_service`（manager.c:840-851）：devman_id != 0 → ds_retrieve_label_endpt("devman") → DEVMAN_BIND（DEVMAN_ENDPOINT + DEVICE_ID）→ 失败 kill_service
- `unpublish_service`（:897-909）：DEVMAN_UNBIND
- `init_slot`（:1742）：devman_id = rs_start->devman_id（rs.h:139,182）
- system.conf：service devman { uid 0; vm SETCACHEPAGE CLEARCACHE }
- 与 09 的闭环：RS 是唯一合法 bind 来源（A-9）

## 边界

- **前置依赖**: 09（bind 消息）+ 99
- **不覆盖（移交）**: devmand 用户态（13）、RS 服务生命周期内部
