# 20-lwip-rtsock — 路由 socket（PF_ROUTE）

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `rtsock_socket`（rtsock.c:311）：PF_ROUTE socket（路由控制面）
- rt_msghdr 解析/生成、RTA 数组压缩/展开（rtsock 独有，其他模块不得引用 rt_msghdr 或 ip_addr_t）
- 路由消息族：RTM_*（add/del/change/get 等），与 route/rttree/ifaddr 的交互
- `net.inet/route` MIB 树节点注册
- Rust: `os/net/lwip`（路由 socket 模块）

## 边界

- **前置依赖**: 19
- **本篇不覆盖**: 路由表内部（19）。
- **讲述结构**: 见 `plan.md` §3.1
