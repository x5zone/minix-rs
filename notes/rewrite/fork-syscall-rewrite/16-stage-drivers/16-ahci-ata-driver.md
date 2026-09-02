# 16-ahci-ata-driver — AHCI/ATA 存储驱动

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

AHCI HBA 参考（端口/命令表/PRDT）+ ATA PIO/DMA 变体。C: drivers/storage/ahci/ + at_wini/。Rust: os/drivers/storage/ahci + at_wini。

## 边界

- **前置依赖**: 02
- **本篇不覆盖**: virtio 存储（15）；存储杂项（17）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
