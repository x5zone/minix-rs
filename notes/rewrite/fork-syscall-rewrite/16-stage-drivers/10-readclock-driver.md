# 10-readclock-driver — 实时时钟驱动 readclock

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

RTC、RTCDEV_* 协议（com.h:995-1008）、CMOS/EFI 时钟、forward。C: drivers/clock/readclock/（readclock.c/forward.c）。Rust: os/drivers/clock/readclock。

## 边界

- **前置依赖**: 01
- **本篇不覆盖**: 时钟服务（01-stage-kernel/15-clock-timer）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
