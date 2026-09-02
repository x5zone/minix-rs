# 18-usb-framework — USB 栈框架 libusb + usbd

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

URB 协议（com.h:816-820）、HCD（hcd.c/hcd_common.c/hcd_schedule.c/hcd_ddekit.c/musb）、枚举。C: lib/libusb/usb.c + drivers/usb/usbd/。Rust: minix-usb + os/drivers/usb/usbd。

## 边界

- **前置依赖**: 00
- **本篇不覆盖**: usb_storage/usb_hub（19）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
