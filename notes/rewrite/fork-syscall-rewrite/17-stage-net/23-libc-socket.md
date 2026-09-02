# 23-libc-socket — 用户态 socket 系统调用封装

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- libc socket 封装 15 文件 / 3173 行：socket/socketpair/bind/connect/listen/accept/sendto/recvfrom/sendmsg/recvmsg/setsockopt/getsockopt/getsockname/getpeername/shutdown
- 主路径：`__socket` → `_syscall(VFS_PROC_NR, VFS_SOCKET, &m)`（socket.c:44-55），VFS 转发 SDEV 到 socket driver
- 旧式 fallback：VFS 返回 EAFNOSUPPORT/ENOSYS 时回退 `open(TCP_DEVICE/UDP_DEVICE/IP_DEVICE/UDS_DEVICE)` + `net/gen/*` 旧设备协议（NWIOSIPOPT 等，socket.c:74-241）—— **[ARCH] N-2 WONTFIX**（minix-rs 丢弃 fallback，VFS SDEV 路径唯一）
- SOCK_CLOEXEC/SOCK_NONBLOCK/SOCK_NOSIGPIPE 标志处理（`_socket_flags`）
- 与 14-stage-runtime 的边界：socket 封装被 14 排除至此（14 plan §3.3 09 行）
- Rust: `os/libs/minix-sys`（vfs socket 模块）

## 边界

- **前置依赖**: 05（`_syscall` 机制）、`../05-stage-vfs/24-socket.md`
- **本篇不覆盖**: VFS 服务端（05）；socket driver 服务端（本 stage 03/21）。
- **讲述结构**: 见 `plan.md` §3.1
