# 99-global-concepts — 驱动子系统全局概念

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

请求常量全集（CDEV 0x400/BDEV 0x500/NDEV 0x1A00/RTCDEV 0x1400/USB_RQ/SDEV 0x1900）、driver 通用模型（driver.h）、minor 布局、/dev 命名约定、endpoint 约定、errno 映射。C: com.h + driver.h + 各 *driver.h。Rust: minix-types。

## 边界

- **前置依赖**: 无
- **本篇不覆盖**: 常量如何被使用（各文档）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
