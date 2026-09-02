# 08-lwip-tcpsock — TCP socket 模块

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `tcpsock_socket`（tcpsock.c:242）：TCP socket 创建（内嵌 ipsock + lwIP `tcp_new_ip_type`）
- 连接状态机（lwIP TCP 语义上的 Minix 层封装）、send/recv 队列（lwIP pbuf，来自 04 mempool）
- 传输参数：MSS/Nagle/delayed ACK、`TCP_SNDBUF_DEF 32768`/`TCP_RCVBUF_DEF MAX(TCP_WND,32768)`（tcpsock.c:87-90）
- listen/accept、SO_* 选项、错误传播（sockevent_set_error）、SIGPIPE（KILL 权限，lwip.conf）
- `tcpisn`：`tcpisn_init`（tcpisn.c:48）、`lwip_hook_tcp_isn`（:136，SHA256 + 4μs 定时器 + `net.inet.tcp.isn_secret` sysctl，根权限可写）
- Rust: `os/net/lwip`（TCP 模块）

## 边界

- **前置依赖**: 06、07
- **本篇不覆盖**: lwIP TCP 内部实现（24）；SIGPIPE 机制（14-stage-runtime/08）。
- **讲述结构**: 见 `plan.md` §3.1
