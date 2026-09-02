# 02-sockevent-framework — socket 事件分发框架 libsockevent

> **状态**: 最小化骨架（依 `plan.md` §2/§3.3 生成，待按 §3.1 章节模板展开）

## 核心点

- libsockevent 全量：`sockevent.c`（2590 行）+ `sockevent_proc.c`（52 行）
- `struct sock` 对象模型：`sockhash` 256 槽（`id + (id >> 16)) % SOCKHASH_SLOTS`）、`socktimer` 定时器队列、`sockevent_pending` 待处理队列
- `sockevent_ops` 回调表 21 项（sop_pair → sop_free，sockevent.h:54-97）
- 悬挂调用续作（sockevent_proc）、select 支持（SDEV_SELECT1/2_REPLY 联动）
- 错误/关闭/shutdown 传播：`sockevent_set_error/set_shutdown/raise`、`sockevent_is_shutdown`
- 分发入口：`sockevent_process`（sockdriver 框架调用）、`sockevent_init(socket_cb)`（socket 回调：域/类型/协议 → 各协议族模块）
- Rust: `os/libs/minix-netdriver`（或独立 minix-sockevent）

## 边界

- **前置依赖**: 01
- **本篇不覆盖**: SDEV 消息编码（01）；各协议族实现（06~12、21/22）。
- **讲述结构**: 见 `plan.md` §3.1
