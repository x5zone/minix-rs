# 05-lwip-util-addr — 公共工具、sockaddr 解析与地址策略

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- util.c（251 行）：`util_convert_err`（lwIP ERR_* ↔ errno 双射）、`util_is_root`（ROOT_EUID 检查）、`util_timeval_to_ticks/ticks_to_timeval`（US=1000000）
- addr.c（699 行）：sockaddr 解析/校验（AF_UNSPEC 检查）、`sockaddr_in/in6/sockaddr_dlx` 互转、`SOCKADDR_MAX=256` 静态断言（STATIC_SOCKADDR_MAX_ASSERT）
- addrpol.c（143 行）：RFC 6724 地址选择策略 `addrpol_get_label/get_scope`（源地址选择，目标地址选择在 libc）
- Rust: `os/net/lwip`（工具模块）+ `minix-types`（SockAddr，ARCH N-8）

## 边界

- **前置依赖**: 03
- **本篇不覆盖**: 各 socket 模块如何使用（06~12）；常量值全集（99）。
- **讲述结构**: 见 `plan.md` §3.1
