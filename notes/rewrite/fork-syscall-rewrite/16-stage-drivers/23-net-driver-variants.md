# 23-net-driver-variants — 网卡驱动变体

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

其余 12 网卡（e1000/rtl8139/rtl8169/fxp/3c90x/atl2/lance/dec21140A/ip1000/lan8710a/vt6105/dpeth）变体差异矩阵。C: drivers/net/（其余 12 目录）。Rust: os/drivers/net/*。

## 边界

- **前置依赖**: 22
- **本篇不覆盖**: dp8390/virtio_net 参考语义（22）。
- **讲述结构**: 见 `plan.md` §3.1（概念/C 源码分析/Rust 设计决策/错误处理/测试/过渡/参见）
