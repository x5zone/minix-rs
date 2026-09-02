# 10-lwip-rawsock — RAW socket 模块

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `rawsock_socket`（rawsock.c:290）：RAW socket 创建（内嵌 pktsock，`raw_new_ip_type`）
- 权限：`util_is_root(user_endpt)` 检查（alloc_socket 中 EACCES）
- IPv4 raw：含 IP 头收发；IPv6 raw：扩展头语义；ICMP（IPv4/IPv6）支撑（dhcpcd/ping6/traceroute6 等工具需求）
- 头部处理差异（IPv4 含头 vs IPv6 不含）与校验和语义
- Rust: `os/net/lwip`（RAW 模块）

## 边界

- **前置依赖**: 06、07
- **本篇不覆盖**: ICMP 协议语义细节（24）。
- **讲述结构**: 见 `plan.md` §3.1
