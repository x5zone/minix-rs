# 12-gpio-devman — GPIO 导出与 libdevman 驱动侧

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

GPIO VTreeFS 导出（gpio.c:287 run_vtreefs）、libdevman 驱动侧注册（generic.c/usb.c）、设备绑定。C: drivers/system/gpio/gpio.c + lib/libdevman/。Rust: os/drivers/system/gpio + minix-devman。

## 边界

- **前置依赖**: 00
- **本篇不覆盖**: devman server（11-stage-devman）；VTreeFS 框架（15-stage-fs/18）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
