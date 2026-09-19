# 99-global-concepts: SCHED 全局概念与常量

> **重写**: 2026-09-20（edge3 卡N/S41）
> **状态**: 正文
> **定位**: 全局概念（所有文档共享的常量表、错误码与跨服务契约）
> **源码**: `minix3/minix/include/minix/com.h`、`config.h`、`sys/sys/signal.h`
> **Rust 模块**: `minix-types/src/types/com.rs`、`os/servers/sched/src/schedproc.rs`

## 1 概念：SCHED 的常量不是数字，是策略边界的围栏

### 1.1 协议族——五个请求，一个命名空间

SCHED 的全部对外协议住在 `SCHEDULING_BASE 0xF00`（`com.h:801`）一个名字空间里，五个请求依次是 `SCHEDULING_NO_QUANTUM`（+1）、`SCHEDULING_START`（+2）、`SCHEDULING_STOP`（+3）、`SCHEDULING_SET_NICE`（+4）、`SCHEDULING_INHERIT`（+5）（`com.h:803-807`）。名字空间的价值在排他性：`0xF00` 段不属于内核调用、不属于 PM、不属于 VFS，五个编号就是 SCHED 与世界之间的全部接口面——12 号（内核接口）与 13/14 号（PM/RS 交互）三章讲的所有跨服务对话，线上一律落在这五个编号里。

Rust 侧的单一事实源是 `minix-types/src/types/com.rs` 的 `SCHEDULING_*` 常量（`SCHEDULING_NO_QUANTUM` 对位 `com.h:803`），SCHED 服务器与 PM 的客户端都从这里 import，不各持副本。

### 1.2 容量与策略常量——16 个队列与两个端点

`NR_SCHED_QUEUES` 是 16（`config.h:66`；Rust `schedproc.rs:18`）：优先级队列从 0 编到 15，数字越小优先级越高。围绕它有三个派生量——系统进程的 `TASK_Q`、用户进程的上限 `MAX_USER_Q` 与默认位 `DEFAULT_USER_Q`——它们的取值与折算规则集中在 `05-priority-timeslice-model`，本表不展开。

端点方面，SCHED 自己占 `SCHED_PROC_NR = 4`（Rust `minix-types::Endpoint::SCHED`）；`schedproc[NR_PROCS]` 表（`schedproc.h:36`）按进程表全容量开槽，槽内记录以端点为键（`03-schedproc-struct`）。

### 1.3 五进程表一致性——SCHED 的视图为什么必须与四处对上

Minix3 的一个进程在五个表里各有一行：内核 `proc`、PM `mproc`、VM `vmproc`、VFS `fproc`、SCHED `schedproc`。五表各自为政，靠**端点一致**对上号：SCHED 表里一行的 `endpoint` 与 `parent`（`schedproc.h:24-25`）必须与内核/PM 的同一进程吻合，否则策略决定会下发给错误的执行对象。`SCHEDULING_START`/`INHERIT` 的边界检查（06）与表检查（04）守的就是这条不变量；Rust 侧 `SchedProc` 结构的 `endpoint`/`parent` 字段（`schedproc.rs:74` 起）逐字 carries C 的锚点。

## 2 错误码——每个拒绝的理由

SCHED 的 handler 拒绝请求只用三个错误码，各有一句判据：

- `EPERM`：调用者没资格——给非父进程发 START/INHERIT，或对系统进程动 nice（`schedule.c` 三处）；
- `EBADEPT`：目标端点不是活进程（端点已死或尚未 START；`sys/errno.h:212`，Rust `minix_types::EBADEPT`）；
- `EINVAL`：参数越界——优先级越出队列界、时间片为零（`schedule.c` 两处；折算与界检查见 05/06/08）。

Rust 侧同一套折算：`valid.rs` 的检查面产出同样的三码，消费 `minix_types` 常量。除此之外协议不发明新错误码——错误面窄是协议窄的另一半。

## 3 执行模型声明

SCHED 是单线程事件循环服务器（与 VFS/DS 同族）：一个 `receive` 循环顺序处理五个请求，无内部并发。这与内核执行面的 SMP 并发是两回事——内核多核同时跑进程，SCHED 单核顺序做决定；11 号（队列均衡）处理的正是"多核的事实如何进入单线程的策略"。

## 4 有意省略表（intentional omissions）

- **内核侧队列实现**：`NR_SCHED_QUEUES` 个队列在内核里的数据结构与切换时机是执行面的事，见 `../01-stage-kernel/`；
- **nice 的系统调用接口**：用户 `nice(2)` 如何到达 SCHED 是 PM 的翻译层，见 `../04-stage-pm/16-scheduling.md`；
- **nice 数值表**：队列折算的具体数值见 05 的换算公式，`SET_NICE` 只改参数不即时迁移队列的策略含义见 08。

## 5 参见

- 阶段内：05（参数折算的权威篇）、04（表检查）、12~14（三方交互）
- 本线状态：`../../edge3.md` S27 行（已通电）、S7/S10 行（PM 调度臂接线）
