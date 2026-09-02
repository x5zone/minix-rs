# 16-lwip-ifaddr — 接口地址管理

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- ifaddr（ifaddr.c:2224）：接口地址管理模块，逻辑上属于 ifdev（仅访问 ifdev 的地址字段）
- IPv4/IPv6 地址列表管理（添加/删除/查询，ifconfig 驱动）
- 硬件地址列表：活动地址切换（`iop_set_hwaddr` 到接口模块）
- 与 route/rtsock 的地址解析交互（RTM_* 消息处理）
- Rust: `os/net/lwip`（地址模块）

## 边界

- **前置依赖**: 14/15
- **本篇不覆盖**: ioctl 配置面（17）。
- **讲述结构**: 见 `plan.md` §3.1
