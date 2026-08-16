# 05-devm-message-contract: 消息面与协议契约

> **状态**: pending（最小骨架，待改写）
> **定位**: DEVMAN_* 消息分发面（阶段 3 消息面）
> **源码**: `minix3/minix/include/minix/com.h:846-866`、`device.c:do_reply`（:213-219）、`main.c:46-58`
> **Rust 模块**: `ipc/message.rs`、`ipc/dispatch.rs`
> **draft 素材**: `draft/README.md`（占位）

## 核心点

- DEVMAN_BASE 0x1200、10 个消息常量（ADD_DEV/DEL_DEV/ADD_BUS/DEL_BUS/ADD_DEVFILE/DEL_DEVFILE/REQUEST/REPLY/BIND/UNBIND）
- 字段宏：DEVMAN_GRANT_ID（m4_l1）/GRANT_SIZE（m4_l2）/ENDPOINT（m4_l3）/DEVICE_ID（m4_l2）/RESULT（m4_l1）
- grant 拷贝：`sys_safecopyfrom(ep, GRANT_ID, 0, devinf, GRANT_SIZE)`
- `do_reply`（DEVMAN_REPLY + DEVMAN_RESULT，ipc_send）
- RS-only 权限（bind.c:14,63，A-9）
- **A-3**：message_hook fall-through 决策（C 无 break → Rust 单 handler 分派 + 三处标注）
- **A-6**：未实现消息（ADD_BUS/DEL_BUS/ADD_DEVFILE/DEL_DEVFILE/REQUEST）fail-closed

## 边界

- **前置依赖**: 01 + 03（消息字段）
- **不覆盖（移交）**: 各 handler 业务（07~09）、客户端消息构造（10）
