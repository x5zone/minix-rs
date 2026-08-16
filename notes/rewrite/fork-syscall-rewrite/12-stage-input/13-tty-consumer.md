# 13-tty-consumer: TTY 消费契约

> **状态**: pending（最小骨架，待改写）
> **定位**: TTY 侧 INPUT 相关契约面（阶段 7 外部消费者）
> **源码**: `minix3/minix/drivers/tty/tty/arch/i386/keyboard.c:124-176,369-384` + `drivers/tty/tty/tty.c:209-210`
> **Rust 模块**: 外部契约（TTY 重写属驱动阶段，不实现）
> **draft 素材**: 无

## 核心点

- TTY_INPUT_UP 握手（keyboard.c:131-144）：`ds_retrieve_label_endpt("input")` 校验发送者 → 保存 `input_endpt` → `set_leds()` 回发当前 LED 状态（锚 10）
- TTY_INPUT_EVENT 消费（keyboard.c:148-176）：发送者校验；`INPUT_PAGE_KEY` 过滤；`code >= NR_SCAN_CODES` 丢弃；`INPUT_RELEASE` → `RELEASE_BIT(0x8000)`；写入 inbuf 环形缓冲（KB_IN_BYTES）+ `tty_events=1`
- `set_leds`（keyboard.c:369-384）：`INPUT_SETLEDS` + `led_mask = locks[ccurrent] & ~ALT_LOCK`，`asynsend3` 到 input_endpt
- 消息接入点：`tty.c:209-210`（TTY_INPUT_UP/TTY_INPUT_EVENT → do_input，不进入 CDEV 路径）

## 边界

- **前置依赖**: 05（TTY 消息）+ 09（转发格式）
- **不覆盖（移交）**: TTY 完整语义（键盘映射/console/驱动阶段）、server 内部（01~12）
