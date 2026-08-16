# 17-pipe: pipe 与阻塞恢复

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 7 — 文件描述符与文件 I/O
> **源码**: `pipe.c` 全文件、`glo.h`（susp_count/reviving）
> **Rust 模块**: （未实现）pipe 模块
> **draft 素材**: 无（新建）

## 核心点

- do_pipe2/create_pipe：管道创建与 fd 对
- pipe_check：读写阻塞条件判断
- suspend/pipe_suspend/unsuspend_by_endpt：FP_BLOCKED_ON_PIPE 挂起
- revive/unpause/release：恢复路径与 vnode 释放
- susp_count/reviving 全局计数（主循环 unblock 入口，09）
- map_vnode：管道 vnode 映射

## 边界

- select 阻塞不覆盖（23）
- 通用阻塞状态机不覆盖（02/09）
