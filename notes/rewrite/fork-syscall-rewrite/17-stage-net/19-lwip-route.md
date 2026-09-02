# 19-lwip-route — 路由表与 lwIP 路由覆盖

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `rttree.c`（744 行）：radix 树路由存储；核心假设——掩码均可表达为 prefix length（rttree.c:5-8），节点可含子节点（非纯叶）
- `route.c`（1654 行）：路由管理；以 weak-symbol 覆盖 lwIP `ip4_route`/`ip6_route` + lwIP gateway hook（lwiphooks.h）替代 lwIP 自带路由
- 默认路由语义：默认网关与默认路由条目的关系（route.c:55-60）
- 网关 hook：`lwip_hook_etharp_get_gw`/`lwip_hook_nd6_get_gw`（lwiphooks.h）
- Rust: `os/net/lwip`（路由模块）

## 边界

- **前置依赖**: 03、24（lwIP hook 面）
- **本篇不覆盖**: 路由 socket 消息（20）。
- **讲述结构**: 见 `plan.md` §3.1
