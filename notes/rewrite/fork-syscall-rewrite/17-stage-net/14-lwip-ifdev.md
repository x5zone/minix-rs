# 14-lwip-ifdev — 接口对象模型与环回接口

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `struct ifdev`（ifdev.h）：接口对象（名称/索引/链路状态/lwIP netif 绑定）
- `ifdev_ops` 回调表 16 项（iop_init → iop_destroy，ifdev.h）：初始化/输入/输出（v4/v6）/头部完成/轮询/标志/能力/媒体/混杂/硬件地址/MTU/销毁
- 硬件地址列表：`IFDEV_NUM_HWADDRS=3`、`IFHWAF_VALID/FACTORY` 标志、活动地址切换
- `loopif`（loopif.c:420）：环回接口（`loopif_init`，回环不经过 NDEV）
- Rust: `os/net/lwip`（接口层）

## 边界

- **前置依赖**: 13
- **本篇不覆盖**: 以太网实现（15）；地址管理（16）；配置（17）。
- **讲述结构**: 见 `plan.md` §3.1
