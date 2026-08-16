# 14-pckbd-driver: pckbd 模型驱动契约

> **状态**: pending（最小骨架，待改写）
> **定位**: pckbd 作为 libinputdriver 模型消费者（阶段 7 外部消费者）
> **源码**: `minix3/minix/drivers/hid/pckbd/pckbd.c`（504 行，libinputdriver 使用面）
> **Rust 模块**: 外部契约（驱动阶段，不实现）
> **draft 素材**: 无

## 核心点

- `pckbd_init`（:465-487）：`flags = INPUT_DEV_KBD`；`aux_available != 0` 时 `flags |= INPUT_DEV_MOUSE`；`inputdriver_announce(flags)`（锚 12）
- 键盘事件上报：`kbd_process` → `inputdriver_send_event(FALSE, page, code, press, 0)`（:365；扫描码状态机 + scanmap 表 → HID 码）
- 鼠标事件上报：`kbdaux_process`（:370-415）→ 按钮事件（`INPUT_PAGE_BUTTON`/`INPUT_BUTTON_1+i`）+ 相对位移（`INPUT_PAGE_GD`/`INPUT_GD_X/Y` + `INPUT_FLAG_REL`）
- `pckbd_leds`（:418-431）：INPUT_LED_* 位 → 键盘端口 LED 位（set_leds 写端口）
- `pckbd_intr`（:434-453）/`pckbd_alarm`（:456-463）：HARDWARE/CLOCK notify 回调（scan_keyboard 取码 + 看门狗）
- 主循环：`pckbd_startup`（SEF）+ `inputdriver_task(&pckbd_tab)`（:500-504）

## 边界

- **前置依赖**: 12（libinputdriver 使用面）+ 04（事件码）
- **不覆盖（移交）**: 驱动硬件细节（端口 I/O/IRQ/watchdog/扫描码表，驱动阶段）、server 内部（01~12）
