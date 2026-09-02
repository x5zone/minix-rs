# 13-pckbd-driver — 键盘/鼠标驱动 pckbd

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

键盘/鼠标、libinputdriver 事件桥（inputdriver_send_event）、按键映射表（table.c）、LED、与 12-stage-input 交互。C: drivers/hid/pckbd/ + lib/libinputdriver/。Rust: os/drivers/hid/pckbd。

## 边界

- **前置依赖**: 01；12-stage-input 事件消费
- **本篇不覆盖**: input server 事件消费（12-stage-input）；TTY 键盘读取（06）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
