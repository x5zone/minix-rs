# 10-input-setleds: LED 状态广播

> **状态**: pending（最小骨架，待改写）
> **定位**: INPUT_SETLEDS 下发：input_set_leds（阶段 4 事件与 LED 机制）
> **源码**: `minix3/minix/servers/input/input.c:204-239`（input_set_leds）
> **Rust 模块**: `setleds.rs`
> **draft 素材**: `../../../../tmp/input/tmp_input.c.md`（素材）

## 核心点

- `input_set_leds`（:204-239）：遍历 FIRST_KBD_DEV..LAST_KBD_DEV；`minor == KBDMUX_MINOR` → 全键盘广播，否则匹配 `dev->minor`
- 保存 `dev->leds = mask`（:228，跨驱动重启，A-6）；`owner != NONE` → `asynsend3(INPUT_SETLEDS, AMF_NOREPLY)` 下发（鼠标设备自然丢弃）
- 请求来源：TTY `INPUT_SETLEDS`（:631 仅 TTY 来源，锚 13）与 ioctl KIOCSLEDS（锚 08）
- 恢复路径：`input_connect` 注册后 `input_set_leds(devs[kbd_id].minor, devs[kbd_id].leds)`（:527，锚 11）
- ARCH A-6（LED 状态跨驱动重启）/A-9（单向 asynsend）

## 边界

- **前置依赖**: 03（leds 字段）+ 05
- **不覆盖（移交）**: LED 请求来源（08/13）、驱动侧回调实现（14）
