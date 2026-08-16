# 04-input-event-format: 事件格式与事件码契约

> **状态**: pending（最小骨架，待改写）
> **定位**: `struct input_event` wire 格式 + 全量事件码（阶段 2 协议面）
> **源码**: `minix3/minix/include/minix/input.h`（`_SYSTEM` 段 :6-15 除外）
> **Rust 模块**: `minix-types`（`event.rs`）
> **draft 素材**: `../../../../tmp/input/tmp_input.h.md`（素材）

## 核心点

- `struct input_event` 字段语义（input.h:25-32）：page/code/value/flags/devid/rsvd[2]（rsvd 未来时间戳，server 置 0）
- 事件页：INPUT_PAGE_GD/KEY/LED/BUTTON/CONS（:35-39）
- 事件值 INPUT_RELEASE=0/INPUT_PRESS=1（:42-43）；事件标志 INPUT_FLAG_ABS/REL（:46-47）
- 全量事件码：INPUT_KEY_*（:50-291，HID Usage 对齐，U.S. 布局）、INPUT_GD_*、INPUT_LED_*（:293-295）、INPUT_BUTTON_1（:299）、INPUT_CONS_*（:303-330）
- `_SYSTEM` 段可见性（INPUT_DEV_KBD/MOUSE、INVALID_INPUT_ID，仅系统组件）
- 与 USB HID 规范的对应关系（注释明言驱动必须转换到该格式）

## 边界

- **前置依赖**: 03（eventbuf 元素类型）
- **不覆盖（移交）**: 消息封装（05）、缓冲/唤醒机制（07/09）、键盘映射/扫描码（TTY 驱动阶段）
