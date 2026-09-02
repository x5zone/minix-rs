# 11-pci-driver — PCI 总线驱动 pci

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

PCI 枚举、配置空间读写、pci_table、中断路由、libpci 客户端面。C: drivers/bus/pci/（main.c/pci.c/pci_table.c）。Rust: os/drivers/bus/pci。

## 边界

- **前置依赖**: 00
- **本篇不覆盖**: 各设备驱动如何用 PCI（14/16/22/23）；I2C（24）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
