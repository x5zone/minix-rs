# 09-lwip-udpsock — UDP socket 模块

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `udpsock_socket`（udpsock.c:116）：UDP socket 创建（内嵌 pktsock，`udp_new_ip_type`）；协议校验（0/IPPROTO_UDP，UDPLITE 不支持）
- send/recv、bind/connect（`ipsock_get_src_addr`，允许组播地址）、校验和
- 多播默认：TTL=1、loop 开启（`udp_set_multicast_ttl`/`UDP_FLAGS_MULTICAST_LOOP`）
- `udpsock_input`（udpsock.c:96）→ `pktsock_input` 输入路径
- MIB：UDPCTL_SENDSPACE/RECVSPACE（RMIB_RO，udpsock.c:60-63）
- Rust: `os/net/lwip`（UDP 模块）

## 边界

- **前置依赖**: 06、07
- **本篇不覆盖**: 组播成员管理（12）；pktsock 层（07）。
- **讲述结构**: 见 `plan.md` §3.1
