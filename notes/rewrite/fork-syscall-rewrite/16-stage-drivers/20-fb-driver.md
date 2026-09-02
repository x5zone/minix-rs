# 20-fb-driver — 帧缓冲驱动 fb

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

帧缓冲（fb.c/fb_edid.c/fb_arch.c）、mmap 到用户、EDID。C: drivers/video/fb/。Rust: os/drivers/video/fb。

## 边界

- **前置依赖**: 00；02-stage-vm mmap 机制
- **本篇不覆盖**: 控制台渲染（06）；VM mmap 机制（02-stage-vm）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
