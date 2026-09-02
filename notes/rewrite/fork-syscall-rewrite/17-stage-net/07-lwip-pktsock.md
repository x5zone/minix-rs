# 07-lwip-pktsock — 包 socket 共享层

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `pktsock_socket`（pktsock.c:59）：udp/raw 共享的包层创建路径（内嵌 ipsock）
- `pktsock_input`：IP 层输入分发（udpsock_input/rawsock 回调）
- snd/rcv buf 管理：默认值（UDP_SNDBUF_DEF 8192 / UDP_RCVBUF_DEF 32768，udpsock.c:30-33）
- 与 lwIP PCB 的绑定（`udp_new_ip_type`/`raw_new_ip_type` 调用面）
- Rust: `os/net/lwip`（包层模块）

## 边界

- **前置依赖**: 06
- **本篇不覆盖**: IP 选项语义（06）；具体协议（08/09/10）。
- **讲述结构**: 见 `plan.md` §3.1
