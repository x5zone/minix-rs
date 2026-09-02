# 03-netdriver-framework — 网卡驱动框架 libnetdriver

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

netdriver_task 主循环、NDEV_* 协议（com.h:1085-1101）、ndo_* 回调表、portio（I/O 端口辅助）、链路状态/组播/统计。C: lib/libnetdriver/（netdriver.c/portio.c）。Rust: minix-netdriver。

## 边界

- **前置依赖**: 00
- **本篇不覆盖**: 具体网卡实现（22/23）；lwip/uds（17-stage-net）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
