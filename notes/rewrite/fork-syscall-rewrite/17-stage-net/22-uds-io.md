# 22-uds-io — uds 数据面（缓冲段 + ancillary + FD 传递）

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- `uds_io_init`（io.c:102）；数据面（io.c:1803）：uds 无发送缓冲，仅接收缓冲，缓冲分**段**（segment：普通数据/ancillary/两者/皆无）
- 缓冲常量：`UDS_BUF=32768`（每 socket 接收缓冲，页大小倍数）、`UDS_CTL_MAX=4096`
- ancillary data 两类：in-flight 文件描述符（`struct uds_fd` 队列，socketpath/copyfd）+ 发送者凭据
- SOCK_STREAM / SOCK_SEQPACKET / SOCK_DGRAM 的缓冲/边界语义差异
- 悬挂续作（与 sockevent_proc 联动）
- Rust: `os/net/uds`（I/O 模块）

## 边界

- **前置依赖**: 21
- **本篇不覆盖**: uds 连接状态机（21）；VFS copyfd 客户端（05-stage-vfs）。
- **讲述结构**: 见 `plan.md` §3.1
