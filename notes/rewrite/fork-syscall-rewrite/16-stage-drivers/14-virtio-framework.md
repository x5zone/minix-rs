# 14-virtio-framework — virtio 框架 libvirtio

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

virtqueue 描述符/可用/使用环、设备协商、MMIO/PCI 传输、barrier。C: lib/libvirtio/（virtio.c + virtio_ring.h）。Rust: minix-virtio。

## 边界

- **前置依赖**: 00
- **本篇不覆盖**: virtio_blk/virtio_net 设备语义（15/22）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
