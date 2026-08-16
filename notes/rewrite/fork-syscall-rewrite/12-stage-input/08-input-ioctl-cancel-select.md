# 08-input-ioctl-cancel-select: ioctl / cancel / select

> **状态**: pending（最小骨架，待改写）
> **定位**: CDEV_IOCTL/CANCEL/SELECT handler（阶段 3 字符设备操作面）
> **源码**: `minix3/minix/servers/input/input.c:241-330`（input_ioctl/input_cancel/input_select）+ `sys/sys/ttycom.h:174` + `sys/kbdio.h`
> **Rust 模块**: `handlers.rs`（ioctl/cancel/select）
> **draft 素材**: `../../../../tmp/input/tmp_input.c.md`（素材）

## 核心点

- `input_ioctl`（:241-280）：仅 KIOCSLEDS；`sys_safecopyfrom` 读 `kio_leds_t`；位映射 KBD_LEDS_NUM→INPUT_LED_NUMLOCK / CAPS / SCROLL；`input_set_leds(minor, mask)`（锚 10）；默认 ENOTTY；`!active` → EIO
- `input_cancel`（:282-301）：匹配挂起读（caller+req_id）→ `suspended=FALSE` + EINTR；不匹配 → EDONTREPLY
- `input_select`（:303-330）：CDEV_OP_RD——`!active || suspended` → 即时就绪（错误）；有数据 → 就绪；空 + CDEV_NOTIFY → 记 selector（`chardriver_reply_select` 异步唤醒）；CDEV_OP_WR 恒就绪（`/* immediate error */`，只读设备）
- ARCH A-8（select/CDEV_NOTIFY 契约）/A-11（错误码）

## 边界

- **前置依赖**: 03 + 02（reply 语义）+ 05
- **不覆盖（移交）**: 读挂起细节（07）、事件生产与唤醒（09）、LED 广播（10）
