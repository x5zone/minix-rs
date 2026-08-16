# 03-input-device-structs: 设备结构与编号映射

> **状态**: pending（最小骨架，待改写）
> **定位**: `struct input_dev` + `devs[10]` + minor/DEV 编号（阶段 2 核心数据结构）
> **源码**: `minix3/minix/servers/input/input.h`（45 行，全部）+ `input.c:44-83`（input_map/input_revmap）
> **Rust 模块**: `structs.rs`
> **draft 素材**: `../../../../tmp/input/tmp_input.h.md`（素材）

## 核心点

- `struct input_dev` 13 字段（input.h:29-43）：minor/owner/label/eventbuf[32]/tail/count/opened/suspended/caller/grant/req_id/selector/leds
- `devs[INPUT_DEV_MAX]` 布局（10 槽：KBDMUX + 4 KBD + MOUSEMUX + 4 MOUSE）
- 常量：`EVENTBUF_SIZE`/minor 编号（KBDMUX=0/KBD0=1/KBD_MINORS=4/MOUSEMUX=64/MOUSE0=65/MOUSE_MINORS=4）/DEV 下标/`INPUT_DEV_MAX=10`（input.h:7-26）
- `input_map`（:44-64）：minor → 结构指针（稀疏编号向后兼容设计）
- `input_revmap`（:67-82）：下标 → minor（非法 id panic，A-7）
- `input_dev_active`/`input_dev_buf_empty`/`input_dev_buf_full` 宏（:24-27，mux 恒 active 语义）
- ARCH A-3（id==下标）/A-7（Newtype 类型化 + revmap 显式错误）

## 边界

- **前置依赖**: 02（minor 挂接）
- **不覆盖（移交）**: 事件格式（04）、消息面（05）、各 handler 行为（06~10）
