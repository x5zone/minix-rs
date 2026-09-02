# 19-usb-storage-hub — USB 存储与 Hub 驱动

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

usb_storage（含 scsi.c）+ usb_hub：BOT 传输、SCSI 命令、hub 端口管理。C: drivers/usb/usb_storage/ + usb_hub/。Rust: os/drivers/usb/usb_storage + usb_hub。

## 边界

- **前置依赖**: 18
- **本篇不覆盖**: USB 框架（18）；存储语义（15~17）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
