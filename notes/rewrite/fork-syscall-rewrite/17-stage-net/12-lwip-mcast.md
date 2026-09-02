# 12-lwip-mcast — 组播成员管理

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `mcast_init`：成员数组/free list 初始化（mcast.c:283）
- 成员结构：`struct mcast_member`（ifdev + 组地址），全局上限 `NR_IPV4_MCAST_GROUP(64) + NR_IPV6_MCAST_GROUP(64) = 128`（lwipopts.h），per-socket 上限 `MAX_GROUPS_PER_SOCKET(8)`（mcast.c:36）
- 加入/离开语义：成员复用（多 socket 同组共享）、与 lwIP IGMP/MLD 成员结构映射（`LWIP_IGMP=1`）
- 接口移除时的成员清理（mcast.c:277-281）
- Rust: `os/net/lwip`（组播模块）

## 边界

- **前置依赖**: 06
- **本篇不覆盖**: IGMP/MLD 协议实现（24）；具体 socket 使用（08/09）。
- **讲述结构**: 见 `plan.md` §3.1
