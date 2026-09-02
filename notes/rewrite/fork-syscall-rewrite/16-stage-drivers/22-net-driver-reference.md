# 22-net-driver-reference — 网卡驱动参考实现（dp8390 + virtio_net）

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

NDEV 完整消费：初始化/收发/链路/组播/统计；dp8390 参考实现 + virtio_net（virtio 框架消费）。C: drivers/net/dp8390/ + virtio_net/。Rust: os/drivers/net/dp8390 + virtio_net。

## 边界

- **前置依赖**: 03/14
- **本篇不覆盖**: 网卡变体（23）；lwip（17-stage-net）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
