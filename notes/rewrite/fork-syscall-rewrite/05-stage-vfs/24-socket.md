# 24-socket: socket 系统调用

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 11 — 网络
> **源码**: `socket.c` 全文件
> **Rust 模块**: （未实现）socket 模块
> **draft 素材**: 无（新建）

## 核心点

- do_socket/do_socketpair/do_bind/do_connect/do_listen/do_accept
- do_sendto/do_recvfrom/do_sockmsg（sendmsg/recvmsg）
- do_setsockopt/do_getsockopt/do_getsockname/do_getpeername/do_shutdown/do_socketpath
- resume_accept/resume_recvfrom/resume_recvmsg：阻塞续作（与 22 的 sdev_finish 协作）
- get_sock_flags/check_sock_fds/make_sock_fd/get_sock：fd 与 flags 管理

## 边界

- 驱动协议不覆盖（22）
- select 机制不覆盖（23）
