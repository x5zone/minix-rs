# 17-storage-misc-driver — 存储杂项驱动

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

floppy/mmc/fbd/filter/vnd 变体差异矩阵。C: drivers/storage/（floppy/mmc/fbd/filter/vnd）。Rust: os/drivers/storage/*。

## 边界

- **前置依赖**: 02
- **本篇不覆盖**: AHCI/ATA（16）；virtio（15）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
