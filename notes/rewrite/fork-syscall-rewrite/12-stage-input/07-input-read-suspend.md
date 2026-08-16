# 07-input-read-suspend: 读取与挂起状态机

> **状态**: pending（最小骨架，待改写）
> **定位**: CDEV_READ handler + 环形缓冲拷贝（阶段 3 字符设备操作面）
> **源码**: `minix3/minix/servers/input/input.c:130-201`（input_copy_events/input_read）
> **Rust 模块**: `handlers.rs`（read）、`eventbuf.rs`
> **draft 素材**: `../../../../tmp/input/tmp_input.c.md`（素材）

## 核心点

- `input_read`（:162-201）：map 失败 → ENXIO；`!active || suspended` → EIO（每设备单挂起读）；`event_count = size / sizeof(input_event)`，为 0 → EIO
- 空缓冲：`CDEV_NONBLOCK` → EAGAIN；否则挂起（`suspended=TRUE` + caller/grant/req_id + 返回 EDONTREPLY 伪回复）
- `input_copy_events`（:130-160）：回绕分段 `sys_safecopyto`（wrap_left 判定，1~2 段拷贝）；`count < event_count` 时 panic（A-11：Rust 改 debug_assert + 错误）
- 挂起状态机三出口：resume（09 事件到达 → `chardriver_reply_task`）、cancel（08）、disconnect（11 → EIO）
- ARCH A-4（环形缓冲）/A-5（显式 `SuspendedRead` 状态）/A-11（错误码）

## 边界

- **前置依赖**: 03 + 02（reply 语义）+ 05
- **不覆盖（移交）**: 唤醒的生产方（09）、ioctl/cancel/select（08）、驱动连接断开（11）
