# 15-virtio-blk-driver — virtio 块设备驱动

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

virtio_blk 块设备、virtio 框架首次完整消费、BDEV 面。C: drivers/storage/virtio_blk/virtio_blk.c。Rust: os/drivers/storage/virtio_blk。

## 边界

- **前置依赖**: 14/02
- **本篇不覆盖**: virtio 框架（14）；其他存储（16/17）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
