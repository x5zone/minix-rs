# 06-input-open-close: 打开与关闭语义

> **状态**: pending（最小骨架，待改写）
> **定位**: CDEV_OPEN/CDEV_CLOSE handler（阶段 3 字符设备操作面）
> **源码**: `minix3/minix/servers/input/input.c:85-127`（input_open/input_close）
> **Rust 模块**: `handlers.rs`（open/close）
> **draft 素材**: `../../../../tmp/input/tmp_input.c.md`（素材）

## 核心点

- `input_open`（:85-105）：`input_map` 失败 → ENXIO；`input_dev_active` 为假 → ENXIO（mux 恒 active，驱动未注册的设备不可开）；`opened` 已开 → EBUSY；置 `opened=TRUE`（access/user_endpt 忽略）
- `input_close`（:107-127）：map 失败 → ENXIO；未打开 → EINVAL（日志）；置 `opened=FALSE` + 清 `tail=0,count=0`
- `input_dev_active` 语义：owner != NONE 或 mux minor（input.h:24-27）
- 错误码映射：ENXIO/EBUSY/EINVAL（A-11）

## 边界

- **前置依赖**: 03（map/active）+ 02（CDEV 分发）
- **不覆盖（移交）**: 读取/挂起（07）、ioctl/cancel/select（08）、事件生产（09）
