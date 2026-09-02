# 01-chardriver-framework — 字符驱动框架 libchardriver

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

chardriver_task 主循环、chardriver_process 分发、CDEV_* 协议（com.h:919-932）、cdr_* 回调表（open/close/read/write/ioctl/cancel/select/intr/alarm/other）、suspend/恢复、reply_select、announce。C: lib/libchardriver/chardriver.c。Rust: minix-chardriver。

## 边界

- **前置依赖**: 00；VFS 侧 CDEV 消费（../05-stage-vfs/）
- **本篇不覆盖**: 具体驱动实现（05~08/13）；协议常量值（99）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
