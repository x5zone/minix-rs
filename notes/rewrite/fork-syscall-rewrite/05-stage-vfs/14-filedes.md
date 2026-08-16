# 14-filedes: 文件描述符表操作

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 7 — 文件描述符与文件 I/O
> **源码**: `filedes.c:88-140,250-312,524-656`、`open.c:690`（close_fd 定义）
> **Rust 模块**: （未实现）fd 模块（fproc.rs 的 fp_filp 字段为 Rust 现状）
> **draft 素材**: `draft/17-filedes.md` + `draft/fd-table-copy.md`（素材）

## 核心点

- get_fd/check_fds：fd 分配（最低空闲位策略）与 nfds 校验
- close_fd（open.c:690）：关 fd → close_filp → put_vnode 链
- do_copyfd：fd 复制系统调用
- invalidate_filp 族：驱动/端点在事件后的 filp 失效（by_char_major/by_sock_drv/by_endpt）
- fd 复用与 fp_cloexec_set 语义

## 边界

- filp 结构字段不覆盖（04）
- PM fork 的 fd 复制不覆盖（10）
