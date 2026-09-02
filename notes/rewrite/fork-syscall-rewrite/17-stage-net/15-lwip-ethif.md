# 15-lwip-ethif — 以太网接口实例

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- ethif（ethif.c:1718）：以太网接口模块，实现 ifdev_ops（初始化/收发/配置请求）
- 发送队列：包发送入队 + 配置请求排队（OOM 时配置请求可能被拒）
- 与 ndev 驱动绑定：ethif ↔ ndev_id（NDEV 驱动的以太网面）
- 输入路径：ndev_recv → ethif_input → lwIP netif
- Rust: `os/net/lwip`（以太网接口）

## 边界

- **前置依赖**: 13/14
- **本篇不覆盖**: ifdev 通用层（14）。
- **讲述结构**: 见 `plan.md` §3.1
