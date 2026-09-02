# 11-lwip-lnksock — AF_LINK socket 与链路层数据

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `lnksock_socket`（lnksock.c:41）：AF_LINK socket（77 行，仅支撑 IOCTL：`sop_ioctl = ifconf_ioctl`，lnksock.c:75，ifconfig(8) 需求）
- `lldata.c`（584 行）：链路层路由数据（与 IP 路由 rttree 分离的原因：lwIP 数据结构隔离 + NetBSD 8 起 IP/链路层路由分离）
- `sockaddr_dlx`（lwip.h）：自定最大尺寸 AF_LINK 地址结构
- Rust: `os/net/lwip`（链路层模块）

## 边界

- **前置依赖**: 06
- **本篇不覆盖**: 以太网接口（15）；ifaddr（16）。
- **讲述结构**: 见 `plan.md` §3.1
