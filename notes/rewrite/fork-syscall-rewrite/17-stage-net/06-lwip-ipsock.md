# 06-lwip-ipsock — IP socket 公共层

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `ipsock_socket`（ipsock.c:123）：IP socket 公共创建路径（sndbuf/rcvbuf/domain）
- 源地址选择：`ipsock_get_src_addr`（与 addrpol 的 label/scope 联动，udpsock.c:107 引用）
- 选项面：IPPROTO_IP / IPPROTO_IPV6（hop limit、TOS、多播选项）
- 连接语义（IP 层）：bind/connect 的地址解析与校验
- IPv6 条件编译（`USE_INET6`，net/lwip/Makefile；ARCH N-5）
- Rust: `os/net/lwip`（IP socket 基类）

## 边界

- **前置依赖**: 05、07（pktsock 共享层）
- **本篇不覆盖**: TCP/UDP/RAW 具体语义（08/09/10）。
- **讲述结构**: 见 `plan.md` §3.1
